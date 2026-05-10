# Codon — Full Implementation Todo List

## Current State Summary

A Zed fork restructured as a terminal-first, always-modal multiplexer editor with Helix keybindings. Fully borderless window (no OS decorations, no title bar). All pane types are equal (no bottom dock). TOML keymap config. Selection-first interfaces with Action::accepts wired into command palette. Yazi-style file manager with icons, file operations, marks, and scrolling.

Custom crates: `codon-mode` (~200 lines), `codon-keymap` (~200 lines), `file-manager` (~900 lines). Vendored Zed changes: ~15 commits across terminal_view, terminal_panel, workspace, vim, command_palette, command_palette_hooks, ui, which_key.

Architecture phase 0 complete, phase 1 complete, phases 2-5 are 0%.

---

## Phase 1 — Modal Shell & Action Layer ✓ COMPLETE

### 1.1 Core UX ✓

- [x] Helix mode force-enabled by default
- [x] Terminal as default first pane (new windows open with terminal)
- [x] Terminal opens as center pane (NewTerminal always uses add_center_terminal)
- [x] Tab bar hidden for single-item panes
- [x] Title bar removed — project name and git branch in status bar
- [x] Borderless window (no OS decorations, no in-window title bar)
- [x] Compact UI (reduced tab bar 32→24px, status bar 1px vertical padding, which-key padding halved)
- [x] Which-key enabled by default (500ms delay)
- [x] UI font reduced to 14

### 1.2 Modal system ✓

- [x] PaneMode (Normal/Insert/Command) — crates/codon-mode
- [x] Mode indicator in left status bar, works for all pane types
- [x] Editor: vim/helix handles mode natively
- [x] Terminal: Insert default, Normal via double-escape (j/k/Ctrl-u/d/gg/G scrollback, i to return, : for command palette)
- [x] File manager: Normal default, three-column yazi layout
- [x] Command mode (:) opens command palette in terminal and file manager Normal mode

### 1.3 Keybinding system ✓

- [x] TOML keymap config — crates/codon-keymap, loads from ~/.config/codon/keymap.toml
- [x] Default keymap embedded (cmd-k h/j/k/l, cmd-k |/-, cmd-k t/e/w)
- [x] Survives keymap reloads (load_codon_keymap called from reload_keymaps)
- [ ] Hot-reload — watch keymap file for changes (follow-up)

### 1.4 Selection-first foundation ✓

- [x] ObjectKind enum (Text, File, Dir, Hunk, Commit, Branch, Block, Url, Diagnostic, Message) — in command_palette_hooks
- [x] Selection enum + SelectionSource trait — in codon-mode
- [x] FileManager implements SelectionSource (returns marked or selected paths)
- [x] ActionAcceptsRegistry — actions register accepted ObjectKinds
- [x] Command palette checks registry when building action list
- [ ] Wire SelectionSource into CodonModeTracker so palette actively filters (currently defaults to None = show all)

### 1.5 File manager ✓

- [x] Three-column yazi layout (parent | current | preview)
- [x] File type icons from Zed's FileIcons system
- [x] Symlink indicators
- [x] Navigation: j/k, h/l/Enter, gg/G
- [x] Scrolling: Ctrl-d/u (half page), PgDn/PgUp (full page), mouse wheel
- [x] Virtualized rendering via uniform_list (handles large directories)
- [x] Direction-aware scroll follow (Bottom strategy going down, Top going up)
- [x] Click to select entries
- [x] Multi-select with v (visual select), full-line highlight for marks
- [x] Full-line focus highlight
- [x] File operations: create file (a), mkdir (A), delete (d), rename (r)
- [x] Yank path to clipboard (y)
- [x] Toggle hidden files (.)
- [x] Insert mode for name input (create/rename) with input bar
- [x] Status bar showing path, position, mark count
- [x] Preview: directory listing or file content (first 80 lines)

---

## Phase 2 — Sessions, Layout, Persistence

### 2.1 Session management

- [x] Session struct — name, cwd, layout tree, pane registry, project context (`crates/codon-session`)
- [x] Session creation — `codon_session::SessionNew` (uses current project cwd)
- [x] Session switching — `codon_session::SessionSwitch` with fuzzy picker
- [x] Session list — name, cwd, last-attached time (in-memory + KVP-persisted)
- [x] Session indicators in status bar (current session name)
- [x] One session visible at a time
- [ ] Cwd picker for session.new (defaults to current project cwd; full in-app dir picker deferred)

