//! Prompt subsystem wiring so readiness detection and logging share one policy.

mod logger;
mod regex;
mod strip;
mod tracker;

pub(crate) use logger::{resolve_prompt_log, PromptLogger};
pub(crate) use regex::resolve_prompt_regex;
pub(crate) use tracker::{should_auto_trigger, PromptTracker};
