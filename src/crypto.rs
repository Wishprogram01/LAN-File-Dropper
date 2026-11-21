use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};

pub fn derive_cipher(pin: &str, salt: &[u8; 16]) -> Aes256Gcm {
    let mut hasher = Sha256::new();
    hasher.update(salt);
    hasher.update(pin.trim().as_bytes());
    let key = hasher.finalize();
    Aes256Gcm::new_from_slice(&key).expect("Kunci SHA-256 sentiasa 32 bait")
}

fn make_nonce(counter: u64) -> [u8; 12] {
    let mut nonce = [0u8; 12];
    nonce[4..12].copy_from_slice(&counter.to_be_bytes());
    nonce
}

pub fn encrypt_chunk(cipher: &Aes256Gcm, counter: u64, data: &[u8]) -> Result<Vec<u8>> {
    let nonce_bytes = make_nonce(counter);
    cipher
        .encrypt(Nonce::from_slice(&nonce_bytes), data)
        .map_err(|_| anyhow!("Gagal menyulitkan blok data"))
}

pub fn decrypt_chunk(cipher: &Aes256Gcm, counter: u64, data: &[u8]) -> Result<Vec<u8>> {
    let nonce_bytes = make_nonce(counter);
    cipher
        .decrypt(Nonce::from_slice(&nonce_bytes), data)
        .map_err(|_| anyhow!("PIN salah atau data telah diubah suai!"))
}
