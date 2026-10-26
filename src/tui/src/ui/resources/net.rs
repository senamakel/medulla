//! Network graph autoscaling and interface selection helpers.

use std::collections::VecDeque;

/// btop-style adaptive scale for one network direction, in bytes per second.
///
/// The five most recent rates are retained independently of the up/down
/// counters so a scale change uses the same rolling window as btop.
#[derive(Debug, Clone)]
pub struct NetScaler {
    max: f64,
    history: VecDeque<f64>,
    above: u8,
    below: u8,
}

impl Default for NetScaler {
    fn default() -> Self {
        Self::new(10.0 * 1024.0)
    }
}

impl NetScaler {
    /// Create a scaler with its maximum clamped to btop's 10 KiB/s floor.
    pub fn new(max: f64) -> Self {
        Self {
            max: max.max(10.0 * 1024.0),
            history: VecDeque::with_capacity(5),
            above: 0,
            below: 0,
        }
    }

    /// Current graph ceiling in bytes per second.
    pub fn max(&self) -> f64 {
        self.max
    }

    /// Incorporate one sample and return the current ceiling.
    pub fn sample(&mut self, rate: f64) -> f64 {
        let rate = rate.max(0.0);
        if self.history.len() == 5 {
            self.history.pop_front();
        }
        self.history.push_back(rate);
        if rate > self.max {
            self.above = self.above.saturating_add(1);
            self.below = self.below.saturating_sub(1);
        } else if self.max > 10.0 * 1024.0 && rate < self.max / 10.0 {
            self.below = self.below.saturating_add(1);
            self.above = self.above.saturating_sub(1);
        }
        if self.above >= 5 {
            self.max = (self.average().floor() * 1.3).floor().max(10.0 * 1024.0);
            self.above = 0;
            self.below = 0;
        } else if self.below >= 5 {
            self.max = (self.average().floor() * 3.0).floor().max(10.0 * 1024.0);
            self.above = 0;
            self.below = 0;
        }
        self.max
    }

    fn average(&self) -> f64 {
        self.history.iter().sum::<f64>() / self.history.len().max(1) as f64
    }
}

/// Return the maximum shared ceiling when sync is enabled.
pub fn synced_max(download: f64, upload: f64, sync: bool) -> (f64, f64) {
    if sync {
        let max = download.max(upload);
        (max, max)
    } else {
        (download, upload)
    }
}

/// Cycle an interface index forward or backward, ignoring down interfaces.
pub fn cycle_interface(
    current: Option<&str>,
    interfaces: &[(&str, bool)],
    forward: bool,
) -> Option<String> {
    let up: Vec<_> = interfaces
        .iter()
        .filter(|(name, is_up)| *is_up && !is_loopback(name))
        .map(|(name, _)| *name)
        .collect();
    if up.is_empty() {
        return None;
    }
    let index = current.and_then(|name| up.iter().position(|candidate| *candidate == name));
    let next = match (index, forward) {
        (Some(index), true) => (index + 1) % up.len(),
        (Some(index), false) => (index + up.len() - 1) % up.len(),
        (None, true) => 0,
        (None, false) => up.len() - 1,
    };
    Some(up[next].to_owned())
}

/// Pick the first up, non-loopback interface with observed traffic,
/// preferring one with a routable address.
///
/// The preference keeps link-only interfaces such as macOS's `awdl0`, which
/// carry a little traffic but hold only a link-local address, from beating
/// the interface the machine actually uses.
pub fn default_interface(
    interfaces: &[crate::ui::resources::sampler::NetworkSnapshot],
) -> Option<&str> {
    let candidates = || {
        interfaces.iter().filter(|interface| {
            interface.up
                && !is_loopback(&interface.interface)
                && (interface.total_received_bytes > 0
                    || interface.total_transmitted_bytes > 0
                    || interface.received_bytes_per_second > 0.0
                    || interface.transmitted_bytes_per_second > 0.0)
        })
    };
    candidates()
        .find(|interface| interface.ip_addresses.iter().any(|a| is_routable(a)))
        .or_else(|| candidates().next())
        .map(|interface| interface.interface.as_str())
}

fn is_loopback(name: &str) -> bool {
    name == "lo"
        || name.starts_with("lo:")
        || name
            .strip_prefix("lo")
            .is_some_and(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
        || name == "Loopback Pseudo-Interface 1"
}

/// Whether an address, optionally with a `/prefix`, is neither loopback nor
/// link-local.
fn is_routable(address: &str) -> bool {
    let address = address.split('/').next().unwrap_or_default();
    match address.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(v4)) => !v4.is_loopback() && !v4.is_link_local(),
        Ok(std::net::IpAddr::V6(v6)) => !v6.is_loopback() && (v6.segments()[0] & 0xffc0) != 0xfe80,
        Err(_) => false,
    }
}

#[cfg(test)]
#[path = "net_tests.rs"]
mod tests;
