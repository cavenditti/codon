use gpui::{App, KeyBinding};
use serde::Deserialize;
use std::{collections::HashMap, time::Duration};

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
# Pane focus (vim-style)
"ctrl-h" = "workspace::ActivatePaneLeft"
"ctrl-j" = "workspace::ActivatePaneDown"
"ctrl-k" = "workspace::ActivatePaneUp"
"ctrl-l" = "workspace::ActivatePaneRight"

# Pane move (swap with adjacent pane)
"ctrl-shift-h" = "workspace::SwapPaneLeft"
"ctrl-shift-j" = "workspace::SwapPaneDown"
"ctrl-shift-k" = "workspace::SwapPaneUp"
"ctrl-shift-l" = "workspace::SwapPaneRight"

# Pane resize (cmd-k prefix to avoid conflict)
"cmd-k shift-h" = "vim::ResizePaneLeft"
"cmd-k shift-j" = "vim::ResizePaneDown"
"cmd-k shift-k" = "vim::ResizePaneUp"
"cmd-k shift-l" = "vim::ResizePaneRight"

# Pane navigation (cmd-k h/j/k/l) — kept for back-compat, equivalent to ctrl-*
"cmd-k h" = "workspace::ActivatePaneLeft"
"cmd-k j" = "workspace::ActivatePaneDown"
"cmd-k k" = "workspace::ActivatePaneUp"
"cmd-k l" = "workspace::ActivatePaneRight"

# Pane splitting
"cmd-k |" = "pane::SplitRight"
"cmd-k -" = "pane::SplitDown"
"cmd-k \\" = "pane::SplitRight"

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
"cmd-k shift-w c" = "codon_session::WindowClose"
"cmd-k shift-w w" = "codon_session::WindowSwitch"
"cmd-k shift-w o" = "codon_session::WindowOverview"

# Fuzzy window picker within the active session (tmux-style ctrl-w w).
"ctrl-w w" = "codon_session::WindowSwitch"

# Agent (cmd-k a prefix)
"cmd-k a a" = "assistant::FocusAgent"
"cmd-k a e" = "codon_agent::AgentExplain"
"cmd-k a s" = "codon_agent::AgentSummarize"
"cmd-k a r" = "codon_agent::AgentRefactor"

# Git (cmd-k g prefix)
"cmd-k g m" = "git::GenerateCommitMessage"
"cmd-k g s" = "git_panel::ToggleFocus"

# Diff / diagnostics panes (cmd-k d prefix).
# `cmd-k d d` opens the project diff view (working tree vs HEAD) — thin
# wrapper over Zed's `git::Diff`; arbitrary file-vs-file true-diff is
# deferred to phase-4/git-diff-pane.
# `cmd-k d g` opens Zed's project diagnostics view (`g` for "diagnostics"
# leaves `d` itself reserved for the diff viewer above).
"cmd-k d d" = "codon_session::DiffOpen"
"cmd-k d g" = "diagnostics::Deploy"

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

