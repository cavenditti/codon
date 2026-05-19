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
/// "prefix h" = "workspace::ActivatePaneLeft"
/// "prefix t" = "workspace::NewTerminal"
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
    #[serde(default)]
    keymap: KeymapTopLevel,
}

/// Top-level `[keymap]` table in `codon.toml`. Currently carries
/// only the chord prefix override; new keymap-wide settings should
/// land here rather than under `[bindings.*]`.
#[derive(Debug, Deserialize, Default)]
struct KeymapTopLevel {
    /// Override the embedded defaults' chord prefix. Any chord in
    /// `DEFAULT_KEYMAP` (or in user `[bindings.*]`) whose keystroke
    /// starts with the literal token `prefix` is rebound under this
    /// chord instead. Defaults to [`DEFAULT_PREFIX`] when absent.
    prefix: Option<String>,
}

/// Fallback chord prefix when no `[keymap] prefix = "..."` is set
/// in the user config — kept on `cmd-k` so existing installs see no
/// change after phase 15 lands.
const DEFAULT_PREFIX: &str = "cmd-k";

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
"prefix left"  = "workspace::ActivatePaneLeft"
"prefix down"  = "workspace::ActivatePaneDown"
"prefix up"    = "workspace::ActivatePaneUp"
"prefix right" = "workspace::ActivatePaneRight"

# Pane move (swap with adjacent pane).
"ctrl-shift-h" = "workspace::SwapPaneLeft"
"ctrl-shift-j" = "workspace::SwapPaneDown"
"ctrl-shift-k" = "workspace::SwapPaneUp"
"ctrl-shift-l" = "workspace::SwapPaneRight"

# Pane move (cmd-k + shift-arrow) — terminal-safe alternative to
# ctrl-shift-hjkl. Mirrors the focus chords above.
"prefix shift-left"  = "workspace::SwapPaneLeft"
"prefix shift-down"  = "workspace::SwapPaneDown"
"prefix shift-up"    = "workspace::SwapPaneUp"
"prefix shift-right" = "workspace::SwapPaneRight"

# Pane resize (cmd-k prefix to avoid conflict). The codon_session
# wrappers nudge the pane via vim::ResizePane* and arm a short sticky
# window — see `crates/codon-session/src/resize_sticky.rs`. During
# that window, bare h/j/k/l keep resizing without the prefix; esc /
# any other key / timeout exits.
"prefix shift-h" = "codon_session::ResizePaneLeft"
"prefix shift-j" = "codon_session::ResizePaneDown"
"prefix shift-k" = "codon_session::ResizePaneUp"
"prefix shift-l" = "codon_session::ResizePaneRight"

# Pane navigation (cmd-k h/k/l) — kept for back-compat, equivalent to ctrl-*.
# `cmd-k j` is repurposed for the jump-hint overlay below; use `ctrl-j` for
# pane-down navigation under the cmd-k prefix path. `cmd-k l` is repurposed
# for WindowLast (tmux `prefix l`); use `ctrl-l` for pane-right.
"prefix h" = "workspace::ActivatePaneLeft"
"prefix k" = "workspace::ActivatePaneUp"

# Pane splitting — contextual on the active pane's current path.
# `\` and `-` open a terminal; `|` and `_` open a file manager. The
# new pane is seeded to the active terminal's shell cwd, the active
# file manager's `current_dir`, or the project's first worktree if
# the active item exposes no path.
"prefix \\" = "codon_session::SplitTerminalRight"
"prefix |"  = "codon_session::SplitFileManagerRight"
"prefix -"  = "codon_session::SplitTerminalDown"
"prefix _"  = "codon_session::SplitFileManagerDown"

# Pane management
"prefix t" = "workspace::NewTerminal"
"prefix e" = "file_manager::Open"

# Goto-or-open: single chord lands on the most-recently-active pane of
# the requested kind, or opens one in the active pane if none exists.
"cmd-t"       = "codon_session::GotoOrOpenTerminal"
"cmd-e"       = "codon_session::GotoOrOpenFileManager"
"cmd-shift-e" = "codon_session::GotoOrOpenEditor"
# Both cmd-w and cmd-k w fall through codon's safe close cascade (close
# item -> close pane -> close session window -> empty pane). The OS
# window is only ever closed by cmd-shift-w or cmd-shift-q.
"cmd-w"   = "codon_session::SafeCloseActiveItem"
"prefix w" = "codon_session::SafeCloseActiveItem"

