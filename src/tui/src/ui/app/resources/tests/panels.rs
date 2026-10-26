//! How each box draws at common terminal sizes.

use super::*;

#[test]
fn resources_draw_btop_style_titled_boxes() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    let backend = TestBackend::new(170, 42);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 170, 42)))
        .unwrap();
    let text = screen_rows(&terminal).join("\n");
    assert!(text.contains("╭─┐¹cpu┌┐menu┌┐‹ preset 0 ›┌"), "{text}");
    assert!(text.contains("┐- 2000ms +┌─╮"), "{text}");
    assert!(text.contains("╭─┐²mem┌"), "{text}");
    assert!(text.contains("╭─┐³net┌"), "{text}");
    assert!(text.contains("╭─┐⁴agents┌┐filter┌"), "{text}");
    assert!(text.contains("┘↑ select ↓└"), "{text}");
}

#[test]
fn resources_paint_a_solid_black_background_by_default() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    let theme = app.theme.resources;
    assert_eq!(theme.main_bg, Color::Rgb(0, 0, 0));
    let backend = TestBackend::new(170, 42);
    let mut terminal = Terminal::new(backend).unwrap();
    for menu_open in [false, true] {
        app.resources.menu_open = menu_open;
        terminal
            .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 170, 42)))
            .unwrap();
        let buffer = terminal.backend().buffer();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let bg = buffer[(x, y)].bg;
                assert!(
                    bg == theme.main_bg || bg == theme.selected_bg,
                    "cell ({x}, {y}) has background {bg:?} with the menu open: {menu_open}"
                );
            }
        }
    }
}

#[test]
fn network_panel_renders_at_desktop_and_narrow_widths() {
    let config = medulla::config::ResourcesConfig::default();
    let theme = crate::ui::theme::ResourcesTheme::default();
    let mut snapshot = crate::ui::resources::sampler::ResourceSnapshot::default();
    snapshot
        .networks
        .push(crate::ui::resources::sampler::NetworkSnapshot {
            interface: "eth0".into(),
            up: true,
            connected: true,
            ip_addresses: vec!["192.0.2.2".into()],
            received_bytes_per_second: 4096.0,
            transmitted_bytes_per_second: 2048.0,
            total_received_bytes: 8192,
            total_transmitted_bytes: 4096,
        });
    let mut history = crate::ui::resources::sampler::ResourceHistory::default();
    history.push(&snapshot, 8);
    let snapshot = snapshot.with_history(Arc::new(history));
    let draw = |terminal: &mut Terminal<TestBackend>,
                config: &medulla::config::ResourcesConfig,
                width: u16| {
        let view = super::super::net::NetView {
            config,
            theme: &theme,
            selected: Some("eth0"),
            zero_offsets: (0, 0),
            download_max: 10_240.0,
            upload_max: 10_240.0,
            auto: true,
            sync: false,
        };
        let mut hits = super::super::types::ResourceHits::default();
        terminal
            .draw(|frame| {
                super::super::net::draw(
                    frame,
                    Rect::new(0, 0, width, 11),
                    &snapshot,
                    &view,
                    &mut hits,
                )
            })
            .unwrap();
        screen_rows(terminal)
    };
    for width in [170, 45] {
        let mut terminal = Terminal::new(TestBackend::new(width, 11)).unwrap();
        let rows = draw(&mut terminal, &config, width);
        let text = rows.join("\n");
        assert!(text.contains("eth0"), "interface absent at width {width}");
        if width == 45 {
            assert!(text.contains("▼ 4.00 KiB/s  Σ 8.00 KiB"), "{text}");
            assert!(text.contains("▲ 2.00 KiB/s  Σ 4.00 KiB"), "{text}");
            continue;
        }
        assert!(text.contains("┐192.0.2.2┌"), "address absent: {text}");
        let line = |needle: &str| {
            rows.iter()
                .find(|row| row.contains(needle))
                .cloned()
                .unwrap_or_else(|| panic!("no row containing {needle:?}: {text}"))
        };
        assert!(line("▼ 4.00 KiB/s").contains("(32.0 Kibps)"), "{text}");
        assert!(line("▼ Top:").contains("(32.0 Kibps)"), "{text}");
        assert!(line("▼ Total:").contains("8.00 KiB"), "{text}");
        assert!(line("▲ Top:").contains("(16.0 Kibps)"), "{text}");
        assert!(line("▲ Total:").contains("4.00 KiB"), "{text}");

        let mut swapped = config.clone();
        swapped.swap_upload_download = true;
        let rows = draw(&mut terminal, &swapped, width);
        let text = rows.join("\n");
        let line = |needle: &str| {
            rows.iter()
                .find(|row| row.contains(needle))
                .cloned()
                .unwrap_or_else(|| panic!("no row containing {needle:?}: {text}"))
        };
        assert!(
            line("▼ Top:").contains("(16.0 Kibps)") && line("▼ Total:").contains("4.00 KiB"),
            "swapped top/totals must follow displayed directions: {text}"
        );
        assert!(line("▲ Total:").contains("8.00 KiB"), "{text}");
    }
}