### 2.2 Layout system

- [x] LayoutSnapshot enum — Group, Stack, Pane (workspace::codon_bridge); Stack present in serde shape, falls back to active member when applied
- [x] Keyboard-resizable separators (binds `vim::ResizePane*` to `cmd-k shift-{h,j,k,l}`)
- [ ] Stack live rendering — needs new `Member::Stack` variant in vendored pane_group; deferred (15+ match-site updates)
- [ ] Stack actions: pane.stack.cycle / pane.stack.add (deferred with above)
- [ ] Simplify indicators in tab bar (remove "X") on stacked-pane indicator (deferred with above)
- [ ] Status bar elements are modal and dynamic depending on selected pane type

### 2.3 Windows

- [x] Windows group panes into sets, only one window visible at a time (Vec<Window> in Session)
- [x] Window indicator in the status bar (`WindowsStatusItem`, tab-bar-style, no close X)
- [x] Keyboard switching (`WindowNew`/`WindowNext`/`WindowPrev`/`WindowGoto(usize)`/`WindowClose`)
- [x] Mouse switching (click on window indicator tabs)
- [x] Tab component reused for windows indicator (close slot omitted via `Tab::end_slot(None)`)

### 2.4 UX

- [x] Default split kind is terminal
- [x] Ctrl+h/j/k/l to select panes (binds to `workspace::ActivatePane*`)
- [x] Ctrl+Shift+h/j/k/l to move panes around (binds to `workspace::SwapPane*`)
- [x] Native-dialog audit — confirmed 5 callsites in vendored Zed (`workspace.rs:2972`,
      `project_panel:3301`, `git_ui::clone:17`, `agent_ui::threads_archive_view:1237`,
      `agent_ui::message_editor:1423`); all route through `cx.prompt_for_paths`. Replacement
      requires building an in-app dir picker modal (deferred to Phase 5)
- [ ] Group status bar buttons to save space (keep diagnostic visible)
- [ ] Remove search button from status bar (has its own shortcut and already behaves as a pane)

### 2.5 Persistence

- [ ] Periodic snapshots (30s) + graceful shutdown write
- [x] Rehydrate on launch — registry loads from KVP at codon_session::init; per-window layouts restore via SerializedPaneGroup deserialize
- [ ] Terminal: last scrollback + "press Enter to respawn" — DEFERRED (needs invasive terminal_view changes; alacritty grid serialization)
- [x] Editor: restore open files + view state (already provided by Zed's SerializableItem; survives codon-session swaps because item ids are preserved)
- [ ] Swap files for unsaved changes

---

## Phase 3 — Agent, Inline Assistant, Commit Editor

- [ ] Fork and rewire Zed's agent, inline_assistant, commit_editor
- [ ] Turn agentic editing into a pane type
- [ ] Cross-pane agent.explain verb
- [ ] AI commit message generation

---

## Phase 4 — Buffer Trait & Git Integration

### 4.1 Buffer trait

- [ ] Analyze Zed's buffer dependencies
- [ ] Define codon_buffer::Buffer trait
- [ ] Implement for helix_view::Document or wrapper

### 4.2 Git pane

- [ ] Fork Zed's git crate, rewire to Buffer trait
- [ ] Build panes-git — status, log, diff, hunk staging
- [ ] Selection::Hunks + cross-pane git.blame.show

---

## Phase 5 — Native UX Coverage

### 5.1 File manager enhancements

- [ ] Fuzzy filter (/ enters Insert mode for filtering)
- [ ] Git status indicators in file listings
- [ ] Copy/paste files (y/d modal operations for file copy/move)
- [ ] Bulk operations on marked files

### 5.2 Additional panes

- [ ] Diff viewer pane
- [ ] Image preview pane
- [ ] Diagnostics pane with j/k navigation

---

## Immediate next steps (priority order)

1. Wire SelectionSource into tracker for active palette filtering
2. File manager: fuzzy filter mode, git status indicators
3. Session management basics (named sessions, switch, persist)
