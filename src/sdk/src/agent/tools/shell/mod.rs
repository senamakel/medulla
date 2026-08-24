//! The shell tool: run a command inside the turn's checkout.
//!
//! # What this does and does not claim
//!
//! This runs a command with the checkout as its working directory and a
//! deadline. It is **not** a sandbox: a command can reach anything the Medulla
//! process can, and `cwd` bounds where relative paths resolve, not what the
//! process may touch. The filesystem tools beside it enforce containment
//! because they own the path; a shell command's reach is the operating system's
//! to decide, and pretending otherwise in a doc comment is how an operator ends
//! up trusting a boundary that was never there.
//!
//! That is a deliberate scope, not an oversight — the embedded core this
//! replaced put a security policy, an approval gate and an optional OS jail in
//! front of the same call. Rebuilding that belongs with whoever re-adds
//! unattended execution; until then the tool is honest about being a plain
//! spawn, and [`ShellTool::new`] takes the deadline rather than defaulting it.

use std::process::Stdio;
use std::sync::Arc;

use tokio::io::AsyncReadExt;

use serde_json::{json, Value};
use tinyagents::error::{Result as TaResult, TinyAgentsError};
use tinyagents::harness::tool::{Tool, ToolCall, ToolResult, ToolSchema};

use super::fs::Workspace;

/// Run a command in the checkout.
pub struct ShellTool {
    workspace: Workspace,
    timeout: std::time::Duration,
}

impl ShellTool {
    /// Root a shell at `workspace`, with `timeout` as the per-command deadline.
    pub fn new(workspace: Workspace, timeout: std::time::Duration) -> Self {
        Self { workspace, timeout }
    }

    /// The interpreter this platform runs a command line through.
    fn interpreter() -> (&'static str, &'static str) {
        if cfg!(windows) {
            ("cmd", "/C")
        } else {
            ("sh", "-c")
        }
    }
}

/// Most output bytes kept per stream.
///
/// A build log is easily megabytes and every byte of it is model context; the
/// tail is kept rather than the head because the useful part of a failed command
/// is almost always at the end.
const MAX_OUTPUT_BYTES: usize = 16 * 1024;

/// A bounded tail of one output stream, read as the process produces it.
///
/// # Why this is not `wait_with_output`
///
/// That call buffers the whole stream in memory and hands it back at exit, so
/// the cap could only ever be applied *after* the fact — and a command like
/// `yes` produces output faster than any deadline expires. The process would
/// take the host down long before the truncation it was supposedly subject to
/// ran. Reading into a ring here means an unbounded command costs a fixed
/// [`MAX_OUTPUT_BYTES`] regardless of how much it writes.
///
/// Draining rather than stopping at the cap is deliberate: closing the pipe
/// would hand the child a `SIGPIPE`/`EPIPE` on its next write, killing a command
/// for being chatty rather than for failing. It keeps writing into a buffer that
/// discards the oldest bytes.
struct BoundedTail {
    /// The most recent bytes, oldest dropped first.
    buf: std::collections::VecDeque<u8>,
    /// Bytes discarded to stay under the cap, reported to the model.
    dropped: usize,
}

impl BoundedTail {
    fn new() -> Self {
        Self {
            buf: std::collections::VecDeque::with_capacity(MAX_OUTPUT_BYTES),
            dropped: 0,
        }
    }

    fn extend(&mut self, chunk: &[u8]) {
        for &byte in chunk {
            if self.buf.len() == MAX_OUTPUT_BYTES {
                self.buf.pop_front();
                self.dropped += 1;
            }
            self.buf.push_back(byte);
        }
    }

    /// The kept bytes as text, prefixed with what was dropped.
    fn finish(self) -> String {
        let dropped = self.dropped;
        let bytes: Vec<u8> = self.buf.into();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if dropped == 0 {
            return text;
        }
        format!("[truncated: {dropped} earlier bytes omitted]\n{text}")
    }
}

/// Drain `reader` to end-of-stream, keeping only a bounded tail.
async fn drain<R>(mut reader: R) -> std::io::Result<String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut tail = BoundedTail::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = reader.read(&mut chunk).await?;
        if read == 0 {
            return Ok(tail.finish());
        }
        tail.extend(&chunk[..read]);
    }
}

