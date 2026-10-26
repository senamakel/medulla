//! Unit tests for the CPU panel.

use super::*;

#[test]
fn core_layout_spreads_many_cores_across_columns() {
    let layout = core_layout(Rect::new(0, 0, 168, 13), 18, false).expect("fits");
    assert_eq!((layout.columns, layout.rows), (2, 9));
    assert_eq!(layout.graph, 10);
}

#[test]
fn core_layout_gives_up_graphs_before_the_box() {
    let layout = core_layout(Rect::new(0, 0, 70, 13), 18, true).expect("fits");
    assert!(
        !layout.temp,
        "temperatures go before the graphs shrink further"
    );
    assert_eq!(layout.graph, 6);
    assert!(layout.width <= 70 * 55 / 100);
}

#[test]
fn core_layout_needs_room() {
    assert_eq!(core_layout(Rect::new(0, 0, 50, 13), 8, false), None);
    assert!(core_layout(Rect::new(0, 0, 120, 13), 0, false).is_some());
    assert_eq!(core_layout(Rect::new(0, 0, 120, 4), 8, false), None);
}

#[test]
fn model_names_lose_vendor_boilerplate() {
    assert_eq!(
        shorten_model("Intel(R) Core(TM) i7-9750H CPU @ 2.60GHz", 40),
        "i7-9750H"
    );
    assert_eq!(shorten_model("Apple M5 Pro", 40), "M5 Pro");
}
