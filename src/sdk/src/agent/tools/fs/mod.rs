//! Filesystem tools: read, write, and list, all rooted at the turn's checkout.
//!
//! Every path argument goes through [`guard::contained`] before it is opened, so
//! a model that asks for `../../etc/passwd` gets a refusal the run can see
//! rather than a file it should not have.
//!
//! # Reads are truncated, and the truncation says so
//!
//! A tool result is model context. Returning a 40 MB file would blow the window
//! and evict the very instructions the turn is following, so a read is capped —
//! and the cap is reported in the returned text rather than silently applied,
//! because a model that cannot tell it received a prefix will reason about the
//! file as though it saw all of it.
//!
//! The cap bounds what is *read*, not just what is returned. Reading the whole
//! file and slicing afterwards would make a model's `read_file` on a multi-
//! gigabyte log an out-of-memory abort of the host process, which is a worse
//! outcome than any answer the tool could have given.

use std::path::PathBuf;
use std::sync::Arc;

use serde_json::{json, Value};
use tinyagents::error::{Result as TaResult, TinyAgentsError};
use tinyagents::harness::tool::{Tool, ToolCall, ToolResult, ToolSchema};
use tokio::io::AsyncReadExt;

use super::guard;

/// Most bytes a single `read_file` returns.
///
/// Roughly 16k tokens of dense source at four bytes per token — large enough for
/// nearly every real file, small enough that one unlucky read cannot evict the
/// turn's own instructions.
const MAX_READ_BYTES: usize = 64 * 1024;

/// Most entries a single `list_dir` returns, for the same reason.
const MAX_ENTRIES: usize = 500;

/// The checkout a turn's tools are rooted at.
///
/// Cloned into each tool rather than read from ambient state: two turns can run
/// concurrently in one process against different checkouts, and a process-global
/// working directory would let one turn's `cd` decide where the other one wrote.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Absolute path to the turn's checkout.
    pub root: PathBuf,
}

/// Pull a required string argument out of a call.
fn arg_str(call: &ToolCall, key: &str) -> TaResult<String> {
    call.arguments
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| TinyAgentsError::Tool(format!("`{key}` is required and must be a string")))
}

/// Read a file from the checkout.
pub struct ReadFileTool {
    workspace: Workspace,
}

impl ReadFileTool {
    /// Root a reader at `workspace`.
    pub fn new(workspace: Workspace) -> Self {
        Self { workspace }
    }
}

#[async_trait::async_trait]
impl Tool<()> for ReadFileTool {
    fn name(&self) -> &str {
        "read_file"
    }

    fn description(&self) -> &str {
        "Read a UTF-8 text file from the workspace. `path` is relative to the workspace root."
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative path to read.",
                    },
                },
                "required": ["path"],
            }),
            format: tinyagents::harness::tool::ToolFormat::default(),
        }
    }

    async fn call(&self, _state: &(), call: ToolCall) -> TaResult<ToolResult> {
        let path = arg_str(&call, "path")?;
        let resolved = guard::contained(&self.workspace.root, &path)
            .map_err(|e| TinyAgentsError::Tool(e.to_string()))?;
        let mut file = tokio::fs::File::open(&resolved)
            .await
            .map_err(|e| TinyAgentsError::Tool(format!("could not read {path}: {e}")))?;
        // The total is taken from metadata rather than by reading to the end,
        // so the truncation notice can name the real size without the read
        // costing it.
        let total = file.metadata().await.map(|m| m.len()).unwrap_or_default();

        // One byte past the cap, so a file sitting exactly on it is not reported
        // as truncated.
        let mut bytes = Vec::new();
        tokio::io::AsyncReadExt::take(&mut file, MAX_READ_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| TinyAgentsError::Tool(format!("could not read {path}: {e}")))?;

        let truncated = bytes.len() > MAX_READ_BYTES;
        bytes.truncate(MAX_READ_BYTES);
        // Lossy rather than a hard error: a file with one stray byte is still
        // worth showing, and refusing it sends the model looking for a problem
        // that is not the one it is solving. Truncating at a byte offset can
        // also split a multi-byte character, which is exactly what lossy
        // decoding is for.
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        if truncated {
            text.push_str(&format!(
                "\n\n[truncated: showing the first {MAX_READ_BYTES} of {total} bytes]"
            ));
        }
        Ok(ToolResult {
            call_id: call.id,
            name: self.name().to_string(),
            content: text,
            raw: None,
            error: None,
            elapsed_ms: 0,
        })
    }
}

/// Write a file into the checkout, creating parent directories as needed.
pub struct WriteFileTool {
    workspace: Workspace,
}

impl WriteFileTool {
    /// Root a writer at `workspace`.
    pub fn new(workspace: Workspace) -> Self {
        Self { workspace }
    }
}

#[async_trait::async_trait]
impl Tool<()> for WriteFileTool {
    fn name(&self) -> &str {
        "write_file"
    }

