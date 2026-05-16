use gpui::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};
use serde::Deserialize;
use std::{collections::HashMap, path::PathBuf, rc::Rc, time::Duration};

/// Resolve the codon config directory (`$XDG_CONFIG_HOME/codon` or
/// `$HOME/.config/codon`).
///
/// Inlined here to keep `codon-keymap` free of a Rust-level dep on
/// `codon-config` — the two crates share the same trivial XDG resolution
/// rule (see `codon_config::codon_config_dir`), so duplicating the four
/// lines is cheaper than carrying the dep just for this lookup. If the
/// resolution rule ever grows complexity, lift it into a shared
/// `codon-paths` helper crate instead of re-introducing the dep.
fn codon_config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(xdg).join("codon"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("codon"))
}

/// Codon binds plenty of three-keystroke chords (`cmd-k s n`, `cmd-k shift-w n`,
/// `cmd-k a e`, …). GPUI's default 1-second chord timeout is too aggressive
/// for that — slow typists hit the timeout mid-chord, the prefix flushes, and
/// the raw keystroke gets replayed (which for terminal panes looks like the
/// chord "leaked into the shell"). Five seconds is comfortable without being
/// long enough to wedge the UI if a chord prefix was hit by accident.
const CHORD_TIMEOUT: Duration = Duration::from_secs(5);

/// Codon's TOML keymap format.
///
/// Example:
/// ```toml
/// [bindings.global]
/// "cmd-k h" = "workspace::ActivatePaneLeft"
/// "cmd-k t" = "workspace::NewTerminal"
///
/// [bindings.file_manager.normal]
/// "j" = "file_manager::NavigateDown"
///
/// [bindings.terminal.normal]
/// "j" = "terminal::ScrollLineDown"
/// ```
#[derive(Debug, Deserialize, Default)]
struct CodonKeymap {
    #[serde(default)]
    bindings: KeymapBindings,
}

#[derive(Debug, Deserialize, Default)]
struct KeymapBindings {
    #[serde(default)]
    global: HashMap<String, String>,
    #[serde(default)]
    editor: Option<ModeBindings>,
    #[serde(default)]
    terminal: Option<ModeBindings>,
    #[serde(default)]
    file_manager: Option<ModeBindings>,
    #[serde(default)]
    git_panel: Option<ModeBindings>,
    #[serde(default)]
    peek_dock: Option<ModeBindings>,
}

#[derive(Debug, Deserialize, Default)]
struct ModeBindings {
    #[serde(default)]
    normal: HashMap<String, String>,
    #[serde(default)]
    insert: HashMap<String, String>,
}

const DEFAULT_KEYMAP: &str = r#"
# Codon default keybindings
# Override in ~/.config/codon/keymap.toml

[bindings.global]
# Pane focus (vim-style). `ctrl-{h,j,k,l}` collide with terminal
# control codes (BS / LF / kill-line / clear) so they're unreachable
# from a focused terminal pane — the `cmd-k {arrow}` chords below are
# the universal path that works from every pane kind, terminals
# included.
"ctrl-h" = "workspace::ActivatePaneLeft"
"ctrl-j" = "workspace::ActivatePaneDown"
"ctrl-k" = "workspace::ActivatePaneUp"
"ctrl-l" = "workspace::ActivatePaneRight"

# Pane focus (cmd-k + arrow) — terminal-safe alternative to ctrl-hjkl.
"cmd-k left"  = "workspace::ActivatePaneLeft"
"cmd-k down"  = "workspace::ActivatePaneDown"
"cmd-k up"    = "workspace::ActivatePaneUp"
"cmd-k right" = "workspace::ActivatePaneRight"

# Pane move (swap with adjacent pane).
"ctrl-shift-h" = "workspace::SwapPaneLeft"
"ctrl-shift-j" = "workspace::SwapPaneDown"
"ctrl-shift-k" = "workspace::SwapPaneUp"
"ctrl-shift-l" = "workspace::SwapPaneRight"

# Pane move (cmd-k + shift-arrow) — terminal-safe alternative to
# ctrl-shift-hjkl. Mirrors the focus chords above.
"cmd-k shift-left"  = "workspace::SwapPaneLeft"
"cmd-k shift-down"  = "workspace::SwapPaneDown"
"cmd-k shift-up"    = "workspace::SwapPaneUp"
"cmd-k shift-right" = "workspace::SwapPaneRight"