# Hold cmd-q to quit (Chrome-style). cmd-shift-q is the unconditional
# escape hatch — keep it on hand the first time a process is wedged.
"cmd-q"       = "codon_session::HoldQuit"
"cmd-shift-q" = "zed::Quit"

# Sessions (cmd-k s prefix)
"prefix s n" = "codon_session::SessionNew"
"prefix s s" = "codon_session::SessionSwitch"
"prefix s o" = "codon_session::SessionOverview"
"prefix s c" = "codon_session::SessionClose"

# Windows (cmd-k W prefix — capital W, lowercase w is "close active item")
"prefix shift-w n" = "codon_session::WindowNew"
"prefix shift-w l" = "codon_session::WindowNext"
"prefix shift-w h" = "codon_session::WindowPrev"
"prefix shift-w shift-l" = "codon_session::WindowLast"
"prefix shift-w c" = "codon_session::WindowClose"
"prefix shift-w w" = "codon_session::WindowSwitch"
"prefix shift-w o" = "codon_session::WindowOverview"
"prefix shift-w r" = "codon_session::WindowRename"
"prefix shift-w !" = "codon_session::BreakPaneToWindow"

# 2-key motion (muscle-memory path, mirrors tmux prefix n/p/l). The 3-key
# `cmd-k shift-w …` chords above remain the discoverable "windows menu"
# entry point for users browsing the cheatsheet.
"prefix n" = "codon_session::WindowNext"
"prefix p" = "codon_session::WindowPrev"
"prefix l" = "codon_session::WindowLast"
"prefix r" = "codon_session::WindowRename"
"prefix !" = "codon_session::BreakPaneToWindow"

# Direct index goto — tmux's `prefix 0-9`. 1-based on the keymap to
# match tmux convention, 0-based inside the `WindowGoto(usize)` action.
# Out-of-range indices are silent no-ops, never a panic.
"prefix 1" = "codon_session::WindowGoto(0)"
"prefix 2" = "codon_session::WindowGoto(1)"
"prefix 3" = "codon_session::WindowGoto(2)"
"prefix 4" = "codon_session::WindowGoto(3)"
"prefix 5" = "codon_session::WindowGoto(4)"
"prefix 6" = "codon_session::WindowGoto(5)"
"prefix 7" = "codon_session::WindowGoto(6)"
"prefix 8" = "codon_session::WindowGoto(7)"
"prefix 9" = "codon_session::WindowGoto(8)"

# Direct pane→window move — tmux's `join-pane -t :N`. Same 1-based-to-
# 0-based mapping. Closes the source window if its last pane moves
# away. Out-of-range indices surface a toast instead of panicking.
"prefix shift-1" = "codon_session::MovePaneToWindow(0)"
"prefix shift-2" = "codon_session::MovePaneToWindow(1)"
"prefix shift-3" = "codon_session::MovePaneToWindow(2)"
"prefix shift-4" = "codon_session::MovePaneToWindow(3)"
"prefix shift-5" = "codon_session::MovePaneToWindow(4)"
"prefix shift-6" = "codon_session::MovePaneToWindow(5)"
"prefix shift-7" = "codon_session::MovePaneToWindow(6)"
"prefix shift-8" = "codon_session::MovePaneToWindow(7)"
"prefix shift-9" = "codon_session::MovePaneToWindow(8)"

# `ctrl-w` is intentionally left unbound — it's reserved for
# delete-word in insert mode. The window-switch picker lives on
# `cmd-k shift-w w` above; add a chord here only if it does not
# stomp `ctrl-w`.

# Agent (cmd-k a prefix). `cmd-k a a` falls through to the codon-panes
# open-as-pane action — `assistant::FocusAgent` only ever worked when the
# panel was dock-hosted, which Phase 12 retires.
"prefix a a" = "codon_panes::OpenAgent"
"prefix a e" = "codon_agent::AgentExplain"
"prefix a s" = "codon_agent::AgentSummarize"
"prefix a r" = "codon_agent::AgentRefactor"

# Git (cmd-k g prefix)
"prefix g m" = "git::GenerateCommitMessage"
# Phase 12 — `cmd-k g s` used to bind `git_panel::ToggleFocus`; rebound
# to the codon-panes "open as pane" action so the panel surfaces in the
# active pane split instead of the (now-empty) left dock.
"prefix g s" = "codon_panes::OpenGit"