[bindings.editor.normal]
":" = "codon_command_palette::Toggle"

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

    let Some(codon_dir) = codon_config::codon_config_dir() else {
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
        if let Some(binding) = resolve_binding(&keystroke, &action_name, context.as_deref()) {
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
    if let Some(binding) = resolve_binding(
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

fn resolve_binding(
    keystroke: &str,
    action_name: &str,
    context: Option<&str>,
) -> Option<KeyBinding> {
    macro_rules! bind {
        ($action:expr) => {
            Some(KeyBinding::new(keystroke, $action, context))
        };
    }
    match action_name {
        // Workspace pane focus
        "workspace::ActivatePaneLeft" => bind!(workspace::ActivatePaneLeft),
        "workspace::ActivatePaneRight" => bind!(workspace::ActivatePaneRight),
        "workspace::ActivatePaneUp" => bind!(workspace::ActivatePaneUp),
        "workspace::ActivatePaneDown" => bind!(workspace::ActivatePaneDown),

        // Workspace pane swap
        "workspace::SwapPaneLeft" => bind!(workspace::SwapPaneLeft),
        "workspace::SwapPaneRight" => bind!(workspace::SwapPaneRight),
        "workspace::SwapPaneUp" => bind!(workspace::SwapPaneUp),
        "workspace::SwapPaneDown" => bind!(workspace::SwapPaneDown),

        // Vim pane resize
        "vim::ResizePaneLeft" => bind!(vim::ResizePaneLeft),
        "vim::ResizePaneRight" => bind!(vim::ResizePaneRight),
        "vim::ResizePaneUp" => bind!(vim::ResizePaneUp),
        "vim::ResizePaneDown" => bind!(vim::ResizePaneDown),

        // Workspace
        "workspace::NewTerminal" => bind!(workspace::NewTerminal { local: false }),

        // Pane
        "pane::SplitRight" => bind!(workspace::pane::SplitRight::default()),
        "pane::SplitDown" => bind!(workspace::pane::SplitDown::default()),
        "pane::SplitLeft" => bind!(workspace::pane::SplitLeft::default()),
        "pane::SplitUp" => bind!(workspace::pane::SplitUp::default()),
        "pane::CloseActiveItem" => bind!(workspace::CloseActiveItem {
            save_intent: None,
            close_pinned: false,
        }),

        // File manager
        "file_manager::Open" => bind!(file_manager::Open),

        // Codon session
        "codon_session::SessionNew" => bind!(codon_session::SessionNew),
        "codon_session::SessionSwitch" => bind!(codon_session::SessionSwitch),
        "codon_session::SessionOverview" => bind!(codon_session::SessionOverview),
        "codon_session::WindowSwitch" => bind!(codon_session::WindowSwitch),
        "codon_session::WindowOverview" => bind!(codon_session::WindowOverview),
        "codon_session::DiffOpen" => bind!(codon_session::DiffOpen),
        "codon_session::SessionClose" => bind!(codon_session::SessionClose),
        "codon_session::SessionRename" => bind!(codon_session::SessionRename),
        "codon_session::WindowNew" => bind!(codon_session::WindowNew),
        "codon_session::WindowNext" => bind!(codon_session::WindowNext),
        "codon_session::WindowPrev" => bind!(codon_session::WindowPrev),
        "codon_session::WindowClose" => bind!(codon_session::WindowClose),
        "codon_session::SafeCloseActiveItem" => bind!(codon_session::SafeCloseActiveItem),
        "codon_session::HoldQuit" => bind!(codon_session::HoldQuit),
        "codon_session::GotoOrOpenTerminal" => bind!(codon_session::GotoOrOpenTerminal),
        "codon_session::GotoOrOpenFileManager" => bind!(codon_session::GotoOrOpenFileManager),
        "codon_session::GotoOrOpenEditor" => bind!(codon_session::GotoOrOpenEditor),
        "zed::Quit" => bind!(zed_actions::Quit),

        // Codon agent
        "codon_agent::AgentExplain" => bind!(codon_agent::AgentExplain),
        "codon_agent::AgentSummarize" => bind!(codon_agent::AgentSummarize),
        "codon_agent::AgentRefactor" => bind!(codon_agent::AgentRefactor),
        "assistant::FocusAgent" => bind!(zed_actions::assistant::FocusAgent),

        // Git
        "git::GenerateCommitMessage" => bind!(git::GenerateCommitMessage),
        "git::StageFile" => bind!(git::StageFile),
        "git::UnstageFile" => bind!(git::UnstageFile),
        "git::ToggleStaged" => bind!(git::ToggleStaged),
        "git_panel::ToggleFocus" => bind!(git_ui::git_panel::ToggleFocus),

        // Diagnostics pane
        "diagnostics::Deploy" => bind!(diagnostics::Deploy),
        "git_panel::FocusEditor" => bind!(git_ui::git_panel::FocusEditor),
        "git_panel::FocusChanges" => bind!(git_ui::git_panel::FocusChanges),
        "git_panel::NextEntry" => bind!(git_ui::git_panel::NextEntry),
        "git_panel::PreviousEntry" => bind!(git_ui::git_panel::PreviousEntry),
        "git_panel::FirstEntry" => bind!(git_ui::git_panel::FirstEntry),
        "git_panel::LastEntry" => bind!(git_ui::git_panel::LastEntry),
        "menu::Confirm" => bind!(menu::Confirm),

        // Help / cheatsheet
        "codon_keymap::ShowKeymap" => bind!(crate::ShowKeymap),

        // Welcome page
        "zed::ShowWelcome" => bind!(workspace::welcome::ShowWelcome),

        // Command palette
        "command_palette::Toggle" => bind!(zed_actions::command_palette::Toggle),
        "codon_command_palette::Toggle" => bind!(codon_command_palette::Toggle),

        _ => {
            log::warn!("Unknown action in keymap: {}", action_name);
            None
        }
    }
}
