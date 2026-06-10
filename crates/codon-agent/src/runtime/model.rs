//! The model-client trait boundary. Codon's agent runtime calls
//! providers exclusively through [`ModelClient`]. The shipped impl
//! ([`ZedModelClient`]) wraps Zed's `LanguageModel` so we inherit
//! every provider that vendored Zed already speaks (Anthropic, OpenAI,
//! Google, Ollama, Bedrock, …) without re-implementing HTTP.
//!
//! Why the indirection: the harness REQ
//! (REQ:codon/agent-harness#c-no-vendor-lock) forbids direct provider
//! references outside the trait surface. A future swap (forge, a
//! custom client, a stub for tests) is a one-file change.

use crate::runtime::error::AgentError;
use anyhow::Result;
use futures::FutureExt as _;
use futures::future::Shared;
use futures::stream::BoxStream;
use gpui::{App, AsyncApp, Global, Task};
use language_model::{
    LanguageModel, LanguageModelCompletionError, LanguageModelCompletionEvent,
    LanguageModelRegistry, LanguageModelRequest,
};
use std::sync::Arc;
use std::time::Duration;

/// How a caller asks the registry for a model.
#[derive(Clone, Debug, Default)]
pub enum ModelSpec {
    /// `provider_id/model_id` — exact pin. Strongest selector;
    /// silently falls through to the next strategy if no match.
    Qualified { provider: String, model: String },
    /// Bare model id. Matched first by exact id across all
    /// authenticated providers, then by id-prefix so e.g.
    /// `claude-haiku-4-5` matches `claude-haiku-4-5-latest`.
    Bare(String),
    /// Whatever the user has wired as the workspace default in Zed
    /// settings. The fallback for every other spec.
    #[default]
    Default,
}

impl ModelSpec {
    /// Parse a `model = "..."` string from `codon.toml`. Accepts
    /// `provider/id`, bare id, or the literal `"default"`.
    pub fn parse(raw: &str) -> Self {
        let raw = raw.trim();
        if raw.is_empty() || raw == "default" {
            return Self::Default;
        }
        if let Some((provider, model)) = raw.split_once('/') {
            return Self::Qualified {
                provider: provider.to_string(),
                model: model.to_string(),
            };
        }
        Self::Bare(raw.to_string())
    }
}

/// The startup provider-authentication pass, kept as a shared future
/// so any agent flow can await its completion. Zed only authenticates
/// providers when the agent panel first builds its model list — codon
/// agent flows (the fish `#@` completer fires earliest) must resolve a
/// model without the panel ever opening, so codon runs the same pass
/// at startup and gates model resolution on it.
struct ProviderAuthPass(Shared<Task<()>>);

impl Global for ProviderAuthPass {}

/// Kick off background authentication of every registered language-
/// model provider. Called once from `codon_agent::init`, after
/// `language_models::init` has registered the providers. No-op when
/// the registry global is absent (tests with stub model clients) or
/// when the pass already started.
pub fn start_provider_authentication(cx: &mut App) {
    if cx.has_global::<ProviderAuthPass>() || LanguageModelRegistry::try_global(cx).is_none() {
        return;
    }
    let pass = language_models::authenticate_all_providers(cx).shared();
    cx.set_global(ProviderAuthPass(pass));
}

/// Await the startup authentication pass, bounded so a hung provider
/// probe can't stall a turn. Resolves immediately once the pass has
/// completed; no-op when it was never started.
pub async fn wait_for_provider_authentication(cx: &AsyncApp) {
    let Some(pass) = cx.update(|app| app.try_global::<ProviderAuthPass>().map(|p| p.0.clone()))
    else {
        return;
    };
    let mut pass = std::pin::pin!(pass.fuse());
    let mut timeout = std::pin::pin!(
        cx.background_executor()
            .timer(Duration::from_secs(10))
            .fuse()
    );
    futures::select_biased! {
        _ = pass => {}
        _ = timeout => {}
    }
}

