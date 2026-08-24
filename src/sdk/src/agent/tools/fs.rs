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

        // A bounded set rather than a Vec that is sorted and truncated after the
        // fact. The cap has to bound the *work*, not just the answer: a cache or
        // build directory with millions of entries would otherwise hold millions
        // of Strings in the daemon before discarding all but 500 of them.
        //
        // `BTreeSet` keeps the retained names ordered as they arrive, so the
        // largest can be dropped once the set is full — leaving exactly the
        // lexicographically first `MAX_ENTRIES`, which is the same answer the
        // sort-then-truncate produced.
        let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        let mut total = 0usize;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| TinyAgentsError::Tool(format!("could not list {path}: {e}")))?
        {
            total += 1;
            let name = entry.file_name().to_string_lossy().into_owned();
            let suffix = match entry.file_type().await {
                Ok(t) if t.is_dir() => "/",
                _ => "",
            };
            names.insert(format!("{name}{suffix}"));
            if names.len() > MAX_ENTRIES {
                // Safe: the set is non-empty, having just been inserted into.
                let last = names
                    .iter()
                    .next_back()
                    .expect("the set has at least one entry")
                    .clone();
                names.remove(&last);
            }
        }
        let names: Vec<String> = names.into_iter().collect();
        let mut content = names.join("\n");
        if total > MAX_ENTRIES {
            content.push_str(&format!(
                "\n[truncated: showing {MAX_ENTRIES} of {total} entries]"
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