#[async_trait::async_trait]
impl Tool<()> for ShellTool {
    fn name(&self) -> &str {
        "shell"
    }

    fn description(&self) -> &str {
        "Run a shell command with the workspace as its working directory. \
         Returns the exit status with captured stdout and stderr."
    }

    fn schema(&self) -> ToolSchema {
        ToolSchema {
            name: self.name().to_string(),
            description: self.description().to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The command line to run.",
                    },
                },
                "required": ["command"],
            }),
            format: tinyagents::harness::tool::ToolFormat::default(),
        }
    }

    async fn call(&self, _state: &(), call: ToolCall) -> TaResult<ToolResult> {
        let command = call
            .arguments
            .get("command")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                TinyAgentsError::Tool("`command` is required and must be a string".to_string())
            })?
            .to_string();

        let (shell, flag) = Self::interpreter();
        let started = std::time::Instant::now();
        let mut child = tokio::process::Command::new(shell)
            .arg(flag)
            .arg(&command)
            .current_dir(&self.workspace.root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| TinyAgentsError::Tool(format!("could not start `{shell}`: {e}")))?;

        // Taken before the wait so both pipes can be drained concurrently with
        // it. Draining is not optional: a child that fills its stdout pipe
        // blocks forever on the write, so a wait that is not reading is a
        // deadlock for any command that outproduces the pipe buffer.
        let stdout_pipe = child.stdout.take();
        let stderr_pipe = child.stderr.take();
        let pump = async {
            let out = async {
                match stdout_pipe {
                    Some(pipe) => drain(pipe).await,
                    None => Ok(String::new()),
                }
            };
            let err = async {
                match stderr_pipe {
                    Some(pipe) => drain(pipe).await,
                    None => Ok(String::new()),
                }
            };
            let (out, err, status) = tokio::join!(out, err, child.wait());
            Ok::<_, std::io::Error>((out?, err?, status?))
        };

        // `kill_on_drop` above is what makes the timeout real: without it the
        // future is dropped on elapse and the process keeps running, holding
        // the checkout and its pipes open with nothing left to read them.
        let (stdout, stderr, status) = match tokio::time::timeout(self.timeout, pump).await {
            Ok(Ok(triple)) => triple,
            Ok(Err(e)) => return Err(TinyAgentsError::Tool(format!("`{command}` failed: {e}"))),
            Err(_) => {
                return Ok(ToolResult {
                    call_id: call.id,
                    name: self.name().to_string(),
                    content: format!(
                        "command timed out after {}s and was killed",
                        self.timeout.as_secs()
                    ),
                    raw: None,
                    // Reported as a tool error, not a run failure: a timeout is
                    // information the model can act on (narrow the command, add
                    // a filter) and ending the turn over it throws that away.
                    error: Some("timeout".to_string()),
                    elapsed_ms: started.elapsed().as_millis() as u64,
                });
            }
        };

        let code = status.code();
        let mut content = format!(
            "exit: {}\n",
            code.map(|c| c.to_string())
                .unwrap_or_else(|| "signal".to_string())
        );
        if !stdout.trim().is_empty() {
            content.push_str(&format!("--- stdout ---\n{stdout}\n"));
        }
        if !stderr.trim().is_empty() {
            content.push_str(&format!("--- stderr ---\n{stderr}\n"));
        }
        if stdout.trim().is_empty() && stderr.trim().is_empty() {
            content.push_str("(no output)\n");
        }

        Ok(ToolResult {
            call_id: call.id,
            name: self.name().to_string(),
            content,
            raw: Some(json!({ "exitCode": code })),
            // A non-zero exit is a *result*, not a tool failure: the model asked
            // to run a command and it ran. Reporting it as an error would end
            // the run on the first failing test, which is usually the moment the
            // turn is most useful.
            error: None,
            elapsed_ms: started.elapsed().as_millis() as u64,
        })
    }
}

/// The shell tool, rooted at one checkout.
pub fn all(workspace: Workspace, timeout: std::time::Duration) -> Vec<Arc<dyn Tool<()>>> {
    vec![Arc::new(ShellTool::new(workspace, timeout))]
}

#[cfg(test)]
mod tests;
