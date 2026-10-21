use super::{
    layout::calc_sizes,
    types::{LayoutOptions, ResourcesState, ShownBoxes},
};
use crate::ui::app::App;
use crossterm::event::KeyCode;
use crossterm::event::{KeyEvent, KeyModifiers};
use medulla::config::LoadedConfig;
use medulla::runtime::mock::MockRuntime;
use ratatui::layout::Rect;
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
        (Rect::new(0, 0, 170, 42), 14, 77, 12, 8, 93),
        (Rect::new(0, 0, 80, 24), 8, 36, 7, 5, 44),
    ] {
        let boxes = calc_sizes(area, all(), &LayoutOptions::default());
        assert_eq!(boxes.cpu.height, cpu_h);
        assert_eq!(boxes.mem.width, mem_w);
        assert_eq!(boxes.mem.height, mem_h);
        assert_eq!(boxes.net.height, net_h);
        assert_eq!(boxes.agents.width, agents_w);
    }
}

#[test]
fn hidden_agents_widens_memory_and_network_and_cpu_alone_fills_area() {
    let mut shown = all();
    shown.agents = false;
    let boxes = calc_sizes(Rect::new(0, 0, 170, 42), shown, &LayoutOptions::default());
    assert_eq!(boxes.mem.width, 170);
    assert_eq!(boxes.net.width, 170);
    shown = ShownBoxes {
        cpu: true,
        ..Default::default()
    };
    let boxes = calc_sizes(Rect::new(0, 0, 170, 42), shown, &LayoutOptions::default());
    assert_eq!(boxes.cpu, Rect::new(0, 0, 170, 42));
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
fn tab_keys_still_cycle_all_tabs() {
    let mut app = App::new(
        Arc::new(MockRuntime::empty()),
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
fn resources_draw_empty_titled_panels_until_data_views_land() {
    let mut app = App::new(
        Arc::new(MockRuntime::empty()),
        LoadedConfig::defaults("test.json".into()),
    );
    let backend = TestBackend::new(170, 42);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 170, 42)))
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("CPU [1]"));
    assert!(text.contains("Mem [2]"));
    assert!(text.contains("Net [3]"));
    assert!(text.contains("Agents [4]"));
}

#[test]
fn clicking_a_panel_title_toggles_that_box() {
    let mut app = App::new(
        Arc::new(MockRuntime::empty()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.tab_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    app.resources.rects = calc_sizes(Rect::new(0, 5, 170, 42), all(), &LayoutOptions::default());
    let cpu = app.resources.rects.cpu;
    app.resources.hits.cpu_toggle = Rect::new(cpu.x + 2, cpu.y, 3, 1);
    app.handle_click(cpu.x + 2, cpu.y);
    assert!(!app.resources.shown.cpu);
}

#[test]
fn cpu_title_controls_have_distinct_click_actions() {
    let mut app = App::new(
        Arc::new(MockRuntime::empty()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.tab_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    app.resources.rects = calc_sizes(Rect::new(20, 5, 170, 42), all(), &LayoutOptions::default());
    let cpu = app.resources.rects.cpu;
    // The menu hit target is separate from the CPU visibility button.
    app.resources.hits.menu = Rect::new(cpu.x + 24, cpu.y, 4, 1);
    app.handle_click(cpu.x + 24, cpu.y);
    assert!(app.resources.menu_open);
    assert!(app.resources.shown.cpu);
}

#[test]
fn resources_keyboard_changes_persist_and_reload() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.toml");
    let mut app = App::new(
        Arc::new(MockRuntime::empty()),
        LoadedConfig::defaults(path.display().to_string()),
    );
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
    assert_eq!(loaded.config.resources.shown_boxes, "mem,net,agents");
}

#[test]
fn explicit_layout_options_override_preset_zero() {
    let config = medulla::config::ResourcesConfig {
        cpu_bottom: true,
        mem_below_net: true,
        agents_left: true,
        ..Default::default()
    };
    let state = ResourcesState::from_config(&config);
    assert!(state.opts.cpu_bottom);
    assert!(state.opts.mem_below_net);
    assert!(state.opts.agents_left);
}
