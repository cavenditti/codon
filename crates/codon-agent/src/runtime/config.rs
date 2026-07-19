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
pub(crate) struct AgentTable {
    #[serde(default)]
    pub(crate) agent: HashMap<String, AgentOverride>,
    #[serde(default)]
    pub(crate) agent_harness: HarnessOverride,
}

/// `[agent_harness]` table from codon.toml. Harness-wide knobs that
/// don't belong to any single agent.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct HarnessOverride {
    #[serde(default)]
    pub(crate) show_token_counter: bool,
    pub(crate) active_flow: Option<String>,
    #[serde(default)]
    pub(crate) flow_paths: Vec<String>,
    #[serde(default)]
    pub(crate) shell_safety_fail_open: bool,
}

/// Resolved harness-wide settings. Default off — codon stays
/// terminal-quiet (REQ:codon/agent-harness#c-cost-bookkeeping).
///
/// The routing knobs (`active_flow`, `flow_paths`,
/// `shell_safety_fail_open`) stay on [`HarnessOverride`] and are read
/// straight from the parsed table by the reload path; only the
/// render-time token-counter toggle needs a resolved global.
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

/// Deserialize the `[agent.*]` + `[agent_harness]` tables in a single
/// pass. `Err` means `content` is not valid TOML (or mistypes a known
/// field) — callers on the reload path treat that as a transient edit
/// and keep the last-good registry rather than resetting
/// (REQ:codon/agent-routing-harness#c-last-good).
pub(crate) fn parse_document(content: &str) -> Result<AgentTable, toml::de::Error> {
    toml::from_str::<AgentTable>(content)
}

/// Parse the `[agent.*]` sub-tree of an in-memory `codon.toml`
/// document. Returns an empty map when the table is absent or the
/// document does not parse.
pub fn parse(content: &str) -> HashMap<String, AgentOverride> {
    match parse_document(content) {
        Ok(table) => table.agent,
        Err(err) => {
            log::warn!("codon-agent: failed to parse [agent.*] from codon.toml: {err}");
            HashMap::new()
        }
    }
}