# Panes-from-panels (Phase 12, `cmd-k <chord>` opens as a pane,
# `cmd-k shift-<chord>` peeks the panel in a transient dock surface).
# See `.specs/codon/panes-from-panels.spec.md` for the design contract.
"prefix a"       = "codon_panes::OpenAgent"
"prefix shift-a" = "codon_panes::PeekAgent"
"prefix g"       = "codon_panes::OpenGit"
"prefix shift-g" = "codon_panes::PeekGit"
"prefix o"       = "codon_panes::OpenOutline"
"prefix shift-o" = "codon_panes::PeekOutline"
"prefix d"       = "codon_panes::OpenDebug"
"prefix shift-d" = "codon_panes::PeekDebug"

# Diff / diagnostics panes (cmd-k d prefix).
# `cmd-k d d` opens the project diff view (working tree vs HEAD) — thin
# wrapper over Zed's `git::Diff`; arbitrary file-vs-file true-diff is
# deferred to phase-4/git-diff-pane.
# `cmd-k d g` opens Zed's project diagnostics view (`g` for "diagnostics"
# leaves `d` itself reserved for the diff viewer above).
"prefix d d" = "codon_session::DiffOpen"
"prefix d g" = "diagnostics::Deploy"

# Pickers (`prefix p` prefix — Helix space-mode mirror).
# `p` is the codon mnemonic for "pickers"; the global chord namespace
# makes these reachable from terminal / file-manager panes too, not just
# editors. The letter map matches Helix's `space <letter>` pickers
# (space f / b / s / S / d / D / r). The 2-key `prefix p` leaf above
# (WindowPrev) is shadowed by these chords once the next key arrives,
# mirroring the same `prefix a/g/o/d` leaf-plus-chord pattern used by
# the panes-from-panels section.
"prefix p f"       = "file_finder::Toggle"
"prefix p b"       = "tab_switcher::Toggle"
"prefix p s"       = "outline::Toggle"
"prefix p shift-s" = "project_symbols::Toggle"
"prefix p d"       = "diagnostics::Deploy"
"prefix p shift-d" = "diagnostics::Deploy"
"prefix p r"       = "projects::OpenRecent"
# Codon-owned pickers (Phase 16). `g` / `j` / `'` mirror Helix's
# `space g` / `space j` / `space '`. The actions are registered by
# `codon-pickers`; the singleton stash backing `LastPicker` lives in
# `codon_pickers::last_picker`.
"prefix p g" = "codon_pickers::ChangedFilesPicker"
"prefix p j" = "codon_pickers::JumplistPicker"
"prefix p '" = "codon_pickers::LastPicker"

# Jump-hint overlay (Vimium-style two-keystroke targeting). `cmd-k j`
# covers every visible word / URL / clickable; the URL-only variant
# `cmd-k u` filters to URL candidates and copies the matched one to
# the system clipboard with a toast.
"prefix j" = "codon_jump::JumpToTarget"
"prefix u" = "codon_jump::JumpToUrl"

# Double-prefix passthrough — tmux `send-prefix`. Tapping the
# configured prefix twice writes the literal prefix keystroke into the
# focused terminal's PTY (so an inner vim/emacs/tmux can receive it).
# Silent no-op outside terminals. See `passthrough.rs`.
"prefix prefix" = "codon_keymap::SendPrefixToFocus"

# Help / cheatsheet
"prefix f1" = "codon_keymap::ShowKeymap"

# Welcome page
"prefix f2" = "zed::ShowWelcome"

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

# Helix-mode mirror. Every binding below is already wired in
# `vendor/zed/assets/keymaps/vim.json` under a helix_normal /
# helix_select context — re-binding the same chord to the same
# action under codon's `vim_mode == normal || helix_normal ||
# helix_select` predicate is a no-op for GPUI's binding table
# (it's keyed on predicate + chord), but `codon_default_bindings`
# re-parses this TOML each time the cheatsheet opens, so mirroring
# the bindings here surfaces them in the editor cheatsheet tab.
# See TASK:phase-16/helix-bindings-mirror.

