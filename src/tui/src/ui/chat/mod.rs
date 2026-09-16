//! The reusable chat surface: a transcript over a composer.
//!
//! The Workflows copilot is "read what happened, then say what you want next",
//! and this module holds the parts of that gesture that are not about *what*
//! is being discussed, so a second conversation surface can share them rather
//! than drift:
//!
//! - [`composer`] — the bordered, focus-aware input with a real multi-line caret.
//! - [`transcript`] — bottom-anchored scrollback with a clamped scroll offset.
//!
//! What stays with each caller is where the lines come from: the copilot
//! renders role-tagged turns.

pub(crate) mod composer;
pub(crate) mod transcript;

pub(crate) mod types;

pub(crate) use composer::{composer_height, draw_composer};
pub(crate) use transcript::draw_transcript;
pub(crate) use types::ComposerChrome;

#[cfg(test)]
mod tests;
