//! Tests for the ssh argv that runs a pairing command: quoting, batch mode,
//! and the backgrounding that lets ssh return.

use medulla::config::RemoteHostSection;

use super::pairing_argv;

#[test]
fn the_argv_is_batch_mode_quoted_and_backgrounded() {
    let host = RemoteHostSection {
        host: "gpu.example.com".to_string(),
        user: "ops".to_string(),
        port: 2222,
        identity_file: "~/.ssh/gpu".to_string(),
        workspace: "/srv/work space".to_string(),
        ..RemoteHostSection::default()
    };
    let argv = pairing_argv(&host, "medulla daemon HK1-AAAA-BBBB", "/srv/work space");
    assert_eq!(&argv[..2], ["-o", "BatchMode=yes"]);
    assert!(argv.windows(2).any(|w| w == ["-p", "2222"]), "{argv:?}");
    assert!(
        argv.windows(2).any(|w| w == ["-i", "~/.ssh/gpu"]),
        "{argv:?}"
    );
    assert!(
        argv.contains(&"ops@gpu.example.com".to_string()),
        "{argv:?}"
    );
    let dash = argv.iter().position(|a| a == "--").unwrap();
    assert_eq!(argv[dash + 1], "medulla");
    assert_eq!(argv[dash + 2], "daemon");
    assert_eq!(argv[dash + 3], "HK1-AAAA-BBBB");
    assert!(argv.contains(&"'/srv/work space'".to_string()), "{argv:?}");
    assert_eq!(argv.last().map(String::as_str), Some("&"));
}

#[test]
fn a_custom_remote_binary_replaces_the_command_name() {
    let host = RemoteHostSection {
        host: "tower".to_string(),
        remote_command: "/opt/medulla/bin/medulla".to_string(),
        ..RemoteHostSection::default()
    };
    let argv = pairing_argv(&host, "medulla daemon HK1-X", "");
    let dash = argv.iter().position(|a| a == "--").unwrap();
    assert_eq!(argv[dash + 1], "/opt/medulla/bin/medulla");
    assert!(!argv.contains(&"--workspace".to_string()));
}