# Motion (display-line variants of j/k; `g j` / `g k` are the
# logical-line escape hatch).
"j"      = "vim::Down({\"display_lines\":true})"
"down"   = "vim::Down({\"display_lines\":true})"
"k"      = "vim::Up({\"display_lines\":true})"
"up"     = "vim::Up({\"display_lines\":true})"
"g j"    = "vim::Down"
"g down" = "vim::Down"
"g k"    = "vim::Up"
"g up"   = "vim::Up"
"h"      = "vim::WrappingLeft"
"left"   = "vim::WrappingLeft"
"l"      = "vim::WrappingRight"
"right"  = "vim::WrappingRight"
"t"       = "vim::PushFindForward({\"before\":true,\"multiline\":true})"
"f"       = "vim::PushFindForward({\"before\":false,\"multiline\":true})"
"shift-t" = "vim::PushFindBackward({\"after\":true,\"multiline\":true})"
"shift-f" = "vim::PushFindBackward({\"after\":false,\"multiline\":true})"
"alt-."   = "vim::RepeatFind"

# Mode entry (helix-flavoured insert/append variants).
"escape"   = "vim::SwitchToHelixNormalMode"
"i"        = "vim::HelixInsert"
"a"        = "vim::HelixAppend"
"shift-a"  = "vim::HelixInsertEndOfLine"
"ctrl-["   = "editor::Cancel"

# Changes
"shift-r" = "editor::Paste"
"`"       = "vim::ConvertToLowerCase"
"alt-`"   = "vim::ConvertToUpperCase"
"insert"  = "vim::InsertBefore"
"shift-u" = "editor::Redo"
"ctrl-r"  = "vim::Redo"
"y"       = "vim::HelixYank"
"p"       = "vim::HelixPaste"
"shift-p" = "vim::HelixPaste({\"before\":true})"
">"       = "vim::Indent"
"<"       = "vim::Outdent"
"="       = "vim::AutoIndent"
"d"       = "vim::HelixDelete"
"alt-d"   = "editor::Delete"
"c"       = "vim::HelixSubstitute"
"alt-c"   = "vim::HelixSubstituteNoYank"

# Selection manipulation
"s"            = "vim::HelixSelectRegex"
"alt-s"        = "editor::SplitSelectionIntoLines({\"keep_selections\":true})"
";"            = "vim::HelixCollapseSelection"
"alt-;"        = "vim::OtherEnd"
","            = "vim::HelixKeepNewestSelection"
"shift-c"      = "vim::HelixDuplicateBelow"
"alt-shift-c"  = "vim::HelixDuplicateAbove"
"%"            = "editor::SelectAll"
"x"            = "vim::HelixSelectLine"
"shift-x"      = "editor::SelectLine"
"ctrl-c"       = "editor::ToggleComments"
"alt-o"        = "editor::SelectLargerSyntaxNode"
"alt-i"        = "editor::SelectSmallerSyntaxNode"
"alt-p"        = "editor::SelectPreviousSyntaxNode"
"alt-n"        = "editor::SelectNextSyntaxNode"

# Search
"n"       = "vim::HelixSelectNext"
"shift-n" = "vim::HelixSelectPrevious"

# Goto-mode chords (`g <verb>`). Helix jump-to-word: visible-region
# overlay labels, two keystrokes jump the cursor. Implementation
# lives in vendored Zed (`vim::HelixJumpToWord`).
"g e"        = "vim::EndOfDocument"
"g h"        = "vim::StartOfLine"
"g l"        = "vim::EndOfLine"
"g s"        = "vim::FirstNonWhitespace"
"g t"        = "vim::WindowTop"
"g c"        = "vim::WindowMiddle"
"g b"        = "vim::WindowBottom"
"g r"        = "editor::FindAllReferences"
"g n"        = "pane::ActivateNextItem"
"shift-l"    = "pane::ActivateNextItem"
"g p"        = "pane::ActivatePreviousItem"
"shift-h"    = "pane::ActivatePreviousItem"
"g w"        = "vim::HelixJumpToWord"
"g ."        = "vim::HelixGotoLastModification"
"g o"        = "editor::ToggleSelectedDiffHunks"
"g shift-o"  = "git::ToggleStaged"
"g shift-r"  = "git::Restore"
"g u"        = "git::StageAndNext"
"g shift-u"  = "git::UnstageAndNext"
"g q"        = "vim::PushRewrap"

# Window mode (helix `space w …`).
"space w v"  = "pane::SplitRight"
"space w s"  = "pane::SplitDown"
"space w h"  = "workspace::ActivatePaneLeft"
"space w j"  = "workspace::ActivatePaneDown"
"space w k"  = "workspace::ActivatePaneUp"
"space w l"  = "workspace::ActivatePaneRight"
"space w q"  = "pane::CloseActiveItem"
"space w r"  = "pane::SplitRight"
"space w d"  = "pane::SplitDown"

