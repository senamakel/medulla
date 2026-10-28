//! [`Btop`]: the safe handle over btop's C ABI.
//!
//! The handle is a token proving [`Btop::start`] succeeded; btop's state itself
//! is process-global on the C++ side, so the handle carries no data and every
//! method is a thin, thread-safe call across the boundary.

use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use crate::types::{AgentTag, StartError};

/// The host's wake callback. btop's threads call it whenever output is ready,
/// so it must be cheap and must not call back into btop.
type WakeFn = Box<dyn Fn() + Send + Sync>;

static WAKER: OnceLock<WakeFn> = OnceLock::new();

/// A running btop. See the crate docs for the one-per-process rule.
#[derive(Debug)]
pub struct Btop {
    _started: (),
}

#[cfg(not(medulla_btop_unsupported))]
mod imp {
    use std::ffi::{c_void, CString};

    use super::WAKER;
    use crate::ffi;

    pub extern "C" fn wake(_ctx: *mut c_void) {
        if let Some(waker) = WAKER.get() {
            waker();
        }
    }

    /// Hands drained bytes to the Rust closure passed as `ctx`.
    pub extern "C" fn sink(ctx: *mut c_void, bytes: *const u8, len: usize) {
        // SAFETY: `ctx` is the `&mut &mut dyn FnMut` built in `Btop::drain`,
        // alive for the whole synchronous `medulla_btop_drain` call, and btop
        // passes a buffer of `len` readable bytes.
        let (consume, bytes) = unsafe {
            (
                &mut *(ctx as *mut &mut dyn FnMut(&[u8])),
                std::slice::from_raw_parts(bytes, len),
            )
        };
        consume(bytes);
    }

    /// Owned labels plus the raw table that borrows them.
    pub fn raw_agents(agents: &[crate::AgentTag]) -> (Vec<CString>, Vec<ffi::RawAgent>) {
        let labels: Vec<CString> = agents
            .iter()
            .map(|agent| CString::new(agent.label.replace('\0', "")).unwrap_or_default())
            .collect();
        let raw = agents
            .iter()
            .zip(&labels)
            .map(|(agent, label)| ffi::RawAgent {
                pid: agent.pid,
                label: label.as_ptr(),
                launched: u8::from(agent.launched),
            })
            .collect();
        (labels, raw)
    }
}

impl Btop {
    /// Start btop's thread with a `cols`×`rows` view, keeping `btop.conf`,
    /// its log and user themes under `config_dir` (created if absent).
    ///
    /// `waker` runs on btop's threads each time output is ready to
    /// [`drain`](Self::drain); use it to schedule a redraw.
    ///
    /// # Errors
    ///
    /// [`StartError::Unsupported`] on targets without btop,
    /// [`StartError::AlreadyStarted`] on any call after the first,
    /// [`StartError::BadPath`] for a path with an interior NUL, and
    /// [`StartError::Spawn`] if the thread could not be created.
    pub fn start(
        config_dir: &Path,
        cols: u16,
        rows: u16,
        waker: impl Fn() + Send + Sync + 'static,
    ) -> Result<Self, StartError> {
        #[cfg(medulla_btop_unsupported)]
        {
            let _ = (config_dir, cols, rows, waker);
            Err(StartError::Unsupported)
        }
        #[cfg(not(medulla_btop_unsupported))]
        {
            let dir = std::ffi::CString::new(config_dir.as_os_str().to_string_lossy().as_bytes())
                .map_err(|_| StartError::BadPath)?;
            if WAKER.set(Box::new(waker)).is_err() {
                return Err(StartError::AlreadyStarted);
            }
            // SAFETY: `dir` is a valid C string for the duration of the call
            // (btop copies it), and `imp::wake` is a plain extern fn.
            let status = unsafe {
                crate::ffi::medulla_btop_start(
                    dir.as_ptr(),
                    cols,
                    rows,
                    Some(imp::wake),
                    std::ptr::null_mut(),
                )
            };
            match status {
                0 => Ok(Self { _started: () }),
                -1 => Err(StartError::AlreadyStarted),
                _ => Err(StartError::Spawn),
            }
        }
    }