# Pane resize (cmd-k prefix to avoid conflict)
"cmd-k shift-h" = "vim::ResizePaneLeft"
"cmd-k shift-j" = "vim::ResizePaneDown"
"cmd-k shift-k" = "vim::ResizePaneUp"
"cmd-k shift-l" = "vim::ResizePaneRight"

# Pane navigation (cmd-k h/k/l) — kept for back-compat, equivalent to ctrl-*.
# `cmd-k j` is repurposed for the jump-hint overlay below; use `ctrl-j` for
# pane-down navigation under the cmd-k prefix path. `cmd-k l` is repurposed
# for WindowLast (tmux `prefix l`); use `ctrl-l` for pane-right.
"cmd-k h" = "workspace::ActivatePaneLeft"
"cmd-k k" = "workspace::ActivatePaneUp"

# Pane splitting — contextual on the active pane's current path.
# `\` and `-` open a terminal; `|` and `_` open a file manager. The
# new pane is seeded to the active terminal's shell cwd, the active
# file manager's `current_dir`, or the project's first worktree if
# the active item exposes no path.
"cmd-k \\" = "codon_session::SplitTerminalRight"
"cmd-k |"  = "codon_session::SplitFileManagerRight"
"cmd-k -"  = "codon_session::SplitTerminalDown"
"cmd-k _"  = "codon_session::SplitFileManagerDown"

# Pane management
"cmd-k t" = "workspace::NewTerminal"
"cmd-k e" = "file_manager::Open"

# Goto-or-open: single chord lands on the most-recently-active pane of
# the requested kind, or opens one in the active pane if none exists.
"cmd-t"       = "codon_session::GotoOrOpenTerminal"
"cmd-e"       = "codon_session::GotoOrOpenFileManager"
"cmd-shift-e" = "codon_session::GotoOrOpenEditor"
# Both cmd-w and cmd-k w fall through codon's safe close cascade (close
# item -> close pane -> close session window -> empty pane). The OS
# window is only ever closed by cmd-shift-w or cmd-shift-q.
"cmd-w"   = "codon_session::SafeCloseActiveItem"
"cmd-k w" = "codon_session::SafeCloseActiveItem"

# Hold cmd-q to quit (Chrome-style). cmd-shift-q is the unconditional
# escape hatch — keep it on hand the first time a process is wedged.
"cmd-q"       = "codon_session::HoldQuit"
"cmd-shift-q" = "zed::Quit"

# Sessions (cmd-k s prefix)
"cmd-k s n" = "codon_session::SessionNew"
"cmd-k s s" = "codon_session::SessionSwitch"
"cmd-k s o" = "codon_session::SessionOverview"
"cmd-k s c" = "codon_session::SessionClose"

# Windows (cmd-k W prefix — capital W, lowercase w is "close active item")
"cmd-k shift-w n" = "codon_session::WindowNew"
"cmd-k shift-w l" = "codon_session::WindowNext"
"cmd-k shift-w h" = "codon_session::WindowPrev"
"cmd-k shift-w shift-l" = "codon_session::WindowLast"
"cmd-k shift-w c" = "codon_session::WindowClose"
"cmd-k shift-w w" = "codon_session::WindowSwitch"
"cmd-k shift-w o" = "codon_session::WindowOverview"
"cmd-k shift-w r" = "codon_session::WindowRename"
"cmd-k shift-w !" = "codon_session::BreakPaneToWindow"

# 2-key motion (muscle-memory path, mirrors tmux prefix n/p/l). The 3-key
# `cmd-k shift-w …` chords above remain the discoverable "windows menu"
# entry point for users browsing the cheatsheet.
"cmd-k n" = "codon_session::WindowNext"
"cmd-k p" = "codon_session::WindowPrev"
"cmd-k l" = "codon_session::WindowLast"
"cmd-k r" = "codon_session::WindowRename"
"cmd-k !" = "codon_session::BreakPaneToWindow"