# Space mode (helix `space …`).
"space f"        = "file_finder::Toggle"
"space k"        = "editor::Hover"
"space s"        = "outline::Toggle"
"space shift-s"  = "project_symbols::Toggle"
"space d"        = "editor::GoToDiagnostic"
"space r"        = "editor::Rename"
"space a"        = "editor::ToggleCodeActions"
"space h"        = "editor::SelectAllMatches"
"space c"        = "editor::ToggleComments"
"space p"        = "editor::Paste"
"space y"        = "editor::Copy"
"space /"        = "pane::DeploySearch"

# Other
"m"       = "vim::PushHelixMatch"
"]"       = "vim::PushHelixNext({\"around\":true})"
"["       = "vim::PushHelixPrevious({\"around\":true})"
"ctrl-s"  = "editor::SaveLocation"

# Helix shell verbs (REQ:codon/shell-integration). Each opens the
# command palette pre-filled with the verb mnemonic; the
# codon-command-palette completers then prompt for the rest of the
# shell command and dispatch `vim::ShellRun { mode, cmd }`.
"|"       = "vim::ShellPipeSelection"
"alt-|"   = "vim::ShellPipeTo"
"!"       = "vim::ShellInsertOutput"
"alt-!"   = "vim::ShellAppendOutput"
"$"       = "vim::ShellKeepPipe"

# Bindable-now gaps — actions already exist in vendored Zed but
# vim.json doesn't bind them under a helix context. Surfacing
# them here is what TASK:phase-16/helix-bindings-mirror lists as
# the "five gaps" (q/Q, ()/), &, g f, g d/g i/g y).
"q"        = "vim::ToggleRecord"
"shift-q"  = "vim::ReplayLastRecording"
"("        = "editor::RotateSelectionsBackward"
")"        = "editor::RotateSelectionsForward"
"&"        = "editor::AlignSelections"
"g f"      = "editor::OpenSelectedFilename"
"g d"      = "editor::GoToDefinition"
"g i"      = "editor::GoToImplementation"
"g y"      = "editor::GoToTypeDefinition"

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

    let prefix = resolve_prefix();

    if let Some(bindings) = parse_keymap(DEFAULT_KEYMAP) {
        apply_bindings(expand_prefix_in_bindings(bindings, &prefix), cx);
    }
    apply_raw_bindings(cx);

    let Some(codon_dir) = codon_config_dir() else {
        return;
    };

    let unified = codon_dir.join("codon.toml");
    if unified.exists() {
        match std::fs::read_to_string(&unified) {
            Ok(content) => match parse_keymap(&content) {
                Some(bindings) => {
                    apply_bindings(expand_prefix_in_bindings(bindings, &prefix), cx)
                }
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
                Some(bindings) => {
                    apply_bindings(expand_prefix_in_bindings(bindings, &prefix), cx)
                }
                None => log::warn!("codon-keymap: failed to parse {}", legacy.display()),
            },
            Err(err) => log::warn!(
                "codon-keymap: could not read {}: {err}",
                legacy.display()
            ),
        }
    }
}

