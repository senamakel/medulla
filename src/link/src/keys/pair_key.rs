//! The pair key and its human-transcribable encoding (protocol §7.1).
//!
//! The pair key is the 128-bit end-to-end secret. It is generated on the
//! owner, read aloud or carried to the host, and typed in there — so its
//! encoding is optimised for a human retyping it, not for compactness:
//!
//! ```text
//! payload   = 128-bit key ‖ 12-bit checksum        (140 bits)
//! checksum  = SHA-256(key)[0..12 bits]
//! encoding  = Crockford base32, no padding          (28 chars)
//! display   = 7 groups of 4, hyphen-separated
//! ```
//!
//! Crockford base32 excludes `I`, `L`, `O` and `U`, and [`PairKey::decode`]
//! folds the confusable characters, so `0`/`O` and `1`/`I`/`L` typos resolve
//! rather than fail. The checksum catches everything else at entry — where the
//! user can see what they mistyped — instead of surfacing later as an
//! unexplained decrypt failure.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};

use super::crockford::{self, CrockfordError};

/// Encoded length: 140 bits / 5 bits per character.
const ENCODED_CHARS: usize = 28;

/// Payload width in bits: the key and its checksum.
const PAYLOAD_BITS: usize = 140;

/// Why a typed pair key was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairKeyError {
    /// Not 28 significant characters once separators are stripped.
    #[error("pair key must be {ENCODED_CHARS} characters, got {0}")]
    Length(usize),
    /// A character that is not in the Crockford alphabet, even after folding.
    #[error("pair key contains an invalid character: {0:?}")]
    InvalidCharacter(char),
    /// The 12-bit checksum did not match — almost certainly a typo.
    #[error("pair key checksum does not match; check for a mistyped character")]
    Checksum,
}

impl From<CrockfordError> for PairKeyError {
    fn from(error: CrockfordError) -> Self {
        match error {
            CrockfordError::Length(got) => PairKeyError::Length(got),
            CrockfordError::InvalidCharacter(ch) => PairKeyError::InvalidCharacter(ch),
        }
    }
}

/// The 128-bit end-to-end key shared by exactly two endpoints.
///
/// `Debug` is redacted and there is no `Display`: printing a pair key is a
/// deliberate act that goes through [`PairKey::encode`].
#[derive(Clone, PartialEq, Eq)]
pub struct PairKey([u8; 16]);

impl PairKey {
    /// Mint a fresh random pair key. This happens on the owner, once per
    /// host, and the result is shown to the user (protocol §7.1).
    pub fn generate() -> Self {
        let mut bytes = [0u8; 16];
        rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut bytes);
        PairKey(bytes)
    }

    /// Wrap raw key bytes.
    pub fn from_bytes(bytes: [u8; 16]) -> Self {
        PairKey(bytes)
    }

    /// The raw key, for the AEAD.
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Encode for display: 7 hyphen-separated groups of 4 Crockford characters,
    /// checksum included.
    pub fn encode(&self) -> String {
        crockford::group(&self.encode_compact())
    }

    /// Encode without the display grouping — 28 characters, no separators.
    pub fn encode_compact(&self) -> String {
        // 140 bits, big-endian: the key then the checksum's low 12 bits, left
        // aligned so the whole payload is exactly 28 five-bit characters.
        let checksum = checksum_of(&self.0);
        let mut payload = [0u8; 18];
        payload[..16].copy_from_slice(&self.0);
        payload[16] = (checksum >> 4) as u8;
        payload[17] = ((checksum & 0x0f) << 4) as u8;
        crockford::encode_bits(&payload, PAYLOAD_BITS)
    }

    /// Decode a key a human typed.
    ///
    /// Hyphens, spaces and case are ignored; `O` folds to `0` and `I`/`L` fold
    /// to `1`. A wrong length, an unfoldable character, or a checksum mismatch
    /// is an error here rather than an opaque decrypt failure later.
    pub fn decode(text: &str) -> Result<Self, PairKeyError> {
        let payload = crockford::decode_bits(text, PAYLOAD_BITS)?;
        let mut key = [0u8; 16];
        key.copy_from_slice(&payload[..16]);
        let found = (u16::from(payload[16]) << 4) | u16::from(payload[17] >> 4);
        if found != checksum_of(&key) {
            return Err(PairKeyError::Checksum);
        }
        Ok(PairKey(key))
    }
}

impl fmt::Debug for PairKey {
    /// Never prints the key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairKey(<redacted>)")
    }
}

impl Serialize for PairKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.encode())
    }
}

impl<'de> Deserialize<'de> for PairKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        PairKey::decode(&text).map_err(D::Error::custom)
    }
}

/// The top 12 bits of `SHA-256(key)`, right-aligned in a `u16`.
fn checksum_of(key: &[u8; 16]) -> u16 {
    let digest = Sha256::digest(key);
    (u16::from(digest[0]) << 4) | u16::from(digest[1] >> 4)
}