# Direct index goto — tmux's `prefix 0-9`. 1-based on the keymap to
# match tmux convention, 0-based inside the `WindowGoto(usize)` action.
# Out-of-range indices are silent no-ops, never a panic.
"cmd-k 1" = "codon_session::WindowGoto(0)"
"cmd-k 2" = "codon_session::WindowGoto(1)"
"cmd-k 3" = "codon_session::WindowGoto(2)"
"cmd-k 4" = "codon_session::WindowGoto(3)"
"cmd-k 5" = "codon_session::WindowGoto(4)"
"cmd-k 6" = "codon_session::WindowGoto(5)"
"cmd-k 7" = "codon_session::WindowGoto(6)"
"cmd-k 8" = "codon_session::WindowGoto(7)"
"cmd-k 9" = "codon_session::WindowGoto(8)"

# `ctrl-w` is intentionally left unbound — it's reserved for
# delete-word in insert mode. The window-switch picker lives on
# `cmd-k shift-w w` above; add a chord here only if it does not
# stomp `ctrl-w`.

# Agent (cmd-k a prefix). `cmd-k a a` falls through to the codon-panes
# open-as-pane action — `assistant::FocusAgent` only ever worked when the
# panel was dock-hosted, which Phase 12 retires.
"cmd-k a a" = "codon_panes::OpenAgent"
"cmd-k a e" = "codon_agent::AgentExplain"
"cmd-k a s" = "codon_agent::AgentSummarize"
"cmd-k a r" = "codon_agent::AgentRefactor"

# Git (cmd-k g prefix)
"cmd-k g m" = "git::GenerateCommitMessage"
# Phase 12 — `cmd-k g s` used to bind `git_panel::ToggleFocus`; rebound
# to the codon-panes "open as pane" action so the panel surfaces in the
# active pane split instead of the (now-empty) left dock.
"cmd-k g s" = "codon_panes::OpenGit"

# Panes-from-panels (Phase 12, `cmd-k <chord>` opens as a pane,
# `cmd-k shift-<chord>` peeks the panel in a transient dock surface).
# See `.specs/codon/panes-from-panels.spec.md` for the design contract.
"cmd-k a"       = "codon_panes::OpenAgent"
"cmd-k shift-a" = "codon_panes::PeekAgent"
"cmd-k g"       = "codon_panes::OpenGit"
"cmd-k shift-g" = "codon_panes::PeekGit"
"cmd-k o"       = "codon_panes::OpenOutline"
"cmd-k shift-o" = "codon_panes::PeekOutline"
"cmd-k d"       = "codon_panes::OpenDebug"
"cmd-k shift-d" = "codon_panes::PeekDebug"

# Diff / diagnostics panes (cmd-k d prefix).
# `cmd-k d d` opens the project diff view (working tree vs HEAD) — thin
# wrapper over Zed's `git::Diff`; arbitrary file-vs-file true-diff is
# deferred to phase-4/git-diff-pane.
# `cmd-k d g` opens Zed's project diagnostics view (`g` for "diagnostics"
# leaves `d` itself reserved for the diff viewer above).
"cmd-k d d" = "codon_session::DiffOpen"
"cmd-k d g" = "diagnostics::Deploy"

# Jump-hint overlay (Vimium-style two-keystroke targeting). `cmd-k j`
# covers every visible word / URL / clickable; the URL-only variant
# `cmd-k u` filters to URL candidates and copies the matched one to
# the system clipboard with a toast.
"cmd-k j" = "codon_jump::JumpToTarget"
"cmd-k u" = "codon_jump::JumpToUrl"

# Help / cheatsheet
"cmd-k f1" = "codon_keymap::ShowKeymap"

# Welcome page
"cmd-k f2" = "zed::ShowWelcome"

# Command palette
"cmd-shift-p" = "codon_command_palette::Toggle"

# `:` in Normal mode opens the codon palette (Helix-style)
[bindings.terminal.normal]
":" = "codon_command_palette::Toggle"

[bindings.file_manager.normal]
":" = "codon_command_palette::Toggle"
# `O` (shift-o) opens the choose-opener picker over the entry under the
# cursor — see `crates/file-manager/src/opener_picker.rs`. Raw key
# handling already triggers the picker; this binding keeps `O` visible
# in the cheatsheet and lets users rebind it from `codon.toml`.
"shift-o" = "file_manager::ChooseOpener"

