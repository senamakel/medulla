//! Fixtures shared by the roster test modules.
//!
//! Every one of these builds the smallest value that still exercises the rule
//! under test, so a test reads as the rule and not as its setup.

use super::super::super::roster::HubWorker;

/// A plain advertised worker: an id, an address, and the default harness.
pub(super) fn worker(id: &str, addr: &str) -> HubWorker {
    HubWorker {
        id: id.to_string(),
        address: addr.to_string(),
        harness: "claude".to_string(),
        label: None,
        selected: false,
        workspace: None,
        ..Default::default()
    }
}

/// The same shape as [`worker`], under the name the dedupe tests use.
pub(super) fn hw(id: &str, address: &str) -> HubWorker {
    worker(id, address)
}
