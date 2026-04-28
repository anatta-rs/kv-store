//! Content-addressed hash wrapper for SHA-256.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// SHA-256 digest wrapping `[u8; 32]`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    /// Compute the SHA-256 hash of the given bytes.
    ///
    /// Returns a new `ContentHash` wrapping the 32-byte SHA-256 digest.
    pub fn from_bytes(data: &[u8]) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(data);
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Self(hash)
    }

    /// Return the hexadecimal string representation (lowercase 64 characters).
    pub fn as_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Return the hexadecimal string representation (alias for `as_hex()`).
    pub fn hex(&self) -> String {
        self.as_hex()
    }

    /// Access the raw 32-byte SHA-256 digest array.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<[u8; 32]> for ContentHash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

/// Parse a `ContentHash` from a hex-encoded string.
///
/// Expects exactly 64 hex characters (256 bits). Returns an error if the input
/// is invalid hex or has the wrong length.
impl TryFrom<&str> for ContentHash {
    type Error = String;

    fn try_from(hex_str: &str) -> Result<Self, Self::Error> {
        let bytes = hex::decode(hex_str).map_err(|e| format!("invalid hex string: {e}"))?;
        if bytes.len() != 32 {
            return Err(format!("expected 32 bytes, got {}", bytes.len()));
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&bytes);
        Ok(Self(hash))
    }
}

impl std::fmt::Display for ContentHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.hex())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_bytes() {
        let data = b"hello world";
        let hash = ContentHash::from_bytes(data);
        assert_eq!(
            hash.hex(),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_try_from_hex() {
        let hex = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        let hash = ContentHash::try_from(hex).unwrap();
        assert_eq!(hash.hex(), hex);
    }

    #[test]
    fn test_invalid_hex() {
        let result = ContentHash::try_from("not_hex");
        assert!(result.is_err());
    }

    #[test]
    fn test_wrong_length() {
        let result = ContentHash::try_from("abcd");
        assert!(result.is_err());
    }

    #[test]
    fn test_display() {
        let hash = ContentHash::from_bytes(b"test");
        assert_eq!(hash.to_string(), hash.hex());
    }
}
