//! Password hashing (Argon2id).
//!
//! Hashes are PHC strings that embed the per-password salt and the Argon2
//! parameters, so parameters can be tuned later without breaking existing users.

use argon2::password_hash::{PasswordHasher, PasswordVerifier, phc};
use argon2::Argon2;

/// Hash a plaintext password with Argon2id and a fresh random salt.
pub fn hash_password(password: &str) -> Result<String, String> {
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|error| format!("failed to hash password: {error}"))?;
    Ok(hash.to_string())
}

/// Verify a plaintext password against a stored PHC hash.
///
/// Returns `false` for malformed hashes and mismatches; never panics.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    let Ok(parsed) = phc::PasswordHash::new(stored_hash) else {
        return false;
    };
    matches!(
        Argon2::default().verify_password(password.as_bytes(), &parsed),
        Ok(())
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_round_trip() {
        let hash = hash_password("correct horse battery staple").expect("hash");
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn hashes_are_salted_per_password() {
        let first = hash_password("same password").expect("hash");
        let second = hash_password("same password").expect("hash");
        assert_ne!(first, second);
        assert!(verify_password("same password", &first));
        assert!(verify_password("same password", &second));
    }

    #[test]
    fn malformed_hashes_are_rejected() {
        assert!(!verify_password("password", "not-a-phc-string"));
        assert!(!verify_password("password", ""));
    }

    #[test]
    fn unicode_passwords_work() {
        let hash = hash_password("pässwörd-日本語").expect("hash");
        assert!(verify_password("pässwörd-日本語", &hash));
        assert!(!verify_password("passwörd-日本語", &hash));
    }
}
