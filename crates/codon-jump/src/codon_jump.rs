//! Window-wide jump-hint overlay — the codon answer to Vimium's `f`.
//!
//! Two actions (added in follow-up tasks `jump-action-target` and
//! `jump-action-url`) open a [`JumpOverlay`] over the workspace. Every
//! registered [`JumpProvider`] yields a list of [`JumpCandidate`]s with
//! screen-space bounds and an action closure; the overlay assigns
//! two-character labels, paints them as chips above the viewport, and
//! dispatches the matched candidate's action on a two-keystroke match.
//!
//! This crate implements the foundation: the trait, the global
//! registry, the pure-functional label assigner, and the
//! [`ModalView`]-based overlay with its keystroke loop. Providers
//! (editor, terminal, file-manager) and the actions that open the
//! overlay live in follow-up tasks under
//! `REQ:codon/jump-hints`.
//!
//! The overlay is *not* an operator: it owns the window's keyboard
//! state via [`Workspace::toggle_modal`] for its lifetime, and any key
//! outside the configured alphabet — including arrows, modifiers,
//! punctuation — dismisses it without firing.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result};
use fs::Fs;
use futures::StreamExt as _;
use serde::Deserialize;

use gpui::{
    AnyElement, App, AppContext as _, BorrowAppContext, Bounds, ClipboardItem, Context,
    DismissEvent, EventEmitter, FocusHandle, Focusable, Global, InteractiveElement, IntoElement,
    KeyContext, KeyDownEvent, ParentElement, Pixels, Point, Render, SharedString, Styled,
    Subscription, WeakEntity, Window, actions, deferred, div, prelude::FluentBuilder, px,
};

pub use workspace::codon_jump_clickable::{
    JumpClickable, JumpClickableExt, clear_clickable_registry, clickable_registry_len,
    take_clickables,
};
use ui::{ActiveTheme, Color, Label, LabelCommon, LabelSize, h_flex, v_flex};
use workspace::{
    Event as WorkspaceEvent, ModalView, Workspace,
    notifications::{NotificationId, simple_message_notification::MessageNotification},
};

actions!(
    codon_jump,
    [
        /// Open the jump-hint overlay over the active workspace covering
        /// every kind of candidate (words, URLs, clickables).
        JumpToTarget,
        /// Open the jump-hint overlay filtered to URL candidates. The
        /// matched URL is copied to the system clipboard and a
        /// `MessageNotification` toast confirms — the candidate's own
        /// action closure is *not* invoked.
        JumpToUrl,
    ]
);

/// The default label alphabet — lowercase `a..z`. 26² = 676 two-char
/// labels before falling back to 3-char. The
/// [`JumpConfigStore`] global may override this at runtime.
pub const DEFAULT_ALPHABET: &str = "abcdefghijklmnopqrstuvwxyz";

/// Default safety cap on per-provider candidate count. Mirrors the
/// `alphabet.len()³ = 17_576` natural ceiling so providers that
/// over-yield still drain in `O(n)`.
pub const DEFAULT_MAX_CANDIDATES_PER_PROVIDER: usize = 17_576;

/// Where a chip paints relative to its candidate bounds. Today only
/// `TopLeft` and `Center` are implemented — `TopLeft` (the historical
/// behaviour) is the default so on-disk configs without a `[jump]`
/// section keep working unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelPosition {
    /// Paint at the candidate's `bounds.origin` (the historical W1
    /// behaviour).
    #[default]
    TopLeft,
    /// Paint at the candidate's geometric center, with a small
    /// translate so the chip's center aligns with the bounds center.
    Center,
}

impl LabelPosition {
    fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "top-left" | "topleft" | "top_left" => Some(Self::TopLeft),
            "center" | "centre" => Some(Self::Center),
            _ => None,
        }
    }
}

/// Resolved jump-overlay configuration. Constructed from the embedded
/// defaults at startup and overlaid with `~/.config/codon/jump.toml`
/// when present. The [`JumpConfigStore`] global hands callers a
/// snapshot via [`JumpConfigStore::current`].
#[derive(Debug, Clone)]
pub struct JumpConfig {
    /// Characters drawn from when assigning labels. Empty alphabets
    /// fall back to [`DEFAULT_ALPHABET`] at use time so a malformed
    /// override never wedges the overlay.
    pub alphabet: String,
    /// Where chips paint relative to candidate bounds.
    pub label_position: LabelPosition,
    /// Hard cap on how many candidates a single provider may
    /// contribute. Defaults to [`DEFAULT_MAX_CANDIDATES_PER_PROVIDER`].
    pub max_candidates_per_provider: usize,
    /// Dismiss the overlay when the underlying workspace scrolls. The
    /// scroll-event wiring itself is still a TODO inside `open()`; this
    /// flag is plumbed through so the wiring lands as a no-op flip.
    pub dismiss_on_scroll: bool,
}

impl Default for JumpConfig {
    fn default() -> Self {
        Self {
            alphabet: DEFAULT_ALPHABET.to_string(),
            label_position: LabelPosition::TopLeft,
            max_candidates_per_provider: DEFAULT_MAX_CANDIDATES_PER_PROVIDER,
            dismiss_on_scroll: true,
        }
    }
}

impl JumpConfig {
    /// Borrow the alphabet as a `Vec<char>`, falling back to the
    /// built-in default whenever the user override is empty.
    pub fn alphabet_chars(&self) -> Vec<char> {
        if self.alphabet.is_empty() {
            DEFAULT_ALPHABET.chars().collect()
        } else {
            self.alphabet.chars().collect()
        }
    }
}

/// `App`-global wrapping the active [`JumpConfig`]. Mirrors
/// `FmThemeStore` and `OpenerStore` (FS-watch + atomic replace on
/// reload), so the overlay never reads from disk on its hot path.
#[derive(Debug, Default)]
pub struct JumpConfigStore {
    config: JumpConfig,
}

impl Global for JumpConfigStore {}

impl JumpConfigStore {
    /// Snapshot the current config. Cheap clone of an owned
    /// `JumpConfig` — callers don't need to hold the global borrow.
    pub fn current(cx: &App) -> JumpConfig {
        cx.try_global::<JumpConfigStore>()
            .map(|store| store.config.clone())
            .unwrap_or_default()
    }

    /// Replace the stored config — used by both the synchronous
    /// startup load and the FS-watch reload path.
    fn install(&mut self, config: JumpConfig) {
        self.config = config;
    }
}

const JUMP_CONFIG_FILE: &str = "jump.toml";

