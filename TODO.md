# Codon — Full Implementation Todo List

## Current State Summary

A Zed fork restructured as a terminal-first, always-modal multiplexer editor with Helix keybindings. Fully borderless window. All pane types are equal (no bottom dock). TOML keymap config. Selection-first interfaces defined. Action::accepts wired into command palette.

Custom crates: `codon-mode` (~200 lines), `codon-keymap` (~200 lines), `file-manager` (~500 lines). Vendored Zed changes: ~12 commits across terminal_view, workspace, vim, command_palette, command_palette_hooks, ui, which_key, title_bar.

Architecture phase 0 complete, phase 1 complete, phases 2-5 are 0%.

---

## Phase 1 — Modal Shell & Action Layer ✓ COMPLETE

### 1.1 Core UX ✓
- [x] Helix mode force-enabled by default
- [x] Terminal as default first pane (new windows open with terminal)
- [x] Terminal opens as center pane (NewTerminal always uses add_center_terminal)
- [x] Tab bar hidden for single-item panes
- [x] Title bar removed — project name and git branch in status bar
- [x] Borderless window (no OS decorations)
- [x] Compact UI (reduced tab bar, status bar, which-key padding)
- [x] Which-key enabled by default (500ms delay)
- [x] UI font reduced to 14

### 1.2 Modal system ✓
- [x] PaneMode (Normal/Insert/Command) — crates/codon-mode
- [x] Mode indicator in left status bar, works for all pane types
- [x] Editor: vim/helix handles mode natively
- [x] Terminal: Insert default, Normal via double-escape (j/k/Ctrl-u/d/gg/G scrollback, i to return)
- [x] File manager: Normal default, three-column yazi layout, j/k/h/l navigation
- [x] Command mode (:) opens command palette in terminal and file manager Normal mode

### 1.3 Keybinding system ✓
- [x] TOML keymap config — crates/codon-keymap, loads from ~/.config/codon/keymap.toml
- [x] Default keymap embedded (cmd-k h/j/k/l, cmd-k |/-, cmd-k t/e/w)
- [x] Survives keymap reloads (load_codon_keymap called from reload_keymaps)
- [ ] Hot-reload — watch keymap file for changes (follow-up)

### 1.4 Selection-first foundation ✓
- [x] ObjectKind enum (Text, File, Dir, Hunk, Commit, Branch, Block, Url, Diagnostic, Message) — in command_palette_hooks
- [x] Selection enum + SelectionSource trait — in codon-mode
- [x] FileManager implements SelectionSource
- [x] ActionAcceptsRegistry — actions register accepted ObjectKinds
- [x] Command palette checks registry when building action list
- [ ] Wire SelectionSource into CodonModeTracker so palette actively filters (currently defaults to None = show all)

---

## Phase 2 — Sessions, Layout, Persistence

### 2.1 Session management
- [ ] Session struct — name, cwd, layout tree, pane registry, project context
- [ ] Session creation — `session.new` action with cwd picker
- [ ] Session switching — `session.switch` with picker
- [ ] Session list — name, cwd, last-attached time
- [ ] One session visible at a time

### 2.2 Layout system
- [ ] LayoutNode enum — Split, Stack, Leaf
- [ ] Stack indicators replacing full tab bar (show "2/5" position)
- [ ] pane.stack.cycle / pane.stack.add actions
- [ ] Keyboard-resizable separators

### 2.3 Persistence
- [ ] Periodic snapshots (30s) + graceful shutdown write
- [ ] Rehydrate on launch — restore layout
- [ ] Terminal: last scrollback + "press Enter to respawn"
- [ ] Editor: restore open files + view state
- [ ] Swap files for unsaved changes

### 2.4 Terminal-first defaults
- [ ] Default split kind is terminal
- [ ] Terminal cwd follows session

---

## Phase 3 — Buffer Trait & Git Integration

### 3.1 Buffer trait
- [ ] Analyze Zed's buffer dependencies
- [ ] Define codon_buffer::Buffer trait
- [ ] Implement for helix_view::Document or wrapper

### 3.2 Git pane
- [ ] Fork Zed's git crate, rewire to Buffer trait
- [ ] Build panes-git — status, log, diff, hunk staging
- [ ] Selection::Hunks + cross-pane git.blame.show

---

## Phase 4 — Native UX Coverage

### 4.1 File manager polish
- [ ] Fuzzy filter (/ enters Insert mode for filtering)
- [ ] File operations — create (a), mkdir (A), delete (d), rename (r), copy (c), move (m)
- [ ] Multi-select with space, operate on marked set
- [ ] Git status indicators in listings
- [ ] Yank path (y)
- [ ] Scrollable columns

### 4.2 Additional panes
- [ ] Diff viewer pane
- [ ] Image preview pane
- [ ] Diagnostics pane with j/k navigation

---

## Phase 5 — Agent, Inline Assistant, Commit Editor

- [ ] Fork and rewire Zed's agent, inline_assistant, commit_editor
- [ ] Cross-pane agent.explain verb
- [ ] AI commit message generation

---

## Immediate next steps (priority order)

1. Wire SelectionSource into tracker for active palette filtering
2. File manager operations (create/delete/rename)
3. Keymap hot-reload
4. Session management basics (named sessions, switch, persist)
5. Stack indicators replacing tab bar
