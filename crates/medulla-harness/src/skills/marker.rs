//! Managed marker encoding and recognition.

use sha2::{Digest, Sha256};

/// The marker layout a `rev` is computed for.
///
/// Mixed into the hash so that moving the marker — as release 0.8 moved it from
/// a line above the frontmatter to a comment inside it — changes every `rev`
/// even where the rendered text is byte-identical. Without it an install would
/// find its own matching `rev` on a file laid out the old way, report
/// `unchanged`, and leave the operator with skills no harness can read.
const MARKER_FORMAT: &str = "medulla-skill-marker/2";

/// Wraps rendered content in its managed marker and returns `(file, rev)`.
///
/// The marker is what every write path checks before touching a file, so it is
/// produced in exactly one place. It goes *inside* the frontmatter, as a YAML
/// comment on its first line, because a harness only recognises frontmatter
/// that opens on line 1: with the marker above it, Claude Code read the whole
/// document as body text and showed the marker itself where the description
/// belongs — which is the one field the model matches a request against, so the
/// skill was installed, listed, and untriggerable. YAML drops `#` comments
/// before anything else sees the document, so the marker stays invisible to the
/// harness and legible to us.
///
/// The id is percent-encoded, because the store accepts any id that is a single
/// path component — spaces, newlines, quotes and non-ASCII included — and the
/// marker is a whitespace-separated, single-line field list. An id written raw
/// would either come back as a different id or split the marker across lines,
/// and both make Medulla disown a file it wrote itself.
///
/// Content that does not open with frontmatter falls back to the marker on its
/// own first line. Nothing here generates such content, but a marker that is
/// silently dropped would make the file unrecognisable — and therefore neither
/// updatable nor removable.
fn seal(workflow_id: &str, content: &str) -> (String, String) {
    let rev = format!(
        "{:x}",
        Sha256::digest(format!("{MARKER_FORMAT}\n{content}").as_bytes())
    );
    let id = encode_marker_id(workflow_id);
    let marker = format!("medulla:managed workflow={id} rev={rev}");
    let file = match content.strip_prefix("---\n") {
        Some(rest) => format!("---\n# {marker}\n{rest}"),
        None => format!("<!-- {marker} -->\n{content}"),
    };
    (file, rev)
}

/// The characters a marker id keeps as itself.
///
/// Deliberately narrow — alphanumerics and the three punctuation marks a
/// workflow id normally uses. Everything else, including `%` itself, becomes an
/// escape, so encoding is injective and decoding needs no lookahead rules.
fn is_marker_literal(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.')
}

/// Percent-encodes a workflow id for the marker line.
///
/// Operates on UTF-8 bytes, so a non-ASCII id round-trips exactly rather than
/// being transliterated.
fn encode_marker_id(workflow_id: &str) -> String {
    let mut out = String::with_capacity(workflow_id.len());
    for byte in workflow_id.as_bytes() {
        if is_marker_literal(*byte) {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Decodes a marker id, or `None` when the field is not something
/// [`encode_marker_id`] could have produced.
///
/// Malformed input is rejected rather than guessed at: a half-written escape or
/// a stray literal byte means the line is not a marker we wrote, and treating
/// it as one would let a foreign file be adopted — or overwritten.
fn decode_marker_id(field: &str) -> Option<String> {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => {
                let hex = field.get(index + 1..index + 3)?;
                if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return None;
                }
                out.push(u8::from_str_radix(hex, 16).ok()?);
                index += 3;
            }
            byte if is_marker_literal(byte) => {
                out.push(byte);
                index += 1;
            }
            _ => return None,
        }
    }
    let decoded = String::from_utf8(out).ok()?;
    if decoded.is_empty() {
        return None;
    }
    Some(decoded)
}

/// Reads the marker out of a file's head.
///
/// Two spellings are accepted, and both must stay: the current one is a YAML
/// comment inside the leading frontmatter, and the legacy one is an HTML
/// comment on line 1, above it. A release that stopped recognising the legacy
/// form would disown every file the previous release installed — reporting a
/// collision against its own skill on reinstall, and refusing to remove it on
/// `sync --prune` or `uninstall`.
///
/// Returns the `(workflow id, rev)` the marker names, or `None` when the file
/// is not ours — which the install path treats as "leave it alone", never as
/// "assume it is stale". A marker whose fields are duplicated, unparseable, or
/// missing counts as not ours for the same reason: the only safe reading of a
/// line we cannot fully account for is that someone else wrote it.
///
/// The marker occupies an exact slot, not merely "somewhere inside the
/// frontmatter": [`seal`] always splices it onto the line immediately after
/// the opening `---`, so a well-formed file of ours has it on line 2 and
/// nowhere else. Accepting it anywhere else in the block would let a
/// hand-written skill whose own frontmatter happens to carry a
/// `# medulla:managed` comment — documentation about this feature, or a
/// migration note — be adopted, and later overwritten or pruned, as if we had
/// written it. A `---` that never closes is rejected outright for the same
/// reason: it is not frontmatter any parser would recognise, so nothing
/// inside it can be read as ours either.
///
/// "Exact slot" includes the indentation: the line is matched without being
/// left-trimmed first, because `seal` never indents it and YAML happily accepts
/// an indented comment. Trimming would adopt a hand-written
/// `  # medulla:managed …` on the strength of whitespace we could not have
/// produced. The legacy HTML form keeps its tolerance — files already on disk
/// were written before this rule existed.
pub(crate) fn parse_marker(file: &str) -> Option<(String, String)> {
    let mut lines = file.lines();
    let first = lines.next()?;
    if let Some(rest) = first.trim().strip_prefix("<!-- medulla:managed ") {
        return parse_marker_fields(rest.strip_suffix("-->")?.trim());
    }
    // The opener is held to the same column-zero rule as the marker below it,
    // and for the same reason: an indented `  ---` is not frontmatter any
    // parser opens, so a file starting with one is not a skill of ours however
    // its second line reads.
    if first.trim_end() != "---" {
        return None;
    }
    // Trailing whitespace only: an editor may strip or add it at end of line,
    // but a leading space is a different line from the one we write.
    let rest = lines.next()?.strip_prefix("# medulla:managed ")?;
    // The block still has to close for this to be frontmatter at all — a
    // marker-shaped second line above an unclosed `---` is not one either.
    if !lines.any(|line| line.trim_end() == "---") {
        return None;
    }
    parse_marker_fields(rest.trim())
}

/// Parses the marker's `key=value` field list, whatever comment wrapped it.
fn parse_marker_fields(rest: &str) -> Option<(String, String)> {
    let mut workflow = None;
    let mut rev = None;
    for field in rest.split_whitespace() {
        // Unknown keys are tolerated so a marker written by a later release
        // still identifies its workflow; a field that is not `key=value` at all
        // is not.
        let (key, value) = field.split_once('=')?;
        match key {
            "workflow" if workflow.is_some() => return None,
            "workflow" => workflow = Some(decode_marker_id(value)?),
            "rev" if rev.is_some() => return None,
            "rev" => {
                if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
                    return None;
                }
                rev = Some(value.to_string());
            }
            _ => {}
        }
    }
    Some((workflow?, rev?))
}
