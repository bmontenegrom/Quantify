use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand_core::OsRng;
use sha2::{Digest, Sha256};

/// Resultado de verificar una contraseña contra su hash almacenado.
pub(crate) enum VerifyResult {
    /// Contraseña incorrecta.
    Invalid,
    /// Contraseña correcta; el hash ya está en formato Argon2.
    Valid,
    /// Contraseña correcta; el hash estaba en el formato SHA-256 legacy y debe actualizarse.
    ValidNeedsRehash(String),
}

/// Genera un hash Argon2id de la contraseña con salt aleatorio.
pub(crate) fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        // inalcanzable: params default + SaltString válido nunca fallan
        .expect("argon2 hash con params default y salt OsRng nunca falla")
        .to_string()
}

/// Verifica una contraseña contra su hash almacenado.
///
/// Soporta el formato legacy SHA-256 (`salt:hex`) y el nuevo formato Argon2id (`$argon2id$…`).
/// Si el hash es legacy y la contraseña es correcta, devuelve `ValidNeedsRehash` con el nuevo
/// hash para que el llamador pueda migrar el registro transparentemente.
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
        // Formato legacy SHA-256: `salt:hex`
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

/// Calcula el digest SHA-256 hexadecimal de `salt:password` (solo para verificar hashes legacy).
pub(crate) fn digest_password(salt: &str, password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(password.as_bytes());
    format!("{:x}", hasher.finalize())
}
