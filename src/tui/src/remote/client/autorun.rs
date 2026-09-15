//! Running the pairing command on a host over ssh, for the operator.
//!
//! Optional: pairing works by pasting the command into any shell on the
//! host. When the host's entry carries SSH details, though, the Hosts page can
//! do the paste itself. This is *not* the SSH bootstrap the direct path uses —
//! nothing is read back — it is one `ssh host 'medulla daemon HK1-… &'` whose
//! exit status says whether the shell over there accepted it. Whether the
//! daemon then came up is learned the same way it always is: by dialling.
//!
//! Key or agent auth only. `BatchMode=yes` makes a password prompt fail fast
//! with ssh's own message rather than hang a task nobody can type into.

use medulla::config::RemoteHostSection;

use super::bootstrap::{posix_quote, remote_command};

/// The `ssh` argv that runs `command` on `host`, detached.
///
/// The command is backgrounded with its stdio redirected, so the remote shell
/// exits at once and `sshd` lets the session close while the daemon keeps
/// running — the paired daemon does not close its own stdio the way the
/// SSH-bootstrapped one does, because nobody is waiting on it.
pub fn pairing_argv(host: &RemoteHostSection, command: &str, workspace: &str) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();
    argv.push("-o".to_string());
    argv.push("BatchMode=yes".to_string());
    if host.port != 0 {
        argv.push("-p".to_string());
        argv.push(host.port.to_string());
    }
    if !host.identity_file.trim().is_empty() {
        argv.push("-i".to_string());
        argv.push(host.identity_file.trim().to_string());
    }
    argv.extend(host.ssh_options.iter().cloned());
    argv.push(match host.user.trim() {
        "" => host.host.trim().to_string(),
        user => format!("{user}@{}", host.host.trim()),
    });
    argv.push("--".to_string());
    // `command` is `medulla daemon <key>`: the binary name is replaced by the
    // host's configured one, and every word is quoted for the remote shell.
    let key = command
        .split_whitespace()
        .last()
        .unwrap_or_default()
        .to_string();
    argv.push(posix_quote(&remote_command(host)));
    argv.push("daemon".to_string());
    argv.push(posix_quote(&key));
    if !workspace.trim().is_empty() {
        argv.push("--workspace".to_string());
        argv.push(posix_quote(workspace.trim()));
    }
    argv.push("</dev/null".to_string());
    argv.push(">/dev/null".to_string());
    argv.push("2>&1".to_string());
    argv.push("&".to_string());
    argv
}

/// Run the pairing command on `host`.
///
/// # Errors
///
/// When `ssh` cannot be started, or exits non-zero — with its stderr, which
/// is where a refused key, an unknown host or a missing `medulla` is named.
pub async fn run_pairing(host: &RemoteHostSection, command: &str) -> Result<(), String> {
    let binary = match host.ssh_binary.trim() {
        "" => "ssh".to_string(),
        value => value.to_string(),
    };
    let argv = pairing_argv(host, command, &host.workspace);
    let output = tokio::process::Command::new(&binary)
        .args(&argv)
        .stdin(std::process::Stdio::null())
        .output()
        .await
        .map_err(|error| format!("could not run {binary}: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let reason = stderr
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("ssh exited with an error")
        .trim()
        .to_string();
    Err(match output.status.code() {
        Some(255) => format!("{reason} (ssh could not connect)"),
        Some(127) => format!("{reason} (medulla is not installed on the host)"),
        _ => reason,
    })
}

#[cfg(test)]
#[path = "autorun_tests.rs"]
mod tests;
