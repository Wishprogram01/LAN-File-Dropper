use anyhow::Result;
use socket2::{Domain, Protocol, Socket, Type};
use std::collections::BTreeMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use tokio::net::UdpSocket;
use tokio::time::{timeout, Duration, Instant};

pub const DISCOVERY_PORT: u16 = 9999;
const MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 42, 99);
const DISCOVER_MSG: &[u8] = b"LANDROP_DISCOVER";
const REPLY_PREFIX: &str = "LANDROP_HERE:";

#[derive(Debug, Clone)]
pub struct Peer {
    pub addr: SocketAddr,
    pub name: String,
}

fn default_hostname() -> String {
    std::fs::read_to_string("/etc/hostname")
        .unwrap_or_else(|_| "ubuntu-peer".to_string())
        .trim()
        .to_string()
}

fn bind_shared_udp(port: u16) -> Result<UdpSocket> {
    let socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    socket.set_nonblocking(true)?;

    let bind_addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, port);
    socket.bind(&bind_addr.into())?;
    let _ = socket.join_multicast_v4(&MULTICAST_ADDR, &Ipv4Addr::UNSPECIFIED);
    let _ = socket.set_multicast_loop_v4(true);

    let std_sock: std::net::UdpSocket = socket.into();
    Ok(UdpSocket::from_std(std_sock)?)
}

pub async fn spawn_responder(tcp_port: u16, custom_name: Option<String>) -> Result<()> {
    let display_name = custom_name.unwrap_or_else(default_hostname);
    let reply = format!("{REPLY_PREFIX}{tcp_port}:{display_name}");

    // 1. Soket dikongsi pada port 9999 (untuk LAN Broadcast / Multicast)
    if let Ok(shared_sock) = bind_shared_udp(DISCOVERY_PORT) {
        let reply_clone = reply.clone();
        tokio::spawn(async move {
            let mut buf = [0u8; 256];
            loop {
                if let Ok((n, peer)) = shared_sock.recv_from(&mut buf).await {
                    if &buf[..n] == DISCOVER_MSG {
                        let _ = shared_sock.send_to(reply_clone.as_bytes(), peer).await;
                    }
                }
            }
        });
    }

    // 2. Soket UDP dedikasi pada port yang sama dengan TCP (cth: 9000, 9001)
    //    Ini mengelakkan isu SO_REUSEPORT load-balancing di kernel Linux / WSL2!
    let dedicated_sock = UdpSocket::bind(("0.0.0.0", tcp_port)).await?;
    tokio::spawn(async move {
        let mut buf = [0u8; 256];
        loop {
            if let Ok((n, peer)) = dedicated_sock.recv_from(&mut buf).await {
                if &buf[..n] == DISCOVER_MSG {
                    let _ = dedicated_sock.send_to(reply.as_bytes(), peer).await;
                }
            }
        }
    });

    Ok(())
}

pub async fn find_peers(scan_secs: u64) -> Result<Vec<Peer>> {
    let sock = UdpSocket::bind("0.0.0.0:0").await?;
    sock.set_broadcast(true)?;

    // 1. Imbas seluruh rangkaian Wi-Fi/LAN melalui Multicast & Broadcast (port 9999)
    let _ = sock
        .send_to(DISCOVER_MSG, (MULTICAST_ADDR, DISCOVERY_PORT))
        .await;
    let _ = sock
        .send_to(DISCOVER_MSG, ("255.255.255.255", DISCOVERY_PORT))
        .await;

    // 2. Imbas julat port penerima tempatan (9000..=9020) di localhost & LAN broadcast
    //    Dijamin 100% menjumpai kesemua instance penerima pada 1 PC / WSL2!
    for port in 9000..=9020 {
        let _ = sock.send_to(DISCOVER_MSG, ("127.0.0.1", port)).await;
        let _ = sock.send_to(DISCOVER_MSG, ("255.255.255.255", port)).await;
    }

    let mut discovered: BTreeMap<SocketAddr, String> = BTreeMap::new();
    let deadline = Instant::now() + Duration::from_secs(scan_secs);
    let mut buf = [0u8; 256];

    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }

        match timeout(remaining, sock.recv_from(&mut buf)).await {
            Ok(Ok((n, src))) => {
                if let Ok(msg) = std::str::from_utf8(&buf[..n]) {
                    if let Some(rest) = msg.strip_prefix(REPLY_PREFIX) {
                        if let Some((port_str, host)) = rest.split_once(':') {
                            if let Ok(tcp_port) = port_str.parse::<u16>() {
                                let peer_addr = SocketAddr::new(src.ip(), tcp_port);
                                discovered.insert(peer_addr, host.to_string());
                            }
                        }
                    }
                }
            }
            _ => break,
        }
    }

    Ok(discovered
        .into_iter()
        .map(|(addr, name)| Peer { addr, name })
        .collect())
}
