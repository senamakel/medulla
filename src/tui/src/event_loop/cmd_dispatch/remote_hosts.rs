//! Bootstrapping a `[[remoteHosts]]` machine off the UI thread.
//!
//! The connection is made here rather than in the App because making one means
//! running `ssh` and waiting on it — a login prompt, a `ProxyJump`, a host that
//! is simply not answering. None of that may happen on the render path, so the
//! App holds a cache and this fills it.

use std::collections::HashMap;

use medulla::config::TuiConfig;

use super::AppMsg;

/// Dial `host_id` and report back what it can do.
///
/// Failure is reported as a message rather than swallowed: a host that cannot be
/// reached is something the operator has to see, and the *reason* is the whole
/// of what makes it actionable — a missing binary, an unknown host key and a
/// refused login all read as "could not connect" otherwise, and have three
/// different fixes.
pub(super) fn spawn_connect(
    host_id: String,
    config: &TuiConfig,
    msg_tx: &tokio::sync::mpsc::UnboundedSender<AppMsg>,
) {
    let Some(section) = config
        .remote_hosts
        .iter()
        .enumerate()
        .find(|(index, section)| medulla::config::remote_host_id(section, *index) == host_id)
        .map(|(_, section)| section.clone())
    else {
        let _ = msg_tx.send(AppMsg::RemoteHostFailed {
            host_id: host_id.clone(),
            reason: format!("no [[remoteHosts]] entry called {host_id}"),
        });
        return;
    };

    let tx = msg_tx.clone();
    tokio::spawn(async move {
        let env: HashMap<String, String> = std::env::vars().collect();
        let home = medulla::home::medulla_home(&env);
        // One identity directory per host: the link takes an advisory lock on it
        // for its lifetime, so hosts sharing a directory would leave all but the
        // first unable to connect.
        let state_dir = home.join("remote").join("clients").join(&host_id);
        let workspace = section.workspace.clone();

        let mut host = match medulla_tui::remote::client::RemoteHost::connect(
            &host_id, &section, &state_dir, &workspace,
        )
        .await
        {
            Ok(host) => host,
            Err(reason) => {
                let _ = tx.send(AppMsg::RemoteHostFailed { host_id, reason });
                return;
            }
        };
        if let Err(reason) = host.hello().await {
            let _ = tx.send(AppMsg::RemoteHostFailed { host_id, reason });
            return;
        }
        // Pump until it answers or the wait runs out. A host that completed its
        // SSH bootstrap but never speaks over UDP is the one failure worth
        // naming specifically: it almost always means the datagrams cannot
        // follow where SSH went.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            host.pump().await;
            if !host.capabilities().harnesses.is_empty() {
                let _ = tx.send(AppMsg::RemoteHostConnected {
                    host_id,
                    capabilities: Box::new(host.capabilities().clone()),
                });
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
        let _ = tx.send(AppMsg::RemoteHostFailed {
            host_id,
            reason: "reached the host over SSH but no datagram came back — \
                    check the UDP path, or set `udpHost` for this entry"
                .to_string(),
        });
    });
}
