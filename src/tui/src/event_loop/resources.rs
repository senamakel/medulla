//! The background resource sampler behind the Resources tab.
//!
//! Sampling runs off the UI task and only while the tab is on screen, so the
//! rest of the app never pays for a whole-host process walk.

use std::time::Duration;

use medulla_tui::ui::app::App;
use medulla_tui::ui::resources::sampler::{self, DiskOptions, SystemResourceSource};
use tokio::sync::{mpsc, watch};

use super::types::AppMsg;

/// The handles the loop uses to retune a running sampler.
pub(super) struct ResourceSampler {
    interval: watch::Sender<Option<Duration>>,
    disk_options: watch::Sender<DiskOptions>,
}

impl ResourceSampler {
    /// Start sampling, keeping `history_capacity` readings for the graphs.
    pub(super) fn spawn(
        app: &App,
        history_capacity: usize,
        msg_tx: &mpsc::UnboundedSender<AppMsg>,
    ) -> Self {
        let (interval, interval_rx) = watch::channel(sampling_period(app));
        let (disk_options, disk_options_rx) = watch::channel(disk_options_for(app));
        let msg_tx = msg_tx.clone();
        sampler::spawn_sampler_with_disk_options(
            SystemResourceSource::default(),
            interval_rx,
            disk_options_rx,
            history_capacity,
            move |snapshot| msg_tx.send(AppMsg::Resources(snapshot)).is_ok(),
        );
        Self {
            interval,
            disk_options,
        }
    }

    /// Follow the visible tab and the configured cadence and disk filters.
    pub(super) fn retune(&self, app: &App) {
        sampler::update_interval(&self.interval, sampling_period(app));
        sampler::update_disk_options(&self.disk_options, disk_options_for(app));
    }
}

/// How often to sample, or `None` to pause while nothing shows the readings.
fn sampling_period(app: &App) -> Option<Duration> {
    (app.tab() == "Resources")
        .then(|| Duration::from_millis(app.loaded.config.resources.update_interval_ms()))
}

fn disk_options_for(app: &App) -> DiskOptions {
    let resources = &app.loaded.config.resources;
    DiskOptions {
        only_physical: resources.only_physical,
        use_fstab: resources.use_fstab,
        filter: resources.disks_filter.clone(),
    }
}
