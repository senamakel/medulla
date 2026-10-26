//! Reading, stepping, formatting, and persisting Resources setting values.

use medulla::config::ResourcesConfig;

use super::types::{Kind, Row};

pub(super) fn step(current: u64, delta: i32, min: u64, max: u64, amount: u64) -> u64 {
    if delta == 0 {
        return if current < max { current + amount } else { min };
    }
    (current as i64 + i64::from(delta) * amount as i64).clamp(min as i64, max as i64) as u64
}
pub(super) fn optional_flag(value: Option<bool>, delta: i32) -> Option<bool> {
    match (value, delta < 0) {
        (None, false) => Some(true),
        (Some(true), false) => Some(false),
        (Some(false), false) => None,
        (None, true) => Some(false),
        (Some(false), true) => Some(true),
        (Some(true), true) => None,
    }
}
pub(super) fn choices(row: Row) -> &'static [&'static str] {
    if let Kind::Choice(values) = row.kind {
        values
    } else {
        &[]
    }
}
pub(super) fn cycle<'a>(current: &str, values: &'a [&'static str], delta: i32) -> &'a str {
    let index = values
        .iter()
        .position(|value| *value == current)
        .unwrap_or(0);
    let next = if delta < 0 {
        (index + values.len() - 1) % values.len()
    } else {
        (index + 1) % values.len()
    };
    values[next]
}
pub(super) fn display(cfg: &ResourcesConfig, row: Row) -> String {
    match row.key {
        "updateMs" => cfg.update_ms.to_string(),
        "shownBoxes" => cfg.shown_boxes.clone(),
        "presets" => cfg.presets.clone(),
        "preset" => cfg.preset.to_string(),
        "cpuBottom" => opt_flag(cfg.cpu_bottom),
        "memBelowNet" => opt_flag(cfg.mem_below_net),
        "agentsLeft" => opt_flag(cfg.agents_left),
        "cpuGraphUpper" => cfg.cpu_graph_upper.clone(),
        "cpuGraphLower" => cfg.cpu_graph_lower.clone(),
        "cpuInvertLower" => flag(cfg.cpu_invert_lower),
        "cpuSingleGraph" => flag(cfg.cpu_single_graph),
        "tempScale" => cfg.temp_scale.clone(),
        "freqMode" => cfg.freq_mode.clone(),
        "showBattery" => flag(cfg.show_battery),
        "memGraphs" => flag(cfg.mem_graphs),
        "showDisks" => flag(cfg.show_disks),
        "showSwap" => flag(cfg.show_swap),
        "swapDisk" => flag(cfg.swap_disk),
        "onlyPhysical" => flag(cfg.only_physical),
        "useFstab" => flag(cfg.use_fstab),
        "disksFilter" => nonempty(&cfg.disks_filter),
        "ioMode" => flag(cfg.io_mode),
        "ioGraphCombined" => flag(cfg.io_graph_combined),
        "ioGraphSpeeds" => nonempty(&cfg.io_graph_speeds),
        "netAuto" => flag(cfg.net_auto),
        "netSync" => flag(cfg.net_sync),
        "netDownload" => cfg.net_download.to_string(),
        "netUpload" => cfg.net_upload.to_string(),
        "netIface" => cfg
            .net_iface
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "Auto".into()),
        "swapUploadDownload" => flag(cfg.swap_upload_download),
        _ => unreachable!(),
    }
}
pub(super) fn flag(value: bool) -> String {
    if value { "On" } else { "Off" }.into()
}
pub(super) fn opt_flag(value: Option<bool>) -> String {
    value.map(flag).unwrap_or_else(|| "Auto".into())
}
pub(super) fn nonempty(value: &str) -> String {
    if value.is_empty() {
        "None".into()
    } else {
        value.into()
    }
}
pub(super) fn persist_value(cfg: &ResourcesConfig, row: Row) -> toml::Value {
    use toml::Value as V;
    match row.key {
        "updateMs" => V::Integer(cfg.update_ms as i64),
        "shownBoxes" => V::String(cfg.shown_boxes.clone()),
        "presets" => V::String(cfg.presets.clone()),
        "preset" => V::Integer(cfg.preset as i64),
        "cpuBottom" => V::Boolean(cfg.cpu_bottom.unwrap_or(false)),
        "memBelowNet" => V::Boolean(cfg.mem_below_net.unwrap_or(false)),
        "agentsLeft" => V::Boolean(cfg.agents_left.unwrap_or(false)),
        "cpuGraphUpper" => V::String(cfg.cpu_graph_upper.clone()),
        "cpuGraphLower" => V::String(cfg.cpu_graph_lower.clone()),
        "cpuInvertLower" => V::Boolean(cfg.cpu_invert_lower),
        "cpuSingleGraph" => V::Boolean(cfg.cpu_single_graph),
        "tempScale" => V::String(cfg.temp_scale.clone()),
        "freqMode" => V::String(cfg.freq_mode.clone()),
        "showBattery" => V::Boolean(cfg.show_battery),
        "memGraphs" => V::Boolean(cfg.mem_graphs),
        "showDisks" => V::Boolean(cfg.show_disks),
        "showSwap" => V::Boolean(cfg.show_swap),
        "swapDisk" => V::Boolean(cfg.swap_disk),
        "onlyPhysical" => V::Boolean(cfg.only_physical),
        "useFstab" => V::Boolean(cfg.use_fstab),
        "disksFilter" => V::String(cfg.disks_filter.clone()),
        "ioMode" => V::Boolean(cfg.io_mode),
        "ioGraphCombined" => V::Boolean(cfg.io_graph_combined),
        "ioGraphSpeeds" => V::String(cfg.io_graph_speeds.clone()),
        "netAuto" => V::Boolean(cfg.net_auto),
        "netSync" => V::Boolean(cfg.net_sync),
        "netDownload" => V::Integer(cfg.net_download as i64),
        "netUpload" => V::Integer(cfg.net_upload as i64),
        "netIface" => V::String(cfg.net_iface.clone().unwrap_or_default()),
        "swapUploadDownload" => V::Boolean(cfg.swap_upload_download),
        _ => unreachable!(),
    }
}
