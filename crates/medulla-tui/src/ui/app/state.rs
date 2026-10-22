//! Construction, state accessors, snapshot refresh, and tab/lane helpers for
//! [`App`], including inspection seams and event-loop mutators.

use std::sync::Arc;

use ratatui::layout::Rect;

use crate::ui::agents::{AgentLane, derive_agent_lanes, merge_host_activity, merge_host_roster};
use crate::ui::command::CommandSpec;
use crate::ui::composer::Draft;
use crate::ui::fleet::{merge_capacity, registry_capacity};
use crate::ui::theme::Theme;
use medulla::config::LoadedConfig;
use medulla::runtime::{ContextItem, Runtime};

use super::types::{
    App, Cmd, PaneView, ResumePicker, SETTINGS_SUBPAGES, SP_CONTEXT, SP_FEEDBACK, SP_HARNESSES,
    SP_SUBSCRIPTIONS, SP_USAGE, TABS,
};

include!("state_constructor.inc.rs");
include!("state_accessors.inc.rs");
include!("state_usage.inc.rs");
