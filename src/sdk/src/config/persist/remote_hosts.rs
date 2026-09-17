//! Writing the `[[remoteHosts]]` list back to the user-global config.
//!
//! The Hosts page is the writer. It always targets the user-global file, never a
//! project layer: [`load`](crate::config::load_config) strips `remoteHosts` from
//! project-local layers because an entry carries `sshOptions` that run commands
//! on this machine, so a row written into a project file would be silently
//! dropped on the next load.

use std::path::Path;

use crate::config::RemoteHostSection;

/// Replace the remote host list (`[[remoteHosts]]`).
///
/// Replacing rather than merging is what makes a removal durable — a merge
/// would leave a deleted machine on disk, to be offered again on the next
/// launch. Fields at their default are omitted so the file stays readable and a
/// later change to a default reaches every host declared today.
///
/// # Errors
///
/// When the file exists but cannot be parsed, or cannot be written.
pub fn persist_remote_hosts(path: &Path, hosts: &[RemoteHostSection]) -> anyhow::Result<()> {
    let rows = hosts
        .iter()
        .map(|host| {
            let mut row = toml::Value::try_from(host)?;
            if let toml::Value::Table(table) = &mut row {
                // `serde(skip_serializing_if)` covers the strings and lists;
                // these three have no "empty" a serializer can see.
                if host.port == 0 {
                    table.remove("port");
                }
                if host.udp_port == 0 {
                    table.remove("udpPort");
                }
                if host.enabled {
                    table.remove("enabled");
                }
            }
            Ok(row)
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    super::persist_root_setting(path, "remoteHosts", toml::Value::Array(rows))
}
