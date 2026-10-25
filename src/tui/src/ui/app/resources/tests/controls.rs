//! Clicks on the box titles and their buttons.

use super::*;

#[test]
fn clicking_a_panel_title_toggles_that_box() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
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
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.tab_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(190, 52)).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(20, 5, 170, 42)))
        .unwrap();
    let menu = app.resources.hits.menu;
    let cpu_toggle = app.resources.hits.cpu_toggle;
    let cpu = app.resources.rects.cpu;
    let buffer = terminal.backend().buffer();
    let right = cpu.right();
    // ╭─┐¹cpu┌┐menu┌┐‹ preset 0 ›┌ … ┐- 2000ms +┌─╮
    for (hit, x, symbol) in [
        (cpu_toggle, cpu.x + 3, "¹"),
        (menu, cpu.x + 9, "m"),
        (app.resources.hits.preset_prev, cpu.x + 15, "‹"),
        (app.resources.hits.preset_next, cpu.x + 26, "›"),
        (app.resources.hits.interval_down, right - 13, "-"),
        (app.resources.hits.interval_up, right - 4, "+"),
    ] {
        assert_eq!(buffer[(x, cpu.y)].symbol(), symbol, "at x={x}");
        assert!(
            hit.contains((x, cpu.y).into()),
            "visible control at x={x} misses hit rect {hit:?}"
        );
    }
    assert!(menu.width > 0);
    assert!(cpu_toggle.width > 0);
    assert!(!menu.intersects(cpu_toggle));
    app.handle_click(cpu.x + 10, cpu.y);
    assert!(app.resources.menu_open);
    assert!(app.resources.shown.cpu);
    app.handle_click(cpu.x + 4, cpu.y);
    assert!(!app.resources.shown.cpu);
}

#[test]
fn cpu_panel_renders_sixteen_cores_without_clipping_at_common_sizes() {
    let mut cpu = crate::ui::resources::CpuSnapshot {
        model: Some("Test CPU".to_owned()),
        cores: (0..16)
            .map(|index| crate::ui::resources::CpuReading {
                total: index as f32,
                ..Default::default()
            })
            .collect(),
        frequencies_mhz: (0..16).map(|index| (index != 0).then_some(2400)).collect(),
        load_average: [0.5, 0.25, 0.1],
        uptime_seconds: Some(1234),
        ..Default::default()
    };
    let mut history = crate::ui::resources::sampler::ResourceHistory::default();
    let mut historical_cpu = cpu.clone();
    historical_cpu.core_temperatures_c = (0..16).map(|index| (index, 50.0, 90.0)).collect();
    history.push(
        &crate::ui::resources::sampler::ResourceSnapshot::default().with_cpu(historical_cpu),
        20,
    );
    let snapshot = crate::ui::resources::sampler::ResourceSnapshot::default()
        .with_cpu(std::mem::take(&mut cpu))
        .with_history(Arc::new(history));
    for (width, height) in [(170, 42), (80, 24)] {
        let mut app = App::new(
            Arc::new(MockBackend::new()),
            LoadedConfig::defaults("test.json".into()),
        );
        app.set_resource_snapshot(Arc::new(snapshot.clone()));
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, width, height)))
            .unwrap();
        let cpu_rect = app.resources.rects.cpu;
        let buffer = terminal.backend().buffer();
        let text = (cpu_rect.y..cpu_rect.bottom())
            .flat_map(|y| (cpu_rect.x..cpu_rect.right()).map(move |x| buffer[(x, y)].symbol()))
            .collect::<String>();
        for core in 0..16 {
            assert!(
                text.contains(&format!("C{core} ")),
                "missing core {core} at {width}x{height}: {text}"
            );
        }
        assert!(text.contains("2.4 GHz"), "{text}");
        if width >= 100 {
            assert!(text.contains("Test CPU"));
            assert!(text.contains("Load avg: 0.50 0.25 0.10"));
            let first_core_row = (0..buffer.area.height)
                .map(|y| {
                    (cpu_rect.x..cpu_rect.right())
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                })
                .find(|row| row.contains("C0 "))
                .expect("a row for core 0");
            assert!(
                first_core_row.chars().any(|c| ('⡀'..='⣿').contains(&c)),
                "missing per-core graphs at {width}x{height}: {first_core_row}"
            );
        } else {
            assert!(
                text.contains("Load avg 0.5"),
                "missing compact load at {width}x{height}: {text}"
            );
        }
        assert!(!text.contains("Pkg"));
        assert!(!text.contains("°C"));
    }
}