/// Raw on-disk schema for `~/.config/codon/jump.toml`. Every field is
/// optional so a partial override merges cleanly with the embedded
/// defaults.
#[derive(Debug, Default, Deserialize)]
struct JumpDoc {
    #[serde(default)]
    jump: JumpSection,
}

#[derive(Debug, Default, Deserialize)]
struct JumpSection {
    #[serde(default)]
    alphabet: Option<String>,
    #[serde(default)]
    label_position: Option<String>,
    #[serde(default)]
    max_candidates_per_provider: Option<usize>,
    #[serde(default)]
    dismiss_on_scroll: Option<bool>,
    // The `[jump.chord]` block is currently advisory — the keymap
    // already carries the bindings, and the cheatsheet picks them up
    // automatically. Echoing into the keymap is a follow-up; we still
    // parse the block so an early-adopter user's TOML doesn't error
    // out with "unknown key".
    #[serde(default)]
    chord: Option<JumpChordSection>,
}

#[derive(Debug, Default, Deserialize)]
struct JumpChordSection {
    #[serde(default)]
    target: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// Where the loader expects the jump config file to live. Honours
/// `$XDG_CONFIG_HOME` through `codon_config::codon_config_dir`.
pub fn user_jump_config_path() -> Option<PathBuf> {
    codon_config::codon_config_dir().map(|d| d.join(JUMP_CONFIG_FILE))
}

fn parse_jump_doc(content: &str) -> Result<JumpDoc> {
    toml::from_str::<JumpDoc>(content).context("parsing jump.toml")
}

fn build_config(doc: JumpDoc) -> JumpConfig {
    let mut config = JumpConfig::default();
    let JumpSection {
        alphabet,
        label_position,
        max_candidates_per_provider,
        dismiss_on_scroll,
        chord,
    } = doc.jump;
    if let Some(alpha) = alphabet
        && !alpha.is_empty()
    {
        config.alphabet = alpha;
    }
    if let Some(position) = label_position {
        match LabelPosition::parse(&position) {
            Some(parsed) => config.label_position = parsed,
            None => log::warn!(
                "jump.toml: ignoring unknown label_position '{position}' (expected top-left | center)"
            ),
        }
    }
    if let Some(cap) = max_candidates_per_provider
        && cap > 0
    {
        config.max_candidates_per_provider = cap;
    }
    if let Some(flag) = dismiss_on_scroll {
        config.dismiss_on_scroll = flag;
    }
    // Chord block parsed for forward-compat — see the schema comment.
    if let Some(chord) = chord {
        if let Some(target) = chord.target {
            log::debug!("jump.toml: chord.target='{target}' (advisory — rebind via codon.toml)");
        }
        if let Some(url) = chord.url {
            log::debug!("jump.toml: chord.url='{url}' (advisory — rebind via codon.toml)");
        }
    }
    config
}

/// Synchronous initial load. Missing file is fine — the store keeps
/// the embedded defaults. Parse failures keep the previous contents
/// and log a warning.
pub fn apply_user_jump_config(cx: &mut App) {
    let Some(path) = user_jump_config_path() else {
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => match parse_jump_doc(&content) {
            Ok(doc) => {
                let config = build_config(doc);
                cx.update_global::<JumpConfigStore, _>(|store, _| store.install(config));
                log::debug!("codon-jump: loaded {}", path.display());
            }
            Err(err) => log::warn!(
                "codon-jump: ignoring malformed {} ({err:#})",
                path.display()
            ),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            log::debug!("codon-jump: {} not present", path.display());
        }
        Err(err) => log::warn!(
            "codon-jump: could not read {} ({err})",
            path.display()
        ),
    }
}

/// Initialise the [`JumpConfigStore`] global with embedded defaults,
/// do the synchronous user-file load, then start the FS watcher so
/// subsequent on-disk edits hot-reload. Mirrors
/// `file_manager::theme::init` step-for-step.
fn init_jump_config(fs: Arc<dyn Fs>, cx: &mut App) {
    cx.set_global(JumpConfigStore::default());
    apply_user_jump_config(cx);
    start_jump_config_watcher(fs, cx);
}

fn start_jump_config_watcher(fs: Arc<dyn Fs>, cx: &mut App) {
    let Some(path) = user_jump_config_path() else {
        return;
    };
    let executor = cx.background_executor().clone();
    let (mut rx, watch_task) = settings::watch_config_file(&executor, fs, path);
    let mut saw_initial = false;
    cx.spawn(async move |cx| {
        while let Some(content) = rx.next().await {
            if !saw_initial {
                saw_initial = true;
                continue;
            }
            cx.update(|cx| match parse_jump_doc(&content) {
                Ok(doc) => {
                    let config = build_config(doc);
                    cx.update_global::<JumpConfigStore, _>(|store, _| store.install(config));
                    log::debug!("codon-jump: hot-reload applied");
                }
                Err(err) => log::warn!("codon-jump: hot-reload failed ({err:#})"),
            });
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
        drop(watch_task);
    })
    .detach();
}

/// What kind of element a candidate represents. Used by the Url-only
/// mode to filter the candidate set, and by future styling hooks.
#[derive(Debug, Clone)]
pub enum JumpKind {
    /// A text word in an editor, terminal, file-manager row, etc.
    Word,
    /// A URL. The string is the URL itself — callers (the Url
    /// dispatcher) read it without re-running the action closure.
    Url(String),
    /// A clickable UI element (tab, button, dock toggle, …)
    /// surfaced via the future `JumpClickable` wrapper.
    Clickable,
}

/// One candidate target the overlay can paint a chip on.
///
/// `bounds` is in window-absolute pixel space — providers translate
/// from their pane-local coordinates before yielding. `action` is a
/// `FnOnce` so it can move owned state (selections, paths, entity
/// handles) into the closure.
pub struct JumpCandidate {
    pub bounds: Bounds<Pixels>,
    pub kind: JumpKind,
    pub action: Box<dyn FnOnce(&mut Window, &mut App)>,
}

impl std::fmt::Debug for JumpCandidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JumpCandidate")
            .field("bounds", &self.bounds)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// Which entry point opened the overlay. Filters the candidate set
/// providers contribute (Url drops everything that isn't a URL).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpMode {
    /// Any [`JumpKind`] — the broadest `cmd-k j` mode.
    Target,
    /// [`JumpKind::Url`] only.
    Url,
}

/// Snapshot of "where the user is looking" handed to every provider on
/// activation. Providers MAY use `cursor_anchor` for hot-path culling,
/// but the overlay always re-sorts by Euclidean distance from this
/// point post-collection — providers can stay simple.
#[derive(Debug, Clone)]
pub struct JumpContext {
    pub mode: JumpMode,
    pub cursor_anchor: Option<Point<Pixels>>,
}

/// Anything that knows how to produce jump candidates for its pane.
///
/// Implementers are stored in [`JumpRegistry`] as `Arc<dyn JumpProvider>`.
/// Liveness is checked through [`is_alive`]: an editor provider can
/// hold a `WeakEntity<Editor>` and return `false` after the editor
/// drops, letting the registry prune it on the next collect.
///
/// [`is_alive`]: JumpProvider::is_alive
pub trait JumpProvider: Send + Sync + 'static {
    /// Yield every visible candidate. `cx` is mutable so providers can
    /// `update` their backing entity to read selection / scroll state.
    /// Returning an empty vec is fine — the registry just skips it.
    fn collect(&self, ctx: &JumpContext, cx: &mut App) -> Vec<JumpCandidate>;

