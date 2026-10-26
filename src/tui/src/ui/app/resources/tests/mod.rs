//! Unit tests for the Resources tab state, layout, and panels.

use super::{
    layout::calc_sizes,
    types::{
        AgentIdentity, AgentSignal, AgentSignalConfirmStage, AgentSignalConfirmation,
        LayoutOptions, ResourcesState, ShownBoxes,
    },
};
use crate::ui::app::App;
use crossterm::event::KeyCode;
use crossterm::event::{KeyEvent, KeyModifiers};
use medulla::backend::MockBackend;
use medulla::config::LoadedConfig;
use ratatui::layout::Rect;
use ratatui::style::Color;
use ratatui::{backend::TestBackend, Terminal};
use std::sync::Arc;

fn all() -> ShownBoxes {
    ShownBoxes {
        cpu: true,
        mem: true,
        net: true,
        agents: true,
    }
}

#[test]
fn default_layout_uses_btop_shares_at_common_terminal_sizes() {
    for (area, cpu_h, mem_w, mem_h, net_h, agents_w) in [
        (Rect::new(0, 0, 170, 42), 14, 77, 16, 12, 93),
        (Rect::new(0, 0, 80, 24), 8, 36, 9, 7, 44),
    ] {
        let boxes = calc_sizes(area, all(), &LayoutOptions::default());
        assert_eq!(boxes.cpu.height, cpu_h);
        assert_eq!(boxes.mem.width, mem_w);
        assert_eq!(boxes.mem.height, mem_h);
        assert_eq!(boxes.net.height, net_h);
        assert_eq!(boxes.agents.width, agents_w);
        assert_eq!(
            boxes.mem.height + boxes.net.height,
            area.height - boxes.cpu.height
        );
    }
}

#[test]
fn hidden_agents_widens_memory_and_network_and_cpu_alone_fills_area() {
    let mut shown = all();
    shown.agents = false;
    let boxes = calc_sizes(Rect::new(0, 0, 170, 42), shown, &LayoutOptions::default());
    assert_eq!(boxes.mem.width, 170);
    assert_eq!(boxes.net.width, 170);
    assert_eq!(boxes.mem.height, 16);
    assert_eq!(boxes.net.height, 12);
    assert_eq!(boxes.mem.height + boxes.net.height, 42 - boxes.cpu.height);
    shown = ShownBoxes {
        cpu: true,
        ..Default::default()
    };
    let boxes = calc_sizes(Rect::new(0, 0, 170, 42), shown, &LayoutOptions::default());
    assert_eq!(boxes.cpu, Rect::new(0, 0, 170, 42));
}

#[test]
fn every_visible_box_combination_has_a_rect_or_a_too_small_result() {
    for area in [Rect::new(0, 0, 170, 42), Rect::new(0, 0, 80, 24)] {
        for mask in 0..16 {
            let shown = ShownBoxes {
                cpu: mask & 1 != 0,
                mem: mask & 2 != 0,
                net: mask & 4 != 0,
                agents: mask & 8 != 0,
            };
            let boxes = calc_sizes(area, shown, &LayoutOptions::default());
            let every_visible_box_has_area = [
                (shown.cpu, boxes.cpu),
                (shown.mem, boxes.mem),
                (shown.net, boxes.net),
                (shown.agents, boxes.agents),
            ]
            .into_iter()
            .all(|(shown, rect)| !shown || (rect.width > 0 && rect.height > 0));
            assert!(
                boxes.too_small.is_some() || every_visible_box_has_area,
                "shown boxes {shown:?} have an unplaced panel at {area:?}: {boxes:?}"
            );
        }
    }
}

#[test]
fn cpu_hidden_gives_mem_seventy_two_percent_and_net_the_remaining_share() {
    for agents in [false, true] {
        let shown = ShownBoxes {
            cpu: false,
            mem: true,
            net: true,
            agents,
        };
        let boxes = calc_sizes(Rect::new(0, 0, 170, 42), shown, &LayoutOptions::default());
        assert_eq!(boxes.mem.height, 30);
        assert_eq!(boxes.net.height, 12);
        assert_eq!(boxes.mem.height + boxes.net.height, 42);
        if agents {
            assert_eq!(boxes.mem.width, 77);
            assert_eq!(boxes.agents.width, 93);
        } else {
            assert_eq!(boxes.mem.width, 170);
            assert_eq!(boxes.net.width, 170);
        }
    }
}

#[test]
fn undersized_terminal_requests_too_small_message() {
    let boxes = calc_sizes(Rect::new(0, 0, 50, 7), all(), &LayoutOptions::default());
    assert_eq!(boxes.too_small, Some(60));
}

