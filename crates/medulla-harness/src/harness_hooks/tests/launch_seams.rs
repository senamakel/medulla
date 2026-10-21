use super::*;

#[test]
fn every_spawn_seam_uses_the_merged_launch_builder() {
    // Each of these seams launches a harness CLI, and each one previously called
    // `attribution_args` directly — which is exactly how a configured hook came
    // to be silently ignored on the interactive paths. Pin them: a new seam that
    // reaches for `attribution_args` instead of `launch_args` fails here.
    let seams = [
        "crates/medulla-core/src/wrapper/run/mod.rs",
        "crates/medulla-core/src/daemon/providers/execute.rs",
        "crates/medulla-tui/src/worker/executor/launch.rs",
        "crates/medulla-tui/src/ui/harness_pane/spawn.rs",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root");
    for seam in seams {
        let source = std::fs::read_to_string(root.join(seam))
            .unwrap_or_else(|error| panic!("{seam}: {error}"));
        assert!(
            source.contains("harness_hooks::launch_args"),
            "{seam} must build its argv with harness_hooks::launch_args",
        );
        assert!(
            !source.contains("attribution::attribution_args"),
            "{seam} still calls attribution_args directly, so configured hooks \
             would not reach the harness it launches",
        );
    }
}

#[test]
fn every_embedded_host_door_applies_a_launch_policy() {
    // These do not build an argv themselves — they start an embedded daemon,
    // which does it for them at its own spawn seam. What they *can* get wrong is
    // never saying what to install: `EmbeddedDaemonOptions`'s `Default` carries
    // no hooks, so a door that fills the field in with `..Default::default()`
    // launches every harness under it with none, silently. That is exactly how
    // workflow runs came to ignore a hook the same config file installed
    // everywhere else. Pin them: every options literal in these files must have
    // a `with_launch_policy` applied to it.
    let doors = [
        "crates/medulla-core/src/workflows/local.rs",
        "crates/medulla-tui/src/commands/workflow.rs",
        "crates/medulla-tui/src/event_loop/cmd_dispatch/workflows.rs",
        "crates/medulla-tui/src/hub_relay/mod.rs",
        "crates/medulla-tui/src/local_host/mod.rs",
    ];
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("workspace root");
    for door in doors {
        let source = std::fs::read_to_string(root.join(door))
            .unwrap_or_else(|error| panic!("{door}: {error}"));
        assert!(
            embedded_options_have_launch_policy(&source),
            "{door} builds an embedded host without applying its launch policy",
        );
    }
}

/// Return whether every `EmbeddedDaemonOptions` literal is immediately finished
/// with `with_launch_policy`.
fn embedded_options_have_launch_policy(source: &str) -> bool {
    let mut remaining = source;
    while let Some(start) = remaining.find("EmbeddedDaemonOptions {") {
        let literal = &remaining[start..];
        let mut depth = 0;
        let Some(end) = literal
            .char_indices()
            .find_map(|(index, character)| match character {
                '{' => {
                    depth += 1;
                    None
                }
                '}' => {
                    depth -= 1;
                    (depth == 0).then_some(index)
                }
                _ => None,
            })
        else {
            return false;
        };
        if !after_comments(&literal[end + 1..]).starts_with(".with_launch_policy(") {
            return false;
        }
        remaining = &literal[end + 1..];
    }
    true
}

/// Skip whitespace and comments between a literal and its builder call.
fn after_comments(mut source: &str) -> &str {
    loop {
        source = source.trim_start();
        if let Some(rest) = source.strip_prefix("//") {
            source = rest.split_once('\n').map_or("", |(_, rest)| rest);
        } else if let Some(rest) = source.strip_prefix("/*") {
            source = rest.split_once("*/").map_or("", |(_, rest)| rest);
        } else {
            return source;
        }
    }
}

#[test]
fn every_embedded_options_literal_needs_its_own_policy() {
    let source = r#"
        let first = EmbeddedDaemonOptions {}.with_launch_policy(policy);
        let second = EmbeddedDaemonOptions {};
    "#;
    assert!(
        !embedded_options_have_launch_policy(source),
        "one protected literal must not mask another missing policy",
    );
}