/// Install the resolved [`HarnessSettings`] global from an already
/// parsed `[agent_harness]` table. The reload path deserializes the
/// document once and hands the same override here and to routing, so
/// the table is never parsed more than once per reload.
pub(crate) fn apply_harness_settings(cx: &mut App, harness: &HarnessOverride) {
    cx.set_global(HarnessSettings {
        show_token_counter: harness.show_token_counter,
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
        flow: current.flow.clone(),
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

    fn write_flow(dir: &std::path::Path, name: &str, body: &str) {
        std::fs::create_dir_all(dir).expect("flow dir");
        std::fs::write(dir.join(format!("{name}.rhai")), body).expect("flow file");
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
        let absent = parse_document("").expect("empty document parses");
        let enabled = parse_document("[agent_harness]\nshow_token_counter = true\n")
            .expect("valid document parses");

        cx.update(|cx| {
            apply_harness_settings(cx, &absent.agent_harness);
            assert!(!HarnessSettings::show_token_counter(cx));

            apply_harness_settings(cx, &enabled.agent_harness);
            assert!(HarnessSettings::show_token_counter(cx));

            // Removing the table flips it back off on the next reload.
            apply_harness_settings(cx, &absent.agent_harness);
            assert!(!HarnessSettings::show_token_counter(cx));
        });
    }

    #[test]
    fn unparseable_toml_yields_no_overrides() {
        assert!(parse("this is { not toml").is_empty());
    }

    #[gpui::test]
    async fn active_flow_registers_scripted_agents(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().expect("tempdir");
        let flow_dir = temp.path().join("flows");
        write_flow(
            &flow_dir,
            "default",
            r#"
                provider("openrouter");
                agent("main", #{ model: "z-ai/glm-5.2", prompt: "main", tools: ["delegate"] });
                agent("reckoning", #{ model: "deepseek/deepseek-4-flash", prompt: "reckon" });
                handoff("main", "reckoning", #{ name: "ask_reckoning", description: "Critique." });
                entrypoint("main");
            "#,
        );
        let config = format!(
            "[agent_harness]\nactive_flow = \"default\"\nflow_paths = [\"{}\"]\n",
            flow_dir.display()
        );

        cx.update(|cx| {
            AgentRegistry::install(cx);
            AgentRegistry::register(cx, demo_agent(0.1));
            crate::runtime::reload_from_toml(cx, &config);

            assert!(AgentRegistry::get(cx, "demo").is_some());
            let main = AgentRegistry::get(cx, "main").expect("scripted main");
            assert_eq!(main.flow.as_deref(), Some("default"));
            assert_eq!(main.tools.len(), 1);
            assert_eq!(AgentRegistry::scripted_flow(cx).as_deref(), Some("default"));
        });
    }

    #[gpui::test]
    async fn invalid_active_flow_keeps_last_good_scripted_agents(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().expect("tempdir");
        let flow_dir = temp.path().join("flows");
        write_flow(
            &flow_dir,
            "default",
            r#"agent("main", #{ model: "default", prompt: "ok" });"#,
        );
        let config = format!(
            "[agent_harness]\nactive_flow = \"default\"\nflow_paths = [\"{}\"]\n",
            flow_dir.display()
        );

        cx.update(|cx| {
            AgentRegistry::install(cx);
            AgentRegistry::register(cx, demo_agent(0.1));
            crate::runtime::reload_from_toml(cx, &config);
            assert!(AgentRegistry::get(cx, "main").is_some());
        });

        write_flow(&flow_dir, "default", r#"handoff("main", "missing", #{});"#);

        cx.update(|cx| {
            crate::runtime::reload_from_toml(cx, &config);
            assert!(
                AgentRegistry::get(cx, "main").is_some(),
                "last-good scripted flow should remain live"
            );
            assert!(
                AgentRegistry::get(cx, "demo").is_some(),
                "built-in agents survive a failed scripted reload"
            );
            assert!(AgentRegistry::routing_error(cx).is_some());
        });
    }

    #[gpui::test]
    async fn agent_override_adjusts_scripted_agent_and_keeps_flow_tag(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().expect("tempdir");
        let flow_dir = temp.path().join("flows");
        write_flow(
            &flow_dir,
            "default",
            r#"agent("main", #{ model: "z-ai/glm-5.2", prompt: "scripted" });"#,
        );
        // An `[agent.main]` table lands on top of the scripted agent the
        // flow registered — the override must win while the `flow` tag
        // survives the merge.
        let config = format!(
            "[agent_harness]\nactive_flow = \"default\"\nflow_paths = [\"{}\"]\n\n[agent.main]\nmodel = \"acme/override-1\"\nsystem_prompt = \"overridden\"\n",
            flow_dir.display()
        );

        cx.update(|cx| {
            AgentRegistry::install(cx);
            AgentRegistry::register(cx, demo_agent(0.1));
            crate::runtime::reload_from_toml(cx, &config);

            let main = AgentRegistry::get(cx, "main").expect("scripted main");
            assert_eq!(
                main.model.id().as_ref(),
                "acme/override-1",
                "[agent.main] override must swap the scripted model"
            );
            assert_eq!(main.system_prompt.as_deref(), Some("overridden"));
            assert_eq!(
                main.flow.as_deref(),
                Some("default"),
                "the override must preserve the scripted agent's flow tag"
            );
        });
    }

    #[gpui::test]
    async fn rhai_eval_error_keeps_builtins_and_last_good_scripted(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().expect("tempdir");
        let flow_dir = temp.path().join("flows");
        write_flow(
            &flow_dir,
            "default",
            r#"agent("main", #{ model: "default", prompt: "ok" });"#,
        );
        let config = format!(
            "[agent_harness]\nactive_flow = \"default\"\nflow_paths = [\"{}\"]\n",
            flow_dir.display()
        );

        cx.update(|cx| {
            AgentRegistry::install(cx);
            AgentRegistry::register(cx, demo_agent(0.1));
            crate::runtime::reload_from_toml(cx, &config);
            assert!(AgentRegistry::get(cx, "main").is_some());
        });

        // A missing comma between call arguments makes the Rhai engine
        // fail to evaluate the flow — the eval-error arm, distinct from
        // the semantic (empty/invalid-topology) failure above.
        write_flow(
            &flow_dir,
            "default",
            r#"agent("main" #{ model: "default" });"#,
        );

        cx.update(|cx| {
            crate::runtime::reload_from_toml(cx, &config);
            assert!(
                AgentRegistry::get(cx, "demo").is_some(),
                "built-in demo agent survives a Rhai eval error"
            );
            assert!(
                AgentRegistry::get(cx, "main").is_some(),
                "last-good scripted agent survives a Rhai eval error"
            );
            assert!(
                AgentRegistry::routing_error(cx).is_some(),
                "the eval error is surfaced as a metadata-only routing error"
            );
        });
    }

    #[gpui::test]
    async fn malformed_codon_toml_keeps_last_good_registry(cx: &mut TestAppContext) {
        let temp = tempfile::tempdir().expect("tempdir");
        let flow_dir = temp.path().join("flows");
        write_flow(
            &flow_dir,
            "default",
            r#"agent("main", #{ model: "default", prompt: "ok" });"#,
        );
        let config = format!(
            "[agent_harness]\nactive_flow = \"default\"\nflow_paths = [\"{}\"]\n",
            flow_dir.display()
        );

        cx.update(|cx| {
            AgentRegistry::install(cx);
            AgentRegistry::register(cx, demo_agent(0.1));
            crate::runtime::reload_from_toml(cx, &config);
            assert!(AgentRegistry::get(cx, "main").is_some());
        });

        // A transient syntax error in codon.toml itself — not the flow
        // file — must not reset built-ins or wipe the scripted layer.
        cx.update(|cx| {
            crate::runtime::reload_from_toml(cx, "this is { not valid toml");
            assert!(
                AgentRegistry::get(cx, "main").is_some(),
                "scripted agents survive a codon.toml parse error"
            );
            assert!(
                AgentRegistry::get(cx, "demo").is_some(),
                "built-in agents are not reset by a codon.toml parse error"
            );
            assert_eq!(
                AgentRegistry::scripted_flow(cx).as_deref(),
                Some("default"),
                "the active scripted flow is preserved"
            );
            assert!(
                AgentRegistry::routing_error(cx).is_some(),
                "the parse error is surfaced as metadata only"
            );
        });
    }
}
