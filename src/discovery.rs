use anyhow::{Context, Result};
use std::net::SocketAddr;
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration};

pub const DISCOVERY_PORT: u16 = 9999;
const DISCOVER_MSG: &[u8] = b"LANDROP_DISCOVER";
const REPLY_PREFIX: &str = "LANDROP_HERE:";

fn local_hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "ubuntu-peer".to_string())
        .trim()
        .to_string()
}

pub async fn spawn_responder(tcp_port: u16) -> Result<()> {
    let sock = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT))
        .await
        .context("Gagal ikat port UDP 9999 (pastikan tiada instance receive lain sedang jalan)")?;
    let host = local_hostname();

    tokio::spawn(async move {
        let mut buf = [0u8; 256];
        loop {
            if let Ok((n, peer)) = sock.recv_from(&mut buf).await {
                if &buf[..n] == DISCOVER_MSG {
                    let reply = format!("{REPLY_PREFIX}{tcp_port}:{host}");
                    let _ = sock.send_to(reply.as_bytes(), peer).await;
                }
            }
        }
    });
    Ok(())
}

pub async fn find_peer() -> Result<(SocketAddr, String)> {
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.set_broadcast(true)?;

    // Siar ke seluruh Wi-Fi LAN & localhost (untuk ujian pantas dalam 1 Ubuntu)
    let _ = sock.send_to(DISCOVER_MSG, ("255.255.255.255", DISCOVERY_PORT)).await;
    let _ = sock.send_to(DISCOVER_MSG, ("127.0.0.1", DISCOVERY_PORT)).await;

    let mut buf = [0u8; 256];
    let (n, src) = timeout(Duration::from_secs(3), sock.recv_from(&mut buf))
        .await
        .context("Tiada peranti penerima ditemui dalam 3 saat (atau guna --addr)")??;

    let msg = std::str::from_utf8(&buf[..n])?;
    if let Some(rest) = msg.strip_prefix(REPLY_PREFIX) {
        if let Some((port_str, host)) = rest.split_once(':') {
            let tcp_port: u16 = port_str.parse()?;
            return Ok((SocketAddr::new(src.ip(), tcp_port), host.to_string()));
        }
    }
    anyhow::bail!("Respons discovery tidak sah")
}