[bindings.editor.normal]
":" = "codon_command_palette::Toggle"
# Helix jump-to-word: visible-region overlay labels, two keystrokes
# jump the cursor. Implementation lives in vendored Zed
# (`vim::HelixJumpToWord`); the upstream `vim.json` already binds
# `g w`, but we list it here so it shows up in the codon cheatsheet
# and can be rebound from `~/.config/codon/codon.toml`. In Visual
# mode the same action extends the selection — no separate binding.
"g w" = "vim::HelixJumpToWord"

# Git panel — Helix-style verbs for the existing Zed git dock. The
# panel publishes pane_mode == normal when the changes list is
# focused, pane_mode == insert when the commit-message editor is.
[bindings.git_panel.normal]
"j" = "git_panel::NextEntry"
"k" = "git_panel::PreviousEntry"
"g g" = "git_panel::FirstEntry"
"shift-g" = "git_panel::LastEntry"
"enter" = "menu::Confirm"
"s" = "git::StageFile"
"u" = "git::UnstageFile"
"space" = "git::ToggleStaged"
"i" = "git_panel::FocusEditor"
":" = "codon_command_palette::Toggle"

[bindings.git_panel.insert]
# Escape from the commit editor back to the changes list. Vim mode
# owns Esc at the editor level — if Helix Normal eats this first,
# rebind to a less-conflicting chord in ~/.config/codon/codon.toml.
"escape" = "git_panel::FocusChanges"

# Peek dock (Phase 12 transient panel surface).
#
# v1 deviation: `PeekDismiss` lives under `[bindings.peek_dock.normal]`
# so a future iteration can scope it under a focus predicate without a
# keymap refactor, but codon-panes does not currently publish the
# `PeekDock && pane_mode == normal` predicate from the peek surface
# (the peek uses Zed's existing dock focus chain). Until that hook
# lands, users dismiss peeks by re-invoking the same `Peek<Name>`
# action (e.g. `cmd-k shift-a` twice). This binding remains as the
# documented shape; rebinding `escape` here in user codon.toml is the
# safe escape hatch.
[bindings.peek_dock.normal]
"escape" = "codon_panes::PeekDismiss"
"#;

/// Load Codon keybindings. Called from reload_keymaps so it survives keymap reloads.
///
/// Load order:
///   1. Embedded defaults (always).
///   2. `~/.config/codon/codon.toml` `[bindings.*]` — the unified file.
///   3. Legacy `~/.config/codon/keymap.toml` — kept as a fall-back for
///      installs that haven't migrated yet (see
///      `TASK:phase-4/unified-config-migration`). A deprecation hint is
///      logged when the legacy file is read.
pub fn load_codon_keymap(cx: &mut App) {
    gpui::set_keystroke_chord_timeout(CHORD_TIMEOUT);

    if let Some(bindings) = parse_keymap(DEFAULT_KEYMAP) {
        apply_bindings(bindings, cx);
    }
    apply_raw_bindings(cx);

    let Some(codon_dir) = codon_config_dir() else {
        return;
    };

    let unified = codon_dir.join("codon.toml");
    if unified.exists() {
        match std::fs::read_to_string(&unified) {
            Ok(content) => match parse_keymap(&content) {
                Some(bindings) => apply_bindings(bindings, cx),
                None => log::warn!(
                    "codon-keymap: failed to parse [bindings.*] in {}",
                    unified.display()
                ),
            },
            Err(err) => log::warn!(
                "codon-keymap: could not read {}: {err}",
                unified.display()
            ),
        }
        return;
    }

    let legacy = codon_dir.join("keymap.toml");
    if legacy.exists() {
        log::info!(
            "codon-keymap: reading legacy {}; migrate to codon.toml when convenient",
            legacy.display()
        );
        match std::fs::read_to_string(&legacy) {
            Ok(content) => match parse_keymap(&content) {
                Some(bindings) => apply_bindings(bindings, cx),
                None => log::warn!("codon-keymap: failed to parse {}", legacy.display()),
            },
            Err(err) => log::warn!(
                "codon-keymap: could not read {}: {err}",
                legacy.display()
            ),
        }
    }
}

/// One entry in the curated codon-keymap surface: a chord string
/// (`"cmd-k s o"`), an action name (`"codon_session::SessionOverview"`),
/// and an optional context predicate the binding fires under.
///
/// Exported so the cheatsheet can filter the global GPUI binding registry
/// down to "only the bindings codon ships by default plus whatever the
/// user added in `~/.config/codon/codon.toml`" — vendor/zed's ~1000+
/// upstream defaults are noise to the codon user.
pub type CuratedBinding = (String, String, Option<String>);

