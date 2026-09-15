//! The Hosts tab's data: which machine the TUI is looking at, the rows the
//! page lists, and the add/edit form as it moves through its fields.

use medulla::config::RemoteHostSection;

/// Which machine the Sessions and Workflows tabs are scoped to.
///
/// Chosen with `H`. Every other tab is about this device regardless: Hosts
/// manages the list itself, Feedback and Settings are this account's, and
/// login is this device's.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum HostSelection {
    /// This machine only — the rail hides remote sessions and the picker
    /// starts here.
    #[default]
    ThisDevice,
    /// Every machine at once, sessions grouped under a header per host.
    All,
    /// One remote host, by its `[[remoteHosts]]` id.
    Host(String),
}

impl HostSelection {
    /// The remote host id, when one host is selected.
    pub fn host_id(&self) -> Option<&str> {
        match self {
            HostSelection::Host(id) => Some(id.as_str()),
            _ => None,
        }
    }

    /// Whether a session filed under `host_id` (empty for this device) is in
    /// view.
    pub fn includes(&self, host_id: &str) -> bool {
        match self {
            HostSelection::All => true,
            HostSelection::ThisDevice => host_id.is_empty(),
            HostSelection::Host(id) => id == host_id,
        }
    }
}

/// How a host is doing, as the page's list and the switcher draw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostLiveness {
    /// Declared with SSH details only; never issued a key.
    Unpaired,
    /// Paired and never dialled this run.
    Idle,
    /// Being dialled, or dialled and not yet answered.
    Connecting,
    /// Answered, heard from recently.
    Live,
    /// Answered, quiet for a while.
    Degraded,
    /// Answered, quiet for a long while.
    Offline,
    /// The dial failed, and this is why.
    Failed(String),
}

impl HostLiveness {
    /// A one-word label for a list row.
    pub fn word(&self) -> &str {
        match self {
            HostLiveness::Unpaired => "unpaired",
            HostLiveness::Idle => "idle",
            HostLiveness::Connecting => "connecting",
            HostLiveness::Live => "live",
            HostLiveness::Degraded => "quiet",
            HostLiveness::Offline => "offline",
            HostLiveness::Failed(_) => "failed",
        }
    }

    /// The lane-colour name the theme maps, so the same green means the same
    /// thing here as on the rail.
    pub fn color(&self) -> &'static str {
        match self {
            HostLiveness::Live => "green",
            HostLiveness::Connecting | HostLiveness::Degraded => "yellow",
            HostLiveness::Offline | HostLiveness::Failed(_) => "red",
            HostLiveness::Unpaired | HostLiveness::Idle => "gray",
        }
    }
}

/// One row on the Hosts page.
#[derive(Debug, Clone)]
pub struct HostEntry {
    /// The resolved `[[remoteHosts]]` id.
    pub id: String,
    /// What to call it.
    pub name: String,
    /// The declaration behind it.
    pub section: RemoteHostSection,
    /// Where its connection is.
    pub liveness: HostLiveness,
    /// How many sessions it reported, when it has answered.
    pub sessions: usize,
}

/// The pairing command the page is currently showing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingKey {
    /// Which host it was minted for.
    pub host_id: String,
    /// The full `medulla daemon HK1-…` line.
    pub command: String,
}

/// The page's state.
#[derive(Debug, Clone, Default)]
pub struct HostsState {
    /// Cursor into [`entries`](Self::entries).
    pub index: usize,
    /// The rows, rebuilt from config plus the connection cache.
    pub entries: Vec<HostEntry>,
    /// A host whose removal is armed and waiting for `y`.
    pub delete_armed: Option<String>,
    /// The last key minted this run, shown until dismissed.
    pub pending_key: Option<PendingKey>,
}

