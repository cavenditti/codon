use codon_pane_bridge::CodonGlanceTable;
use gpui::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate, SharedString};
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
    #[serde(default)]
    glance: GlanceTable,
}

/// `[glance.<pane>.<mode>]` table — curated 3–5-verb hints surfaced by
/// the status-bar mode indicator on every pane focus / mode transition
/// (REQ:codon/discoverability#c-status-bar-mode-glance).
///
/// Shape:
/// ```toml
/// [glance.editor.normal]
/// verbs = ["d (delete)", "c (change)", "y (yank)", "s (select)", "?"]
/// ```
///
/// Each pane carries optional `normal` / `insert` sub-tables; absent
/// pairs render no glance. An explicit empty `verbs = []` is the
/// user-visible escape hatch for "hide the glance here".
#[derive(Debug, Deserialize, Default, Clone)]
struct GlanceTable {
    #[serde(default)]
    editor: Option<GlancePane>,
    #[serde(default)]
    terminal: Option<GlancePane>,
    #[serde(default)]
    file_manager: Option<GlancePane>,
    #[serde(default)]
    git_panel: Option<GlancePane>,
    #[serde(default)]
    peek_dock: Option<GlancePane>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct GlancePane {
    #[serde(default)]
    normal: Option<GlanceVerbs>,
    #[serde(default)]
    insert: Option<GlanceVerbs>,
}

#[derive(Debug, Deserialize, Default, Clone)]
struct GlanceVerbs {
    #[serde(default)]
    verbs: Vec<String>,
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
    /// `[bindings.global]` — chords with no pane-mode predicate; fire
    /// anywhere (including modal contexts that don't publish a
    /// `pane_mode`). Bare keystrokes plus the nested `[bindings.global.normal]`
    /// table live here; the loader splits them in `add_global_bindings`.
    #[serde(default)]
    global: GlobalTable,
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

/// `[bindings.global]` table. Bare chord entries live in `flat`;
/// `[bindings.global.normal]` lands in `normal` and compiles to the
/// union Normal-mode predicate so a single chord covers every pane
/// kind's Normal mode (editor's vim/helix Normal + every codon
/// `pane_mode == normal`). See `global_normal_predicate`.
#[derive(Debug, Default)]
struct GlobalTable {
    flat: HashMap<String, String>,
    normal: HashMap<String, String>,
}

impl<'de> Deserialize<'de> for GlobalTable {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // TOML lets us mix `key = "value"` rows with nested tables in
        // the same section. Deserialize as a permissive map, then split
        // the `normal` sub-table out from the flat entries.
        let raw: HashMap<String, toml::Value> = HashMap::deserialize(deserializer)?;
        let mut flat = HashMap::new();
        let mut normal = HashMap::new();
        for (k, v) in raw {
            match v {
                toml::Value::String(s) => {
                    flat.insert(k, s);
                }
                toml::Value::Table(t) if k == "normal" => {
                    for (kk, vv) in t {
                        if let Some(s) = vv.as_str() {
                            normal.insert(kk, s.to_string());
                        }
                    }
                }
                _ => {
                    // Skip unknown shapes (e.g. nested non-normal tables)
                    // silently; load_codon_keymap surfaces a single warn
                    // per-keymap parse if the whole table fails.
                }
            }
        }
        Ok(GlobalTable { flat, normal })
    }
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

# Pane splitting — contextual on the active pane's focus and current
# path. `\` / `-` open whatever kind the active pane already is
# (terminal-focused → terminal, fm-focused → fm, editor-focused →
# editor). `|` / `_` flip the kind in the terminal ↔ file-manager
# pair (editor focus resolves to terminal). New pane is seeded to the
# active terminal's shell cwd, the active file manager's `current_dir`,
# or the project's first worktree if the active item exposes no path.
"prefix \\" = "codon_session::SplitRight"
"prefix |"  = "codon_session::SplitRightOther"
"prefix -"  = "codon_session::SplitDown"
"prefix _"  = "codon_session::SplitDownOther"

# Pane management — `prefix t` / `prefix e` mirror `cmd-t` / `cmd-e`
# (goto-or-open). `prefix shift-t` / `prefix shift-e` are the
# always-new variants for users who want a fresh pane regardless of
# whether one already exists.
"prefix t"       = "codon_session::GotoOrOpenTerminal"
"prefix e"       = "codon_session::GotoOrOpenFileManager"
"prefix shift-t" = "workspace::NewTerminal"
"prefix shift-e" = "file_manager::OpenNew"

# Goto-or-open: single chord lands on the most-recently-active pane of
# the requested kind, or opens one in the active pane if none exists.
"cmd-t"       = "codon_session::GotoOrOpenTerminal"
"cmd-e"       = "codon_session::GotoOrOpenFileManager"
"cmd-shift-e" = "codon_session::GotoOrOpenEditor"
# `cmd-w` is the single user-facing close verb across pane kinds.
# Falls through codon's safe close cascade (close item -> close pane
# -> close session window -> empty pane). The OS window is only ever
# closed by `cmd-shift-w` or `cmd-shift-q`. The previous `prefix w`
# close binding is gone — `prefix w` is now the windows sub-prefix.
"cmd-w"   = "codon_session::Close"

# Hold cmd-q to quit (Chrome-style). cmd-shift-q is the unconditional
# escape hatch — keep it on hand the first time a process is wedged.
"cmd-q"       = "codon_session::HoldQuit"
"cmd-shift-q" = "zed::Quit"

# Sessions (cmd-k s prefix)
"prefix s n" = "codon_session::SessionNew"
"prefix s s" = "codon_session::SessionSwitch"
"prefix s o" = "codon_session::SessionOverview"
"prefix s c" = "codon_session::SessionClose"

# Windows (`prefix w …` sub-prefix). The single chord `prefix shift-w`
# opens the window overview directly; the discoverable menu lives
# under `prefix w …`.
"prefix shift-w" = "codon_session::WindowOverview"
"prefix w n"         = "codon_session::WindowNew"
"prefix w l"         = "codon_session::WindowNext"
"prefix w h"         = "codon_session::WindowPrev"
"prefix w shift-l"   = "codon_session::WindowLast"
"prefix w c"         = "codon_session::WindowClose"
"prefix w w"         = "codon_session::WindowSwitch"
"prefix w r"         = "codon_session::WindowRename"
"prefix w !"         = "codon_session::BreakPaneToWindow"

# 2-key motion (muscle-memory path, mirrors tmux prefix n/p). `prefix l`
# (the WindowLast bare-leaf) was dropped per
# REQ:codon/keymap-vocabulary#c-chord-window-nav-leaves — WindowLast
# remains reachable via `prefix w shift-l`. The 3-key `prefix w …`
# chords above are the discoverable "windows menu" entry point.
"prefix n" = "codon_session::WindowNext"
"prefix p" = "codon_session::WindowPrev"
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

# Pickers live under the global `space`-leader flow below
# (`[bindings.global.normal]`), so they fire from every pane kind whose
# Normal mode is published — terminal, file-manager, git-panel, editor.
# The previous `prefix p X` chain was removed in phase 20; the bare
# `prefix p` leaf above (WindowPrev) stays as the tmux muscle-memory
# path.

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

# Action-history picker — `prefix ;` opens a fuzzy picker over the
# last ~10 non-motion actions; arrow-keys + enter re-fires the
# highlighted one against the currently focused pane. The 1-key
# repeat `.` lives in `[bindings.global.normal]` below so it fires
# in every pane kind whose Normal mode is published.
"prefix ;" = "codon_history::HistoryPicker"

# Global Normal-mode bindings — fire in every pane kind whose Normal
# mode is published (editor's vim/helix Normal + each codon
# `pane_mode == normal`). See REQ:codon/keymap-vocabulary#c-leader-pickers.
[bindings.global.normal]
# Action-history repeat. Re-fires the last non-motion action against
# the currently focused pane. The bare `.` chord is freed across
# every Normal-mode pane by REQ:codon/keymap-vocabulary#c-fm-hidden-rebind
# (the file-manager toggle-hidden binding moved to `, h`).
"." = "codon_history::RepeatLast"

# Space-leader pickers. The letter map matches Helix's `space <letter>`
# pickers (`space f / b / s / S / d / r`) plus the codon-owned
# `space g / j / '` chords introduced in Phase 16.
"space f"       = "file_finder::Toggle"
"space b"       = "tab_switcher::Toggle"
"space s"       = "outline::Toggle"
"space shift-s" = "project_symbols::Toggle"
"space d"       = "diagnostics::Deploy"
"space r"       = "projects::OpenRecent"
"space g"       = "codon_pickers::ChangedFilesPicker"
"space j"       = "codon_pickers::JumplistPicker"
"space '"       = "codon_pickers::LastPicker"

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
# `, h` toggles the show-hidden flag. Joined the `,` view-options
# sub-prefix that already hosts the sort chords so the bare `.` chord
# is free for the global action-history repeat. The fm's raw `,` chord
# handler routes through `handle_sort_chord` which now dispatches `h`
# to ToggleHidden. The binding here exists for cheatsheet visibility
# and to let users rebind from `codon.toml`.
", h" = "file_manager::ToggleHidden"

# Phase-19 object-grammar verbs. The fm is the proof-of-concept pane
# implementation — `w` / `b` cursor-step over files; `mip` / `map` mark
# the whole containing directory; `%f` marks every visible row. See
# `crates/codon-pane-bridge/src/object_grammar.rs` for the trait and
# `REQ:codon/object-grammar` for the cross-pane design. Other pane
# kinds opt in by implementing `ObjectGrammar` + wiring their own
# `on_action` handlers (follow-up tasks).
"w"     = "codon_panes::ObjectNext"
"b"     = "codon_panes::ObjectPrev"
"m i p" = "codon_panes::InnerContainer(\"file\")"
"m a p" = "codon_panes::AroundContainer(\"file\")"
"% f"   = "codon_panes::SelectAll(\"file\")"

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

# Window mode (helix `space w …`). `space w q` rebound to the codon
# safe-close cascade per REQ:codon/keymap-vocabulary#c-verb-collapse-close
# so the Helix mirror and `cmd-w` produce identical behaviour.
# `space w v` / `space w s` route through the contextual split actions
# so the new pane's kind tracks the active pane's focus.
"space w v"  = "codon_session::SplitRight"
"space w s"  = "codon_session::SplitDown"
"space w h"  = "workspace::ActivatePaneLeft"
"space w j"  = "workspace::ActivatePaneDown"
"space w k"  = "workspace::ActivatePaneUp"
"space w l"  = "workspace::ActivatePaneRight"
"space w q"  = "codon_session::Close"
"space w r"  = "codon_session::SplitRight"
"space w d"  = "codon_session::SplitDown"

# Space mode (helix `space …`). Picker letters (f / b / s / S / d / r /
# g / j / ') are bound under `[bindings.global.normal]` below so they
# fire from every pane Normal mode, not just the editor; the editor-
# specific chords below are the verbs that need an editor cursor.
# `space d` (previously `editor::GoToDiagnostic`) is unbound here so
# the global `space d` picker wins; `]d` / `[d` cover the next/prev-
# diagnostic motion via Helix's standard `]/[` chord vocabulary.
# `space r` (previously `editor::Rename`) is unbound here so the
# global `space r` recent-projects picker wins; LSP rename remains
# reachable from the code-actions chord (`space a`) and the command
# palette.
"space k"        = "editor::Hover"
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

# Status-bar mode glance — curated 3–5-verb hints rendered on every
# pane focus / mode transition. Decays after ~2 s or the next
# non-motion keypress. Override per pane × mode in
# ~/.config/codon/codon.toml; set `verbs = []` to hide the glance for
# a given pane × mode. See REQ:codon/discoverability#c-status-bar-mode-glance.
[glance.editor.normal]
verbs = ["d (delete)", "c (change)", "y (yank)", "s (select)", "?"]

[glance.terminal.normal]
verbs = ["w (next block)", "b (prev block)", "y (copy)", ":"]

[glance.file_manager.normal]
verbs = ["j/k (move)", "enter (open)", "y (yank path)", ", h (hidden)", ":"]

[glance.git_panel.normal]
verbs = ["j/k (move)", "s (stage)", "u (unstage)", "i (msg)", ":"]
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
    apply_register_prefix_bindings(cx);
    publish_glance_table(cx);

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

/// Status-bar mode-glance verbs for the given `pane_kind` × `mode`
/// pair. The keys are the snake-case forms used in the `[glance]`
/// TOML table — pane kinds are `editor`, `terminal`, `file_manager`,
/// `git_panel`, `peek_dock`; modes are `normal` and `insert`.
///
/// Resolution order matches `load_codon_keymap`:
///   1. Embedded `DEFAULT_KEYMAP` `[glance.*]` block — the curated
///      starting point shipped with codon.
///   2. User `~/.config/codon/codon.toml` (with legacy `keymap.toml`
///      as a fall-back) — last writer wins, so an explicit empty
///      `verbs = []` row hides the glance for that pair (the
///      user-visible escape hatch per the spec).
///
/// Returns an empty vec when no entry exists or when the user
/// explicitly emptied the row.
pub fn codon_glance_verbs(pane_kind: &str, mode: &str) -> Vec<String> {
    let mut merged: HashMap<String, Vec<String>> = HashMap::new();
    for content in glance_sources() {
        let Ok(parsed) = toml::from_str::<CodonKeymap>(&content) else {
            continue;
        };
        merge_glance(&mut merged, &parsed.glance);
    }
    let key = format!("{pane_kind}.{mode}");
    merged.remove(&key).unwrap_or_default()
}

/// Source TOML strings consulted by `codon_glance_verbs` and
/// `publish_glance_table`, ordered embedded-first so user files take
/// precedence.
fn glance_sources() -> Vec<String> {
    let mut sources = vec![DEFAULT_KEYMAP.to_string()];
    let Some(codon_dir) = codon_config_dir() else {
        return sources;
    };
    for filename in ["codon.toml", "keymap.toml"] {
        let path = codon_dir.join(filename);
        if !path.exists() {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            sources.push(content);
        }
    }
    sources
}

fn merge_glance(acc: &mut HashMap<String, Vec<String>>, table: &GlanceTable) {
    let pairs = [
        ("editor", &table.editor),
        ("terminal", &table.terminal),
        ("file_manager", &table.file_manager),
        ("git_panel", &table.git_panel),
        ("peek_dock", &table.peek_dock),
    ];
    for (pane, pane_table) in pairs {
        let Some(pane_table) = pane_table else {
            continue;
        };
        if let Some(normal) = &pane_table.normal {
            acc.insert(format!("{pane}.normal"), normal.verbs.clone());
        }
        if let Some(insert) = &pane_table.insert {
            acc.insert(format!("{pane}.insert"), insert.verbs.clone());
        }
    }
}

/// Resolve the merged glance table and publish it to the
/// `CodonGlanceTable` App global so the status-bar mode indicator
/// (in `codon-mode`) can read it without a cyclic dep on this crate.
/// Called from `load_codon_keymap` after bindings are applied.
fn publish_glance_table(cx: &mut App) {
    let mut merged: HashMap<String, Vec<String>> = HashMap::new();
    for content in glance_sources() {
        let Ok(parsed) = toml::from_str::<CodonKeymap>(&content) else {
            continue;
        };
        merge_glance(&mut merged, &parsed.glance);
    }
    let entries = merged
        .into_iter()
        .map(|(k, v)| {
            let verbs: Vec<SharedString> = v.into_iter().map(SharedString::from).collect();
            (k, verbs)
        })
        .collect();
    cx.set_global(CodonGlanceTable { entries });
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

    for (keystroke, action) in &keymap.bindings.global.flat {
        result.push((keystroke.clone(), action.clone(), None));
    }
    let global_normal_pred = global_normal_predicate();
    for (keystroke, action) in &keymap.bindings.global.normal {
        result.push((
            keystroke.clone(),
            action.clone(),
            Some(global_normal_pred.clone()),
        ));
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

/// Union of Normal-mode predicates across every pane kind that publishes
/// one. Used to compile `[bindings.global.normal]` chords so a single
/// `space f` chord works from a terminal, file-manager, git-panel,
/// agent / outline / debug pane, or editor without per-pane
/// duplication. Centralised here so the `space`-leader pickers don't
/// drift from the rest of the `global.normal` surface.
fn global_normal_predicate() -> String {
    let editor = "(Editor && (vim_mode == normal || vim_mode == helix_normal || vim_mode == helix_select))";
    let panes = [
        "Terminal",
        "FileManager",
        "GitPanel",
        "AgentPanel",
        "OutlinePanel",
        "DebugPanel",
    ];
    let mut parts = vec![editor.to_string()];
    for p in panes {
        parts.push(format!("({p} && pane_mode == normal)"));
    }
    parts.join(" || ")
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

/// Register the `"<char>` Normal-mode register-prefix bindings.
///
/// Each printable name in [`REGISTER_NAME_ALPHABET`] is bound to its
/// own `codon_registers::SelectRegister("<char>")` payload — the
/// dispatcher consumes it via the workspace action handler installed
/// in `codon-session::registers::register_for_workspace`.
///
/// We enumerate rather than parse on-demand so the binding sits in the
/// regular `cx.bind_keys` pipeline (and shows up in the cheatsheet);
/// the alphabet is small enough (~40 entries) that the size cost is
/// trivial. The named slots (`"`, `_`, `+`, `*`, `-`) are bound only
/// when their keystroke representation is stable across keyboards —
/// `_`, `+`, `*`, `-` are. The unnamed default register `"`-then-`"`
/// is deferred to the follow-up task that owns the default-register
/// semantics (`REQ:codon/selection-registers#c-default-register`).
fn apply_register_prefix_bindings(cx: &mut App) {
    let mut bindings = Vec::new();
    for c in REGISTER_NAME_ALPHABET {
        let keystroke = format!("\" {c}");
        let action_payload = format!("codon_registers::SelectRegister(\"{c}\")");
        // Bound at the global predicate — codon's Normal-mode handling
        // is per-pane in `[bindings.*.normal]` but the register arming
        // is genuinely workspace-level (the next verb can land in any
        // pane). The dispatcher consumes the arming itself; there's no
        // pane-mode predicate to scope here.
        if let Some(binding) = build_binding(cx, &keystroke, &action_payload, None) {
            bindings.push(binding);
        }
    }
    if !bindings.is_empty() {
        cx.bind_keys(bindings);
    }
}

/// Single-char register-name alphabet bound by [`apply_register_prefix_bindings`].
/// Lowercase + uppercase ASCII + digits + the small Helix-named slots
/// the register store allows (see `RegisterName::try_new`). The unnamed
/// `"` default register is deferred — see the function's doc.
const REGISTER_NAME_ALPHABET: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z',
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S',
    'T', 'U', 'V', 'W', 'X', 'Y', 'Z',
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
    '_', '+', '*', '-',
];

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
        assert_eq!(parsed.bindings.global.flat.len(), 1);
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

    /// The embedded `[glance.*]` table parses and exposes verbs for
    /// every curated pane × mode pair listed in the spec. Locks in
    /// the curated starting point so a future trim of DEFAULT_KEYMAP
    /// doesn't silently zero out the status-bar glance.
    #[test]
    fn glance_embedded_defaults_have_verbs() {
        let parsed: CodonKeymap = toml::from_str(DEFAULT_KEYMAP).expect("DEFAULT_KEYMAP parses");
        let mut merged: HashMap<String, Vec<String>> = HashMap::new();
        merge_glance(&mut merged, &parsed.glance);
        for key in ["editor.normal", "terminal.normal", "file_manager.normal", "git_panel.normal"] {
            let verbs = merged.get(key).unwrap_or_else(|| panic!("missing glance row for {key}"));
            assert!(
                !verbs.is_empty(),
                "embedded glance row '{key}' should ship with verbs, got empty"
            );
            assert!(
                verbs.len() <= 5,
                "embedded glance row '{key}' should have at most 5 verbs, got {}",
                verbs.len()
            );
        }
    }

    /// A user `[glance.editor.normal] verbs = []` overrides the
    /// embedded default — the escape-hatch contract.
    #[test]
    fn glance_user_empty_overrides_default() {
        let user = r#"
            [glance.editor.normal]
            verbs = []
        "#;
        let mut merged: HashMap<String, Vec<String>> = HashMap::new();
        let default: CodonKeymap = toml::from_str(DEFAULT_KEYMAP).expect("DEFAULT_KEYMAP parses");
        merge_glance(&mut merged, &default.glance);
        let user_parsed: CodonKeymap = toml::from_str(user).expect("user toml parses");
        merge_glance(&mut merged, &user_parsed.glance);
        assert!(
            merged.get("editor.normal").expect("row exists").is_empty(),
            "user empty verbs = [] should win and zero the row"
        );
        // Unrelated rows untouched by the override.
        assert!(!merged.get("terminal.normal").expect("row exists").is_empty());
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
