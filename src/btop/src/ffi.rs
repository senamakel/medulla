//! Raw bindings to `vendor/btop/include/medulla_btop.h`. Every call is
//! thread-safe on the C++ side; [`crate::Btop`] is the only caller.

use std::ffi::{c_char, c_int, c_void};

/// Mirrors `medulla_btop_agent`.
#[repr(C)]
pub struct RawAgent {
    pub pid: u32,
    pub label: *const c_char,
    pub launched: u8,
}

pub type Waker = extern "C" fn(ctx: *mut c_void);
pub type Sink = extern "C" fn(ctx: *mut c_void, bytes: *const u8, len: usize);

extern "C" {
    pub fn medulla_btop_start(
        config_dir: *const c_char,
        cols: u16,
        rows: u16,
        waker: Option<Waker>,
        waker_ctx: *mut c_void,
    ) -> c_int;
    pub fn medulla_btop_resize(cols: u16, rows: u16);
    pub fn medulla_btop_input(bytes: *const u8, len: usize);
    pub fn medulla_btop_set_visible(visible: c_int);
    pub fn medulla_btop_set_agents(agents: *const RawAgent, len: usize);
    pub fn medulla_btop_drain(sink: Option<Sink>, ctx: *mut c_void);
    pub fn medulla_btop_take_leave() -> c_int;
    pub fn medulla_btop_error(buf: *mut c_char, cap: usize) -> usize;
    pub fn medulla_btop_stop(timeout_ms: u32);
}
