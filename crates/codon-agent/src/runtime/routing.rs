use crate::runtime::agent::Agent;
use crate::runtime::cancel::CancelToken;
use crate::runtime::config::HarnessOverride;
use crate::runtime::delegate::DelegateTool;
use crate::runtime::error::ToolError;
use crate::runtime::model::{ModelSpec, ZedModelClient};
use crate::runtime::registry::AgentRegistry;
use crate::runtime::safety::{
    self, SafetyDecision, SafetySource, SafetyVerdict, ShellPermissionRule,
};
use crate::runtime::tool::{Tool, ToolSet};
use anyhow::anyhow;
use futures::{AsyncReadExt as _, FutureExt as _, select_biased};
use gpui::{App, AsyncApp};
use rhai::{Array, Dynamic, Engine, EvalAltResult, FLOAT, INT, ImmutableString, Map, Position};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum RoutingFlowError {
    #[error("flow `{0}` was not found in configured flow paths")]
    NotFound(String),
    #[error("failed to read flow `{path}`: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to evaluate flow `{0}`: {1}")]
    Eval(String, Box<EvalAltResult>),
    #[error("invalid flow `{flow}`: {message}")]
    Invalid { flow: String, message: String },
}

#[derive(Clone, Debug, Default)]
struct FlowBuilder {
    default_provider: Option<String>,
    agents: Vec<ScriptAgentSpec>,
    handoffs: Vec<HandoffSpec>,
    entrypoint: Option<String>,
    safety_for: HashMap<String, SafetyChainSpec>,
}

/// Safety consult chain for a gated tool: the primary classifier and
/// an optional second-opinion agent for deny escalation
/// (REQ:codon/agent-shell-safety#c-deny-escalation).
#[derive(Clone, Debug)]
struct SafetyChainSpec {
    primary: String,
    escalation: Option<String>,
}

#[derive(Clone, Debug)]
struct ScriptAgentSpec {
    name: String,
    provider: Option<String>,
    model: String,
    system_prompt: Option<String>,
    user_prefix: Option<String>,
    temperature: Option<f32>,
    max_turns: Option<usize>,
    cache_system_prompt: Option<bool>,
    tools: Vec<String>,
}

#[derive(Clone, Debug)]
struct HandoffSpec {
    from: String,
    to: String,
    tool_name: String,
    description: String,
}

#[derive(Clone, Debug)]
pub struct RoutingFlow {
    name: Arc<str>,
    builder: FlowBuilder,
    shell_safety_fail_open: bool,
    permission_rules: Arc<Vec<ShellPermissionRule>>,
}

impl RoutingFlow {
    pub fn from_source(
        name: impl Into<Arc<str>>,
        source: &str,
        shell_safety_fail_open: bool,
    ) -> Result<Self, RoutingFlowError> {
        let name = name.into();
        let builder = evaluate_source(name.as_ref(), source)?;
        Ok(Self {
            name,
            builder,
            shell_safety_fail_open,
            permission_rules: Arc::new(Vec::new()),
        })
    }

    /// Attach the user's `[agent_harness] shell_permissions` rules —
    /// evaluated inside the deterministic gate pipeline
    /// (REQ:codon/agent-shell-safety#c-permission-rules).
    pub fn with_permission_rules(mut self, rules: Vec<ShellPermissionRule>) -> Self {
        self.permission_rules = Arc::new(rules);
        self
    }

    pub fn into_agents(self) -> Result<Vec<Arc<Agent>>, RoutingFlowError> {
        FlowCompiler::new(self).compile()
    }
}

pub(crate) fn reload_from_harness_settings(cx: &mut App, settings: &HarnessOverride) {
    let Some(flow_name) = settings
        .active_flow
        .as_deref()
        .map(str::trim)
        .filter(|flow| !flow.is_empty())
    else {
        AgentRegistry::clear_scripted_flow(cx);
        return;
    };

    match load_flow(
        flow_name,
        &settings.flow_paths,
        settings.shell_safety_fail_open,
    )
    .map(|flow| flow.with_permission_rules(settings.shell_permissions.clone()))
    .and_then(RoutingFlow::into_agents)
    {
        Ok(agents) => AgentRegistry::set_scripted_flow(cx, Arc::from(flow_name), agents),
        Err(err) => {
            log::warn!("codon-agent: failed to load routing flow `{flow_name}`: {err}");
            AgentRegistry::reapply_scripted_flow(cx);
            AgentRegistry::set_routing_error(cx, err.to_string());
        }
    }
}

fn load_flow(
    name: &str,
    flow_paths: &[String],
    shell_safety_fail_open: bool,
) -> Result<RoutingFlow, RoutingFlowError> {
    let path = resolve_flow_path(name, flow_paths)
        .ok_or_else(|| RoutingFlowError::NotFound(name.to_string()))?;
    let source = std::fs::read_to_string(&path).map_err(|source| RoutingFlowError::Read {
        path: path.clone(),
        source,
    })?;
    RoutingFlow::from_source(name.to_string(), &source, shell_safety_fail_open)
}

