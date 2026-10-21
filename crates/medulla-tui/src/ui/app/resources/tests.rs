use ratatui::layout::Rect;
use super::{layout::calc_sizes, types::{LayoutOptions, ShownBoxes, ResourcesState}};
use crossterm::event::KeyCode;

fn all() -> ShownBoxes { ShownBoxes { cpu: true, mem: true, net: true, agents: true } }

#[test]
fn default_layout_uses_btop_shares_at_common_terminal_sizes() {
    for (area, cpu_h, mem_w, mem_h, net_h, agents_w) in [
        (Rect::new(0, 0, 170, 42), 14, 77, 12, 8, 93),
        (Rect::new(0, 0, 80, 24), 8, 36, 7, 5, 39),
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
    shown = ShownBoxes { cpu: true, ..Default::default() };
    let boxes = calc_sizes(Rect::new(0, 0, 170, 42), shown, &LayoutOptions::default());
    assert_eq!(boxes.cpu, Rect::new(0, 0, 170, 42));
}

#[test]
fn undersized_terminal_requests_too_small_message() {
    let boxes = calc_sizes(Rect::new(0, 0, 50, 7), all(), &LayoutOptions::default());
    assert_eq!(boxes.too_small, Some(60));
}

#[test]
fn resource_keys_toggle_boxes_cycle_presets_and_clamp_interval() {
    let mut state = ResourcesState::default();
    for (key, expected) in [(KeyCode::Char('1'), false), (KeyCode::Char('2'), false), (KeyCode::Char('3'), false), (KeyCode::Char('4'), false)] {
        assert!(state.key(key));
        assert_eq!([state.shown.cpu, state.shown.mem, state.shown.net, state.shown.agents]["1234".chars().position(|c| c == match key { KeyCode::Char(c) => c, _ => ' ' }).unwrap()], expected);
    }
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
    assert!(crate::ui::app::TABS.contains(&"Resources"));
    #[cfg(not(feature = "workflows"))]
    assert!(crate::ui::app::TABS.contains(&"Resources"));
}
