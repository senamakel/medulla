//! Collect facts for the Harnesses Settings page away from the render path.

use std::collections::{BTreeSet, HashMap};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use medulla::config::{CustomHarnessConfig, TuiConfig};
use medulla::protocol::HarnessProvider;

use super::types::{DetectedProvider, HarnessesView};

const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Build the configuration-only fallback used before the collector returns.
pub(super) fn view_from_config(config: &TuiConfig) -> HarnessesView {
    let mut view = HarnessesView::default();
    if let Some(router) = &config.router {
        view.routing.extend(
            router
                .models
                .iter()
                .map(|(tier, model)| (tier.clone(), model.clone())),
        );
        view.routing.sort();
        if view.routing.is_empty() {
            view.routing.push(("router".into(), "configured".into()));
        }
    }
    if let Some(budget) = &config.budget {
        let mut rows: Vec<_> = budget.providers.iter().collect();
        rows.sort_by(|a, b| a.0.cmp(b.0));
        view.budgets
            .extend(rows.into_iter().map(|(provider, budget)| {
                let mut details = Vec::new();
                if let Some(seat) = &budget.seat {
                    details.push(format!("seat {seat}"));
                }
                if let Some(used) = budget.used_tokens {
                    details.push(format!("used {used}"));
                }
                if let Some(limit) = budget.limit_tokens {
                    details.push(format!("limit {limit}"));
                }
                if let Some(remaining) = budget.remaining_tokens {
                    details.push(format!("remaining {remaining}"));
                }
                (
                    provider.clone(),
                    if details.is_empty() {
                        "budget configured; values unavailable".into()
                    } else {
                        details.join(" · ")
                    },
                )
            }));
    }
    view.configuration.push(format!(
        "hooks: {} in effective configuration",
        config.hooks.hooks.len()
    ));
    view.configuration.push(format!(
        "attribution: commits {}",
        if config.attribution.commit {
            "enabled"
        } else {
            "disabled"
        }
    ));
    view.configuration.push(format!(
        "MCP: fleet tools {} · depth {}",
        if config.mcp.fleet_tools {
            "enabled"
        } else {
            "disabled"
        },
        config.mcp.max_depth
    ));
    view.configuration
        .push("Custom harness settings: not sampled".into());
    view.live_facts.push("Managed skills: not sampled".into());
    view
}

/// Read provider binaries, layered custom-harness settings, and managed skills.
/// The caller runs this on a blocking worker; the renderer only reads its result.
pub fn collect_harnesses(
    config: &TuiConfig,
    sources: &[String],
    medulla_home: &Path,
    env: &HashMap<String, String>,
) -> HarnessesView {
    let providers = medulla_providers::detect_providers(env, None, None);
    let mut view = view_from_config(config);
    view.detection_complete = true;
    view.providers = providers
        .into_iter()
        .map(|provider| DetectedProvider {
            name: medulla_providers::provider_name(provider).to_string(),
            version: probe_provider_version(provider, env),
        })
        .collect();

    view.configuration
        .retain(|fact| fact != "Custom harness settings: not sampled");
    match medulla::config::load_layered_custom_harnesses(sources) {
        Ok(custom_harnesses) => view
            .configuration
            .push(codex_override_fact(&custom_harnesses)),
        Err(_) => view.configuration.push(
            "Codex overrides: unavailable (custom harness settings could not be read)".into(),
        ),
    }

    view.live_facts
        .retain(|fact| fact != "Managed skills: not sampled");
    let install_options = medulla_harness::skills::InstallOptions {
        targets: medulla_harness::skills::SkillTarget::ALL.to_vec(),
        scope: medulla_harness::skills::SkillScope::Managed,
        root: medulla_home.to_path_buf(),
        with_commands: false,
        dry_run: true,
    };
    match medulla_harness::skills::installed(&install_options) {
        Ok(skills) => {
            let workflows: BTreeSet<_> =
                skills.into_iter().map(|skill| skill.workflow_id).collect();
            view.live_facts.push(if workflows.is_empty() {
                "No managed skills installed".into()
            } else {
                format!("Managed skills installed: {}", workflows.len())
            });
        }
        Err(_) => view
            .live_facts
            .push("Managed skills: unavailable (installation scan failed)".into()),
    }
    view
}

fn codex_override_fact(custom_harnesses: &[CustomHarnessConfig]) -> String {
    let configured = custom_harnesses
        .iter()
        .filter(|preset| preset.base_harness == HarnessProvider::Codex && preset.codex_overrides)
        .count();
    if configured == 0 {
        "No Codex override presets configured".into()
    } else {
        format!("Codex overrides configured for {configured} preset(s)")
    }
}

fn probe_provider_version(
    provider: HarnessProvider,
    env: &HashMap<String, String>,
) -> Option<String> {
    let mut child = Command::new(medulla_providers::provider_bin(provider, env))
        .arg("--version")
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => {
                let mut output = String::new();
                child.stdout.take()?.read_to_string(&mut output).ok()?;
                return output
                    .lines()
                    .next()
                    .map(str::trim)
                    .filter(|version| !version.is_empty())
                    .map(str::to_string);
            }
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if started.elapsed() < VERSION_PROBE_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::collect_harnesses;
    use std::collections::HashMap;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn collector_uses_injected_path_and_effective_customizations() {
        let temp = tempfile::tempdir().unwrap();
        let bin_dir = temp.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        let claude = bin_dir.join("claude");
        std::fs::write(&claude, "#!/bin/sh\nprintf 'Claude Code 2.4.1\\n'\n").unwrap();
        let mut permissions = std::fs::metadata(&claude).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&claude, permissions).unwrap();

        let source = temp.path().join("medulla.tui.json");
        std::fs::write(
            &source,
            r#"{"customHarnesses":[{"id":"codex-router","name":"Codex Router","baseHarness":"codex","model":"openai/gpt-4.1","hostId":"local","codexOverrides":true}]}"#,
        )
        .unwrap();
        let home = temp.path().join("medulla-home");
        std::fs::create_dir_all(&home).unwrap();
        let env = HashMap::from([
            ("PATH".to_string(), bin_dir.display().to_string()),
            ("HOME".to_string(), temp.path().display().to_string()),
        ]);

        let view = collect_harnesses(
            &medulla::config::TuiConfig::default(),
            &[source.display().to_string()],
            &home,
            &env,
        );

        assert!(view.detection_complete);
        assert_eq!(view.providers.len(), 1);
        assert_eq!(view.providers[0].name, "claude");
        assert_eq!(
            view.providers[0].version.as_deref(),
            Some("Claude Code 2.4.1")
        );
        assert!(
            view.configuration
                .iter()
                .any(|fact| fact.contains("Codex overrides configured for 1"))
        );
        assert!(
            view.live_facts
                .iter()
                .any(|fact| fact.contains("No managed skills installed"))
        );
    }
}
