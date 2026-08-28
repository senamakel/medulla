//! `medulla remote <host> --exec <command>`: run one command on a remote host
//! and print what its screen showed.
//!
//! The smallest thing that exercises every link in the chain — SSH bootstrap,
//! identity enrollment, direct UDP link, session open, keystrokes up, frames
//! down, screen folded — and the one that makes the chain testable from a shell
//! script rather than only from Rust. The Docker suite is built on it.
//!
//! It is also genuinely useful on its own: "what does `git status` say on the
//! build box" is a question worth one command.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use medulla::config::RemoteHostSection;

use super::RemoteHost;

/// How long to wait for the command's output to appear on the screen.
const OUTPUT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long to wait for the host to answer a request.
const REPLY_TIMEOUT: Duration = Duration::from_secs(30);

/// Run `command` on `host_id` and print the resulting screen.
///
/// # Errors
///
/// Anything that stops the chain: an unknown host id, a failed bootstrap (whose
/// message names the specific cause), a host that offers no shell, or a session
/// that never produced output.
pub async fn run(args: &[String]) -> anyhow::Result<()> {
    let host_id = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("usage: medulla remote <host> --exec <command>"))?;
    let command =
        flag(args, "--exec").ok_or_else(|| anyhow::anyhow!("--exec <command> is required"))?;

    let env: HashMap<String, String> = std::env::vars().collect();
    let cwd = std::env::current_dir()?;
    let loaded = medulla::config::load_config(flag(args, "--config").as_deref(), &env, &cwd)?;
    let section = loaded
        .config
        .remote_hosts
        .iter()
        .find(|section| {
            medulla::config::remote_host_id(section, 0) == host_id || section.host.trim() == host_id
        })
        .cloned()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no [[remoteHosts]] entry called {host_id}; known hosts: {}",
                known(&loaded.config.remote_hosts)
            )
        })?;

    let home = medulla::home::medulla_home(&env);
    // One identity directory per host, because the link takes an advisory lock
    // on it for its lifetime: hosts sharing a directory would leave all but the
    // first unable to connect.
    let state_dir = home
        .join("remote")
        .join("clients")
        .join(medulla::config::remote_host_id(&section, 0));

    let workspace = flag(args, "--workspace").unwrap_or_else(|| section.workspace.clone());
    eprintln!("connecting to {host_id}…");
    let mut host = RemoteHost::connect(&host_id, &section, &state_dir, &workspace)
        .await
        .map_err(|error| anyhow::anyhow!("{error}"))?;

    host.hello().await.map_err(|e| anyhow::anyhow!(e))?;
    await_for("the host to answer", REPLY_TIMEOUT, &mut host, |host| {
        !host.capabilities().harnesses.is_empty()
    })
    .await?;

    let shell = host
        .capabilities()
        .harnesses
        .iter()
        .find(|choice| choice.provider == "shell")
        .ok_or_else(|| anyhow::anyhow!("{host_id} offers no shell"))?
        .id
        .clone();

    host.open("exec", &shell, &workspace, 120, 40, None)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let mut session_id = String::new();
    let deadline = Instant::now() + REPLY_TIMEOUT;
    while Instant::now() < deadline && session_id.is_empty() {
        if let Some(id) = host.pump().await.first() {
            session_id = id.clone();
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    if session_id.is_empty() {
        anyhow::bail!("{host_id} never confirmed the session");
    }

    host.watch(&session_id)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    // A beat for the subscription to land before typing, so the first frame is
    // the shell's prompt rather than a blank screen.
    tokio::time::sleep(Duration::from_millis(200)).await;
    // A sentinel after the command, so we can tell "the output has arrived" from
    // "the screen has not caught up yet" without guessing at a delay.
    let marker = format!("__medulla_done_{}", std::process::id());
    host.input(format!("{command}; echo {marker}\n").as_bytes())
        .await
        .map_err(|e| anyhow::anyhow!(e))?;

    await_for("the command to finish", OUTPUT_TIMEOUT, &mut host, |host| {
        // Twice: once in the echoed command line, once as the actual output.
        screen_text(host).matches(&marker).count() >= 2
    })
    .await?;

    println!("{}", screen_text(&host));
    let _ = host.close(&session_id).await;
    Ok(())
}

/// Pump the host until `check` passes or the deadline expires.
async fn await_for(
    what: &str,
    timeout: Duration,
    host: &mut RemoteHost,
    mut check: impl FnMut(&RemoteHost) -> bool,
) -> anyhow::Result<()> {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        host.pump().await;
        if check(host) {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("timed out waiting for {what}")
}

/// The watched screen as text, trailing blanks trimmed.
fn screen_text(host: &RemoteHost) -> String {
    host.screen()
        .map(|snapshot| {
            snapshot
                .cells
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| cell.text.as_str())
                        .collect::<String>()
                        .trim_end()
                        .to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
                .trim_end()
                .to_string()
        })
        .unwrap_or_default()
}

/// The configured host ids, for an error message worth reading.
fn known(sections: &[RemoteHostSection]) -> String {
    let names: Vec<String> = medulla::config::remote_hosts(sections)
        .into_iter()
        .map(|host| host.id)
        .collect();
    match names.is_empty() {
        true => "none are configured".to_string(),
        false => names.join(", "),
    }
}

/// Read `--name value` or `--name=value`.
fn flag(args: &[String], name: &str) -> Option<String> {
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        if arg == name {
            return it.next().cloned().filter(|value| !value.is_empty());
        }
        if let Some(rest) = arg.strip_prefix(name).and_then(|r| r.strip_prefix('=')) {
            return (!rest.is_empty()).then(|| rest.to_string());
        }
    }
    None
}