#[test]
fn network_zero_offsets_totals_and_interface_keys_skip_down_links() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    let mut sample = crate::ui::resources::sampler::ResourceSnapshot::default();
    for (interface, up, rx) in [
        ("lo", true, 20),
        ("eth0", false, 40),
        ("wlan0", true, 100),
        ("enp1s0", true, 300),
    ] {
        sample
            .networks
            .push(crate::ui::resources::sampler::NetworkSnapshot {
                interface: interface.into(),
                up,
                connected: up && interface != "lo",
                ip_addresses: Vec::new(),
                received_bytes_per_second: 0.0,
                transmitted_bytes_per_second: 0.0,
                total_received_bytes: rx,
                total_transmitted_bytes: rx * 2,
            });
    }
    app.set_resource_snapshot(Arc::new(sample.clone()));
    app.resources.net_iface = Some("wlan0".into());
    assert!(app.resources_key(KeyCode::Char('z')));
    assert_eq!(app.resources.net_zero_offsets["wlan0"], (100, 200));
    let mut later = sample.clone();
    let wlan = later
        .networks
        .iter_mut()
        .find(|entry| entry.interface == "wlan0")
        .unwrap();
    wlan.total_received_bytes += 10;
    wlan.total_transmitted_bytes += 20;
    app.set_resource_snapshot(Arc::new(later));
    let mut terminal = Terminal::new(TestBackend::new(170, 42)).unwrap();
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
    let rows = screen_rows(&terminal);
    let total = |arrow: &str| {
        rows.iter()
            .find(|row| row.contains(&format!("{arrow} Total:")))
            .cloned()
            .unwrap_or_default()
    };
    assert!(
        total("▼").contains("10 B") && total("▲").contains("20 B"),
        "zeroed counters must continue from their offset: {text}"
    );
    assert!(app.resources_key(KeyCode::Char('n')));
    assert_eq!(app.resources.net_iface.as_deref(), Some("enp1s0"));
    assert!(app.resources_key(KeyCode::Char('z')));
    assert_eq!(app.resources.net_zero_offsets["enp1s0"], (300, 600));
    assert!(app.resources_key(KeyCode::Char('b')));
    assert_eq!(app.resources.net_iface.as_deref(), Some("wlan0"));
    assert_eq!(app.resources.net_zero_offsets["wlan0"], (100, 200));
}

#[test]
fn network_border_auto_and_sync_controls_are_clickable() {
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    let mut terminal = Terminal::new(TestBackend::new(170, 42)).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 170, 42)))
        .unwrap();
    assert!(app.resources.net_auto);
    let auto = app.resources.hits.net_auto;
    app.resources_click(auto.x, auto.y);
    assert!(!app.resources.net_auto);
    let sync = app.resources.hits.net_sync;
    app.resources_click(sync.x, sync.y);
    assert!(app.resources.net_sync);
}

