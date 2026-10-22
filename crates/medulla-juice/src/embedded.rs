use crate::{CompressionTokenTotals, EmbeddedCompressionStats};
use serde::Deserialize;
use std::fs::File;
use std::io::{self, ErrorKind, Read};
use std::path::Path;

const MAX_STATS_BYTES: u64 = 1024 * 1024;

#[derive(Deserialize)]
struct SavingsFile {
    total: SavingsTotal,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavingsTotal {
    events: u64,
    original_tokens: u64,
    compacted_tokens: u64,
}

/// Reads OpenHuman's persisted workspace aggregate, if it exists.
///
/// The source contains estimated token totals shared across embedded turns.
/// It has no byte counts or per-session attribution, so neither is inferred.
pub fn read_embedded_stats(workspace: &Path) -> io::Result<Option<EmbeddedCompressionStats>> {
    let path = workspace.join("state").join("tokenjuice_savings.json");
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    file.take(MAX_STATS_BYTES + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_STATS_BYTES {
        return Err(invalid_data("embedded compression stats exceed 1 MiB"));
    }
    let parsed: SavingsFile = serde_json::from_slice(&bytes)
        .map_err(|error| invalid_data(format!("invalid embedded compression stats: {error}")))?;
    if parsed.total.compacted_tokens > parsed.total.original_tokens {
        return Err(invalid_data(
            "compacted token count exceeds original token count",
        ));
    }
    Ok(Some(EmbeddedCompressionStats {
        events: parsed.total.events,
        tokens: CompressionTokenTotals {
            before_tokens: parsed.total.original_tokens,
            after_tokens: parsed.total.compacted_tokens,
        },
    }))
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, message.into())
}