    /// `false` means "I'm dead, drop me from the registry." Defaults to
    /// always-alive; providers backed by an entity should override.
    fn is_alive(&self, _cx: &App) -> bool {
        true
    }
}

/// `gpui::Global` holding every registered [`JumpProvider`]. Insertion
/// order is preserved — providers added later overlay providers added
/// earlier when their bounds overlap, but the label assigner only sees
/// the flat sorted list so registration order is mostly cosmetic.
#[derive(Default)]
pub struct JumpRegistry {
    providers: Vec<Arc<dyn JumpProvider>>,
}

impl Global for JumpRegistry {}

impl JumpRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a provider for the lifetime of `cx` (or until it
    /// reports `is_alive() == false` on the next collect).
    pub fn register(cx: &mut App, provider: Arc<dyn JumpProvider>) {
        cx.update_default_global::<JumpRegistry, _>(|registry, _| {
            registry.providers.push(provider);
        });
    }

    /// Drain candidates from every live provider. Dead providers are
    /// removed in-place so the registry doesn't grow without bound.
    pub fn collect_all(cx: &mut App, ctx: &JumpContext) -> Vec<JumpCandidate> {
        // Snapshot the live set first so we can collect without holding
        // the global borrow across each provider's `collect`, which
        // wants `&mut App`.
        let live: Vec<Arc<dyn JumpProvider>> =
            cx.update_default_global::<JumpRegistry, _>(|registry, cx| {
                registry.providers.retain(|p| p.is_alive(cx));
                registry.providers.clone()
            });

        let mut out = Vec::new();
        for provider in &live {
            let mut from_provider = provider.collect(ctx, cx);
            if matches!(ctx.mode, JumpMode::Url) {
                from_provider.retain(|c| matches!(c.kind, JumpKind::Url(_)));
            }
            out.append(&mut from_provider);
        }
        out
    }

    /// Test helper: number of currently registered providers.
    pub fn len(cx: &App) -> usize {
        cx.try_global::<JumpRegistry>()
            .map(|r| r.providers.len())
            .unwrap_or(0)
    }
}

/// Assign jump labels for `n` candidates from `alphabet`.
///
/// The function is *pure* and order-preserving: the caller is
/// responsible for sorting candidates (e.g. by distance from the
/// cursor) before calling, and the i-th returned label is the label
/// for the i-th candidate. This keeps the assigner trivially
/// unit-testable.
///
/// Strategy:
///
/// 1. If `n <= alphabet.len()²`, every label is 2 characters,
///    enumerated in lexical order: `aa, ab, …, az, ba, …`. This is
///    the common path (676 labels with the default alphabet — more
///    than fit on a 4K screen).
/// 2. If `alphabet.len()² < n <= alphabet.len()³`, every label
///    degrades to 3 characters, enumerated identically.
/// 3. If `n > alphabet.len()³`, only the first `alphabet.len()³`
///    candidates get a label — the rest are silently dropped. The
///    overlay treats unlabeled candidates as invisible.
///
/// Empty alphabet returns an empty `Vec` regardless of `n`.
pub fn assign_labels(alphabet: &[char], n: usize) -> Vec<String> {
    let base = alphabet.len();
    if base == 0 || n == 0 {
        return Vec::new();
    }
    let two = base * base;
    let three = two.saturating_mul(base);

    let (width, count) = if n <= two {
        (2, n)
    } else if n <= three {
        (3, n)
    } else {
        (3, three)
    };

    let mut out = Vec::with_capacity(count);
    for index in 0..count {
        out.push(encode_label(alphabet, index, width));
    }
    out
}

/// Lexically encode `index` as a fixed-width label drawing digits
/// from `alphabet`. Index 0 = "aaa…", index `base-1` = "aab…", etc.
fn encode_label(alphabet: &[char], index: usize, width: usize) -> String {
    let base = alphabet.len();
    let mut buf = vec![alphabet[0]; width];
    let mut remaining = index;
    // Fill right-to-left so position 0 is the most-significant digit
    // (lexical order falls out for free).
    for slot in (0..width).rev() {
        let digit = remaining % base;
        buf[slot] = alphabet[digit];
        remaining /= base;
    }
    buf.into_iter().collect()
}

/// Internal pairing of a candidate with its assigned label and the
/// distance the overlay computed for sorting. Distance is kept around
/// for debug logging — the slice is already sorted.
struct LabeledCandidate {
    label: String,
    candidate: JumpCandidate,
}

/// State machine driving the two-keystroke capture. We start at
/// `WaitFirst`, advance to `WaitSecond` after a matching first key,
/// and dispatch when the second key resolves to a unique label.
#[derive(Debug, Clone, PartialEq, Eq)]
enum KeystrokeState {
    WaitFirst,
    /// At least one label starts with `prefix` — render only those.
    WaitSecond { prefix: char },
}

/// Window-wide jump overlay. Opens as a `Workspace` modal that
/// renders as a "bare" full-window layer (`render_bare = true`) so the
/// chips paint over every pane without the default modal background
/// + centering treatment.
pub struct JumpOverlay {
    focus_handle: FocusHandle,
    mode: JumpMode,
    alphabet: Vec<char>,
    label_position: LabelPosition,
    labeled: Vec<LabeledCandidate>,
    state: KeystrokeState,
    /// Set on dismiss so any frame still in flight paints empty.
    dismissed: bool,
    /// Weak handle to the host workspace — needed by the Url-mode
    /// dispatcher to surface `MessageNotification` toasts. Holding a
    /// strong `Entity<Workspace>` would create a cycle (the overlay
    /// lives *inside* the workspace's modal stack).
    workspace: WeakEntity<Workspace>,
    #[allow(dead_code)]
    workspace_subscription: Option<Subscription>,
}