    fn description(&self) -> &str {
        "Write a UTF-8 text file into the workspace, replacing it if it exists. \
         `path` is relative to the workspace root."
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative path to write.",
                    },
                    "content": {
                        "type": "string",
                        "description": "Full file contents to write.",
                    },
                },
                "required": ["path", "content"],
            }),
            format: tinyagents::harness::tool::ToolFormat::default(),
        }
    }

    async fn call(&self, _state: &(), call: ToolCall) -> TaResult<ToolResult> {
        let path = arg_str(&call, "path")?;
        let content = arg_str(&call, "content")?;
        let resolved = guard::contained(&self.workspace.root, &path)
            .map_err(|e| TinyAgentsError::Tool(e.to_string()))?;
        // The guard resolved the parent, so it exists in the escape sense —
        // but a nested create still needs the intermediate directories made.
        if let Some(parent) = resolved.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| TinyAgentsError::Tool(format!("could not create {path}: {e}")))?;
        }
        tokio::fs::write(&resolved, content.as_bytes())
            .await
            .map_err(|e| TinyAgentsError::Tool(format!("could not write {path}: {e}")))?;
        Ok(ToolResult {
            call_id: call.id,
            name: self.name().to_string(),
            content: format!("wrote {} bytes to {path}", content.len()),
            raw: None,
            error: None,
            elapsed_ms: 0,
        })
    }
}

/// List a directory in the checkout.
pub struct ListDirTool {
    workspace: Workspace,
}

impl ListDirTool {
    /// Root a lister at `workspace`.
    pub fn new(workspace: Workspace) -> Self {
        Self { workspace }
    }
}

#[async_trait::async_trait]
impl Tool<()> for ListDirTool {
    fn name(&self) -> &str {
        "list_dir"
    }

    fn description(&self) -> &str {
        "List the entries of a workspace directory. `path` is relative to the \
         workspace root and defaults to the root itself."
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Workspace-relative directory to list.",
                    },
                },
            }),
            format: tinyagents::harness::tool::ToolFormat::default(),
        }
    }

    async fn call(&self, _state: &(), call: ToolCall) -> TaResult<ToolResult> {
        let path = call
            .arguments
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or(".")
            .to_string();
        let resolved = guard::contained(&self.workspace.root, &path)
            .map_err(|e| TinyAgentsError::Tool(e.to_string()))?;
        let mut entries = tokio::fs::read_dir(&resolved)
            .await
            .map_err(|e| TinyAgentsError::Tool(format!("could not list {path}: {e}")))?;

        // The scan stops once truncation is established, not at end of directory.
        // Bounding retained memory was half the fix: a generated or cache
        // directory with millions of entries still cost millions of
        // `next_entry()` calls before returning 500 names, so the cap bounded
        // the answer and not the work.
        //
        // The cost is the exact total, and with it some determinism. Reading
        // everything and sorting returned the lexicographically first
        // `MAX_ENTRIES` — stable across runs regardless of filesystem order.
        // Stopping early returns the first `MAX_ENTRIES` in *readdir* order,
        // which is filesystem-dependent, so which names come back from an
        // oversized directory can differ between runs. Sorting still applies to
        // what was read, so the listing itself is ordered either way.
        //
        // That trade is worth taking because the two cases that matter are not
        // affected: a directory at or under the cap is read whole and is exactly
        // as deterministic as before, and one far over it was never going to be
        // usefully summarised by 500 arbitrary names anyway. What the model
        // needs there is to know the listing is incomplete, which the notice
        // says.
        let mut names: Vec<String> = Vec::new();
        let mut truncated = false;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| TinyAgentsError::Tool(format!("could not list {path}: {e}")))?
        {
            if names.len() == MAX_ENTRIES {
                // One entry past the cap is what proves there are more; nothing
                // beyond it is read.
                truncated = true;
                break;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let suffix = match entry.file_type().await {
                Ok(t) if t.is_dir() => "/",
                _ => "",
            };
            names.push(format!("{name}{suffix}"));
        }
        // Sorted so a directory read whole produces the same transcript twice;
        // `read_dir` order is filesystem-dependent and not stable.
        names.sort();
        let mut content = names.join("\n");
        if truncated {
            // No exact total: establishing it is the scan this avoids. The
            // model needs to know the listing is incomplete, not how incomplete.
            content.push_str(&format!(
                "\n[truncated: showing {MAX_ENTRIES} entries; more were omitted]"
            ));
        }
        Ok(ToolResult {
            call_id: call.id,
            name: self.name().to_string(),
            content,
            raw: None,
            error: None,
            elapsed_ms: 0,
        })
    }
}

/// Every filesystem tool, rooted at one checkout.
pub fn all(workspace: Workspace) -> Vec<Arc<dyn Tool<()>>> {
    vec![
        Arc::new(ReadFileTool::new(workspace.clone())),
        Arc::new(WriteFileTool::new(workspace.clone())),
        Arc::new(ListDirTool::new(workspace)),
    ]
}

#[cfg(test)]
mod tests;
