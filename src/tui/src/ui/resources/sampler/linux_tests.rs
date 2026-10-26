//! Unit tests for `linux`.

#[test]
pub(super) fn linux_flags_distinguish_admin_up_from_running() {
    assert_eq!(super::linux_interface_state(0x1), (true, false));
    assert_eq!(super::linux_interface_state(0x40), (false, true));
}
