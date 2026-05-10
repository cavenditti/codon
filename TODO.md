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

- [ ] Session struct — name, cwd, layout tree, pane registry, project context
- [ ] Session creation — `session.new` action with cwd picker
- [ ] Session switching — `session.switch` with picker
- [ ] Session list — name, cwd, last-attached time
- [ ] Session indicators in status bar (current session name)
- [ ] One session visible at a time

### 2.2 Layout system

- [ ] LayoutNode enum — Split, Stack, Leaf
- [ ] Simplify indicators in tab bar (remove "X")
- [ ] pane.stack.cycle / pane.stack.add actions
- [ ] Keyboard-resizable separators

### 2.3 Persistence

- [ ] Periodic snapshots (30s) + graceful shutdown write
- [ ] Rehydrate on launch — restore layout
- [ ] Terminal: last scrollback + "press Enter to respawn"
- [ ] Editor: restore open files + view state
- [ ] Swap files for unsaved changes

### 2.4 UX

- [x] Default split kind is terminal
- [ ] NEVER use native dialogs for OS
- [ ] 

---

## Phase 3 — Agent, Inline Assistant, Commit Editor

- [ ] Fork and rewire Zed's agent, inline_assistant, commit_editor
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