fn resolve_flow_path(name: &str, flow_paths: &[String]) -> Option<PathBuf> {
    let literal = expand_tilde(name);
    if literal.is_absolute() || name.contains(std::path::MAIN_SEPARATOR) {
        return literal.exists().then_some(literal);
    }

    let file_name = if name.ends_with(".rhai") {
        name.to_string()
    } else {
        format!("{name}.rhai")
    };
    let search_paths = if flow_paths.is_empty() {
        default_flow_paths()
    } else {
        flow_paths.iter().map(|path| expand_tilde(path)).collect()
    };
    search_paths
        .into_iter()
        .map(|dir| dir.join(&file_name))
        .find(|path| path.exists())
}

fn default_flow_paths() -> Vec<PathBuf> {
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
        return vec![PathBuf::from(config_home).join("codon").join("flows")];
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| vec![home.join(".config").join("codon").join("flows")])
        .unwrap_or_default()
}

fn expand_tilde(raw: &str) -> PathBuf {
    if raw == "~" {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    Path::new(raw).to_path_buf()
}

fn evaluate_source(flow_name: &str, source: &str) -> Result<FlowBuilder, RoutingFlowError> {
    let mut engine = Engine::new();
    engine.set_max_expr_depths(64, 64);
    engine.set_max_call_levels(32);
    engine.set_max_operations(20_000);
    // The operation limit bounds CPU but not allocation, and a fresh
    // `Engine` ships a filesystem module resolver plus `eval`. A flow is
    // a declaration file authored outside review, so also bound memory
    // and cut off host reach — otherwise a doubling loop OOM-aborts the
    // editor and `import`/`eval` escape the sandbox (#c-rhai-declarations).
    engine.set_max_string_size(64 * 1024);
    engine.set_max_array_size(4_096);
    engine.set_max_map_size(4_096);
    engine.set_module_resolver(rhai::module_resolvers::DummyModuleResolver::new());
    engine.disable_symbol("eval");
    engine.on_print(|text| log::debug!("codon-agent routing flow print: {text}"));
    engine.on_debug(|text, source, position| {
        log::debug!("codon-agent routing flow debug {source:?}:{position:?}: {text}");
    });

    let builder = Arc::new(Mutex::new(FlowBuilder::default()));

    {
        let builder = builder.clone();
        engine.register_fn(
            "provider",
            move |provider: &str| -> Result<(), Box<EvalAltResult>> {
                builder
                    .lock()
                    .map_err(|_| rhai_error("routing builder lock poisoned"))?
                    .default_provider = Some(provider.trim().to_string());
                Ok(())
            },
        );
    }

    {
        let builder = builder.clone();
        engine.register_fn(
            "agent",
            move |name: &str, options: Map| -> Result<(), Box<EvalAltResult>> {
                let spec = parse_agent(name, options)
                    .map_err(|err| rhai_error(format!("invalid agent `{name}`: {err}")))?;
                builder
                    .lock()
                    .map_err(|_| rhai_error("routing builder lock poisoned"))?
                    .agents
                    .push(spec);
                Ok(())
            },
        );
    }

    {
        let builder = builder.clone();
        engine.register_fn(
            "handoff",
            move |from: &str, to: &str, options: Map| -> Result<(), Box<EvalAltResult>> {
                let tool_name = string_option(&options, "name")
                    .map_err(rhai_error)?
                    .unwrap_or_else(|| format!("delegate_{to}"));
                let description = string_option(&options, "description")
                    .map_err(rhai_error)?
                    .unwrap_or_else(|| format!("Delegate a subtask to `{to}`."));
                builder
                    .lock()
                    .map_err(|_| rhai_error("routing builder lock poisoned"))?
                    .handoffs
                    .push(HandoffSpec {
                        from: from.to_string(),
                        to: to.to_string(),
                        tool_name,
                        description,
                    });
                Ok(())
            },
        );
    }

    {
        let builder = builder.clone();
        engine.register_fn(
            "entrypoint",
            move |name: &str| -> Result<(), Box<EvalAltResult>> {
                builder
                    .lock()
                    .map_err(|_| rhai_error("routing builder lock poisoned"))?
                    .entrypoint = Some(name.to_string());
                Ok(())
            },
        );
    }

    {
        let builder = builder.clone();
        engine.register_fn(
            "safety_for",
            move |tool: &str, agent: &str| -> Result<(), Box<EvalAltResult>> {
                builder
                    .lock()
                    .map_err(|_| rhai_error("routing builder lock poisoned"))?
                    .safety_for
                    .insert(
                        tool.to_string(),
                        SafetyChainSpec {
                            primary: agent.to_string(),
                            escalation: None,
                        },
                    );
                Ok(())
            },
        );
    }

    {
        // Three-arg overload: `safety_for(tool, primary, escalation)`
        // registers a second-opinion agent for deny escalation.
        let builder = builder.clone();
        engine.register_fn(
            "safety_for",
            move |tool: &str, agent: &str, escalation: &str| -> Result<(), Box<EvalAltResult>> {
                builder
                    .lock()
                    .map_err(|_| rhai_error("routing builder lock poisoned"))?
                    .safety_for
                    .insert(
                        tool.to_string(),
                        SafetyChainSpec {
                            primary: agent.to_string(),
                            escalation: Some(escalation.to_string()),
                        },
                    );
                Ok(())
            },
        );
    }

    engine
        .eval::<()>(&source)
        .map_err(|err| RoutingFlowError::Eval(flow_name.to_string(), err))?;
    let builder = builder
        .lock()
        .map_err(|_| RoutingFlowError::Invalid {
            flow: flow_name.to_string(),
            message: "routing builder lock poisoned".to_string(),
        })?
        .clone();
    Ok(builder)
}

fn parse_agent(name: &str, options: Map) -> Result<ScriptAgentSpec, String> {
    let model = string_option(&options, "model")?.unwrap_or_else(|| "default".to_string());
    Ok(ScriptAgentSpec {
        name: name.to_string(),
        provider: string_option(&options, "provider")?,
        model,
        system_prompt: string_option(&options, "prompt")?
            .or(string_option(&options, "system_prompt")?),
        user_prefix: string_option(&options, "user_prefix")?,
        temperature: f32_option(&options, "temperature")?,
        max_turns: usize_option(&options, "max_turns")?,
        cache_system_prompt: bool_option(&options, "cache_system_prompt")?,
        tools: string_array_option(&options, "tools")?.unwrap_or_default(),
    })
}

fn string_option(map: &Map, key: &str) -> Result<Option<String>, String> {
    let Some(value) = map.get(key) else {
        return Ok(None);
    };
    dynamic_string(value)
        .map(Some)
        .ok_or_else(|| format!("`{key}` must be a string"))
}

fn string_array_option(map: &Map, key: &str) -> Result<Option<Vec<String>>, String> {
    let Some(value) = map.get(key) else {
        return Ok(None);
    };
    let Some(array) = value.clone().try_cast::<Array>() else {
        return Err(format!("`{key}` must be an array of strings"));
    };
    array
        .iter()
        .map(|item| {
            dynamic_string(item).ok_or_else(|| format!("`{key}` must be an array of strings"))
        })
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn f32_option(map: &Map, key: &str) -> Result<Option<f32>, String> {
    let Some(value) = map.get(key) else {
        return Ok(None);
    };
    if let Some(float) = value.clone().try_cast::<FLOAT>() {
        return Ok(Some(float as f32));
    }
    if let Some(int) = value.clone().try_cast::<INT>() {
        return Ok(Some(int as f32));
    }
    Err(format!("`{key}` must be a number"))
}

fn usize_option(map: &Map, key: &str) -> Result<Option<usize>, String> {
    let Some(value) = map.get(key) else {
        return Ok(None);
    };
    let Some(int) = value.clone().try_cast::<INT>() else {
        return Err(format!("`{key}` must be an integer"));
    };
    usize::try_from(int)
        .map(Some)
        .map_err(|_| format!("`{key}` must be non-negative"))
}

fn bool_option(map: &Map, key: &str) -> Result<Option<bool>, String> {
    let Some(value) = map.get(key) else {
        return Ok(None);
    };
    value
        .clone()
        .try_cast::<bool>()
        .map(Some)
        .ok_or_else(|| format!("`{key}` must be a boolean"))
}

fn dynamic_string(value: &Dynamic) -> Option<String> {
    value
        .clone()
        .try_cast::<ImmutableString>()
        .map(|s| s.to_string())
}

fn rhai_error(message: impl Into<String>) -> Box<EvalAltResult> {
    Box::new(EvalAltResult::ErrorRuntime(
        Dynamic::from(message.into()),
        Position::NONE,
    ))
}

struct FlowCompiler {
    flow: RoutingFlow,
    specs: HashMap<String, ScriptAgentSpec>,
    handoffs_by_source: HashMap<String, Vec<HandoffSpec>>,
    built: HashMap<String, Arc<Agent>>,
    visiting: HashSet<String>,
}

impl FlowCompiler {
    fn new(flow: RoutingFlow) -> Self {
        let handoffs_by_source = flow.builder.handoffs.iter().cloned().fold(
            HashMap::<String, Vec<HandoffSpec>>::new(),
            |mut acc, handoff| {
                acc.entry(handoff.from.clone()).or_default().push(handoff);
                acc
            },
        );
        let specs = flow
            .builder
            .agents
            .iter()
            .cloned()
            .map(|spec| (spec.name.clone(), spec))
            .collect();
        Self {
            flow,
            specs,
            handoffs_by_source,
            built: HashMap::new(),
            visiting: HashSet::new(),
        }
    }

    fn compile(mut self) -> Result<Vec<Arc<Agent>>, RoutingFlowError> {
        if self.specs.is_empty() {
            return Err(self.invalid("flow must declare at least one agent"));
        }
        // `specs` is keyed by name, so a duplicate declaration would
        // silently shadow the earlier one. Reject it instead of picking
        // a winner the author never chose.
        if self.flow.builder.agents.len() != self.specs.len() {
            let mut seen = HashSet::new();
            let duplicate = self
                .flow
                .builder
                .agents
                .iter()
                .find(|spec| !seen.insert(spec.name.clone()))
                .map(|spec| spec.name.clone())
                .unwrap_or_default();
            return Err(self.invalid(format!("agent `{duplicate}` is declared more than once")));
        }
        if let Some(entrypoint) = &self.flow.builder.entrypoint
            && !self.specs.contains_key(entrypoint)
        {
            return Err(self.invalid(format!("entrypoint `{entrypoint}` is not an agent")));
        }
        for handoff in &self.flow.builder.handoffs {
            if !self.specs.contains_key(&handoff.from) {
                return Err(
                    self.invalid(format!("handoff source `{}` is not an agent", handoff.from))
                );
            }
            if !self.specs.contains_key(&handoff.to) {
                return Err(
                    self.invalid(format!("handoff target `{}` is not an agent", handoff.to))
                );
            }
        }
        for chain in self.flow.builder.safety_for.values() {
            if !self.specs.contains_key(&chain.primary) {
                return Err(
                    self.invalid(format!("safety agent `{}` is not an agent", chain.primary))
                );
            }
            if let Some(escalation) = &chain.escalation
                && !self.specs.contains_key(escalation)
            {
                return Err(self.invalid(format!(
                    "safety escalation agent `{escalation}` is not an agent"
                )));
            }
        }

        let names: Vec<String> = self.specs.keys().cloned().collect();
        for name in names {
            self.build_agent(&name)?;
        }
        Ok(self.built.into_values().collect())
    }

    fn build_agent(&mut self, name: &str) -> Result<Arc<Agent>, RoutingFlowError> {
        if let Some(agent) = self.built.get(name) {
            return Ok(agent.clone());
        }
        if !self.visiting.insert(name.to_string()) {
            return Err(self.invalid(format!("delegation cycle includes `{name}`")));
        }

        let spec = self
            .specs
            .get(name)
            .cloned()
            .ok_or_else(|| self.invalid(format!("unknown agent `{name}`")))?;
        let mut tools = ToolSet::new();
        for tool in &spec.tools {
            match tool.as_str() {
                "shell" | "shell_command" => {
                    let (primary, escalation) = self.shell_safety_chain()?;
                    tools.push(Arc::new(ShellCommandTool::new(
                        Arc::from(name),
                        primary,
                        escalation,
                        self.flow.shell_safety_fail_open,
                        self.flow.permission_rules.clone(),
                    )))
                }
                "delegate" => {}
                other => return Err(self.invalid(format!("unknown tool `{other}` on `{name}`"))),
            }
        }

        for handoff in self
            .handoffs_by_source
            .get(name)
            .cloned()
            .unwrap_or_default()
        {
            let target = self.build_agent(&handoff.to)?;
            tools.push(Arc::new(
                DelegateTool::new(handoff.tool_name, handoff.description, target)
                    .with_parent_agent(Arc::from(name)),
            ));
        }

        let mut builder = Agent::builder(
            spec.name.clone(),
            Arc::new(ZedModelClient::new(ModelSpec::parse(
                &self.model_selector(&spec),
            ))),
        )
        .flow(self.flow.name.clone())
        .tools(tools);
        if let Some(system_prompt) = spec.system_prompt {
            builder = builder.system_prompt(system_prompt);
        }
        if let Some(user_prefix) = spec.user_prefix {
            builder = builder.user_prefix(user_prefix);
        }
        if let Some(temperature) = spec.temperature {
            builder = builder.temperature(temperature);
        }
        if let Some(max_turns) = spec.max_turns {
            builder = builder.max_turns(max_turns);
        }
        if let Some(cache_system_prompt) = spec.cache_system_prompt {
            builder = builder.cache_system_prompt(cache_system_prompt);
        }
        let agent = Arc::new(builder.build());
        self.visiting.remove(name);
        self.built.insert(name.to_string(), agent.clone());
        Ok(agent)
    }

    fn shell_safety_chain(&self) -> Result<(Arc<str>, Option<Arc<str>>), RoutingFlowError> {
        self.flow
            .builder
            .safety_for
            .get("shell")
            .or_else(|| self.flow.builder.safety_for.get("shell_command"))
            .map(|chain| {
                (
                    Arc::from(chain.primary.as_str()),
                    chain
                        .escalation
                        .as_deref()
                        .map(|escalation| Arc::from(escalation)),
                )
            })
            .ok_or_else(|| self.invalid("shell tool requires `safety_for(\"shell\", <agent>)`"))
    }

    fn model_selector(&self, spec: &ScriptAgentSpec) -> String {
        let Some(provider) = spec
            .provider
            .as_ref()
            .or(self.flow.builder.default_provider.as_ref())
            .map(|provider| provider.trim())
            .filter(|provider| !provider.is_empty())
        else {
            return spec.model.clone();
        };
        if spec.model == "default" || spec.model.starts_with(&format!("{provider}/")) {
            spec.model.clone()
        } else {
            format!("{provider}/{}", spec.model)
        }
    }

    fn invalid(&self, message: impl Into<String>) -> RoutingFlowError {
        RoutingFlowError::Invalid {
            flow: self.flow.name.to_string(),
            message: message.into(),
        }
    }
}

/// Byte cap on the combined stdout+stderr a shell command may return
/// to the model (REQ:codon/agent-shell-safety#c-execution).
const SHELL_OUTPUT_CAP: usize = 32 * 1024;

pub struct ShellCommandTool {
    owner_agent: Arc<str>,
    safety_agent: Arc<str>,
    escalation_agent: Option<Arc<str>>,
    fail_open: bool,
    permission_rules: Arc<Vec<ShellPermissionRule>>,
    /// Trace annotation for the most recent `run`, taken by the agent
    /// loop via `take_trace_safety_decision`. Metadata-only: a rare
    /// concurrent turn sharing this tool can at worst swap two
    /// summaries, never leak command bytes.
    last_decision: Mutex<Option<String>>,
}

/// One safety-agent consult, separated by how it can fail: an
/// unusable reply and an unreachable agent both resolve to the
/// fail-safe `ask`, but with different reasons.
enum ConsultOutcome {
    Verdict(SafetyVerdict),
    InvalidReply,
    Unavailable(String),
}

impl ShellCommandTool {
    pub fn new(
        owner_agent: Arc<str>,
        safety_agent: Arc<str>,
        escalation_agent: Option<Arc<str>>,
        fail_open: bool,
        permission_rules: Arc<Vec<ShellPermissionRule>>,
    ) -> Self {
        Self {
            owner_agent,
            safety_agent,
            escalation_agent,
            fail_open,
            permission_rules,
            last_decision: Mutex::new(None),
        }
    }

    fn record(&self, verdict: &SafetyVerdict) {
        match self.last_decision.lock() {
            Ok(mut slot) => *slot = Some(verdict.trace_summary()),
            Err(err) => log::warn!("codon-agent: shell trace slot poisoned: {err}"),
        }
    }

    /// Consult a safety agent with the tool-side contract prompt.
    /// Attribute the consult turn to the requesting agent so the trace
    /// ties the decision back to its parent (#c-monitoring).
    /// Cancellation propagates; every other failure becomes a
    /// [`ConsultOutcome`] the caller folds into the fail-safe path.
    async fn consult(
        &self,
        agent_name: &Arc<str>,
        command: &str,
        cwd: Option<&str>,
        intent: Option<&str>,
        second_opinion: bool,
        cancel: &CancelToken,
        cx: &AsyncApp,
    ) -> Result<ConsultOutcome, ToolError> {
        let Some(agent) = cx.update(|app| AgentRegistry::get(app, agent_name.as_ref())) else {
            return Ok(ConsultOutcome::Unavailable(format!(
                "agent `{agent_name}` is not registered"
            )));
        };
        let prompt = safety::contract_prompt(command, cwd, intent, second_opinion);
        match agent
            .run_as_child(&prompt, cancel.clone(), cx, self.owner_agent.clone())
            .await
        {
            Ok(outcome) => {
                let source = if second_opinion {
                    SafetySource::Escalation
                } else {
                    SafetySource::Classifier
                };
                Ok(match safety::parse_verdict_reply(&outcome.text, source) {
                    Some(verdict) => ConsultOutcome::Verdict(verdict),
                    None => ConsultOutcome::InvalidReply,
                })
            }
            Err(crate::runtime::error::AgentError::Cancelled) => Err(ToolError::Cancelled),
            Err(err) => Ok(ConsultOutcome::Unavailable(err.to_string())),
        }
    }

    /// Run the full pipeline for one command: deterministic gates,
    /// classifier consult, deny escalation, fail-safe `ask`.
    async fn resolve_verdict(
        &self,
        command: &str,
        cwd: Option<&str>,
        intent: Option<&str>,
        cancel: &CancelToken,
        cx: &AsyncApp,
    ) -> Result<SafetyVerdict, ToolError> {
        if let Some(verdict) = safety::deterministic_verdict(command, &self.permission_rules) {
            return Ok(verdict);
        }
        let first = self
            .consult(&self.safety_agent, command, cwd, intent, false, cancel, cx)
            .await?;
        let verdict = match first {
            ConsultOutcome::Verdict(first) if first.decision == SafetyDecision::Deny => {
                // An LLM deny is never final on its own
                // (#c-deny-escalation).
                match &self.escalation_agent {
                    Some(escalation) => match self
                        .consult(escalation, command, cwd, intent, true, cancel, cx)
                        .await?
                    {
                        ConsultOutcome::Verdict(second) => {
                            safety::apply_escalation_policy(&first, &second)
                        }
                        ConsultOutcome::InvalidReply => safety::fail_safe_ask(
                            format!(
                                "classifier denied ({}); second opinion returned an unusable reply",
                                first.reason
                            ),
                            "invalid-classifier-response",
                        ),
                        ConsultOutcome::Unavailable(reason) => safety::fail_safe_ask(
                            format!(
                                "classifier denied ({}); second opinion unavailable: {reason}",
                                first.reason
                            ),
                            "classifier-unavailable",
                        ),
                    },
                    None => safety::unescalated_deny_to_ask(&first),
                }
            }
            ConsultOutcome::Verdict(first) => first,
            ConsultOutcome::InvalidReply => safety::fail_safe_ask(
                "safety classifier returned an unusable reply",
                "invalid-classifier-response",
            ),
            ConsultOutcome::Unavailable(reason) => safety::fail_safe_ask(
                format!("safety classifier unavailable: {reason}"),
                "classifier-unavailable",
            ),
        };
        Ok(verdict)
    }
}

#[async_trait::async_trait(?Send)]
impl Tool for ShellCommandTool {
    fn name(&self) -> &str {
        "shell_command"
    }

    fn description(&self) -> &str {
        "Run a shell command. The command passes a layered safety pipeline (deterministic \
         gates, then a safety-agent consult) and only executes once allowed. Provide a one-line \
         `description` stating why you are running it — the safety layer weighs it as evidence."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to run." },
                "cwd": { "type": "string", "description": "Working directory for the command." },
                "description": {
                    "type": "string",
                    "description": "One short sentence stating WHY this command is being run; surfaced to the safety layer as intent."
                }
            },
            "required": ["command"]
        })
    }

    fn trace_args(&self, input: &serde_json::Value) -> serde_json::Value {
        // The command and the stated intent may carry credentials —
        // record only size/presence, never bytes (#c-monitoring,
        // #c-intent). `cwd` is a path, kept as metadata.
        let mut redacted = serde_json::Map::new();
        if let Some(command) = input.get("command").and_then(|value| value.as_str()) {
            redacted.insert(
                "command".to_string(),
                serde_json::Value::String(format!("<redacted {}B>", command.len())),
            );
        }
        if input
            .get("description")
            .and_then(|value| value.as_str())
            .is_some()
        {
            redacted.insert(
                "description".to_string(),
                serde_json::Value::String("<intent provided>".to_string()),
            );
        }
        if let Some(cwd) = input.get("cwd") {
            redacted.insert("cwd".to_string(), cwd.clone());
        }
        serde_json::Value::Object(redacted)
    }

    fn take_trace_safety_decision(&self) -> Option<String> {
        self.last_decision
            .lock()
            .ok()
            .and_then(|mut slot| slot.take())
    }

    async fn run(
        &self,
        input: serde_json::Value,
        cancel: CancelToken,
        cx: AsyncApp,
    ) -> Result<String, ToolError> {
        let command = input
            .get("command")
            .and_then(|value| value.as_str())
            .ok_or_else(|| ToolError::BadInput("expected `command: string`".to_string()))?;
        let cwd = input
            .get("cwd")
            .and_then(|value| value.as_str())
            .filter(|cwd| !cwd.is_empty());
        let intent = input
            .get("description")
            .and_then(|value| value.as_str())
            .filter(|intent| !intent.is_empty());

        let mut verdict = self
            .resolve_verdict(command, cwd, intent, &cancel, &cx)
            .await?;

        // Dev mode collapses `ask` to allow (#c-ask-decision). The
        // hard-deny layers produce `deny`, never `ask`, so they are
        // structurally out of fail-open's reach.
        if verdict.decision == SafetyDecision::Ask && self.fail_open {
            verdict.decision = SafetyDecision::Allow;
            verdict.source = SafetySource::FailOpen;
            verdict.categories.push("fail-open".to_string());
        }
        self.record(&verdict);

        match verdict.decision {
            SafetyDecision::Deny => Err(ToolError::SafetyDenied(verdict.reason)),
            SafetyDecision::Ask => Err(ToolError::SafetyDenied(format!(
                "requires user approval (approval overlay pending — TASK:phase-23/shell-ask-overlay): {}",
                verdict.reason
            ))),
            SafetyDecision::Allow => execute_shell(command, cwd, &cancel).await,
        }
    }
}

