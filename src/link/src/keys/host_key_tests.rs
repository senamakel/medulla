//! Unit tests for the host key codec: round trips, the checksum, and the
//! transcription folding a pasted key survives.

use super::*;

fn sample() -> HostKey {
    HostKey {
        client: NodeId([0x11; 16]),
        host: NodeId([0x22; 16]),
        pair_key: PairKey::from_bytes([0x33; 16]),
        port: 60001,
    }
}

#[test]
fn round_trips_through_the_display_form() {
    let key = HostKey::generate(NodeId::generate(), 60001);
    let text = key.encode();
    assert!(text.starts_with("HK1-"), "{text}");
    assert_eq!(text.len(), 4 + 85 + 21, "85 chars in groups of 4: {text}");
    assert_eq!(HostKey::decode(&text).unwrap(), key);
}

#[test]
fn is_stable_for_a_fixed_input() {
    let text = sample().encode();
    assert_eq!(HostKey::decode(&text).unwrap(), sample());
    // Pinned so a layout change cannot slip through as a refactor.
    assert_eq!(
        text,
        "HK1-0H8H-44H2-8J49-24H2-8J49-24H2-8J4H-44H2-8J49-24H2-8J49-24H2-8J4S-6CS6-6CS6-6CS6-6CS6-6CS6-6CS6-6CS6-6CS7-8Y7A-3G"
    );
}

#[test]
fn ignores_case_whitespace_and_separators_and_folds_confusables() {
    let key = sample();
    let text = key.encode();
    let messy: String = text
        .chars()
        .map(|ch| match ch {
            '-' => ' ',
            '0' => 'O',
            '1' => 'l',
            other => other.to_ascii_lowercase(),
        })
        .collect();
    assert_eq!(HostKey::decode(&messy).unwrap(), key);
}

#[test]
fn rejects_a_flipped_character_by_checksum() {
    let text = sample().encode();
    // Flip a character inside the pair key region.
    let mut chars: Vec<char> = text.chars().collect();
    let index = 40;
    chars[index] = if chars[index] == 'A' { 'B' } else { 'A' };
    let flipped: String = chars.into_iter().collect();
    assert_eq!(HostKey::decode(&flipped), Err(HostKeyError::Checksum));
}

#[test]
fn rejects_a_truncated_paste_by_length() {
    let text = sample().encode();
    let short = &text[..text.len() - 6];
    assert!(matches!(
        HostKey::decode(short),
        Err(HostKeyError::Length(_))
    ));
}

#[test]
fn rejects_a_missing_prefix_and_an_invalid_character() {
    let text = sample().encode();
    assert_eq!(
        HostKey::decode(&text[4..]),
        Err(HostKeyError::Prefix),
        "the prefix is what marks this as a host key"
    );
    let bad = text.replacen('8', "U", 1);
    assert_eq!(
        HostKey::decode(&bad),
        Err(HostKeyError::InvalidCharacter('U'))
    );
}

#[test]
fn rejects_an_unknown_version() {
    let key = sample();
    let mut raw = crockford::decode_bits(&key.encode_compact(), PAYLOAD_BITS).unwrap();
    raw[0] = 9;
    let sum = checksum_of(&raw[..BODY_LEN]);
    raw[51..53].copy_from_slice(&sum);
    let text = format!("HK1-{}", crockford::encode_bits(&raw, PAYLOAD_BITS));
    assert_eq!(HostKey::decode(&text), Err(HostKeyError::Version(9)));
}

#[test]
fn looks_like_only_matches_the_prefix() {
    assert!(HostKey::looks_like(" hk1-abcd "));
    assert!(!HostKey::looks_like("--workspace"));
    assert!(!HostKey::looks_like("HK1-"));
}

#[test]
fn debug_redacts_the_pair_key() {
    let printed = format!("{:?}", sample());
    assert!(printed.contains("<redacted>"), "{printed}");
    assert!(!printed.contains("33333333"), "{printed}");
}