#[test]
fn cpu_panel_shows_zero_load_average_with_required_label() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    let snapshot = crate::ui::resources::sampler::ResourceSnapshot::default().with_cpu(
        crate::ui::resources::CpuSnapshot {
            load_average: [0.0; 3],
            ..Default::default()
        },
    );
    app.set_resource_snapshot(Arc::new(snapshot));
    let mut terminal = Terminal::new(TestBackend::new(170, 42)).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 170, 42)))
        .unwrap();
    let cpu = app.resources.rects.cpu;
    let buffer = terminal.backend().buffer();
    let text = (cpu.y..cpu.bottom())
        .flat_map(|y| (cpu.x..cpu.right()).map(move |x| buffer[(x, y)].symbol()))
        .collect::<String>();
    assert!(text.contains("Load avg: 0.00 0.00 0.00"));
}

#[test]
fn cpu_frequency_mode_formats_mhz_ghz_and_auto() {
    assert_eq!(super::super::cpu::format_frequency(800, "auto"), "800 MHz");
    assert_eq!(super::super::cpu::format_frequency(2400, "auto"), "2.4 GHz");
    assert_eq!(super::super::cpu::format_frequency(2400, "mhz"), "2400 MHz");
    assert_eq!(super::super::cpu::format_frequency(800, "ghz"), "0.8 GHz");
}

#[test]
fn each_panel_title_button_toggles_only_its_box() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.tab_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(190, 52)).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(20, 5, 170, 42)))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let targets = [
        (
            app.resources.rects.cpu,
            app.resources.hits.cpu_toggle,
            3,
            '1',
        ),
        (
            app.resources.rects.mem,
            app.resources.hits.mem_toggle,
            3,
            '2',
        ),
        (
            app.resources.rects.net,
            app.resources.hits.net_toggle,
            3,
            '3',
        ),
        (
            app.resources.rects.agents,
            app.resources.hits.agents_toggle,
            3,
            '4',
        ),
    ];
    for (panel, hit, offset, key) in targets {
        assert!(hit.width > 0);
        let x = panel.x + offset;
        let superscript = ['¹', '²', '³', '⁴'][key as usize - '1' as usize];
        assert_eq!(buffer[(x, panel.y)].symbol(), superscript.to_string());
        assert!(
            hit.contains((x, panel.y).into()),
            "visible [{key}] misses hit rect {hit:?}"
        );
        app.handle_click(x, panel.y);
        let shown = match key {
            '1' => app.resources.shown.cpu,
            '2' => app.resources.shown.mem,
            '3' => app.resources.shown.net,
            _ => app.resources.shown.agents,
        };
        assert!(!shown, "clicking [{key}] should hide its own box");
    }
}

#[test]
fn cpu_title_preset_and_interval_buttons_are_clickable() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.tab_index = crate::ui::app::TABS
        .iter()
        .position(|tab| *tab == "Resources")
        .unwrap();
    app.resources
        .presets
        .push("cpu:1:block,mem:1:block,net:0:block,agents:0:tty".into());
    let mut terminal = Terminal::new(TestBackend::new(190, 52)).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(20, 5, 170, 42)))
        .unwrap();
    let hits = app.resources.hits;
    for hit in [
        hits.preset_prev,
        hits.preset_next,
        hits.interval_down,
        hits.interval_up,
    ] {
        assert!(hit.width > 0);
    }
    app.handle_click(hits.preset_next.x, hits.preset_next.y);
    assert_eq!(app.resources.preset, 1);
    app.handle_click(hits.preset_prev.x, hits.preset_prev.y);
    assert_eq!(app.resources.preset, 0);
    app.handle_click(hits.interval_up.x, hits.interval_up.y);
    assert_eq!(app.resources.interval_ms, 2_100);
    app.handle_click(hits.interval_down.x, hits.interval_down.y);
    assert_eq!(app.resources.interval_ms, 2_000);
}