#[test]
fn memory_and_disk_panel_renders_at_desktop_and_compact_terminal_sizes() {
    let mut loaded = LoadedConfig::defaults("test.json".into());
    loaded.config.resources.shown_boxes = "mem".into();
    let mut history = crate::ui::resources::sampler::ResourceHistory::default();
    let sample = crate::ui::resources::sampler::ResourceSnapshot {
        memory_total_bytes: 16_000,
        memory_used_bytes: 8_000,
        memory_available_bytes: 8_000,
        memory_cached_bytes: 2_000,
        memory_free_bytes: 1_000,
        swap_total_bytes: 4_000,
        swap_used_bytes: 500,
        disks: vec![crate::ui::resources::sampler::DiskSnapshot {
            device: "/dev/sda1".into(),
            mount: "/mnt/data".into(),
            total_bytes: 10_000,
            used_bytes: 2_500,
            read_bytes_per_second: (32 * 1024 * 1024) as f64,
            write_bytes_per_second: (64 * 1024 * 1024) as f64,
            busy_percent: Some(15.0),
            has_io: true,
        }],
        ..Default::default()
    };
    history.push(&sample, 8);
    let sample = sample.with_history(Arc::new(history));
    for (width, height) in [(170, 42), (80, 24), (60, 16)] {
        for (mem_graphs, show_disks, io_mode, combined, speeds) in [
            (true, true, false, false, ""),
            (false, true, false, false, ""),
            (true, true, true, true, ""),
            (true, true, true, false, "/mnt/data:1"),
            (true, false, false, false, ""),
            (true, false, true, false, ""),
        ] {
            let mut configured = loaded.clone();
            configured.config.resources.mem_graphs = mem_graphs;
            configured.config.resources.show_disks = show_disks;
            configured.config.resources.io_mode = io_mode;
            configured.config.resources.io_graph_combined = combined;
            configured.config.resources.io_graph_speeds = speeds.into();
            configured.config.resources.swap_disk = true;
            let mut app = App::new(Arc::new(MockBackend::new()), configured);
            app.set_resource_snapshot(Arc::new(sample.clone()));
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, width, height)))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            let mem = app.resources.rects.mem;
            let buffer = terminal.backend().buffer();
            let inside = (mem.y + 1..mem.bottom() - 1)
                .flat_map(|y| (mem.x + 1..mem.right() - 1).map(move |x| buffer[(x, y)].symbol()))
                .collect::<String>();
            assert!(
                inside.contains("Total:"),
                "total missing at {width}x{height}: {text}"
            );
            assert!(
                inside.contains("Used"),
                "used missing at {width}x{height}: {text}"
            );
            assert!(
                inside.contains("50%"),
                "RAM percentage missing at {width}x{height}: {text}"
            );
            assert_eq!(
                inside.contains("data"),
                show_disks,
                "disk visibility at {width}x{height}: {text}"
            );
            assert!(
                inside.contains("12%"),
                "swap missing at {width}x{height}: {text}"
            );
            assert_eq!(
                inside.contains("swap"),
                show_disks,
                "swap disk visibility at {width}x{height}: {text}"
            );
            if mem_graphs {
                assert!(
                    inside.chars().any(|c| ('⡀'..='⣿').contains(&c)),
                    "memory history graphs missing at {width}x{height}: {text}"
                );
            } else {
                assert!(
                    inside.contains('■'),
                    "memory meters missing at {width}x{height}: {text}"
                );
            }
            if show_disks && width == 170 {
                if io_mode {
                    assert!(
                        inside.contains("▲32M ▼64M"),
                        "IO rates missing at {width}x{height}: {text}"
                    );
                } else {
                    assert!(
                        inside.contains("IO%"),
                        "per-disk busy graph missing at {width}x{height}: {text}"
                    );
                }
            }
        }
    }
}

