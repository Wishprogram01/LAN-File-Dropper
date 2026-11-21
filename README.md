# 🚀 lan-drop

[![CI & Release](https://github.com/<username-anda>/lan-drop/actions/workflows/release.yml/badge.svg)](https://github.com/<username-anda>/lan-drop/actions)
![Rust](https://img.shields.io/badge/Language-Rust_2021-orange?logo=rust)
![Security](https://img.shields.io/badge/Encryption-AES--256--GCM-success)
![License](https://img.shields.io/badge/License-MIT-blue)

**`lan-drop`** is a fast, zero-configuration, encrypted Peer-to-Peer (P2P) file transfer CLI for Local Area Networks (LAN), built in **Rust** with asynchronous I/O powered by **Tokio**.

No cloud servers, no file size limits, and no manual IP lookups required.

---

## ✨ Key Features

- 🔍 **Zero-Config UDP Auto-Discovery:** Automatically locates active receivers on the same Wi-Fi/LAN subnet via UDP broadcast (`port 9999`) without colliding with Linux `avahi-daemon` (`port 5353`).
- 🔐 **End-to-End Authenticated Encryption:** Every file header and data chunk is encrypted using **AES-256-GCM** derived from a 4-digit session PIN and a random 128-bit cryptographic salt.
- ⚡ **Constant-Memory Async Streaming:** Streams files in `64 KB` chunks over TCP (`tokio`), allowing multi-gigabyte transfers with `< 10 MB` RAM footprint.
- 🛡️ **End-to-End Integrity & Path Sanitization:** Verifies full-file **SHA-256** checksums on completion and strips directory traversal sequences (`../`) automatically.
- 📊 **Live Terminal Telemetry:** Real-time transfer speed (`MB/s`), progress bar, and ETA powered by `indicatif`.

---

## 🏗️ System Architecture & Protocol Flow

```mermaid
sequenceDiagram
    autonumber
    participant S as 💻 Sender CLI
    participant U as 📡 UDP Broadcast (9999)
    participant R as 🖥️ Receiver CLI (TCP 9000)

    Note over R: Generates 4-digit PIN & starts UDP Responder
    S->>U: Broadcast "LANDROP_DISCOVER"
    U-->>R: Packet received on LAN
    R-->>S: Reply "LANDROP_HERE:<tcp_port>:<hostname>"
    
    Note over S,R: Key Derivation: SHA-256(16-byte Random Salt + 4-digit PIN)
    S->>R: TCP Connect + Send 16-byte Salt + AES-256-GCM Encrypted Header
    R-->>S: Verify Auth Tag -> Send ACK (0x01)
    
    loop 64 KB Chunked Stream (Counter Nonce 1..N)
        S->>R: [4-byte Length] + [AES-256-GCM Encrypted Chunk]
    end
    
    S->>R: Send EOF Marker (0x00000000)
    Note over R: Recomputes SHA-256 Checksum
    R-->>S: Integrity Verified ACK (0x01)
```

---

## 📦 Installation

### Build from Source (Cargo)
```bash
git clone https://github.com/<username-anda>/lan-drop.git
cd lan-drop
cargo build --release
sudo cp target/release/lan-drop /usr/local/bin/
```

---

## 🚀 Quick Start Usage

### 1. Start Receiver (Device A)
```bash
lan-drop receive
# Output:
# 📡 Menunggu kiriman fail di port TCP 9000 (UDP Auto-Discovery aktif)
# 🔐 Kod PIN Keselamatan: 4829
```

### 2. Send a File Automatically (Device B)
```bash
lan-drop send --file archive.tar.gz
# Output:
# 🔍 Mencari peranti penerima dalam rangkaian LAN...
# ✅ Ditemui 'ubuntu-peer' di 192.168.1.42:9000
# 🔑 Masukkan PIN 4-digit daripada skrin penerima: 4829
```

### 3. Direct IP Mode (Optional Bypass for Restricted Subnets)
```bash
lan-drop send --file archive.tar.gz --addr 192.168.1.42:9000 --pin 4829
```

---

## 🐧 Linux Firewall Configuration (`ufw`)

If you are transferring across two physical machines on Ubuntu/Debian with `ufw` enabled, allow the discovery and transfer ports:

```bash
sudo ufw allow 9999/udp
sudo ufw allow 9000/tcp
```

---

## 📄 License

Distributed under the MIT License.
