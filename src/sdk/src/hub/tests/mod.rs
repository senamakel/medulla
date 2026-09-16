//! Unit tests for the hub, split by surface so no file exceeds the
//! repo's 500-line ceiling: [`activity`] covers the in-memory activity ring and
//! its attribution; [`roster`] covers addressing and dedupe;
//! [`dispatch`] the sender-runner's full dispatch/route/settle path against a
//! fake worker; [`liveness`] the two-layer timeout gate of host-link protocol
//! §6.3; [`held_watchdog`] the third gate on that window — a session an operator
//! is holding.

mod activity;
mod activity_retirement;
mod capabilities;
mod dispatch;
mod held;
mod held_watchdog;
mod liveness;
mod roster;
mod system_info;
