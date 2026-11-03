use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand_core::OsRng;
use sha2::{Digest, Sha256};

pub(crate) enum VerifyResult {
    Invalid,
    Valid,
    ValidNeedsRehash(String),
}

pub(crate) fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("argon2 hash con params default y salt OsRng nunca falla")
        .to_string()
}

pub(crate) fn verify_password(password: &str, stored_hash: &str) -> VerifyResult {
    if stored_hash.starts_with("$argon2") {
        let Ok(parsed) = PasswordHash::new(stored_hash) else {
            return VerifyResult::Invalid;
        };
        if Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
        {
            VerifyResult::Valid
        } else {
            VerifyResult::Invalid
        }
    } else {
        let Some((salt, expected)) = stored_hash.split_once(':') else {
            return VerifyResult::Invalid;
        };
        if digest_password(salt, password) == expected {
            VerifyResult::ValidNeedsRehash(hash_password(password))
        } else {
            VerifyResult::Invalid
        }
    }
}

pub(crate) fn digest_password(salt: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}
