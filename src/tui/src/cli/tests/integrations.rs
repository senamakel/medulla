//! Tests for the integrations-doctor command-line surface.

use crate::cli::{parse_command, parse_integrations_args, Command, IntegrationsArgs};

use super::argv;

#[test]
fn integrations_command_and_json_flag_parse() {
    assert_eq!(
        parse_command(&argv(&["integrations", "doctor"])),
        Command::Integrations
    );
    assert_eq!(
        parse_integrations_args(&argv(&["doctor", "--json"])).unwrap(),
        IntegrationsArgs { json: true }
    );
}

#[test]
fn integrations_rejects_an_unknown_action() {
    assert!(parse_integrations_args(&argv(&["repair"])).is_err());
}