#[test]
fn agents_take_the_full_remaining_area_before_mem_and_net_are_suppressed() {
    let boxes = calc_sizes(Rect::new(0, 0, 70, 24), all(), &LayoutOptions::default());
    assert_eq!(boxes.agents.width, 70);
    assert_eq!(boxes.mem.width, 0);
    assert_eq!(boxes.net.width, 0);
}

#[test]
fn resource_keys_toggle_boxes_cycle_presets_and_clamp_interval() {
    let mut state = ResourcesState::default();
    for (key, expected) in [
        (KeyCode::Char('1'), false),
        (KeyCode::Char('2'), false),
        (KeyCode::Char('3'), false),
        (KeyCode::Char('4'), false),
    ] {
        assert!(state.key(key));
        assert_eq!(
            [
                state.shown.cpu,
                state.shown.mem,
                state.shown.net,
                state.shown.agents
            ]["1234"
                .chars()
                .position(|c| c
                    == match key {
                        KeyCode::Char(c) => c,
                        _ => ' ',
                    })
                .unwrap()],
            expected
        );
    }
    state.presets.push("cpu:1:block,agents:0:tty".into());
    assert!(state.key(KeyCode::Char('P')));
    assert_eq!(state.preset, 1);
    state.interval_ms = 9_950;
    state.key(KeyCode::Char('+'));
    assert_eq!(state.interval_ms, 10_000);
    state.interval_ms = 150;
    state.key(KeyCode::Char('-'));
    assert_eq!(state.interval_ms, 100);
}

#[test]
fn disks_and_io_keys_toggle_the_memory_panel_modes() {
    let mut state = ResourcesState::default();
    assert!(state.show_disks && !state.io_mode);
    assert!(state.key(KeyCode::Char('d')));
    assert!(!state.show_disks);
    assert!(state.key(KeyCode::Char('i')));
    assert!(state.io_mode);
    assert!(state.key(KeyCode::Char('i')));
    assert!(!state.io_mode);
}

#[test]
fn io_graph_speed_limits_override_per_mount_in_mib_per_second() {
    let config = medulla::config::ResourcesConfig {
        io_graph_speeds: "/mnt/data:1 /boot:4".into(),
        ..Default::default()
    };
    assert_eq!(super::io_graph_speed(&config, "/mnt/data"), 1.0);
    assert_eq!(super::io_graph_speed(&config, "/boot"), 4.0);
    assert_eq!(super::io_graph_speed(&config, "/"), 100.0);
}

#[test]
fn selecting_a_preset_shows_only_the_boxes_named_by_its_entries() {
    let mut state = ResourcesState {
        presets: vec![
            "cpu:0:default,mem:0:default,net:0:default,agents:1:default".into(),
            "cpu:1:block,net:0:tty".into(),
        ],
        ..ResourcesState::default()
    };
    assert!(state.key(KeyCode::Char('P')));
    assert_eq!(
        state.shown,
        ShownBoxes {
            cpu: true,
            mem: false,
            net: true,
            agents: false,
        }
    );
}

/// Control chords are global: with Agents visible, ⌃T must reach the global
/// session-picker chord instead of the table's bare `t` (terminate) binding.
#[test]
fn control_chords_on_the_resources_tab_are_not_agent_shortcuts() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.tab_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    app.set_resource_snapshot(Arc::new(
        crate::ui::resources::sampler::ResourceSnapshot::default()
            .with_processes(vec![test_agent_process(10, "claude", 12.0)]),
    ));

    app.on_key(KeyEvent::new(KeyCode::Char('t'), KeyModifiers::CONTROL));

    assert!(
        app.resources.agent_signal_armed.is_none(),
        "⌃T must not arm a terminate signal"
    );
}

#[test]
fn tab_keys_still_cycle_all_tabs() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    for _ in 0..crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap()
    {
        app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    }
    assert_eq!(app.tab(), "Resources");
    app.on_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    assert!(!app.resources.shown.cpu);
    assert_eq!(app.loaded.config.resources.update_ms, 2_000);
    app.on_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE));
    assert_eq!(app.loaded.config.resources.update_ms, 2_100);
    app.on_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
    let resources_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    assert_eq!(
        app.tab(),
        crate::ui::app::TABS[(resources_index + 1) % crate::ui::app::TABS.len()]
    );
}