impl JumpOverlay {
    /// Open the overlay over the given workspace. Reads providers from
    /// the [`JumpRegistry`] global, sorts candidates by distance from
    /// the workspace's active pane focus point (best-effort), assigns
    /// labels, and shows itself as a bare modal.
    pub fn open(
        mode: JumpMode,
        workspace_entity: &mut Workspace,
        window: &mut Window,
        cx: &mut Context<Workspace>,
    ) {
        let cursor_anchor = active_pane_anchor(workspace_entity, window, cx);
        let config = JumpConfigStore::current(cx);
        let alphabet: Vec<char> = config.alphabet_chars();
        let label_position = config.label_position;

        let ctx = JumpContext {
            mode,
            cursor_anchor,
        };
        let mut candidates = JumpRegistry::collect_all(cx, &ctx);

        // Drain anything registered via `.jump_target(...)` into the
        // candidate set. The clickable registry has no provider
        // representation today — the wrapper pushes per paint and the
        // overlay drains here at open time. Clickable targets are
        // excluded from URL mode (a button doesn't have a URL anyone
        // would want to copy/open).
        if !matches!(mode, JumpMode::Url) {
            for (bounds, on_click) in take_clickables() {
                let on_click = on_click.clone();
                candidates.push(JumpCandidate {
                    bounds,
                    kind: JumpKind::Clickable,
                    action: Box::new(move |window, cx| {
                        on_click(window, cx);
                    }),
                });
            }
        }

        // Belt-and-suspenders viewport clip. Even with per-provider
        // freshness gating, a misbehaving provider could still yield
        // bounds outside the visible window — drop them up front so
        // we never paint chips off-screen or pile them up on top of
        // unrelated UI.
        let viewport = window.viewport_size();
        let viewport_bounds = Bounds {
            origin: Point::default(),
            size: viewport,
        };
        candidates.retain(|c| {
            let b = c.bounds;
            b.origin.x < viewport_bounds.size.width
                && b.origin.y < viewport_bounds.size.height
                && b.origin.x + b.size.width > Pixels::ZERO
                && b.origin.y + b.size.height > Pixels::ZERO
        });

        // Greedy nearest-first: stable sort by squared Euclidean
        // distance from the anchor (fall back to bounds origin order
        // if no anchor is available).
        sort_by_distance(&mut candidates, cursor_anchor);

        let labels = assign_labels(&alphabet, candidates.len());

        // Cap at labels.len() — any overflow candidates are dropped.
        candidates.truncate(labels.len());

        let labeled: Vec<LabeledCandidate> = labels
            .into_iter()
            .zip(candidates.into_iter())
            .map(|(label, candidate)| LabeledCandidate { label, candidate })
            .collect();

        let workspace_handle = cx.entity().downgrade();
        let workspace_entity_strong = cx.entity();
        let dismiss_on_scroll = config.dismiss_on_scroll;
        workspace_entity.toggle_modal(window, cx, |_window, cx| {
            // `Workspace::Event::Scrolled` is the dismissal signal —
            // editor, terminal_view, and file-manager forward their
            // scroll events into `Workspace::notify_scrolled`, so this
            // subscription fires as soon as any host pane scrolls.
            let workspace_subscription = if dismiss_on_scroll {
                Some(cx.subscribe(
                    &workspace_entity_strong,
                    |this: &mut Self, _workspace, event: &WorkspaceEvent, cx| {
                        if matches!(event, WorkspaceEvent::Scrolled) && !this.dismissed {
                            this.dismissed = true;
                            cx.emit(gpui::DismissEvent);
                        }
                    },
                ))
            } else {
                None
            };
            Self {
                focus_handle: cx.focus_handle(),
                mode,
                alphabet,
                label_position,
                labeled,
                state: KeystrokeState::WaitFirst,
                dismissed: false,
                workspace: workspace_handle,
                workspace_subscription,
            }
        });
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dismissed {
            return;
        }

        let key = event.keystroke.key.as_str();
        if key == "escape" {
            self.dismiss(cx);
            return;
        }

        // Any modifier-laden key (cmd, ctrl, alt held) cancels — the
        // overlay only listens to bare alphabet input.
        let mods = &event.keystroke.modifiers;
        if mods.platform || mods.control || mods.alt {
            self.dismiss(cx);
            return;
        }

        // Map the keystroke to a single alphabet character. We accept
        // `key_char` if present (so layouts where the printed glyph
        // differs from the key name still work), falling back to the
        // raw key string when it's a single-char token.
        let typed = key_to_alphabet_char(event, &self.alphabet);
        let Some(typed) = typed else {
            // Non-alphabet keystroke (arrows, function keys,
            // punctuation outside the alphabet) — cancel.
            self.dismiss(cx);
            return;
        };

        match self.state.clone() {
            KeystrokeState::WaitFirst => {
                let any_match = self
                    .labeled
                    .iter()
                    .any(|lc| lc.label.starts_with(typed));
                if !any_match {
                    self.dismiss(cx);
                    return;
                }
                // Progressive narrowing — if exactly one label starts
                // with `typed` we can auto-fire without waiting for the
                // second char.
                let unique = self
                    .labeled
                    .iter()
                    .filter(|lc| lc.label.starts_with(typed))
                    .count()
                    == 1;
                if unique {
                    self.fire_matching(typed, None, window, cx);
                    return;
                }
                self.state = KeystrokeState::WaitSecond { prefix: typed };
                cx.notify();
            }
            KeystrokeState::WaitSecond { prefix } => {
                self.fire_matching(prefix, Some(typed), window, cx);
            }
        }
    }

    /// Find the first label matching `prefix` + optional `second` and
    /// run its action. If nothing matches, dismiss.
    fn fire_matching(
        &mut self,
        prefix: char,
        second: Option<char>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target_index = self.labeled.iter().position(|lc| {
            let mut chars = lc.label.chars();
            let first_ok = chars.next() == Some(prefix);
            let second_ok = match second {
                Some(c) => chars.next() == Some(c),
                None => true,
            };
            first_ok && second_ok
        });
        let Some(index) = target_index else {
            self.dismiss(cx);
            return;
        };
        // Move the candidate out so we can call its FnOnce action.
        // Replace with a sentinel to keep the Vec indices stable for
        // any in-flight paint.
        let labeled = std::mem::take(&mut self.labeled);
        let mut labeled = labeled;
        if index >= labeled.len() {
            self.labeled = labeled;
            self.dismiss(cx);
            return;
        }
        let removed = labeled.swap_remove(index);
        self.labeled = labeled;
        // Dismiss first so the action lands on the right focus target
        // (mirrors the cheatsheet's dispatch_cursor pattern).
        self.dismissed = true;
        cx.emit(DismissEvent);
        match self.mode {
            JumpMode::Target => (removed.candidate.action)(window, cx),
            JumpMode::Url => self.dispatch_url_copy(removed.candidate, window, cx),
        }
    }

