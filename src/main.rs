mod crypto;
mod discovery;

use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

const CHUNK_SIZE: usize = 64 * 1024; // 64 KB setiap blok
const MAX_HEADER_LEN: usize = 4096;

#[derive(Parser)]
#[command(name = "lan-drop", version, about = "Encrypted P2P LAN File Transfer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Sedia menerima fail dalam rangkaian tempatan (LAN)
    Receive {
        #[arg(short, long, default_value_t = 9000)]
        port: u16,
        #[arg(long)]
        pin: Option<String>,
    },
    /// Cari penerima secara automatik & hantar fail dengan penyulitan AES-256-GCM
    Send {
        #[arg(short, long)]
        file: PathBuf,
        #[arg(short, long)]
        addr: Option<String>,
        #[arg(long)]
        pin: Option<String>,
    },
}

#[derive(Serialize, Deserialize)]
struct FileHeader {
    filename: String,
    size: u64,
    sha256: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Receive { port, pin } => receive_file(port, pin).await,
        Commands::Send { file, addr, pin } => send_file(&file, addr, pin).await,
    }
}

fn make_progress_bar(total: u64) -> Result<ProgressBar> {
    let pb = ProgressBar::new(total);
    pb.set_style(
        ProgressStyle::with_template(
            "{spinner:.green} [{elapsed_precise}] [{bar:35.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA {eta})",
        )?
        .progress_chars("#>-"),
    );
    Ok(pb)
}

async fn hash_file(path: &Path) -> Result<(u64, String)> {
    let mut f = File::open(path).await.context("Gagal membuka fail sumber")?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    Ok((size, hex::encode(hasher.finalize())))
}

async fn send_file(path: &Path, addr: Option<String>, pin_opt: Option<String>) -> Result<()> {
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .context("Nama fail tidak sah")?
        .to_string();

    let target = match addr {
        Some(a) => a,
        None => {
            println!("🔍 Mencari peranti penerima dalam rangkaian LAN...");
            let (peer_addr, host) = discovery::find_peer().await?;
            println!("✅ Ditemui '{}' di {}", host, peer_addr);
            peer_addr.to_string()
        }
    };

    let pin = match pin_opt {
        Some(p) => p,
        None => {
            print!("🔑 Masukkan PIN 4-digit daripada skrin penerima: ");
            io::stdout().flush()?;
            let mut input = String::new();
            io::stdin().read_line(&mut input)?;
            input.trim().to_string()
        }
    };

    let (size, sha256) = hash_file(path).await?;
    let mut salt = [0u8; 16];
    rand::thread_rng().fill(&mut salt);
    let cipher = crypto::derive_cipher(&pin, &salt);

    let mut stream = TcpStream::connect(&target).await.context("Gagal sambung TCP")?;

    // Handshake: Hantar salt (16 bait) + encrypted header (nonce = 0)
    let header_json = serde_json::to_vec(&FileHeader { filename: filename.clone(), size, sha256 })?;
    let enc_header = crypto::encrypt_chunk(&cipher, 0, &header_json)?;

    stream.write_all(&salt).await?;
    stream.write_u32(enc_header.len() as u32).await?;
    stream.write_all(&enc_header).await?;

    if stream.read_u8().await? != 1 {
        bail!("❌ Penerima menolak sambungan (PIN tidak sepadan)!");
    }

    // Stream fail dengan AES-256-GCM
    let pb = make_progress_bar(size)?;
    let mut file = File::open(path).await?;
    let mut buf = vec![0u8; CHUNK_SIZE];
    let mut counter = 1u64;

    loop {
        let n = file.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        let enc = crypto::encrypt_chunk(&cipher, counter, &buf[..n])?;
        stream.write_u32(enc.len() as u32).await?;
        stream.write_all(&enc).await?;
        pb.inc(n as u64);
        counter += 1;
    }

    stream.write_u32(0).await?; // Penanda tamat fail (EOF)
    stream.flush().await?;
    pb.finish_with_message("Selesai");

    if stream.read_u8().await? == 1 {
        println!("🎉 '{}' berjaya dihantar & disahkan SHA-256!", filename);
    }
    Ok(())
}

async fn receive_file(port: u16, pin_opt: Option<String>) -> Result<()> {
    let pin = pin_opt.unwrap_or_else(|| rand::thread_rng().gen_range(1000..=9999).to_string());
    discovery::spawn_responder(port).await?;

    let listener = TcpListener::bind(("0.0.0.0", port)).await?;
    println!("📡 Menunggu kiriman fail di port TCP {} (UDP Auto-Discovery aktif)", port);
    println!("🔐 Kod PIN Keselamatan: {}", pin);

    let (mut stream, peer) = listener.accept().await?;
    println!("🔗 Sambungan masuk daripada {}", peer);

    let mut salt = [0u8; 16];
    stream.read_exact(&mut salt).await?;
    let cipher = crypto::derive_cipher(&pin, &salt);

    let header_len = stream.read_u32().await? as usize;
    if header_len > MAX_HEADER_LEN {
        bail!("Saiz header melebihi had keselamatan");
    }
    let mut enc_header = vec![0u8; header_len];
    stream.read_exact(&mut enc_header).await?;

    let header_bytes = match crypto::decrypt_chunk(&cipher, 0, &enc_header) {
        Ok(b) => {
            stream.write_u8(1).await?;
            b
        }
        Err(e) => {
            let _ = stream.write_u8(0).await;
            return Err(e);
        }
    };

    let header: FileHeader = serde_json::from_slice(&header_bytes)?;
    let safe_name = Path::new(&header.filename).file_name().context("Nama fail tidak sah")?;
    tokio::fs::create_dir_all("received").await?;
    let out_path = Path::new("received").join(safe_name);

    let pb = make_progress_bar(header.size)?;
    let mut out_file = File::create(&out_path).await?;
    let mut hasher = Sha256::new();
    let mut counter = 1u64;

    loop {
        let enc_len = stream.read_u32().await? as usize;
        if enc_len == 0 {
            break;
        }
        if enc_len > CHUNK_SIZE + 64 {
            bail!("Saiz blok melebihi had keselamatan!");
        }
        let mut enc_buf = vec![0u8; enc_len];
        stream.read_exact(&mut enc_buf).await?;

        let plain = crypto::decrypt_chunk(&cipher, counter, &enc_buf)?;
        out_file.write_all(&plain).await?;
        hasher.update(&plain);
        pb.inc(plain.len() as u64);
        counter += 1;
    }
    out_file.flush().await?;
    pb.finish_with_message("Selesai");

    if hex::encode(hasher.finalize()) != header.sha256 {
        stream.write_u8(0).await?;
        bail!("❌ Hash SHA-256 tidak sepadan! Fail dipadam.");
    }

    stream.write_u8(1).await?;
    println!("✅ Disimpan di {:?} (AES-256-GCM + SHA-256 sah!)", out_path);
    Ok(())
}
