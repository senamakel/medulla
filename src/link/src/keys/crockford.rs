//! Crockford base32 for key material a human carries between machines.
//!
//! Shared by the pair key (§7.1) and the host key that bundles it. The codec
//! is bit-oriented rather than byte-oriented because both payloads carry a
//! checksum that is not a whole number of bytes; callers say how many bits
//! they mean and get back exactly that many characters.
//!
//! The alphabet excludes `I`, `L`, `O` and `U`. Decoding folds the confusables
//! (`O`→`0`, `I`/`L`→`1`, any case) so a transcription slip resolves rather
//! than fails; `U` stays invalid because folding it would guess at intent.

/// Crockford's base32 alphabet: no `I`, `L`, `O` or `U`.
pub(super) const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// Characters per hyphen-separated display group.
pub(super) const GROUP: usize = 4;

/// Why a typed Crockford string was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum CrockfordError {
    /// Not the expected number of significant characters once separators are stripped.
    Length(usize),
    /// A character that is not in the alphabet, even after folding.
    InvalidCharacter(char),
}

/// Encode the first `bits` bits of `bytes` (big-endian, most significant bit
/// first) as `ceil(bits / 5)` characters with no separators. Any padding bits
/// needed to reach a multiple of five are zero.
pub(super) fn encode_bits(bytes: &[u8], bits: usize) -> String {
    debug_assert!(bits <= bytes.len() * 8);
    let mut out = String::with_capacity(bits.div_ceil(5));
    let mut chunk = 0u8;
    let mut filled = 0;
    for index in 0..bits {
        let bit = (bytes[index / 8] >> (7 - index % 8)) & 1;
        chunk = (chunk << 1) | bit;
        filled += 1;
        if filled == 5 {
            out.push(ALPHABET[chunk as usize] as char);
            chunk = 0;
            filled = 0;
        }
    }
    if filled > 0 {
        out.push(ALPHABET[(chunk << (5 - filled)) as usize] as char);
    }
    out
}

/// Decode `text` into `bits` bits, packed big-endian into `ceil(bits / 8)`
/// bytes. Hyphens, whitespace and case are ignored; confusables are folded.
///
/// # Errors
///
/// When the significant character count is not `ceil(bits / 5)`, or a
/// character is outside the alphabet even after folding.
pub(super) fn decode_bits(text: &str, bits: usize) -> Result<Vec<u8>, CrockfordError> {
    let expected = bits.div_ceil(5);
    let mut values = Vec::with_capacity(expected);
    for ch in text.chars() {
        if ch == '-' || ch.is_whitespace() {
            continue;
        }
        values.push(decode_char(ch).ok_or(CrockfordError::InvalidCharacter(ch))?);
    }
    if values.len() != expected {
        return Err(CrockfordError::Length(values.len()));
    }
    let mut bytes = vec![0u8; bits.div_ceil(8)];
    for (index, value) in values.iter().enumerate() {
        for offset in 0..5 {
            let bit_index = index * 5 + offset;
            if bit_index >= bits {
                break;
            }
            let bit = (value >> (4 - offset)) & 1;
            bytes[bit_index / 8] |= bit << (7 - bit_index % 8);
        }
    }
    Ok(bytes)
}

/// Insert a hyphen every [`GROUP`] characters.
pub(super) fn group(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + raw.len() / GROUP);
    for (index, ch) in raw.chars().enumerate() {
        if index > 0 && index.is_multiple_of(GROUP) {
            out.push('-');
        }
        out.push(ch);
    }
    out
}

/// Map one typed character to its 5-bit value, folding the confusables
/// Crockford's alphabet deliberately omits.
fn decode_char(ch: char) -> Option<u8> {
    let upper = ch.to_ascii_uppercase();
    let folded = match upper {
        'O' => '0',
        'I' | 'L' => '1',
        other => other,
    };
    ALPHABET
        .iter()
        .position(|candidate| *candidate as char == folded)
        .map(|index| index as u8)
}