#[test]
fn resources_keyboard_changes_persist_and_reload() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let mut loaded = LoadedConfig::defaults(path.display().to_string());
    loaded.config.resources.presets = "cpu:0:default,mem:0:default,net:0:default,agents:1:default cpu:1:block,mem:1:block,net:0:block,agents:0:tty".into();
    let mut app = App::new(Arc::new(MockBackend::new()), loaded);
    app.set_config_path(path.clone());
    app.tab_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));
    app.on_key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::NONE));

    let path_text = path.to_str().unwrap().to_owned();
    let loaded =
        medulla::config::load_config(Some(&path_text), &Default::default(), temp.path()).unwrap();
    assert_eq!(loaded.config.resources.update_ms, 2_100);
    assert_eq!(loaded.config.resources.preset, 1);
    assert_eq!(loaded.config.resources.shown_boxes, "cpu,mem,net,agents");
    assert_eq!(loaded.config.resources.cpu_bottom, Some(true));
    assert_eq!(loaded.config.resources.mem_below_net, Some(true));
    assert_eq!(loaded.config.resources.agents_left, Some(true));
}

#[test]
fn box_visibility_changes_persist_and_reload_without_a_preset_change() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults(path.display().to_string()),
    );
    app.set_config_path(path.clone());
    app.tab_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    app.on_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE));

    let path_text = path.to_str().unwrap().to_owned();
    let loaded =
        medulla::config::load_config(Some(&path_text), &Default::default(), temp.path()).unwrap();
    assert_eq!(loaded.config.resources.shown_boxes, "mem,net,agents");
}

#[test]
fn explicit_layout_options_override_preset_zero() {
    let config = medulla::config::ResourcesConfig {
        preset: 0,
        cpu_bottom: Some(true),
        mem_below_net: Some(true),
        agents_left: Some(true),
        ..Default::default()
    };
    let state = ResourcesState::from_config(&config);
    assert_eq!(state.preset, 0);
    assert!(state.opts.cpu_bottom);
    assert!(state.opts.mem_below_net);
    assert!(state.opts.agents_left);
}

/// Each buffer row as a string, for assertions about what one line shows.
fn screen_rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn test_agent_process(
    pid: u32,
    command: &str,
    cpu: f64,
) -> crate::ui::resources::sampler::ProcessSnapshot {
    crate::ui::resources::sampler::ProcessSnapshot {
        pid,
        ppid: None,
        exe: Some(command.into()),
        cmd: vec![command.into()],
        cwd: Some(format!("/work/{command}")),
        start_time: 1,
        owner_uid: Some("test-user".into()),
        cpu_percent: cpu,
        memory_bytes: 2 * 1024 * 1024,
        disk_read_bytes: 0,
        disk_written_bytes: 0,
        threads: 2,
    }
}

mod agent_rows;
mod agents;
mod controls;
mod panels;

#[test]
fn short_terminals_without_agents_keep_every_panel_inside_the_area() {
    let shown = ShownBoxes {
        agents: false,
        ..all()
    };
    for height in 8..=14 {
        let area = Rect::new(0, 0, 100, height);
        let boxes = calc_sizes(area, shown, &LayoutOptions::default());
        for rect in [boxes.cpu, boxes.mem, boxes.net] {
            assert!(rect.bottom() <= area.bottom(), "height {height}: {rect:?}");
        }
    }
}

#[test]
fn swapped_network_directions_scale_each_graph_by_its_own_samples() {
    let mut state = ResourcesState {
        swap_upload_download: true,
        ..ResourcesState::default()
    };
    let mut snapshot = crate::ui::resources::sampler::ResourceSnapshot::default();
    snapshot
        .networks
        .push(crate::ui::resources::sampler::NetworkSnapshot {
            interface: "eth0".into(),
            up: true,
            connected: true,
            ip_addresses: Vec::new(),
            received_bytes_per_second: 10.0,
            transmitted_bytes_per_second: 50_000_000.0,
            total_received_bytes: 0,
            total_transmitted_bytes: 0,
        });
    state.net_iface = Some("eth0".into());
    for _ in 0..5 {
        state.sample_network(&snapshot);
    }
    assert!(
        state.net_download_scaler.max() > state.net_upload_scaler.max(),
        "the busy transmitted direction is drawn as download when swapped"
    );
}

#[test]
fn escape_cancels_an_armed_signal_while_the_menu_is_open() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.tab_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    app.resources.menu_open = true;
    app.resources.agent_signal_armed = Some(AgentSignalConfirmation {
        pid: 1,
        started: 1,
        signal: AgentSignal::Kill,
        stage: AgentSignalConfirmStage::First,
    });
    app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    assert!(!app.resources.menu_open);
    assert!(app.resources.agent_signal_armed.is_none());
}
