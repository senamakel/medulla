//! The compiled adapter registry and executable-probe implementation.

use std::process::Command;

use super::types::{
    AttentionRule, IntegrationCapabilities, IntegrationReport, ProbeStatus, ProviderAdapter,
};

/// The deterministic result supplied to [`diagnose_with`] for one executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterProbe {
    /// The executable was not found.
    Missing,
    /// The version command completed, including its exit status and output.
    Completed {
        /// Whether the command exited successfully.
        success: bool,
        /// Standard output captured from the version command.
        stdout: String,
        /// Standard error captured from the version command.
        stderr: String,
    },
    /// Starting the executable failed before it could return output.
    Failed(String),
}

const CLAUDE_RULES: &[(&str, &str)] = &[
    ("permission_menu", "Claude is asking permission"),
    ("live_progress", "Claude is working"),
    ("terminal_error", "Claude stopped on a blocking error"),
];
const CODEX_RULES: &[(&str, &str)] = &[
    ("approval_marker", "Codex is asking permission"),
    ("working_footer", "Codex is working"),
    ("terminal_error", "Codex stopped on a blocking error"),
];
const OPENCODE_RULES: &[(&str, &str)] = &[
    ("permission_menu", "OpenCode is asking permission"),
    ("working_footer", "OpenCode is working"),
    ("terminal_error", "OpenCode stopped on a blocking error"),
];

const ADAPTERS: &[ProviderAdapter] = &[
    ProviderAdapter {
        id: "claude",
        display_name: "Claude Code",
        executable_aliases: &["claude"],
        version_probe: &["--version"],
        capabilities: IntegrationCapabilities {
            launch: true,
            input: true,
            resume: true,
        },
        attention_rules: CLAUDE_RULES,
    },
    ProviderAdapter {
        id: "codex",
        display_name: "Codex",
        executable_aliases: &["codex"],
        version_probe: &["--version"],
        capabilities: IntegrationCapabilities {
            launch: true,
            input: true,
            resume: true,
        },
        attention_rules: CODEX_RULES,
    },
    ProviderAdapter {
        id: "opencode",
        display_name: "OpenCode",
        executable_aliases: &["opencode"],
        version_probe: &["--version"],
        capabilities: IntegrationCapabilities {
            launch: true,
            input: true,
            resume: true,
        },
        attention_rules: OPENCODE_RULES,
    },
];

/// Return every adapter bundled in this build, in stable display order.
pub fn adapters() -> &'static [ProviderAdapter] {
    ADAPTERS
}

/// Probe every bundled adapter against the current process environment.
///
/// A broken or absent CLI is represented in the returned report, never as an
/// error. That keeps `medulla integrations doctor` useful on a fresh machine.
pub fn diagnose() -> Vec<IntegrationReport> {
    diagnose_with(probe_system)
}

/// Build reports using `probe`, keeping fixtures deterministic and I/O-free.
pub fn diagnose_with<F>(mut probe: F) -> Vec<IntegrationReport>
where
    F: FnMut(&ProviderAdapter) -> AdapterProbe,
{
    adapters()
        .iter()
        .map(|adapter| report(adapter, probe(adapter)))
        .collect()
}

fn probe_system(adapter: &ProviderAdapter) -> AdapterProbe {
    for executable in adapter.executable_aliases {
        match Command::new(executable)
            .args(adapter.version_probe)
            .output()
        {
            Ok(output) => {
                return AdapterProbe::Completed {
                    success: output.status.success(),
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                };
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return AdapterProbe::Failed(error.to_string()),
        }
    }
    AdapterProbe::Missing
}

fn report(adapter: &ProviderAdapter, probe: AdapterProbe) -> IntegrationReport {
    let rules = adapter
        .attention_rules
        .iter()
        .map(|(id, explanation)| AttentionRule {
            id: (*id).into(),
            explanation: (*explanation).into(),
        })
        .collect();
    let executable = adapter.executable_aliases[0].to_string();
    let (status, version, warnings, repair_guidance) = match probe {
        AdapterProbe::Missing => (
            ProbeStatus::Missing,
            None,
            vec![format!("{} was not found on PATH", executable)],
            vec![format!(
                "Install {} and make `{}` available on PATH.",
                adapter.display_name, executable
            )],
        ),
        AdapterProbe::Completed {
            success: true,
            stdout,
            ..
        } => (
            ProbeStatus::Available,
            version_line(&stdout),
            Vec::new(),
            Vec::new(),
        ),
        AdapterProbe::Completed {
            success: false,
            stdout,
            stderr,
        } => (
            ProbeStatus::Unhealthy,
            version_line(&stdout),
            vec![format!(
                "{} --version exited unsuccessfully{}",
                executable,
                failure_suffix(&stderr)
            )],
            vec![format!(
                "Repair or reinstall {} and run `{} --version`.",
                adapter.display_name, executable
            )],
        ),
        AdapterProbe::Failed(error) => (
            ProbeStatus::Unhealthy,
            None,
            vec![format!(
                "could not run {} --version: {}",
                executable,
                scrub(&error)
            )],
            vec![format!(
                "Check that `{}` is executable and retry.",
                executable
            )],
        ),
    };
    IntegrationReport {
        id: adapter.id.into(),
        display_name: adapter.display_name.into(),
        executable,
        status,
        version,
        capabilities: adapter.capabilities.clone(),
        attention_rules: rules,
        warnings,
        repair_guidance,
    }
}

fn version_line(output: &str) -> Option<String> {
    output.lines().map(scrub).find(|line| !line.is_empty())
}

fn failure_suffix(stderr: &str) -> String {
    version_line(stderr)
        .map(|line| format!(": {line}"))
        .unwrap_or_default()
}

/// Strip control characters and bound subprocess output before it reaches JSON.
fn scrub(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(200)
        .collect()
}
