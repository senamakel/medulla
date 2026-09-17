//! Deterministic fixture tests for bundled adapter reports.

use super::{adapters, diagnose_with, AdapterProbe, ProbeStatus};

#[test]
fn bundled_adapters_are_stable_and_complete() {
    assert_eq!(
        adapters()
            .iter()
            .map(|adapter| adapter.id)
            .collect::<Vec<_>>(),
        ["claude", "codex", "opencode"]
    );
    assert!(adapters().iter().all(|adapter| adapter.capabilities.launch
        && adapter.capabilities.input
        && adapter.capabilities.resume));
}

#[test]
fn missing_executable_degrades_to_a_repairable_warning() {
    let reports = diagnose_with(|_| AdapterProbe::Missing);
    assert!(reports
        .iter()
        .all(|report| report.status == ProbeStatus::Missing));
    assert!(reports.iter().all(|report| report.version.is_none()
        && !report.warnings.is_empty()
        && !report.repair_guidance.is_empty()));
}

#[test]
fn successful_version_is_normalized_without_control_characters() {
    let reports = diagnose_with(|_| AdapterProbe::Completed {
        success: true,
        stdout: "\nCodex 1.2.3\u{1b}[0m\nextra".into(),
        stderr: String::new(),
    });
    assert_eq!(reports[1].status, ProbeStatus::Available);
    assert_eq!(reports[1].version.as_deref(), Some("Codex 1.2.3[0m"));
}

#[test]
fn failed_version_is_a_warning_not_a_diagnosis_error() {
    let reports = diagnose_with(|_| AdapterProbe::Completed {
        success: false,
        stdout: String::new(),
        stderr: "unsupported runtime".into(),
    });
    assert!(reports
        .iter()
        .all(|report| report.status == ProbeStatus::Unhealthy));
    assert!(reports
        .iter()
        .all(|report| report.warnings[0].contains("unsupported runtime")));
}
