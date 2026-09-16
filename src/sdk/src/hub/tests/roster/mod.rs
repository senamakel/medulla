//! Tests for the hub roster: how a worker is addressed and kept unique.
//!
//! The roster is the only thing standing between a dispatcher's `agentId`
//! and a link address, so these pin the resolution rules rather than the
//! transport — dispatch itself is covered in [`super::dispatch`].

mod helpers;

mod dedupe;
