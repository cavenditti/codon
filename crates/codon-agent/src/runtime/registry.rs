//! Process-wide registry of named [`Agent`] instances. Every codon
//! flow that needs an agent looks one up by name here — never
//! constructs one inline — so configuration overrides (model swaps,
//! prompt edits) take effect uniformly. Built-in agents are
//! registered at startup; user `[agent.<name>]` overrides apply on
//! top of them at config-load time.
//!
//! The registry keeps two maps: `agents` (what lookups resolve) and
//! `defaults` (the pristine built-in definitions). A config reload
//! resets `agents` from `defaults` before re-applying whatever
//! overrides remain in the TOML — so *removing* an override restores
//! the built-in behaviour without a restart.

use crate::runtime::agent::Agent;
use gpui::{App, BorrowAppContext as _, Global};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct AgentRegistry {
    agents: HashMap<Arc<str>, Arc<Agent>>,
    defaults: HashMap<Arc<str>, Arc<Agent>>,
    scripted: HashMap<Arc<str>, Arc<Agent>>,
    scripted_flow: Option<Arc<str>>,
    routing_error: Option<String>,
}

impl Global for AgentRegistry {}

impl AgentRegistry {
    pub fn install(cx: &mut App) {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
    }

    pub fn read(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// Register a built-in agent. Records the definition as both the
    /// live entry and the pristine default that config reloads reset
    /// to. Code paths register here; config overrides go through
    /// [`Self::set_override`].
    pub fn register(cx: &mut App, agent: Agent) {
        let name = agent.name.clone();
        let agent = Arc::new(agent);
        cx.update_global::<Self, _>(|registry, _| {
            registry.defaults.insert(name.clone(), agent.clone());
            registry.agents.insert(name, agent);
        });
    }

    /// Replace the live entry only — the recorded default is left
    /// untouched so a later [`Self::reset_to_defaults`] can undo this.
    pub fn set_override(cx: &mut App, agent: Agent) {
        let name = agent.name.clone();
        cx.update_global::<Self, _>(|registry, _| {
            registry.agents.insert(name, Arc::new(agent));
        });
    }

    /// Restore every live entry from its pristine default. Called at
    /// the top of each config reload so overrides the user *removed*
    /// from codon.toml stop applying.
    pub fn reset_to_defaults(cx: &mut App) {
        cx.update_global::<Self, _>(|registry, _| {
            registry.agents = registry.defaults.clone();
        });
    }

    /// Replace the last-good scripted layer and apply it over the
    /// built-in defaults. Scripted agents are not defaults: removing
    /// `active_flow` drops them on the next reload.
    pub fn set_scripted_flow(cx: &mut App, flow: Arc<str>, agents: Vec<Arc<Agent>>) {
        cx.update_global::<Self, _>(|registry, _| {
            registry.scripted.clear();
            registry.scripted_flow = Some(flow);
            registry.routing_error = None;
            for agent in agents {
                let name = agent.name.clone();
                registry.scripted.insert(name.clone(), agent.clone());
                registry.agents.insert(name, agent);
            }
        });
    }

    pub fn clear_scripted_flow(cx: &mut App) {
        cx.update_global::<Self, _>(|registry, _| {
            registry.scripted.clear();
            registry.scripted_flow = None;
            registry.routing_error = None;
        });
    }

    pub fn reapply_scripted_flow(cx: &mut App) {
        cx.update_global::<Self, _>(|registry, _| {
            for (name, agent) in registry.scripted.clone() {
                registry.agents.insert(name, agent);
            }
        });
    }

    pub fn set_routing_error(cx: &mut App, error: impl Into<String>) {
        cx.update_global::<Self, _>(|registry, _| {
            registry.routing_error = Some(error.into());
        });
    }

    pub fn scripted_flow(cx: &App) -> Option<Arc<str>> {
        cx.global::<Self>().scripted_flow.clone()
    }

    pub fn routing_error(cx: &App) -> Option<String> {
        cx.global::<Self>().routing_error.clone()
    }

    pub fn get(cx: &App, name: &str) -> Option<Arc<Agent>> {
        cx.global::<Self>().agents.get(name).cloned()
    }

    pub fn names(cx: &App) -> Vec<Arc<str>> {
        cx.global::<Self>().agents.keys().cloned().collect()
    }
}