/// Execute an approved command via `/bin/sh -c` — never the user's
/// interactive shell — honoring `cwd`, killing the child on
/// cancellation, and capping combined output
/// (REQ:codon/agent-shell-safety#c-execution).
async fn execute_shell(
    command: &str,
    cwd: Option<&str>,
    cancel: &CancelToken,
) -> Result<String, ToolError> {
    let mut child = {
        let mut spawn = smol::process::Command::new("/bin/sh");
        spawn
            .arg("-c")
            .arg(command)
            .stdin(smol::process::Stdio::null())
            .stdout(smol::process::Stdio::piped())
            .stderr(smol::process::Stdio::piped());
        if let Some(cwd) = cwd {
            spawn.current_dir(cwd);
        }
        spawn
            .spawn()
            .map_err(|err| ToolError::Failed(anyhow!("failed to spawn `/bin/sh`: {err}")))?
    };

    let mut stdout_pipe = child.stdout.take();
    let mut stderr_pipe = child.stderr.take();

    let completed = {
        // Read both pipes concurrently — draining them one after the
        // other can deadlock a child that fills the second pipe's
        // buffer while the first is still open.
        let io = async {
            let stdout_read = async {
                let mut bytes = Vec::new();
                if let Some(pipe) = stdout_pipe.as_mut()
                    && let Err(err) = pipe.read_to_end(&mut bytes).await
                {
                    log::debug!("codon-agent: shell stdout read ended early: {err}");
                }
                bytes
            };
            let stderr_read = async {
                let mut bytes = Vec::new();
                if let Some(pipe) = stderr_pipe.as_mut()
                    && let Err(err) = pipe.read_to_end(&mut bytes).await
                {
                    log::debug!("codon-agent: shell stderr read ended early: {err}");
                }
                bytes
            };
            let (stdout, stderr) = futures::join!(stdout_read, stderr_read);
            let status = child.status().await;
            (stdout, stderr, status)
        };
        let mut io = std::pin::pin!(io.fuse());
        let mut cancel_fired = std::pin::pin!(cancel.cancelled().fuse());
        select_biased! {
            _ = cancel_fired => None,
            done = io => Some(done),
        }
    };

    let Some((stdout, stderr, status)) = completed else {
        if let Err(err) = child.kill() {
            log::warn!("codon-agent: failed to kill cancelled shell command: {err}");
        }
        return Err(ToolError::Cancelled);
    };
    let status =
        status.map_err(|err| ToolError::Failed(anyhow!("failed to await shell command: {err}")))?;
    Ok(render_shell_result(&stdout, &stderr, status.code()))
}

