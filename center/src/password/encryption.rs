use chacha20poly1305::{
    XChaCha20Poly1305,
    aead::{Aead, AeadCore, KeyInit, OsRng},
};
use sha2::{Digest, Sha256};

use super::PasswordEncryption;

#[derive(Debug)]
pub(crate) struct BuiltInPasswordEncryption {
    key: Vec<u8>,
}

impl BuiltInPasswordEncryption {
    pub(crate) fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into().into_bytes(),
        }
    }
}

impl PasswordEncryption for BuiltInPasswordEncryption {
    fn seal(&self, plaintext: &str) -> anyhow::Result<String> {
        if self.key.is_empty() {
            anyhow::bail!("password encryption key must not be empty");
        }

        let cipher = XChaCha20Poly1305::new_from_slice(&password_encryption_key(&self.key))?;
        let nonce = XChaCha20Poly1305::generate_nonce(&mut OsRng);
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| anyhow::anyhow!("password encryption failed"))?;
        Ok(format!(
            "pw:v1:{}:{}",
            hex::encode(nonce),
            hex::encode(ciphertext)
        ))
    }

    fn open(&self, ciphertext: &str) -> anyhow::Result<String> {
        if self.key.is_empty() {
            anyhow::bail!("password encryption key must not be empty");
        }

        let payload = ciphertext
            .strip_prefix("pw:v1:")
            .ok_or_else(|| anyhow::anyhow!("unsupported password ciphertext"))?;
        let (nonce, ciphertext) = payload
            .split_once(':')
            .ok_or_else(|| anyhow::anyhow!("invalid password ciphertext"))?;
        let nonce = hex::decode(nonce)?;
        let ciphertext = hex::decode(ciphertext)?;
        let cipher = XChaCha20Poly1305::new_from_slice(&password_encryption_key(&self.key))?;
        let plaintext = cipher
            .decrypt(nonce.as_slice().into(), ciphertext.as_ref())
            .map_err(|_| anyhow::anyhow!("password decryption failed"))?;
        Ok(String::from_utf8(plaintext)?)
    }
}

fn password_encryption_key(password_encryption_key: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"adss:password-encryption:v1");
    hasher.update(password_encryption_key);
    hasher.finalize().into()
}