/// Curated codon defaults — every `[bindings.*]` entry in the embedded
/// `DEFAULT_KEYMAP`. Returns an empty vec if the embedded TOML fails to
/// parse (which would be a build-time bug; we still don't panic).
pub fn codon_default_bindings() -> Vec<CuratedBinding> {
    parse_keymap(DEFAULT_KEYMAP).unwrap_or_default()
}

/// Curated user overrides — every `[bindings.*]` entry in the user's
/// `~/.config/codon/codon.toml` (with legacy `keymap.toml` as a fallback).
/// Returns an empty vec if no user file exists or the file is unparsable.
/// Stable across cheatsheet invocations as long as the file doesn't
/// change between opens; we re-read on each call so a fresh edit is
/// reflected without a process restart.
pub fn codon_user_bindings() -> Vec<CuratedBinding> {
    let Some(codon_dir) = codon_config_dir() else {
        return Vec::new();
    };
    let unified = codon_dir.join("codon.toml");
    let legacy = codon_dir.join("keymap.toml");
    let path = if unified.exists() {
        unified
    } else if legacy.exists() {
        legacy
    } else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    parse_keymap(&content).unwrap_or_default()
}

fn parse_keymap(content: &str) -> Option<Vec<(String, String, Option<String>)>> {
    let keymap: CodonKeymap = toml::from_str(content).ok()?;
    let mut result = Vec::new();

    for (keystroke, action) in &keymap.bindings.global {
        result.push((keystroke.clone(), action.clone(), None));
    }

    if let Some(editor) = &keymap.bindings.editor {
        add_mode_bindings(&mut result, "Editor", editor);
    }
    if let Some(terminal) = &keymap.bindings.terminal {
        add_mode_bindings(&mut result, "Terminal", terminal);
    }
    if let Some(fm) = &keymap.bindings.file_manager {
        add_mode_bindings(&mut result, "FileManager", fm);
    }
    if let Some(gp) = &keymap.bindings.git_panel {
        add_mode_bindings(&mut result, "GitPanel", gp);
    }
    if let Some(pd) = &keymap.bindings.peek_dock {
        add_mode_bindings(&mut result, "PeekDock", pd);
    }

    Some(result)
}

fn add_mode_bindings(
    result: &mut Vec<(String, String, Option<String>)>,
    pane_kind: &str,
    modes: &ModeBindings,
) {
    let (normal_pred, insert_pred) = mode_predicates(pane_kind);
    for (keystroke, action) in &modes.normal {
        result.push((keystroke.clone(), action.clone(), Some(normal_pred.clone())));
    }
    for (keystroke, action) in &modes.insert {
        result.push((keystroke.clone(), action.clone(), Some(insert_pred.clone())));
    }
}

/// Map a pane-kind name to the codon Normal / Insert key-context predicates.
///
/// Different panes publish "I am in Normal mode" via different predicates —
/// the editor uses Zed/vim's `vim_mode == normal` (Helix is force-on so this
/// is also the codon editor Normal mode), while the file manager publishes
/// `pane_mode == normal` from `codon-mode`. Codon-keymap centralizes the
/// translation here so user keymap TOML stays uniform.
fn mode_predicates(pane_kind: &str) -> (String, String) {
    match pane_kind {
        // Editor: cover both Vim and Helix Normal modes — Helix is force-on
        // by default but a user can also be in plain Vim Normal. The colon
        // binding (and most codon overrides) want both. Helix Select counts
        // as Normal-ish for our purposes; vim.json binds the same actions
        // under helix_select.
        "Editor" => (
            "vim_mode == normal || vim_mode == helix_normal || vim_mode == helix_select"
                .to_string(),
            "vim_mode == insert".to_string(),
        ),
        other => (
            format!("{other} && pane_mode == normal"),
            format!("{other} && pane_mode == insert"),
        ),
    }
}

fn apply_bindings(bindings: Vec<(String, String, Option<String>)>, cx: &mut App) {
    let mut key_bindings = Vec::new();

    for (keystroke, action_name, context) in bindings {
        if let Some(binding) = build_binding(cx, &keystroke, &action_name, context.as_deref()) {
            key_bindings.push(binding);
        }
    }

    cx.bind_keys(key_bindings);
}