/// The fields the add/edit form walks, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostFormField {
    /// What to call it. Required.
    Name,
    /// Where to reach it: a hostname or address. Required.
    Host,
    /// The UDP port the paired daemon listens on.
    UdpPort,
    /// SSH login user, for running the pairing command over ssh.
    User,
    /// SSH port.
    SshPort,
    /// SSH identity file.
    IdentityFile,
    /// Where sessions on it start.
    Workspace,
}

impl HostFormField {
    /// Every field in the order the form asks.
    pub const ORDER: [HostFormField; 7] = [
        HostFormField::Name,
        HostFormField::Host,
        HostFormField::UdpPort,
        HostFormField::User,
        HostFormField::SshPort,
        HostFormField::IdentityFile,
        HostFormField::Workspace,
    ];

    /// The field after this one, or `None` on the last.
    pub fn next(self) -> Option<HostFormField> {
        let index = Self::ORDER.iter().position(|field| *field == self)?;
        Self::ORDER.get(index + 1).copied()
    }

    /// The prompt title.
    pub fn title(self) -> &'static str {
        match self {
            HostFormField::Name => "Host name",
            HostFormField::Host => "Address (hostname or IP the TUI dials)",
            HostFormField::UdpPort => "UDP port the host listens on",
            HostFormField::User => "SSH user (optional, for running the command for you)",
            HostFormField::SshPort => "SSH port (optional)",
            HostFormField::IdentityFile => "SSH identity file (optional)",
            HostFormField::Workspace => "Default workspace on the host (optional)",
        }
    }
}

/// The add/edit form's answers so far. Carried on the prompt, not the App,
/// so an abandoned form leaves nothing behind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostForm {
    /// The id being edited, or `None` for a new host.
    pub editing: Option<String>,
    /// Answers, by field.
    pub name: String,
    /// See [`HostFormField::Host`].
    pub host: String,
    /// See [`HostFormField::UdpPort`], as typed.
    pub udp_port: String,
    /// See [`HostFormField::User`].
    pub user: String,
    /// See [`HostFormField::SshPort`], as typed.
    pub ssh_port: String,
    /// See [`HostFormField::IdentityFile`].
    pub identity_file: String,
    /// See [`HostFormField::Workspace`].
    pub workspace: String,
}

impl HostForm {
    /// Start from an existing declaration, for editing.
    pub fn from_section(id: &str, section: &RemoteHostSection) -> Self {
        HostForm {
            editing: Some(id.to_string()),
            name: section.name.clone(),
            host: section.host.clone(),
            udp_port: match section.udp_port {
                0 => String::new(),
                port => port.to_string(),
            },
            user: section.user.clone(),
            ssh_port: match section.port {
                0 => String::new(),
                port => port.to_string(),
            },
            identity_file: section.identity_file.clone(),
            workspace: section.workspace.clone(),
        }
    }

    /// The current answer for `field`, to prefill the prompt.
    pub fn value(&self, field: HostFormField) -> &str {
        match field {
            HostFormField::Name => &self.name,
            HostFormField::Host => &self.host,
            HostFormField::UdpPort => &self.udp_port,
            HostFormField::User => &self.user,
            HostFormField::SshPort => &self.ssh_port,
            HostFormField::IdentityFile => &self.identity_file,
            HostFormField::Workspace => &self.workspace,
        }
    }

    /// Record an answer.
    pub fn set(&mut self, field: HostFormField, value: String) {
        match field {
            HostFormField::Name => self.name = value,
            HostFormField::Host => self.host = value,
            HostFormField::UdpPort => self.udp_port = value,
            HostFormField::User => self.user = value,
            HostFormField::SshPort => self.ssh_port = value,
            HostFormField::IdentityFile => self.identity_file = value,
            HostFormField::Workspace => self.workspace = value,
        }
    }
}

/// The `H` overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostSwitcher {
    /// Every choice, with its label and how it is doing.
    pub rows: Vec<(HostSelection, String, HostLiveness)>,
    /// The highlighted row.
    pub index: usize,
}
