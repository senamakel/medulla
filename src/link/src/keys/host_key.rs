//! The host key: everything one machine needs to be paired with this device,
//! bundled into a single string the operator pastes once.
//!
//! Pairing a remote host the way mosh pairs a session means both ends must
//! agree on four things before the first datagram — who dials whom, under which
//! key, and where. All four are minted on the client (the TUI) and carried to
//! the host inside the key, so the host needs no bootstrap channel back:
//!
//! ```text
//! raw      = 0x01 ‖ client node id (16) ‖ host node id (16) ‖ pair key (16) ‖ udp port (2, BE)
//!            ‖ checksum (2) = SHA-256(raw[0..51])[0..2]                       (53 bytes)
//! encoding = Crockford base32 of 424 bits                                     (85 chars)
//! display  = "HK1-" + groups of 4, hyphen-separated
//! ```
//!
//! The key is pasted, not typed — it lands on the clipboard when it is minted —
//! so its length is not the constraint the pair key's is. It still uses the
//! pair key's alphabet and folding so a copy that went through a chat window or
//! a terminal that changed case survives intact, and the checksum turns a
//! truncated paste into an error at entry rather than a link that never comes
//! up.
//!
//! The pair key rides in argv, which the protocol document forbade for the
//! typed pair key. The trade is deliberate: a one-shot command that works on
//! any box is the whole point of pairing this way, the host can re-pair with a
//! fresh key at any time, and the exposure window is one process start on a
//! machine the operator already controls.

use std::fmt;

use sha2::{Digest, Sha256};

use super::crockford::{self, CrockfordError};
use super::{NodeId, PairKey};

/// The display prefix, so a key is recognisable on the command line.
pub const PREFIX: &str = "HK1";

/// Wire version byte; bumps when the layout changes.
const VERSION: u8 = 1;

/// Bytes before the checksum.
const BODY_LEN: usize = 1 + 16 + 16 + 16 + 2;

/// Bytes including the checksum.
const RAW_LEN: usize = BODY_LEN + 2;

/// Payload width in bits.
const PAYLOAD_BITS: usize = RAW_LEN * 8;

/// Why a pasted host key was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostKeyError {
    /// The `HK1-` prefix is missing.
    #[error("host key must start with {PREFIX}-")]
    Prefix,
    /// Wrong number of significant characters — usually a truncated paste.
    #[error("host key has {0} characters after the prefix; expected 85")]
    Length(usize),
    /// A character outside the Crockford alphabet, even after folding.
    #[error("host key contains an invalid character: {0:?}")]
    InvalidCharacter(char),
    /// The checksum did not match.
    #[error("host key checksum does not match; the paste may be incomplete")]
    Checksum,
    /// A key from a newer or older layout.
    #[error("host key version {0} is not supported by this build")]
    Version(u8),
}

impl From<CrockfordError> for HostKeyError {
    fn from(error: CrockfordError) -> Self {
        match error {
            CrockfordError::Length(got) => HostKeyError::Length(got),
            CrockfordError::InvalidCharacter(ch) => HostKeyError::InvalidCharacter(ch),
        }
    }
}

/// One pairing, as minted on the client.
///
/// `Debug` is redacted because the pair key is inside.
#[derive(Clone, PartialEq, Eq)]
pub struct HostKey {
    /// The node id of the machine that will dial in — the TUI.
    pub client: NodeId,
    /// The node id the host will serve under.
    pub host: NodeId,
    /// The shared AEAD secret.
    pub pair_key: PairKey,
    /// The UDP port the host listens on, and the client dials.
    pub port: u16,
}

impl HostKey {
    /// Mint a pairing for `client`: a fresh host node id and pair key, listening
    /// on `port`.
    pub fn generate(client: NodeId, port: u16) -> Self {
        HostKey {
            client,
            host: NodeId::generate(),
            pair_key: PairKey::generate(),
            port,
        }
    }

    /// Whether `text` is shaped like a host key — enough to route a bare
    /// argument to the pairing command without decoding it.
    pub fn looks_like(text: &str) -> bool {
        let trimmed = text.trim();
        trimmed.len() > PREFIX.len() + 1
            && trimmed
                .get(..PREFIX.len() + 1)
                .is_some_and(|head| head.eq_ignore_ascii_case(&format!("{PREFIX}-")))
    }

    /// Encode for display and the clipboard: `HK1-` then hyphenated groups.
    pub fn encode(&self) -> String {
        format!("{PREFIX}-{}", crockford::group(&self.encode_compact()))
    }

    /// The bare Crockford payload, no prefix and no separators.
    pub fn encode_compact(&self) -> String {
        let mut raw = [0u8; RAW_LEN];
        raw[0] = VERSION;
        raw[1..17].copy_from_slice(&self.client.0);
        raw[17..33].copy_from_slice(&self.host.0);
        raw[33..49].copy_from_slice(self.pair_key.as_bytes());
        raw[49..51].copy_from_slice(&self.port.to_be_bytes());
        raw[51..53].copy_from_slice(&checksum_of(&raw[..BODY_LEN]));
        crockford::encode_bits(&raw, PAYLOAD_BITS)
    }

    /// Decode a pasted key.
    ///
    /// Case, whitespace and hyphens are ignored and confusables are folded.
    ///
    /// # Errors
    ///
    /// When the prefix is missing, the length is wrong, a character cannot be
    /// folded, the checksum fails, or the version is unknown.
    pub fn decode(text: &str) -> Result<Self, HostKeyError> {
        let trimmed = text.trim();
        if !Self::looks_like(trimmed) {
            return Err(HostKeyError::Prefix);
        }
        let payload = &trimmed[PREFIX.len() + 1..];
        let raw = crockford::decode_bits(payload, PAYLOAD_BITS)?;
        let mut expected = [0u8; 2];
        expected.copy_from_slice(&raw[BODY_LEN..RAW_LEN]);
        if checksum_of(&raw[..BODY_LEN]) != expected {
            return Err(HostKeyError::Checksum);
        }
        if raw[0] != VERSION {
            return Err(HostKeyError::Version(raw[0]));
        }
        let mut client = [0u8; 16];
        client.copy_from_slice(&raw[1..17]);
        let mut host = [0u8; 16];
        host.copy_from_slice(&raw[17..33]);
        let mut pair = [0u8; 16];
        pair.copy_from_slice(&raw[33..49]);
        Ok(HostKey {
            client: NodeId(client),
            host: NodeId(host),
            pair_key: PairKey::from_bytes(pair),
            port: u16::from_be_bytes([raw[49], raw[50]]),
        })
    }
}

impl fmt::Debug for HostKey {
    /// Never prints the pair key.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostKey")
            .field("client", &self.client)
            .field("host", &self.host)
            .field("pair_key", &"<redacted>")
            .field("port", &self.port)
            .finish()
    }
}

/// The first two bytes of `SHA-256(body)`.
fn checksum_of(body: &[u8]) -> [u8; 2] {
    let digest = Sha256::digest(body);
    [digest[0], digest[1]]
}

#[cfg(test)]
#[path = "host_key_tests.rs"]
mod tests;