/// Bindings whose context predicate doesn't fit the
/// `[bindings.<pane>.<mode>]` shape get applied directly here. Kept small
/// on purpose — extend `parse_keymap` instead when a pattern becomes
/// general.
fn apply_raw_bindings(cx: &mut App) {
    let mut bindings = Vec::new();
    // `:` opens the codon palette on any non-editor, non-terminal
    // surface: welcome page, onboarding, empty pane, file manager.
    // In an editor we want `:` to only open the palette while in a
    // Helix/Vim normal-ish mode — that's covered by the
    // [bindings.editor.normal] block (which compiles to a `vim_mode`
    // predicate; see `mode_predicates`). The terminal binding lives
    // under its own pane_mode predicate. The `!Editor && !Terminal`
    // predicate mirrors vim.json's own fall-through `:` binding
    // (vim.json line 930) so we know it matches everywhere a
    // bare-context palette is wanted — `Workspace && !Editor` failed
    // to match in the Onboarding focus chain in practice.
    if let Some(binding) = build_binding(
        cx,
        ":",
        "codon_command_palette::Toggle",
        Some("!Editor && !Terminal"),
    ) {
        bindings.push(binding);
    }
    if !bindings.is_empty() {
        cx.bind_keys(bindings);
    }
}

/// Build a [`KeyBinding`] by looking the action up in GPUI's global action
/// registry — no allowlist. Any action declared with `actions!(...)` or
/// `#[derive(Action)]` that's linked into the binary is bindable from codon
/// TOML by its `namespace::Name` string.
///
/// Returns `None` (with a logged warning) for an unknown action name, a
/// malformed keystroke, or a malformed context predicate — the rest of the
/// keymap continues to apply.
fn build_binding(
    cx: &App,
    keystroke: &str,
    action_name: &str,
    context: Option<&str>,
) -> Option<KeyBinding> {
    let (name, params) = match parse_action_spec(action_name) {
        Ok(parsed) => parsed,
        Err(err) => {
            log::warn!("codon-keymap: invalid action spec '{action_name}': {err}");
            return None;
        }
    };
    let action = match cx.build_action(name, params) {
        Ok(action) => action,
        Err(err) => {
            log::warn!("codon-keymap: cannot build action '{action_name}': {err}");
            return None;
        }
    };
    let context_predicate = match context {
        Some(ctx) => match KeyBindingContextPredicate::parse(ctx) {
            Ok(predicate) => Some(Rc::new(predicate)),
            Err(err) => {
                log::warn!(
                    "codon-keymap: invalid context predicate '{ctx}' for '{action_name}': {err}"
                );
                return None;
            }
        },
        None => None,
    };
    match KeyBinding::load(
        keystroke,
        action,
        context_predicate,
        false,
        None,
        &DummyKeyboardMapper,
    ) {
        Ok(binding) => Some(binding),
        Err(err) => {
            log::warn!(
                "codon-keymap: invalid keystroke '{keystroke}' for '{action_name}': {err}"
            );
            None
        }
    }
}

/// Split a TOML action spec into the registered action name and optional
/// JSON-encoded arguments.
///
/// Supports two forms:
///
/// * Bare name — `"codon_session::WindowNext"`. Returns `(name, None)`.
/// * Name with parenthesised args — `"codon_session::WindowGoto(0)"`. The
///   substring between the outermost matching parens is parsed as JSON
///   and forwarded to [`gpui::App::build_action`]; serde then deserialises
///   it into the action's struct (newtype tuple structs like
///   `WindowGoto(usize)` accept the bare inner value, so `(0)` works).
///
/// Errors return a string suitable for `log::warn!`.
fn parse_action_spec(spec: &str) -> Result<(&str, Option<serde_json::Value>), String> {
    let Some(open) = spec.find('(') else {
        return Ok((spec, None));
    };
    if !spec.ends_with(')') {
        return Err(format!("expected closing ')' in action spec '{spec}'"));
    }
    let name = &spec[..open];
    let args = &spec[open + 1..spec.len() - 1];
    let value: serde_json::Value = serde_json::from_str(args)
        .map_err(|err| format!("invalid JSON args '{args}': {err}"))?;
    Ok((name, Some(value)))
}