/// Resolve a [`ModelSpec`] against Zed's global registry. Returns
/// `None` when nothing matches — callers turn that into
/// `AgentError::NoModelAvailable` at the boundary.
pub fn pick_zed_model(app: &App, spec: &ModelSpec) -> Option<Arc<dyn LanguageModel>> {
    let registry = LanguageModelRegistry::read_global(app);
    let lookup_exact = |provider: &str, model: &str| {
        registry
            .available_models(app)
            .find(|m| m.provider_id().0.as_ref() == provider && m.id().0.as_ref() == model)
    };
    let lookup_bare = |needle: &str| -> Option<Arc<dyn LanguageModel>> {
        registry
            .available_models(app)
            .find(|m| m.id().0.as_ref() == needle)
            .or_else(|| {
                registry
                    .available_models(app)
                    .find(|m| m.id().0.as_ref().starts_with(needle))
            })
    };
    match spec {
        ModelSpec::Qualified { provider, model } => lookup_exact(provider, model)
            .or_else(|| lookup_bare(model))
            .or_else(|| registry.default_model().map(|c| c.model)),
        ModelSpec::Bare(needle) => {
            lookup_bare(needle).or_else(|| registry.default_model().map(|c| c.model))
        }
        ModelSpec::Default => registry.default_model().map(|c| c.model),
    }
}

/// Codon's trait boundary over a single-turn streaming model call.
/// The agent runtime owns the multi-turn loop; this trait only
/// requires "send one request, stream one response."
pub trait ModelClient: Send + Sync {
    /// A stable identifier shown in traces. Typically
    /// `provider/model`. Not load-bearing.
    fn id(&self) -> Arc<str>;

    /// Stream a single completion. Implementations may freely use
    /// `cx` for async-app interaction; the runtime always calls this
    /// from a foreground spawn.
    fn stream_completion(
        &self,
        request: LanguageModelRequest,
        cx: &AsyncApp,
    ) -> futures::future::BoxFuture<
        'static,
        Result<
            BoxStream<'static, Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>,
            LanguageModelCompletionError,
        >,
    >;
}

/// Resolve-on-call adapter over a [`ModelSpec`]. Picking the model
/// late means `codon.toml` reloads + workspace-default changes show
/// up on the next turn without reconstructing the agent.
pub struct ZedModelClient {
    pub spec: ModelSpec,
}

impl ZedModelClient {
    pub fn new(spec: ModelSpec) -> Self {
        Self { spec }
    }

    pub fn resolve(&self, app: &App) -> Result<Arc<dyn LanguageModel>, AgentError> {
        pick_zed_model(app, &self.spec).ok_or(AgentError::NoModelAvailable)
    }
}

impl ModelClient for ZedModelClient {
    fn id(&self) -> Arc<str> {
        match &self.spec {
            ModelSpec::Qualified { provider, model } => Arc::from(format!("{provider}/{model}")),
            ModelSpec::Bare(id) => Arc::from(id.as_str()),
            ModelSpec::Default => Arc::from("default"),
        }
    }

    fn stream_completion(
        &self,
        request: LanguageModelRequest,
        cx: &AsyncApp,
    ) -> futures::future::BoxFuture<
        'static,
        Result<
            BoxStream<'static, Result<LanguageModelCompletionEvent, LanguageModelCompletionError>>,
            LanguageModelCompletionError,
        >,
    > {
        // Resolve the model on the foreground thread before constructing
        // the future. The returned `BoxFuture` from Zed's
        // `stream_completion` is already `Send`; capturing `cx` here
        // would taint our future with the `!Send` AsyncApp.
        let Some(model) = cx.update(|app| pick_zed_model(app, &self.spec)) else {
            return Box::pin(async {
                Err(LanguageModelCompletionError::Other(anyhow::anyhow!(
                    "no language model is available — open the agent panel and configure one"
                )))
            });
        };
        model.stream_completion(request, cx)
    }
}