/// Resolve the chord prefix to use when expanding the `prefix` sentinel
/// in `DEFAULT_KEYMAP` and user `[bindings.*]` chords.
///
/// Order: user `codon.toml` `[keymap] prefix` → legacy `keymap.toml`
/// `[keymap] prefix` → [`DEFAULT_PREFIX`]. An empty string in either
/// file is treated as "unset" and falls back to the default.
///
/// Kept pub(crate) rather than `pub` because the only consumers today
/// live in this crate (loader + curated-binding accessors); promote
/// to `pub` once a downstream crate needs the resolved value
/// (e.g. the passthrough handler in
/// `TASK:phase-15/keymap-prefix-passthrough`).
pub(crate) fn resolve_prefix() -> String {
    let Some(codon_dir) = codon_config_dir() else {
        return DEFAULT_PREFIX.to_string();
    };
    for filename in ["codon.toml", "keymap.toml"] {
        let path = codon_dir.join(filename);
        if !path.exists() {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(parsed) = toml::from_str::<CodonKeymap>(&content) else {
            continue;
        };
        if let Some(prefix) = parsed.keymap.prefix.filter(|s| !s.is_empty()) {
            return prefix;
        }
    }
    DEFAULT_PREFIX.to_string()
}

/// Expand the leading `prefix` token in every keystroke string.
///
/// `"prefix c"` becomes `"<prefix> c"` (e.g. `"cmd-k c"` or
/// `"ctrl-x c"`). The bare keystroke `"prefix"` — used by
/// `TASK:phase-15/keymap-prefix-passthrough` for the `prefix prefix`
/// double-tap — expands to `"<prefix>"`. Any keystroke whose first
/// space-delimited token is not exactly `prefix` passes through
/// unchanged (so a literal `"alt-prefix"` chord or a user binding
/// like `"cmd-shift-s"` is unaffected).
fn expand_prefix_in_bindings(
    bindings: Vec<(String, String, Option<String>)>,
    prefix: &str,
) -> Vec<(String, String, Option<String>)> {
    bindings
        .into_iter()
        .map(|(keystroke, action, context)| (expand_prefix(&keystroke, prefix), action, context))
        .collect()
}

fn expand_prefix(keystroke: &str, prefix: &str) -> String {
    match keystroke.split_once(' ') {
        Some(("prefix", rest)) => format!("{prefix} {rest}"),
        Some(_) => keystroke.to_string(),
        None if keystroke == "prefix" => prefix.to_string(),
        None => keystroke.to_string(),
    }
}

/// One entry in the curated codon-keymap surface: a chord string
/// (`"prefix s o"`), an action name (`"codon_session::SessionOverview"`),
/// and an optional context predicate the binding fires under.
///
/// Exported so the cheatsheet can filter the global GPUI binding registry
/// down to "only the bindings codon ships by default plus whatever the
/// user added in `~/.config/codon/codon.toml`" — vendor/zed's ~1000+
/// upstream defaults are noise to the codon user.
pub type CuratedBinding = (String, String, Option<String>);

/// Curated codon defaults — every `[bindings.*]` entry in the embedded
/// `DEFAULT_KEYMAP`, with the `prefix` sentinel expanded using the
/// user's resolved chord prefix. Returns an empty vec if the embedded
/// TOML fails to parse (which would be a build-time bug; we still
/// don't panic).
pub fn codon_default_bindings() -> Vec<CuratedBinding> {
    let prefix = resolve_prefix();
    expand_prefix_in_bindings(parse_keymap(DEFAULT_KEYMAP).unwrap_or_default(), &prefix)
}

/// Curated user overrides — every `[bindings.*]` entry in the user's
/// `~/.config/codon/codon.toml` (with legacy `keymap.toml` as a fallback),
/// with the `prefix` sentinel expanded using the user's resolved chord
/// prefix. Returns an empty vec if no user file exists or the file is
/// unparsable. Stable across cheatsheet invocations as long as the
/// file doesn't change between opens; we re-read on each call so a
/// fresh edit is reflected without a process restart.
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
    let prefix = resolve_prefix();
    expand_prefix_in_bindings(parse_keymap(&content).unwrap_or_default(), &prefix)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `prefix c` under the fallback prefix expands to `cmd-k c` — the
    /// `c-prefix-configurable` invariant that existing installs see no
    /// change.
    #[test]
    fn prefix_default_substitutes_cmd_k() {
        let bindings = expand_prefix_in_bindings(
            vec![("prefix c".to_string(), "codon_session::WindowNew".to_string(), None)],
            DEFAULT_PREFIX,
        );
        assert_eq!(bindings[0].0, "cmd-k c");
    }

    /// With a user override the same sentinel re-keys atomically.
    #[test]
    fn prefix_override_rekeys_defaults() {
        let bindings = expand_prefix_in_bindings(
            vec![
                ("prefix c".to_string(), "codon_session::WindowNew".to_string(), None),
                ("prefix shift-w n".to_string(), "codon_session::WindowNew".to_string(), None),
                ("prefix \\".to_string(), "codon_session::SplitTerminalRight".to_string(), None),
            ],
            "ctrl-x",
        );
        assert_eq!(bindings[0].0, "ctrl-x c");
        assert_eq!(bindings[1].0, "ctrl-x shift-w n");
        assert_eq!(bindings[2].0, "ctrl-x \\");
    }

    /// User bindings use the same expansion as the defaults.
    #[test]
    fn prefix_substitutes_in_user_bindings() {
        let toml = r#"
            [keymap]
            prefix = "ctrl-x"

            [bindings.global]
            "prefix t" = "workspace::NewTerminal"
        "#;
        let parsed = parse_keymap(toml).expect("parses");
        let expanded = expand_prefix_in_bindings(parsed, "ctrl-x");
        assert!(
            expanded.iter().any(|(k, _, _)| k == "ctrl-x t"),
            "user binding should expand to 'ctrl-x t', got {expanded:?}"
        );
    }

    /// Non-`prefix` chords pass through untouched.
    #[test]
    fn non_prefix_chord_unchanged() {
        let bindings = expand_prefix_in_bindings(
            vec![
                ("cmd-shift-s".to_string(), "codon_session::SessionSwitch".to_string(), None),
                ("ctrl-l".to_string(), "workspace::ActivatePaneRight".to_string(), None),
                // Hypothetical chord that *contains* the literal token
                // `prefix` but not as the leading word.
                ("alt-prefix".to_string(), "noop".to_string(), None),
            ],
            "ctrl-x",
        );
        assert_eq!(bindings[0].0, "cmd-shift-s");
        assert_eq!(bindings[1].0, "ctrl-l");
        assert_eq!(bindings[2].0, "alt-prefix");
    }

    /// Bare `"prefix"` (used by the double-tap passthrough in the sibling
    /// `c-prefix-passthrough` task) expands to the resolved prefix chord
    /// without a trailing space.
    #[test]
    fn prefix_bare_token_expands() {
        let bindings = expand_prefix_in_bindings(
            vec![("prefix".to_string(), "codon_keymap::SendPrefixToFocus".to_string(), None)],
            "ctrl-x",
        );
        assert_eq!(bindings[0].0, "ctrl-x");
    }

    /// The `[keymap]` table parses cleanly alongside `[bindings.*]`.
    #[test]
    fn keymap_table_parses_with_bindings() {
        let toml = r#"
            [keymap]
            prefix = "ctrl-x"

            [bindings.global]
            "cmd-shift-s" = "codon_session::SessionSwitch"
        "#;
        let parsed: CodonKeymap = toml::from_str(toml).expect("parses");
        assert_eq!(parsed.keymap.prefix.as_deref(), Some("ctrl-x"));
        assert_eq!(parsed.bindings.global.len(), 1);
    }

    /// Mirroring helix-mode bindings into `[bindings.editor.normal]`
    /// (see TASK:phase-16/helix-bindings-mirror) lifted the editor
    /// section from a handful of entries to north of 60. Lock the
    /// floor in so a future trim doesn't silently strip the
    /// cheatsheet back to "`:` and `g w`".
    #[test]
    fn mirrored_helix_bindings_appear_in_default_bindings() {
        let parsed = parse_keymap(DEFAULT_KEYMAP).expect("DEFAULT_KEYMAP parses");
        let editor_normal_pred = mode_predicates("Editor").0;
        let editor_normal_count = parsed
            .iter()
            .filter(|(_, _, ctx)| ctx.as_deref() == Some(editor_normal_pred.as_str()))
            .count();
        assert!(
            editor_normal_count >= 60,
            "expected ≥60 mirrored editor.normal bindings, got {editor_normal_count}"
        );

        // Spot-check a handful of representative chords to catch a
        // wholesale rewrite that happens to keep the count up.
        let editor_normal_keys: Vec<&str> = parsed
            .iter()
            .filter(|(_, _, ctx)| ctx.as_deref() == Some(editor_normal_pred.as_str()))
            .map(|(k, _, _)| k.as_str())
            .collect();
        for expected in [
            "d", "c", "y", "p", "s", ";", ",", "x", "m", "]", "[", "g e", "g h",
            "space f", "space d", "q", "(", ")", "&", "g f", "g d", "g i", "g y",
        ] {
            assert!(
                editor_normal_keys.contains(&expected),
                "editor.normal should contain '{expected}'; got {editor_normal_keys:?}"
            );
        }
    }

    /// An empty `prefix = ""` falls back to the default — guards against
    /// a misconfigured file silently disabling every chord.
    #[test]
    fn empty_prefix_falls_back_to_default() {
        let toml = r#"
            [keymap]
            prefix = ""
        "#;
        let parsed: CodonKeymap = toml::from_str(toml).expect("parses");
        assert_eq!(parsed.keymap.prefix.as_deref(), Some(""));
        // resolve_prefix would filter this; emulate that contract:
        let resolved = parsed.keymap.prefix.filter(|s| !s.is_empty());
        assert!(resolved.is_none());
    }
}
