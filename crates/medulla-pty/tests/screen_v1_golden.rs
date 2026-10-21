use medulla_pty::screen::{encode_screen_message, parse_screen_message};

#[test]
fn golden_screen_v1_frame_decodes_and_reencodes_byte_identically() {
    let golden = include_str!("golden/screen_v1_subscribe.json").trim_end();
    let decoded = parse_screen_message(golden).expect("golden frame should decode");
    assert_eq!(encode_screen_message(&decoded), golden);
}