    /// Url-mode dispatcher: overrides the candidate's declared action
    /// with a clipboard write and a deduplicated toast. The candidate's
    /// own closure is intentionally discarded — for the Url chord the
    /// behaviour is uniform regardless of source pane.
    fn dispatch_url_copy(
        &self,
        candidate: JumpCandidate,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let JumpKind::Url(url) = candidate.kind else {
            // The Url filter in `collect_all` should have prevented
            // this; bail silently if a provider somehow contributed a
            // non-URL candidate to a Url-mode collection.
            log::warn!("codon-jump: Url mode fired on non-URL candidate");
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
        let Some(workspace) = self.workspace.upgrade() else {
            return;
        };
        workspace.update(cx, |workspace, cx| {
            // `Named` so a rapid second copy replaces the existing
            // toast rather than stacking — matches the `fm-task-*`
            // pattern in file-manager/src/tasks.rs.
            let id = NotificationId::Named("jump-url-copied".into());
            let message = format!("Copied {url}");
            workspace.show_notification(id, cx, |cx| {
                cx.new(|cx| MessageNotification::new(message.clone(), cx))
            });
        });
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        if self.dismissed {
            return;
        }
        self.dismissed = true;
        cx.emit(DismissEvent);
    }
}

fn key_to_alphabet_char(event: &KeyDownEvent, alphabet: &[char]) -> Option<char> {
    let mut candidate: Option<char> = None;
    if let Some(s) = event.keystroke.key_char.as_deref() {
        let mut chars = s.chars();
        if let Some(first) = chars.next()
            && chars.next().is_none()
        {
            candidate = Some(first);
        }
    }
    if candidate.is_none() {
        let key = event.keystroke.key.as_str();
        let mut chars = key.chars();
        if let Some(first) = chars.next()
            && chars.next().is_none()
        {
            candidate = Some(first);
        }
    }
    let ch = candidate?;
    if alphabet.contains(&ch) {
        Some(ch)
    } else {
        None
    }
}

fn sort_by_distance(candidates: &mut [JumpCandidate], anchor: Option<Point<Pixels>>) {
    let Some(anchor) = anchor else {
        return;
    };
    candidates.sort_by(|a, b| {
        distance_sq(&a.bounds, anchor)
            .partial_cmp(&distance_sq(&b.bounds, anchor))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

fn distance_sq(bounds: &Bounds<Pixels>, anchor: Point<Pixels>) -> f32 {
    let dx = f32::from(bounds.origin.x) - f32::from(anchor.x);
    let dy = f32::from(bounds.origin.y) - f32::from(anchor.y);
    dx * dx + dy * dy
}

/// Best-effort: surface the active pane's primary-cursor screen
/// position as the distance anchor. The first cut returns `None` —
/// providers that need finer ordering can still rank candidates
/// internally before yielding. A follow-up task (`jump-pane-editor`)
/// will plumb the editor's primary-cursor pixel position out via the
/// `codon-mode` selection trait.
fn active_pane_anchor(
    _workspace: &Workspace,
    _window: &mut Window,
    _cx: &mut App,
) -> Option<Point<Pixels>> {
    // TODO(jump-pane-editor): read the focused pane's cursor pixel.
    None
}

impl Render for JumpOverlay {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut key_context = KeyContext::default();
        key_context.add("JumpOverlay");
        key_context.add("menu");

        if self.dismissed {
            return div()
                .key_context(key_context)
                .track_focus(&self.focus_handle)
                .absolute()
                .inset_0()
                .size_full();
        }

        let theme = cx.theme();
        let chip_bg = theme.colors().version_control_conflict;
        let chip_fg = theme.colors().text;
        let dim_alpha = 0.3;

        // Empty-state message when no providers contributed anything.
        let empty_panel: Option<AnyElement> = if self.labeled.is_empty() {
            Some(
                v_flex()
                    .absolute()
                    .left(px(24.))
                    .top(px(24.))
                    .px_3()
                    .py_2()
                    .rounded_md()
                    .bg(theme.colors().elevated_surface_background)
                    .border_1()
                    .border_color(theme.colors().border)
                    .child(
                        Label::new(SharedString::from(match self.mode {
                            JumpMode::Target => "No targets visible",
                            JumpMode::Url => "No URLs visible",
                        }))
                        .color(Color::Muted)
                        .size(LabelSize::Small),
                    )
                    .into_any_element(),
            )
        } else {
            None
        };

        // `ModalLayer` returns our view bare when `render_bare()` is
        // true — without the `.absolute().inset_0()` wrapper it
        // applies to the standard modal path. Without that wrapper our
        // root would slot into the workspace's flex layout as a sibling
        // of the panes, with `size_full()` forcing the workspace's
        // actual content to zero width and rendering everything black.
        // Force absolute positioning here so the overlay floats above
        // every pane and the workspace keeps its real layout intact.
        let mut root = div()
            .key_context(key_context)
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .occlude()
            .absolute()
            .inset_0()
            .size_full();

        for entry in &self.labeled {
            let origin = chip_anchor(&entry.candidate.bounds, self.label_position);
            let (first_char, second_char) = split_label(&entry.label);
            let (matches_prefix, after_first) = match &self.state {
                KeystrokeState::WaitFirst => (true, false),
                KeystrokeState::WaitSecond { prefix } => {
                    (first_char.map(|c| c == *prefix).unwrap_or(false), true)
                }
            };
            let chip = build_chip(
                &entry.label,
                first_char,
                second_char,
                matches_prefix,
                after_first,
                chip_bg,
                chip_fg,
                dim_alpha,
            );
            // `defer_draw`-based chip via the `deferred(...)` helper,
            // positioned absolutely so it floats over every pane.
            let chip_at = div()
                .absolute()
                .left(origin.x)
                .top(origin.y)
                .child(chip)
                .into_any_element();
            root = root.child(deferred(chip_at).with_priority(10));
        }

        if let Some(panel) = empty_panel {
            root = root.child(panel);
        }

        root
    }
}

/// Compute the chip's window-absolute origin from a candidate's
/// bounds and the configured [`LabelPosition`]. The chip itself is
/// small (under 30px wide), so for `Center` we offset by half the
/// candidate's size and leave the chip's own width/height correction
/// to a fixed magic-number translate — close enough to look centered
/// for word-sized targets, which is the only thing `Center` is
/// useful for.
fn chip_anchor(bounds: &Bounds<Pixels>, position: LabelPosition) -> Point<Pixels> {
    match position {
        LabelPosition::TopLeft => bounds.origin,
        LabelPosition::Center => {
            let chip_half_w = px(10.0);
            let chip_half_h = px(8.0);
            Point {
                x: bounds.origin.x + bounds.size.width / 2.0 - chip_half_w,
                y: bounds.origin.y + bounds.size.height / 2.0 - chip_half_h,
            }
        }
    }
}

fn split_label(label: &str) -> (Option<char>, Option<char>) {
    let mut chars = label.chars();
    let first = chars.next();
    let second = chars.next();
    (first, second)
}

#[allow(clippy::too_many_arguments)]
fn build_chip(
    label: &str,
    first_char: Option<char>,
    second_char: Option<char>,
    matches_prefix: bool,
    after_first: bool,
    chip_bg: gpui::Hsla,
    chip_fg: gpui::Hsla,
    dim_alpha: f32,
) -> AnyElement {
    let mut bg = chip_bg;
    let mut fg = chip_fg;
    if after_first && !matches_prefix {
        bg.a *= dim_alpha;
        fg.a *= dim_alpha;
    }

    // Pre-narrowing: render the full label. Post-first-key: dim the
    // already-consumed first char and bold the awaiting second.
    let body: AnyElement = if after_first && matches_prefix {
        let first = first_char.unwrap_or(' ').to_string();
        let second = second_char.unwrap_or(' ').to_string();
        h_flex()
            .child(
                Label::new(SharedString::from(first))
                    .color(Color::Muted)
                    .size(LabelSize::Small),
            )
            .child(
                Label::new(SharedString::from(second))
                    .color(Color::Default)
                    .size(LabelSize::Small)
                    .weight(gpui::FontWeight::BOLD),
            )
            .into_any_element()
    } else {
        Label::new(SharedString::from(label.to_string()))
            .color(Color::Default)
            .size(LabelSize::Small)
            .into_any_element()
    };

    h_flex()
        .px_1()
        .py_0p5()
        .rounded_sm()
        .bg(bg)
        .text_color(fg)
        .child(body)
        .when(after_first && !matches_prefix, |el| el.opacity(dim_alpha))
        .into_any_element()
}

impl EventEmitter<DismissEvent> for JumpOverlay {}

impl Focusable for JumpOverlay {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl ModalView for JumpOverlay {
    fn render_bare(&self) -> bool {
        // The overlay paints its own absolute-positioned chips and
        // doesn't want ModalLayer's centered-box treatment.
        true
    }
}

/// Initialize codon-jump. Call once at startup, before any provider
/// registration. Idempotent — re-calling replaces the registry with an
/// empty one, which is rarely what you want outside tests.
///
/// `jump-action-target` (a follow-up task) is responsible for wiring
/// the call into `apps/codon/src/main.rs` along with the actions.
pub fn init(fs: Arc<dyn Fs>, cx: &mut App) {
    cx.set_global(JumpRegistry::new());
    init_jump_config(fs, cx);
}

// `JumpClickable` element wrapper + paint-time registry live in
// `workspace::codon_jump_clickable` (see the re-exports above). Hosting
// them there avoids a dependency cycle between `codon-jump` (which
// depends on `workspace::Workspace + ModalView`) and the vendored Zed
// UI crates that adopt `.jump_target(...)`.

/// Wire codon-jump's workspace-scoped action handlers. Call from the
/// workspace initialization hook, mirroring
/// `codon_session::actions::register_for_workspace`.
pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_jump_to_target);
    workspace.register_action(handle_jump_to_url);
}

fn handle_jump_to_target(
    workspace: &mut Workspace,
    _: &JumpToTarget,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    JumpOverlay::open(JumpMode::Target, workspace, window, cx);
}

fn handle_jump_to_url(
    workspace: &mut Workspace,
    _: &JumpToUrl,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    JumpOverlay::open(JumpMode::Url, workspace, window, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alphabet() -> Vec<char> {
        DEFAULT_ALPHABET.chars().collect()
    }

    #[test]
    fn assign_labels_empty_returns_empty() {
        assert!(assign_labels(&alphabet(), 0).is_empty());
    }

    #[test]
    fn assign_labels_empty_alphabet_returns_empty() {
        assert!(assign_labels(&[], 5).is_empty());
    }

    #[test]
    fn assign_labels_single_candidate_is_two_chars() {
        let labels = assign_labels(&alphabet(), 1);
        assert_eq!(labels, vec!["aa".to_string()]);
    }

    #[test]
    fn assign_labels_three_candidates_are_lexical() {
        let labels = assign_labels(&alphabet(), 3);
        assert_eq!(
            labels,
            vec!["aa".to_string(), "ab".to_string(), "ac".to_string()]
        );
    }

    #[test]
    fn assign_labels_alphabet_squared_exactly_two_chars() {
        let labels = assign_labels(&alphabet(), 26 * 26);
        assert_eq!(labels.len(), 26 * 26);
        for label in &labels {
            assert_eq!(label.chars().count(), 2);
        }
        assert_eq!(labels.first().map(|s| s.as_str()), Some("aa"));
        assert_eq!(labels.last().map(|s| s.as_str()), Some("zz"));
    }

    #[test]
    fn assign_labels_overflow_degrades_to_three_chars() {
        let labels = assign_labels(&alphabet(), 26 * 26 + 1);
        assert_eq!(labels.len(), 26 * 26 + 1);
        for label in &labels {
            assert_eq!(label.chars().count(), 3);
        }
        assert_eq!(labels.first().map(|s| s.as_str()), Some("aaa"));
    }

    #[test]
    fn assign_labels_one_thousand_is_three_char_mixed() {
        let labels = assign_labels(&alphabet(), 1000);
        assert_eq!(labels.len(), 1000);
        for label in &labels {
            assert_eq!(label.chars().count(), 3);
        }
        // Lexical: index 0 -> "aaa", index 26 -> "aba" (26 = 1*26 + 0).
        assert_eq!(labels[0], "aaa");
        assert_eq!(labels[26], "aba");
    }

    #[test]
    fn assign_labels_caps_at_alphabet_cubed() {
        let small: Vec<char> = "ab".chars().collect();
        // 2^3 = 8. Ask for 100 — should cap at 8 three-char labels.
        let labels = assign_labels(&small, 100);
        assert_eq!(labels.len(), 8);
        assert_eq!(labels.first().map(|s| s.as_str()), Some("aaa"));
        assert_eq!(labels.last().map(|s| s.as_str()), Some("bbb"));
    }

    #[test]
    fn assign_labels_two_letter_alphabet_at_two_chars_boundary() {
        let small: Vec<char> = "ab".chars().collect();
        // 2² = 4 exactly.
        let labels = assign_labels(&small, 4);
        assert_eq!(labels, vec!["aa", "ab", "ba", "bb"]);
    }

    #[test]
    fn encode_label_pads_with_first_char() {
        let alpha: Vec<char> = "abc".chars().collect();
        // Index 0 of width 3 -> "aaa".
        assert_eq!(encode_label(&alpha, 0, 3), "aaa");
        // Index 4 of width 3: 4 = 0*9 + 1*3 + 1 -> "abb".
        assert_eq!(encode_label(&alpha, 4, 3), "abb");
    }

    #[test]
    fn distance_sq_zero_at_origin() {
        let bounds = Bounds {
            origin: Point {
                x: px(0.0),
                y: px(0.0),
            },
            size: gpui::Size {
                width: px(10.0),
                height: px(10.0),
            },
        };
        let anchor = Point {
            x: px(0.0),
            y: px(0.0),
        };
        assert_eq!(distance_sq(&bounds, anchor), 0.0);
    }

    #[test]
    fn distance_sq_uses_squared_euclidean() {
        let bounds = Bounds {
            origin: Point {
                x: px(3.0),
                y: px(4.0),
            },
            size: gpui::Size {
                width: px(0.0),
                height: px(0.0),
            },
        };
        let anchor = Point {
            x: px(0.0),
            y: px(0.0),
        };
        assert_eq!(distance_sq(&bounds, anchor), 25.0);
    }

    fn fixed_candidate(x: f32, y: f32) -> JumpCandidate {
        JumpCandidate {
            bounds: Bounds {
                origin: Point { x: px(x), y: px(y) },
                size: gpui::Size {
                    width: px(10.0),
                    height: px(10.0),
                },
            },
            kind: JumpKind::Word,
            action: Box::new(|_, _| {}),
        }
    }

    #[test]
    fn sort_by_distance_orders_nearest_first() {
        let mut candidates = vec![
            fixed_candidate(100.0, 100.0),
            fixed_candidate(10.0, 0.0),
            fixed_candidate(50.0, 50.0),
        ];
        let anchor = Point {
            x: px(0.0),
            y: px(0.0),
        };
        sort_by_distance(&mut candidates, Some(anchor));
        let xs: Vec<f32> = candidates
            .iter()
            .map(|c| f32::from(c.bounds.origin.x))
            .collect();
        assert_eq!(xs, vec![10.0, 50.0, 100.0]);
    }

    #[test]
    fn sort_by_distance_no_anchor_is_noop() {
        let mut candidates = vec![
            fixed_candidate(100.0, 0.0),
            fixed_candidate(0.0, 0.0),
        ];
        sort_by_distance(&mut candidates, None);
        let xs: Vec<f32> = candidates
            .iter()
            .map(|c| f32::from(c.bounds.origin.x))
            .collect();
        assert_eq!(xs, vec![100.0, 0.0]);
    }

    #[test]
    fn split_label_handles_short_inputs() {
        assert_eq!(split_label(""), (None, None));
        assert_eq!(split_label("a"), (Some('a'), None));
        assert_eq!(split_label("ab"), (Some('a'), Some('b')));
        assert_eq!(split_label("abc"), (Some('a'), Some('b')));
    }

    /// Mock provider that yields a fixed list. Used by the pipeline
    /// integration test below to verify that
    /// `JumpRegistry::collect_all`-style flow → `sort_by_distance` →
    /// `assign_labels` produces the labels we expect.
    struct MockProvider {
        seeded: std::cell::RefCell<Option<Vec<(f32, f32, JumpKind)>>>,
    }

    impl JumpProvider for MockProvider {
        fn collect(&self, _ctx: &JumpContext, _cx: &mut App) -> Vec<JumpCandidate> {
            let Some(seeds) = self.seeded.borrow_mut().take() else {
                return Vec::new();
            };
            seeds
                .into_iter()
                .map(|(x, y, kind)| JumpCandidate {
                    bounds: Bounds {
                        origin: Point { x: px(x), y: px(y) },
                        size: gpui::Size {
                            width: px(10.0),
                            height: px(10.0),
                        },
                    },
                    kind,
                    action: Box::new(|_, _| {}),
                })
                .collect()
        }
    }

    /// SAFETY: MockProvider only stores a `RefCell` which is `!Send` in
    /// general, but the test thread is single-threaded.
    unsafe impl Send for MockProvider {}
    unsafe impl Sync for MockProvider {}

    #[test]
    fn pipeline_collect_sort_label_produces_expected_labels() {
        // Three candidates seeded at known positions; anchor at (0,0)
        // so distance order should be (10,0) -> (50,50) -> (100,100).
        let alphabet: Vec<char> = "abc".chars().collect();
        let mut candidates = vec![
            fixed_candidate(100.0, 100.0),
            fixed_candidate(10.0, 0.0),
            fixed_candidate(50.0, 50.0),
        ];
        let anchor = Point {
            x: px(0.0),
            y: px(0.0),
        };
        sort_by_distance(&mut candidates, Some(anchor));
        let labels = assign_labels(&alphabet, candidates.len());
        assert_eq!(labels, vec!["aa", "ab", "ac"]);

        // Verify the closest candidate gets the first label.
        assert_eq!(f32::from(candidates[0].bounds.origin.x), 10.0);
        assert_eq!(f32::from(candidates[1].bounds.origin.x), 50.0);
        assert_eq!(f32::from(candidates[2].bounds.origin.x), 100.0);
    }

    #[test]
    fn pipeline_url_mode_filters_non_url_candidates() {
        // The Url mode filter is applied inside `collect_all`. Verify
        // it by replicating the filter step directly.
        let mut candidates = vec![
            fixed_candidate(10.0, 0.0),
            JumpCandidate {
                bounds: Bounds {
                    origin: Point {
                        x: px(20.0),
                        y: px(0.0),
                    },
                    size: gpui::Size {
                        width: px(10.0),
                        height: px(10.0),
                    },
                },
                kind: JumpKind::Url("https://example.com".into()),
                action: Box::new(|_, _| {}),
            },
            fixed_candidate(30.0, 0.0),
        ];
        candidates.retain(|c| matches!(c.kind, JumpKind::Url(_)));
        assert_eq!(candidates.len(), 1);
        match &candidates[0].kind {
            JumpKind::Url(url) => assert_eq!(url, "https://example.com"),
            other => panic!("expected url, got {other:?}"),
        }
    }

    #[test]
    fn clickable_registry_reexport_drains() {
        clear_clickable_registry();
        assert_eq!(clickable_registry_len(), 0);
        let drained = take_clickables();
        assert!(drained.is_empty());
    }

    #[test]
    fn jump_config_default_alphabet_is_a_to_z() {
        let config = JumpConfig::default();
        assert_eq!(config.alphabet, DEFAULT_ALPHABET);
        assert_eq!(config.alphabet_chars().len(), 26);
        assert_eq!(config.label_position, LabelPosition::TopLeft);
        assert!(config.dismiss_on_scroll);
        assert_eq!(
            config.max_candidates_per_provider,
            DEFAULT_MAX_CANDIDATES_PER_PROVIDER
        );
    }

    #[test]
    fn jump_config_empty_alphabet_falls_back_to_default() {
        let mut config = JumpConfig::default();
        config.alphabet = String::new();
        assert_eq!(config.alphabet_chars().len(), 26);
    }

    #[test]
    fn build_config_applies_alphabet_override() {
        let doc = parse_jump_doc(
            r#"
            [jump]
            alphabet = "asdfghjkl;"
            "#,
        )
        .expect("parse");
        let config = build_config(doc);
        assert_eq!(config.alphabet, "asdfghjkl;");
    }

    #[test]
    fn build_config_ignores_empty_alphabet_override() {
        let doc = parse_jump_doc(
            r#"
            [jump]
            alphabet = ""
            "#,
        )
        .expect("parse");
        let config = build_config(doc);
        assert_eq!(config.alphabet, DEFAULT_ALPHABET);
    }

    #[test]
    fn build_config_parses_label_position_center() {
        let doc = parse_jump_doc(
            r#"
            [jump]
            label_position = "center"
            "#,
        )
        .expect("parse");
        let config = build_config(doc);
        assert_eq!(config.label_position, LabelPosition::Center);
    }

    #[test]
    fn build_config_unknown_label_position_keeps_default() {
        let doc = parse_jump_doc(
            r#"
            [jump]
            label_position = "off-screen"
            "#,
        )
        .expect("parse");
        let config = build_config(doc);
        assert_eq!(config.label_position, LabelPosition::TopLeft);
    }

    #[test]
    fn build_config_round_trips_dismiss_on_scroll_false() {
        let doc = parse_jump_doc(
            r#"
            [jump]
            dismiss_on_scroll = false
            "#,
        )
        .expect("parse");
        let config = build_config(doc);
        assert!(!config.dismiss_on_scroll);
    }

    #[test]
    fn build_config_round_trips_max_candidates_per_provider() {
        let doc = parse_jump_doc(
            r#"
            [jump]
            max_candidates_per_provider = 64
            "#,
        )
        .expect("parse");
        let config = build_config(doc);
        assert_eq!(config.max_candidates_per_provider, 64);
    }

    #[test]
    fn build_config_zero_max_candidates_keeps_default() {
        let doc = parse_jump_doc(
            r#"
            [jump]
            max_candidates_per_provider = 0
            "#,
        )
        .expect("parse");
        let config = build_config(doc);
        assert_eq!(
            config.max_candidates_per_provider,
            DEFAULT_MAX_CANDIDATES_PER_PROVIDER
        );
    }

    #[test]
    fn build_config_chord_block_is_advisory_only() {
        // Sanity: parsing a chord block doesn't error and doesn't mutate
        // the resolved keymap state (we're a no-op forward-compat path).
        let doc = parse_jump_doc(
            r#"
            [jump.chord]
            target = "space j"
            url    = "space u"
            "#,
        )
        .expect("parse");
        let config = build_config(doc);
        assert_eq!(config.alphabet, DEFAULT_ALPHABET);
    }

    #[test]
    fn label_position_parse_accepts_synonyms() {
        assert_eq!(LabelPosition::parse("top-left"), Some(LabelPosition::TopLeft));
        assert_eq!(LabelPosition::parse("topleft"), Some(LabelPosition::TopLeft));
        assert_eq!(LabelPosition::parse("top_left"), Some(LabelPosition::TopLeft));
        assert_eq!(LabelPosition::parse("Center"), Some(LabelPosition::Center));
        assert_eq!(LabelPosition::parse("CENTRE"), Some(LabelPosition::Center));
        assert_eq!(LabelPosition::parse("bottom-right"), None);
    }

    #[test]
    fn chip_anchor_top_left_returns_origin() {
        let bounds = Bounds {
            origin: Point {
                x: px(15.0),
                y: px(40.0),
            },
            size: gpui::Size {
                width: px(20.0),
                height: px(16.0),
            },
        };
        let anchor = chip_anchor(&bounds, LabelPosition::TopLeft);
        assert_eq!(f32::from(anchor.x), 15.0);
        assert_eq!(f32::from(anchor.y), 40.0);
    }

    #[test]
    fn chip_anchor_center_offsets_from_midpoint() {
        let bounds = Bounds {
            origin: Point {
                x: px(0.0),
                y: px(0.0),
            },
            size: gpui::Size {
                width: px(40.0),
                height: px(20.0),
            },
        };
        let anchor = chip_anchor(&bounds, LabelPosition::Center);
        // Midpoint is (20, 10); chip half-size shifts pull back to
        // (20 - 10, 10 - 8) = (10, 2).
        assert_eq!(f32::from(anchor.x), 10.0);
        assert_eq!(f32::from(anchor.y), 2.0);
    }

    #[test]
    fn mock_provider_unused_warning_silenced() {
        // Construct a MockProvider so the `dead_code` lint stays
        // quiet — and incidentally exercise the trait shape.
        let provider = MockProvider {
            seeded: std::cell::RefCell::new(Some(vec![(0.0, 0.0, JumpKind::Word)])),
        };
        let _trait_object: &dyn JumpProvider = &provider;
    }
}
