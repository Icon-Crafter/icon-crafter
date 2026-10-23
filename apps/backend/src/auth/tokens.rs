//! Opaque session tokens and their hashes.
//!
//! A session token is 32 bytes from the OS CSPRNG, shown to the client once as
//! 64 lowercase hex characters. Only the SHA-256 digest is stored in the
//! database, so a database leak does not hand out live sessions.

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// Number of random bytes in a session token (256 bits).
pub const TOKEN_BYTES: usize = 32;
/// Length of the hex encoded token.
pub const TOKEN_HEX_LEN: usize = TOKEN_BYTES * 2;

/// Reasons token generation can fail.
#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    /// The operating system CSPRNG failed.
    #[error("secure random number generator failed: {0}")]
    Entropy(#[from] getrandom::Error),
}

/// Generate a fresh session token, hex encoded.
pub fn generate_token() -> Result<String, TokenError> {
    let mut bytes = [0_u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes)?;
    Ok(hex::encode(bytes))
}

/// SHA-256 digest of a token, as stored in the database.
pub fn token_hash(token: &str) -> [u8; 32] {
    let digest = Sha256::digest(token.as_bytes());
    let mut hash = [0_u8; 32];
    hash.copy_from_slice(&digest);
    hash
}

/// Digest of a CSRF token (same construction as session tokens).
pub fn csrf_hash(token: &str) -> [u8; 32] {
    token_hash(token)
}

/// Constant-time comparison for two byte strings of arbitrary length.
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.ct_eq(right).into()
}

/// Constant-time comparison for two secrets given as strings.
pub fn constant_time_eq_str(left: &str, right: &str) -> bool {
    constant_time_eq(left.as_bytes(), right.as_bytes())
}

/// Validate the shape of a client supplied token before hashing it.
pub fn is_valid_token_shape(token: &str) -> bool {
    token.len() == TOKEN_HEX_LEN
        && token
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_random_and_well_formed() {
        let first = generate_token().expect("token");
        let second = generate_token().expect("token");
        assert_eq!(first.len(), TOKEN_HEX_LEN);
        assert_ne!(first, second);
        assert!(is_valid_token_shape(&first));
        assert_eq!(first, first.to_ascii_lowercase());
    }

    #[test]
    fn hashes_are_stable_and_32_bytes() {
        let token = generate_token().expect("token");
        let first = token_hash(&token);
        let second = token_hash(&token);
        assert_eq!(first, second);
        assert_eq!(first.len(), 32);
        assert_ne!(token_hash("a"), token_hash("b"));
    }

    #[test]
    fn constant_time_equality() {
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
        assert!(!constant_time_eq(b"abc", b"abcd"));
        assert!(constant_time_eq_str("token", "token"));
        assert!(!constant_time_eq_str("token", "toke"));
    }

    #[test]
    fn token_shape_validation() {
        assert!(is_valid_token_shape(&"a".repeat(64)));
        assert!(!is_valid_token_shape(&"a".repeat(63)));
        assert!(!is_valid_token_shape(&"A".repeat(64)));
        assert!(!is_valid_token_shape(&"g".repeat(64)));
        assert!(!is_valid_token_shape(""));
    }
}
