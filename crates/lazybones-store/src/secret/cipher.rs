//! Authenticated encryption for secrets at rest (AES-256-GCM).
//!
//! The cipher key is derived from a master passphrase (SCOPE: the daemon's
//! `LAZYBONES_SECRET_KEY`) by SHA-256, giving a 32-byte key. Each secret is
//! sealed with a fresh random 96-bit nonce; the stored blob is
//! `base64(nonce ‖ ciphertext ‖ tag)`. Without the master key the stored bytes
//! are opaque — the embedded DB files never hold a plaintext credential.

use aes_gcm::aead::{Aead, OsRng, rand_core::RngCore};
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use sha2::{Digest, Sha256};

use crate::error::StoreError;

/// AES-GCM 96-bit nonce length.
const NONCE_LEN: usize = 12;

/// A cipher bound to one master key — seals and opens secret values.
#[derive(Clone)]
pub(crate) struct Cipher {
    cipher: Aes256Gcm,
}

impl Cipher {
    /// Derive the cipher from a master passphrase via SHA-256.
    pub(crate) fn from_master(master: &str) -> Self {
        let digest = Sha256::digest(master.as_bytes());
        let key = Key::<Aes256Gcm>::from_slice(&digest);
        Self {
            cipher: Aes256Gcm::new(key),
        }
    }

    /// Seal `plaintext` into a base64 `nonce ‖ ciphertext` blob.
    pub(crate) fn seal(&self, plaintext: &str) -> Result<String, StoreError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| StoreError::Secret("encryption failed".into()))?;

        let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
        blob.extend_from_slice(&nonce_bytes);
        blob.extend_from_slice(&ciphertext);
        Ok(B64.encode(blob))
    }

    /// Open a base64 `nonce ‖ ciphertext` blob back to plaintext.
    ///
    /// Fails (`Secret`) if the blob is malformed or the master key is wrong —
    /// GCM's tag makes a wrong key indistinguishable from tampering.
    pub(crate) fn open(&self, blob: &str) -> Result<String, StoreError> {
        let bytes = B64
            .decode(blob.as_bytes())
            .map_err(|_| StoreError::Secret("corrupt secret blob".into()))?;
        if bytes.len() <= NONCE_LEN {
            return Err(StoreError::Secret("truncated secret blob".into()));
        }
        let (nonce_bytes, ciphertext) = bytes.split_at(NONCE_LEN);
        let nonce = Nonce::from_slice(nonce_bytes);

        let plaintext = self
            .cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| StoreError::Secret("decryption failed (wrong key?)".into()))?;
        String::from_utf8(plaintext)
            .map_err(|_| StoreError::Secret("decrypted secret is not utf-8".into()))
    }
}