/// btop's disks column: every volume headed by a divider carrying its name and
/// size, IO above the used meter, swap second and with no IO row of its own.
#[test]
fn disks_are_listed_btop_style_with_swap_after_root() {
    let disk = |device: &str, mount: &str, has_io| crate::ui::resources::sampler::DiskSnapshot {
        device: device.into(),
        mount: mount.into(),
        total_bytes: 2 * 1024 * 1024 * 1024 * 1024,
        used_bytes: 1024 * 1024 * 1024 * 1024,
        read_bytes_per_second: 0.0,
        write_bytes_per_second: 0.0,
        busy_percent: None,
        has_io,
    };
    let snapshot = crate::ui::resources::sampler::ResourceSnapshot {
        disks: vec![
            disk("/dev/disk3s1s1", "/", true),
            disk("/dev/disk3s6", "/System/Volumes/VM", true),
            disk("/dev/disk1s2", "/System/Volumes/xarts", false),
        ],
        swap_total_bytes: 6 * 1024 * 1024 * 1024,
        swap_used_bytes: 3 * 1024 * 1024 * 1024,
        ..Default::default()
    };
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.set_resource_snapshot(Arc::new(snapshot));
    let mut terminal = Terminal::new(TestBackend::new(170, 42)).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 170, 42)))
        .unwrap();
    let rows = screen_rows(&terminal);
    let headers: Vec<(usize, &str)> = rows
        .iter()
        .enumerate()
        .filter_map(|(y, row)| {
            ["root", "swap", "VM", "xarts"]
                .into_iter()
                .find(|name| row.contains(&format!("├─{name}─")))
                .map(|name| (y, name))
        })
        .collect();
    assert_eq!(
        headers.iter().map(|(_, name)| *name).collect::<Vec<_>>(),
        ["root", "swap", "VM", "xarts"],
        "{}",
        rows.join("\n")
    );
    let row = |y: usize| rows[y].as_str();
    let (root, swap) = (headers[0].0, headers[1].0);
    assert!(row(root).contains("2.00 TiB─┤"), "{}", row(root));
    assert!(row(root + 1).contains("IO%"), "{}", row(root + 1));
    assert!(row(root + 2).contains("Used:"), "{}", row(root + 2));
    assert!(row(swap + 1).contains("Used:"), "{}", row(swap + 1));
    assert_eq!(headers[2].0, swap + 2, "swap has no IO row");
    let xarts = headers[3].0;
    assert!(
        row(xarts + 1).contains("Used:"),
        "a volume with no IO counters has no IO row"
    );
}

/// More volumes than the column has room to graph: every one is still listed,
/// and the spare rows give IO graphs to the first disks, root first.
#[test]
fn a_crowded_disks_column_lists_every_volume_and_graphs_root_first() {
    let disk = |index: usize| crate::ui::resources::sampler::DiskSnapshot {
        device: format!("/dev/disk3s{index}"),
        mount: if index == 0 {
            "/".into()
        } else {
            format!("/Volumes/v{index}")
        },
        total_bytes: 1024,
        used_bytes: 512,
        read_bytes_per_second: 0.0,
        write_bytes_per_second: 0.0,
        busy_percent: None,
        has_io: true,
    };
    let snapshot = crate::ui::resources::sampler::ResourceSnapshot {
        disks: (0..10).map(disk).collect(),
        ..Default::default()
    };
    let mut app = App::new(
        Arc::new(MockBackend::new()),
        LoadedConfig::defaults("test.json".into()),
    );
    app.set_resource_snapshot(Arc::new(snapshot));
    let mut terminal = Terminal::new(TestBackend::new(215, 61)).unwrap();
    terminal
        .draw(|frame| app.draw_resources(frame, Rect::new(0, 0, 215, 61)))
        .unwrap();
    let rows = screen_rows(&terminal);
    let text = rows.join("\n");
    for name in std::iter::once("root".to_string()).chain((1..10).map(|i| format!("v{i}"))) {
        assert!(
            text.contains(&format!("├─{name}─")),
            "{name} missing:\n{text}"
        );
    }
    let root = rows.iter().position(|row| row.contains("├─root─")).unwrap();
    assert!(rows[root + 1].contains("IO%"), "{}", rows[root + 1]);
}
