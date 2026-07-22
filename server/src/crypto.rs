//! AES-256-GCM encryption for credentials stored in the database. The 32-byte
//! key comes from `[secrets] encryption_key` in config.toml (base64 or hex).
//! Ciphertext is `base64(nonce ‖ ct)`.

use std::sync::OnceLock;

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;

static KEY: OnceLock<[u8; 32]> = OnceLock::new();

pub fn set_key(key: [u8; 32]) {
    let _ = KEY.set(key);
}

pub fn has_key() -> bool {
    KEY.get().is_some()
}

fn key() -> &'static [u8; 32] {
    KEY.get().expect("encryption key not loaded (set [secrets] encryption_key)")
}

/// Load the key from a config string. Accepts base64 (44 chars incl. padding)
/// or hex (64 chars) encoding of 32 bytes. Generate one with
/// `openssl rand -base64 32`.
pub fn load_from_config(encoded: &str) -> anyhow::Result<()> {
    let s = encoded.trim();
    // Try base64 then hex, accepting whichever yields exactly 32 bytes.
    let bytes = B64
        .decode(s)
        .ok()
        .filter(|b| b.len() == 32)
        .or_else(|| hex_decode(s).ok().filter(|b| b.len() == 32))
        .ok_or_else(|| anyhow::anyhow!("encryption_key must be base64 or hex for 32 bytes"))?;
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&bytes);
    set_key(arr);
    Ok(())
}

fn hex_decode(s: &str) -> anyhow::Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        anyhow::bail!("odd hex length");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(Into::into))
        .collect()
}

/// Encrypt a UTF-8 string, returning `base64(nonce ‖ ciphertext)`.
pub fn encrypt(plaintext: &str) -> String {
    use rand::RngCore;
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key()));
    let mut nonce_bytes = [0u8; 12];
    rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher.encrypt(nonce, plaintext.as_bytes()).expect("encrypt");
    let mut out = nonce_bytes.to_vec();
    out.extend_from_slice(&ct);
    B64.encode(out)
}

/// Decrypt a `base64(nonce ‖ ciphertext)` blob produced by [`encrypt`].
pub fn decrypt(blob: &str) -> anyhow::Result<String> {
    let raw = B64.decode(blob.trim())?;
    if raw.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }
    let (nonce_bytes, ct) = raw.split_at(12);
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key()));
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|_| anyhow::anyhow!("decryption failed (wrong key?)"))?;
    Ok(String::from_utf8(pt)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        set_key([7u8; 32]);
        let ct = encrypt("hunter2");
        assert_ne!(ct, "hunter2");
        assert_eq!(decrypt(&ct).unwrap(), "hunter2");
        // Nonce randomization => distinct ciphertexts for the same input.
        assert_ne!(encrypt("x"), encrypt("x"));
    }
}
