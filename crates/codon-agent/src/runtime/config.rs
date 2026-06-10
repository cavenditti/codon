//! `[agent.<name>]` overrides read from codon.toml. The loader merges
//! into the existing built-in agents — anything the user omits keeps
//! its built-in default. Unknown agent names are accepted (so users
//! can pre-register a future agent), but registration only takes
//! effect once code wires the matching name.
//!
//! Schema:
//! ```toml
//! [agent.fish_complete]
//! model = "anthropic/claude-haiku-4-5"   # or bare id, or "default"
//! system_prompt = "..."                   # optional
//! user_prefix = "..."                     # optional (hand-off flows)
//! temperature = 0.0                       # optional
//! max_turns = 4                           # optional
//! cache_system_prompt = true              # optional, default true
//! ```

use crate::runtime::agent::Agent;
use crate::runtime::model::{ModelSpec, ZedModelClient};
use crate::runtime::registry::AgentRegistry;
use gpui::{App, Global};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Default, Deserialize)]
pub struct AgentOverride {
    pub model: Option<String>,
    pub system_prompt: Option<String>,
    pub user_prefix: Option<String>,
    pub temperature: Option<f32>,
    pub max_turns: Option<usize>,
    pub cache_system_prompt: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct AgentTable {
    #[serde(default)]
    agent: HashMap<String, AgentOverride>,
    #[serde(default)]
    agent_harness: HarnessOverride,
}

/// `[agent_harness]` table from codon.toml. Harness-wide knobs that
/// don't belong to any single agent.
#[derive(Debug, Default, Deserialize)]
struct HarnessOverride {
    #[serde(default)]
    show_token_counter: bool,
}

/// Resolved harness-wide settings. Default off — codon stays
/// terminal-quiet (REQ:codon/agent-harness#c-cost-bookkeeping).
#[derive(Debug, Default)]
pub struct HarnessSettings {
    pub show_token_counter: bool,
}

impl Global for HarnessSettings {}

impl HarnessSettings {
    pub fn show_token_counter(cx: &App) -> bool {
        cx.try_global::<Self>()
            .map(|settings| settings.show_token_counter)
            .unwrap_or(false)
    }
}

/// Parse the `[agent.*]` sub-tree of an in-memory `codon.toml`
/// document. Returns an empty map when the table is absent.
pub fn parse(content: &str) -> HashMap<String, AgentOverride> {
    match toml::from_str::<AgentTable>(content) {
        Ok(parsed) => parsed.agent,
        Err(err) => {
            log::warn!("codon-agent: failed to parse [agent.*] from codon.toml: {err}");
            HashMap::new()
        }
    }
}

/// Parse the `[agent_harness]` table and install the resolved
/// [`HarnessSettings`] global. Absent table → defaults (all off).
pub fn apply_harness_settings(cx: &mut App, content: &str) {
    let parsed = toml::from_str::<AgentTable>(content)
        .map(|t| t.agent_harness)
        .unwrap_or_default();
    cx.set_global(HarnessSettings {
        show_token_counter: parsed.show_token_counter,
    });
}

/// Apply overrides on top of the registered agents' *defaults*. Each
/// override produces a new `Agent` value (the registry stores
/// `Arc<Agent>` so this is a swap, not a mutation). Callers reset the
/// registry to defaults first (see `runtime::reload_from_toml`) so
/// removed overrides stop applying.
pub fn apply_overrides(cx: &mut App, overrides: HashMap<String, AgentOverride>) {
    for (name, ov) in overrides {
        let Some(current) = AgentRegistry::get(cx, &name) else {
            log::debug!(
                "codon-agent: ignoring [agent.{name}] — no built-in agent with that name yet"
            );
            continue;
        };
        let merged = merge(&current, ov);
        AgentRegistry::set_override(cx, merged);
    }
}

fn merge(current: &Agent, ov: AgentOverride) -> Agent {
    let model = ov
        .model
        .as_deref()
        .map(|raw| {
            Arc::new(ZedModelClient::new(ModelSpec::parse(raw)))
                as Arc<dyn crate::runtime::model::ModelClient>
        })
        .unwrap_or_else(|| current.model.clone());
    Agent {
        name: current.name.clone(),
        model,
        system_prompt: ov.system_prompt.or_else(|| current.system_prompt.clone()),
        user_prefix: ov.user_prefix.or_else(|| current.user_prefix.clone()),
        temperature: ov.temperature.or(current.temperature),
        tools: current.tools.clone(),
        max_turns: ov.max_turns.unwrap_or(current.max_turns),
        cache_system_prompt: ov
            .cache_system_prompt
            .unwrap_or(current.cache_system_prompt),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    fn demo_agent(temperature: f32) -> Agent {
        Agent::builder("demo", Arc::new(ZedModelClient::new(ModelSpec::Default)))
            .temperature(temperature)
            .build()
    }

    #[gpui::test]
    async fn removed_override_restores_builtin_default(cx: &mut TestAppContext) {
        cx.update(|cx| {
            AgentRegistry::install(cx);
            AgentRegistry::register(cx, demo_agent(0.1));

            crate::runtime::reload_from_toml(cx, "[agent.demo]\ntemperature = 0.9\n");
            let overridden = AgentRegistry::get(cx, "demo").expect("registered");
            assert_eq!(overridden.temperature, Some(0.9));

            // The user deletes the override: the built-in default must
            // come back on the next reload, not stick at 0.9.
            crate::runtime::reload_from_toml(cx, "");
            let restored = AgentRegistry::get(cx, "demo").expect("registered");
            assert_eq!(restored.temperature, Some(0.1));
        });
    }

    #[gpui::test]
    async fn model_override_swaps_the_client(cx: &mut TestAppContext) {
        cx.update(|cx| {
            AgentRegistry::install(cx);
            AgentRegistry::register(cx, demo_agent(0.0));

            crate::runtime::reload_from_toml(cx, "[agent.demo]\nmodel = \"acme/fast-1\"\n");
            let agent = AgentRegistry::get(cx, "demo").expect("registered");
            assert_eq!(agent.model.id().as_ref(), "acme/fast-1");

            crate::runtime::reload_from_toml(cx, "");
            let agent = AgentRegistry::get(cx, "demo").expect("registered");
            assert_eq!(agent.model.id().as_ref(), "default");
        });
    }

    #[gpui::test]
    async fn harness_settings_default_off_and_parse(cx: &mut TestAppContext) {
        cx.update(|cx| {
            apply_harness_settings(cx, "");
            assert!(!HarnessSettings::show_token_counter(cx));

            apply_harness_settings(cx, "[agent_harness]\nshow_token_counter = true\n");
            assert!(HarnessSettings::show_token_counter(cx));

            // Removing the table flips it back off on the next reload.
            apply_harness_settings(cx, "");
            assert!(!HarnessSettings::show_token_counter(cx));
        });
    }

    #[test]
    fn unparseable_toml_yields_no_overrides() {
        assert!(parse("this is { not toml").is_empty());
    }
}
