use argon2::{
    Algorithm, Argon2, Params, PasswordHash, Version,
    password_hash::{
        PasswordHasher, PasswordVerifier as Argon2PasswordVerifier, SaltString, rand_core::OsRng,
    },
};
use sha2::{Digest, Sha256};

use crate::auth::constant_time_eq;

use super::PasswordHashProvider;

#[derive(Debug)]
pub(super) struct Argon2idPasswordHash;

impl PasswordHashProvider for Argon2idPasswordHash {
    fn hash(&self, password: &str) -> anyhow::Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        Ok(argon2id()
            .hash_password(password.as_bytes(), &salt)?
            .to_string())
    }

    fn verify(&self, password: &str, verifier: &str) -> bool {
        let Ok(parsed_hash) = PasswordHash::new(verifier) else {
            return false;
        };

        argon2id()
            .verify_password(password.as_bytes(), &parsed_hash)
            .is_ok()
    }
}

#[derive(Debug)]
pub(crate) struct DeterministicPasswordHash;

impl PasswordHashProvider for DeterministicPasswordHash {
    fn hash(&self, password: &str) -> anyhow::Result<String> {
        Ok(deterministic_password_verifier(password))
    }

    fn verify(&self, password: &str, verifier: &str) -> bool {
        constant_time_eq(
            deterministic_password_verifier(password).as_bytes(),
            verifier.as_bytes(),
        )
    }
}

fn argon2id() -> Argon2<'static> {
    Argon2::new(Algorithm::Argon2id, Version::V0x13, Params::default())
}

fn deterministic_password_verifier(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"adss:test-password-verifier:v1");
    hasher.update(password.as_bytes());
    format!("test-verifier:v1:{}", hex::encode(hasher.finalize()))
}
