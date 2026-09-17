//! The read-only `medulla integrations doctor` command.
//!
//! The SDK owns probing and the report schema, leaving this module responsible
//! only for command-line parsing and rendering. It intentionally makes no
//! provider configuration changes; repair guidance is advice, not an action.

use medulla_tui::cli::parse_integrations_args;

/// Run `medulla integrations doctor [--json]`.
pub(crate) fn run_integrations(args: &[String]) -> anyhow::Result<()> {
    let parsed = parse_integrations_args(args).map_err(anyhow::Error::msg)?;
    let reports = medulla::integrations::diagnose();
    if parsed.json {
        println!("{}", serde_json::to_string_pretty(&reports)?);
        return Ok(());
    }
    for report in reports {
        let version = report.version.as_deref().unwrap_or("not available");
        println!(
            "{} ({}) — {:?}: {}",
            report.display_name, report.executable, report.status, version
        );
        for warning in report.warnings {
            println!("  warning: {warning}");
        }
        for guidance in report.repair_guidance {
            println!("  repair: {guidance}");
        }
    }
    Ok(())
}