    /// Report the view size in cells. A no-op when unchanged.
    pub fn resize(&self, cols: u16, rows: u16) {
        #[cfg(not(medulla_btop_unsupported))]
        // SAFETY: plain value arguments.
        unsafe {
            crate::ffi::medulla_btop_resize(cols, rows)
        };
        #[cfg(medulla_btop_unsupported)]
        let _ = (cols, rows);
    }

    /// Queue one key or mouse event, as the bytes a terminal would send.
    pub fn send(&self, bytes: &[u8]) {
        #[cfg(not(medulla_btop_unsupported))]
        // SAFETY: btop copies `bytes.len()` bytes from a valid slice.
        unsafe {
            crate::ffi::medulla_btop_input(bytes.as_ptr(), bytes.len())
        };
        #[cfg(medulla_btop_unsupported)]
        let _ = bytes;
    }

    /// Show or hide btop. Hidden, it neither collects nor draws; shown again,
    /// it redraws everything.
    pub fn set_visible(&self, visible: bool) {
        #[cfg(not(medulla_btop_unsupported))]
        // SAFETY: plain value argument.
        unsafe {
            crate::ffi::medulla_btop_set_visible(i32::from(visible))
        };
        #[cfg(medulla_btop_unsupported)]
        let _ = visible;
    }

    /// Replace the set of agent processes btop's agents box shows.
    pub fn set_agents(&self, agents: &[AgentTag]) {
        #[cfg(not(medulla_btop_unsupported))]
        {
            let (_labels, raw) = imp::raw_agents(agents);
            // SAFETY: `raw` and the labels it points into outlive the call, and
            // btop copies every string before returning.
            unsafe { crate::ffi::medulla_btop_set_agents(raw.as_ptr(), raw.len()) };
        }
        #[cfg(medulla_btop_unsupported)]
        let _ = agents;
    }

    /// Take everything btop has drawn since the last drain, passing it to
    /// `consume` (not called when there is nothing new).
    pub fn drain(&self, mut consume: impl FnMut(&[u8])) {
        #[cfg(not(medulla_btop_unsupported))]
        {
            let mut consume: &mut dyn FnMut(&[u8]) = &mut consume;
            let ctx = &mut consume as *mut &mut dyn FnMut(&[u8]) as *mut std::ffi::c_void;
            // SAFETY: `ctx` points at `consume`, which outlives this
            // synchronous call; `imp::sink` casts it back to the same type.
            unsafe { crate::ffi::medulla_btop_drain(Some(imp::sink), ctx) };
        }
        #[cfg(medulla_btop_unsupported)]
        let _ = &mut consume;
    }

    /// Whether the operator asked to leave btop (`q`, or Quit in its menu)
    /// since the last call.
    pub fn take_leave(&self) -> bool {
        #[cfg(not(medulla_btop_unsupported))]
        // SAFETY: no arguments.
        return unsafe { crate::ffi::medulla_btop_take_leave() } != 0;
        #[cfg(medulla_btop_unsupported)]
        false
    }

    /// The message btop's thread died with, if it has.
    pub fn error(&self) -> Option<String> {
        #[cfg(not(medulla_btop_unsupported))]
        {
            let mut buf = vec![0u8; 512];
            // SAFETY: btop writes at most `buf.len()` bytes, NUL included.
            let len = unsafe { crate::ffi::medulla_btop_error(buf.as_mut_ptr().cast(), buf.len()) };
            if len == 0 {
                return None;
            }
            buf.truncate(len.min(buf.len() - 1));
            Some(String::from_utf8_lossy(&buf).into_owned())
        }
        #[cfg(medulla_btop_unsupported)]
        None
    }

    /// Stop btop's thread, saving its config, waiting at most `timeout`.
    /// btop cannot be started again in this process afterwards.
    pub fn stop(&self, timeout: Duration) {
        #[cfg(not(medulla_btop_unsupported))]
        // SAFETY: plain value argument.
        unsafe {
            crate::ffi::medulla_btop_stop(u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX))
        };
        #[cfg(medulla_btop_unsupported)]
        let _ = timeout;
    }
}
