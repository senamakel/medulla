//! The add/edit form: one prompt per field, answers carried on the prompt.
//!
//! The same shape as a workflow's declared inputs: each submission re-opens
//! the prompt for the next field with the answer so far, and `Esc` at any
//! step abandons the whole form because nothing was parked on the App. Only
//! the name and the address are required; the SSH fields exist so the page
//! can run the pairing command for the operator, and are skipped by an
//! operator who will paste it themselves.

use medulla::config::RemoteHostSection;

use super::super::types::{App, Cmd, Prompt, PromptKind};
use super::{HostForm, HostFormField};

impl App {
    /// Open the prompt for `field`, prefilled from `form`.
    pub(in crate::ui::app) fn open_host_form(&mut self, form: HostForm, field: HostFormField) {
        let prefill = form.value(field).to_string();
        self.prompt = Some(Prompt::with_text(
            PromptKind::HostField {
                form: Box::new(form),
                field,
            },
            field.title(),
            prefill,
        ));
    }

    /// Take one answer; move to the next field, or finish.
    pub(in crate::ui::app) fn submit_host_field(
        &mut self,
        mut form: HostForm,
        field: HostFormField,
        text: String,
    ) -> Option<Cmd> {
        let text = text.trim().to_string();
        match field {
            HostFormField::Name | HostFormField::Host if text.is_empty() => {
                self.set_status(format!("{} is required", field.title()));
                self.open_host_form(form, field);
                return None;
            }
            HostFormField::UdpPort | HostFormField::SshPort
                if !text.is_empty() && text.parse::<u16>().is_err() =>
            {
                self.set_status("Port must be a number between 1 and 65535");
                self.open_host_form(form, field);
                return None;
            }
            _ => {}
        }
        form.set(field, text);
        match field.next() {
            Some(next) => {
                self.open_host_form(form, next);
                None
            }
            None => self.finish_host_form(form),
        }
    }

    /// Write the form into config and, for a new host, issue its key.
    fn finish_host_form(&mut self, form: HostForm) -> Option<Cmd> {
        let udp_port = form.udp_port.parse::<u16>().unwrap_or(0);
        let ssh_port = form.ssh_port.parse::<u16>().unwrap_or(0);
        let editing = form.editing.clone();
        let mut section = match &editing {
            Some(id) => medulla::config::remote_host_section(&self.loaded.config.remote_hosts, id)
                .cloned()
                .unwrap_or_default(),
            None => RemoteHostSection::default(),
        };
        // Captured before the possible overwrite below: a paired entry's
        // client and the remote host's `pairing.json` both name the *old*
        // port, so changing it here without re-pairing would leave the
        // client dialling a port nothing listens on and the host still
        // bound to the one it was told.
        let previously_paired_port = section.paired().then_some(section.udp_port);
        section.name = form.name.clone();
        section.host = form.host.clone();
        section.user = form.user.clone();
        section.port = ssh_port;
        section.identity_file = form.identity_file.clone();
        section.workspace = form.workspace.clone();
        section.enabled = true;
        if udp_port != 0 {
            section.udp_port = udp_port;
        }
        let id = match &editing {
            Some(id) => {
                let index = self.loaded.config.remote_hosts.iter().position(|entry| {
                    medulla::config::remote_host_section(std::slice::from_ref(entry), id).is_some()
                });
                match index {
                    Some(index) => self.loaded.config.remote_hosts[index] = section,
                    None => self.loaded.config.remote_hosts.push(section),
                }
                id.clone()
            }
            None => {
                // A fresh row gets an explicit id, so renaming it later cannot
                // orphan the sessions filed under it.
                let base = medulla::config::remote_host_id(&section, 0);
                let mut id = base.clone();
                let mut n = 2;
                while medulla::config::remote_host_section(&self.loaded.config.remote_hosts, &id)
                    .is_some()
                {
                    id = format!("{base}-{n}");
                    n += 1;
                }
                section.id = id.clone();
                self.loaded.config.remote_hosts.push(section);
                id
            }
        };
        if let Err(error) = self.persist_remote_hosts() {
            self.set_status(format!("Saved for this run only: {error}"));
        }
        self.rebuild_host_entries();
        if let Some(index) = self.hosts.entries.iter().position(|entry| entry.id == id) {
            self.hosts.index = index;
        }
        match editing {
            Some(_) => self.set_status(format!("Saved {id}")),
            None => self.issue_host_key(&id),
        }
        None
    }
}