/// Combined, trimmed, byte-capped output plus an exit line the model
/// always sees.
fn render_shell_result(stdout: &[u8], stderr: &[u8], exit_code: Option<i32>) -> String {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    let mut body = String::new();
    if !stdout.trim().is_empty() {
        body.push_str(stdout.trim());
    }
    if !stderr.trim().is_empty() {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(stderr.trim());
    }
    if body.len() > SHELL_OUTPUT_CAP {
        let mut end = SHELL_OUTPUT_CAP;
        while !body.is_char_boundary(end) {
            end -= 1;
        }
        body.truncate(end);
        body.push_str("\n[truncated: output exceeded 32 KiB]");
    }
    let exit_line = match exit_code {
        Some(code) => format!("[exit code: {code}]"),
        None => "[terminated by signal]".to_string(),
    };
    if body.is_empty() {
        format!("(no output)\n{exit_line}")
    } else {
        format!("{body}\n{exit_line}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mirrors the documented example flow in assets/config/codon.example.toml
    // so the two cannot drift silently.
    const BASIC_FLOW: &str = r#"
        provider("openrouter");
        agent("main", #{ model: "z-ai/glm-5.2", prompt: "main", tools: ["shell"] });
        agent("reckoning", #{ model: "deepseek/deepseek-4-flash", prompt: "critique" });
        agent("edit_applier", #{ model: "morph/morph-v3-large", prompt: "edit" });
        agent("safety", #{ provider: "ollama", model: "qwen3", prompt: "safety" });
        handoff("main", "reckoning", #{ name: "ask_reckoning", description: "Critique a plan." });
        handoff("main", "edit_applier", #{ name: "apply_edits", description: "Apply edits." });
        safety_for("shell", "safety");
        entrypoint("main");
    "#;

    #[test]
    fn parses_default_flow_and_builds_agents() {
        let flow = RoutingFlow::from_source("default", BASIC_FLOW, false).expect("valid flow");
        let agents = flow.into_agents().expect("agents build");
        let names: HashSet<String> = agents.iter().map(|agent| agent.name.to_string()).collect();
        assert!(names.contains("main"));
        assert!(names.contains("reckoning"));
        assert!(names.contains("edit_applier"));
        assert!(names.contains("safety"));
        let main = agents
            .iter()
            .find(|agent| agent.name.as_ref() == "main")
            .expect("main agent");
        // shell tool + two delegation handoffs.
        assert_eq!(main.tools.len(), 3);
        assert!(main.tools.iter().any(|tool| tool.name() == "shell_command"));
        assert_eq!(main.flow.as_deref(), Some("default"));
    }

    #[test]
    fn rejects_unknown_handoff_target() {
        let err = RoutingFlow::from_source(
            "bad",
            r#"
                agent("main", #{ model: "default" });
                handoff("main", "missing", #{});
            "#,
            false,
        )
        .and_then(RoutingFlow::into_agents)
        .err()
        .expect("target is missing");
        assert!(err.to_string().contains("handoff target `missing`"));
    }

    #[test]
    fn shell_tool_requires_safety_agent() {
        let err = RoutingFlow::from_source(
            "bad",
            r#"agent("main", #{ model: "default", tools: ["shell"] });"#,
            false,
        )
        .and_then(RoutingFlow::into_agents)
        .err()
        .expect("safety agent missing");
        assert!(err.to_string().contains("shell tool requires"));
    }

    #[test]
    fn shell_command_trace_args_redacts_command_and_intent() {
        let tool = ShellCommandTool::new(
            Arc::from("main"),
            Arc::from("safety"),
            None,
            false,
            Arc::new(Vec::new()),
        );
        let input = serde_json::json!({
            "command": "curl -u carlo:hunter2 https://internal/api",
            "cwd": "/home/carlo/proj",
            "description": "upload the hunter2 report"
        });
        let rendered = serde_json::to_string(&tool.trace_args(&input)).expect("serialize");
        assert!(
            !rendered.contains("hunter2"),
            "credential leaked: {rendered}"
        );
        assert!(!rendered.contains("curl"), "command leaked: {rendered}");
        assert!(!rendered.contains("upload"), "intent leaked: {rendered}");
        assert!(rendered.contains("<redacted"));
        assert!(rendered.contains("<intent provided>"));
        assert!(rendered.contains("/home/carlo/proj"), "cwd should survive");
    }

    #[test]
    fn three_arg_safety_for_registers_an_escalation_agent() {
        let flow = RoutingFlow::from_source(
            "escalated",
            r#"
                agent("main", #{ model: "default", tools: ["shell"] });
                agent("safety", #{ model: "default", prompt: "classify" });
                agent("second", #{ model: "default", prompt: "re-examine" });
                safety_for("shell", "safety", "second");
            "#,
            false,
        )
        .expect("valid flow");
        let agents = flow.into_agents().expect("agents build");
        assert!(agents.iter().any(|agent| agent.name.as_ref() == "second"));
    }

    #[test]
    fn unknown_escalation_agent_is_rejected() {
        let err = RoutingFlow::from_source(
            "bad",
            r#"
                agent("main", #{ model: "default", tools: ["shell"] });
                agent("safety", #{ model: "default" });
                safety_for("shell", "safety", "missing");
            "#,
            false,
        )
        .and_then(RoutingFlow::into_agents)
        .err()
        .expect("escalation agent is missing");
        assert!(
            err.to_string()
                .contains("safety escalation agent `missing`")
        );
    }

    #[test]
    fn shell_result_reports_output_and_exit_code() {
        let rendered = render_shell_result(b"hello\n", b"", Some(0));
        assert_eq!(rendered, "hello\n[exit code: 0]");

        let rendered = render_shell_result(b"", b"boom\n", Some(2));
        assert_eq!(rendered, "boom\n[exit code: 2]");

        let rendered = render_shell_result(b"", b"", Some(0));
        assert_eq!(rendered, "(no output)\n[exit code: 0]");

        let rendered = render_shell_result(b"out\n", b"err\n", None);
        assert_eq!(rendered, "out\nerr\n[terminated by signal]");
    }

    #[test]
    fn shell_result_caps_oversized_output() {
        let big = vec![b'x'; SHELL_OUTPUT_CAP + 4096];
        let rendered = render_shell_result(&big, b"", Some(0));
        assert!(rendered.contains("[truncated: output exceeded 32 KiB]"));
        assert!(rendered.ends_with("[exit code: 0]"));
        assert!(rendered.len() < SHELL_OUTPUT_CAP + 256);
    }

    #[test]
    fn rejects_duplicate_agent_names() {
        let err = RoutingFlow::from_source(
            "dup",
            r#"
                agent("main", #{ model: "a" });
                agent("main", #{ model: "b" });
            "#,
            false,
        )
        .and_then(RoutingFlow::into_agents)
        .err()
        .expect("duplicate agent name must be rejected");
        assert!(err.to_string().contains("declared more than once"));
    }

    #[test]
    fn sandbox_rejects_unbounded_string_growth() {
        let err = RoutingFlow::from_source("bad", r#"let s = "x"; loop { s += s; }"#, false)
            .err()
            .expect("string growth must abort inside the sandbox, not the process");
        assert!(matches!(err, RoutingFlowError::Eval(..)));
    }

    #[test]
    fn sandbox_rejects_module_import() {
        let err = RoutingFlow::from_source(
            "bad",
            r#"import "std" as std; agent("main", #{ model: "default" });"#,
            false,
        )
        .err()
        .expect("import must be rejected");
        assert!(matches!(err, RoutingFlowError::Eval(..)));
    }

    #[test]
    fn sandbox_disables_eval() {
        let err = RoutingFlow::from_source("bad", r#"eval("1 + 1");"#, false)
            .err()
            .expect("eval must be disabled");
        assert!(matches!(err, RoutingFlowError::Eval(..)));
    }

    #[test]
    fn resolve_flow_path_honors_precedence_and_absolute_paths() {
        let first = tempfile::tempdir().expect("tempdir");
        let second = tempfile::tempdir().expect("tempdir");
        let paths = vec![
            first.path().to_string_lossy().into_owned(),
            second.path().to_string_lossy().into_owned(),
        ];

        // Present only in the second path: resolution falls through to it.
        let in_second = second.path().join("demo.rhai");
        std::fs::write(&in_second, "agent(\"m\", #{});").expect("write");
        assert_eq!(resolve_flow_path("demo", &paths).as_ref(), Some(&in_second));

        // Same name in the first path now shadows the second.
        let in_first = first.path().join("demo.rhai");
        std::fs::write(&in_first, "agent(\"m\", #{});").expect("write");
        assert_eq!(resolve_flow_path("demo", &paths).as_ref(), Some(&in_first));

        // An absolute path to an existing file resolves directly.
        assert_eq!(
            resolve_flow_path(&in_second.to_string_lossy(), &[]).as_ref(),
            Some(&in_second)
        );

        // A name present in no path resolves to nothing.
        assert!(resolve_flow_path("absent", &paths).is_none());
    }
}
