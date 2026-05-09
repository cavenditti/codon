# Codon — Full Implementation Todo List

## Current State Summary

Working: Zed fork builds, Helix editing in editors, terminal with Normal mode (double-escape), file manager (three-column), mode indicator, tab hiding for single items, pane splits/navigation (cmd-k prefix).

Custom code: ~644 lines across `codon-mode` (150), `file-manager` (484), plus vendored changes (~100 lines across 4 Zed commits).

Architecture phases 0-1 are complete, phases 2-5 are 0%.

---

## Phase 1 — Modal Shell & Action Layer ✓ COMPLETE

### 1.1 Fix & polish what exists ✓
- [x] Verify cmd-k keybindings actually fire
- [x] Terminal Normal mode works (double-escape, j/k scroll, i to return)
- [x] Terminal opens as center pane (NewTerminal override, not bottom dock)
- [x] Terminal as default first pane (new windows open with terminal)
- [x] Mode indicator shows correctly for all pane types

### 1.2 Command mode (`:` prefix) ✓
- [x] `:` opens command palette in terminal Normal mode and file manager Normal mode
- [x] Action completion via Zed's command palette

### 1.3 Keybinding system ✓
- [x] TOML keymap config — crates/codon-keymap loads from `~/.config/codon/keymap.toml`
- [x] Layered resolution — global + per-pane-kind per-mode
- [x] Default keymap ships embedded in the binary
- [ ] Hot-reload — watch keymap file for changes (follow-up)

### 1.4 Selection-first foundation (interfaces only) ✓
- [x] Selection enum with Text, Files, Hunks, Commits, Blocks, Diagnostics, Messages
- [x] ObjectKind enum with all pane object types
- [x] SelectionSource trait — current_selection() + object_kinds()
- [x] FileManager implements SelectionSource
- [ ] Action::accepts field (follow-up — needs action registry)
- [ ] Command palette filters by selection kind (follow-up)

### 1.5 Pane abstraction ✓
- [x] Unified PaneMode (Normal/Insert/Command) across all pane types
- [x] Editor: vim handles mode switching natively
- [x] Terminal: Insert default, Normal via double-escape
- [x] File manager: Normal default
- [x] Mode indicator never blank — tracks vim_focused flag

---

## Phase 2 — Sessions, Layout, Persistence

### 2.1 Session management
- [ ] Session struct — name, cwd, layout tree, pane registry, project context
- [ ] Session creation — `session.new` action opens a "create session" flow with cwd picker
- [ ] Session switching — `session.switch` opens picker (using Zed's picker infrastructure)
- [ ] Session list — shows name, cwd, last-attached time
- [ ] One session visible at a time — switching swaps the layout tree

### 2.2 Layout system
- [ ] LayoutNode enum — `Split { dir, ratio, a, b }`, `Stack { panes, visible }`, `Leaf(PaneId)`
- [ ] Replace Zed's tab strip with stack indicators — when items are stacked, show position (e.g., "2/5") instead of full tab bar
- [ ] pane.stack.cycle action — cycle through stacked items
- [ ] pane.stack.add action — add current item to stack
- [ ] Keyboard-resizable separators — resize splits without mouse

### 2.3 Persistence
- [ ] Periodic snapshots — save session state every 30 seconds
- [ ] Graceful shutdown write — save on quit
- [ ] Rehydrate on launch — restore last session's layout
- [ ] Terminal rehydrate — show last scrollback with "press Enter to respawn" placeholder
- [ ] Editor rehydrate — restore open file paths and view state
- [ ] Swap files — persist unsaved editor changes

### 2.4 Default terminal-first behavior
- [ ] New session opens with terminal — not empty editor
- [ ] Default split kind is terminal — `pane.split` without args creates terminal
- [ ] Terminal cwd follows session — new terminals inherit session's cwd

---

## Phase 3 — Buffer Trait & Git Integration

### 3.1 Buffer trait
- [ ] Analyze Zed's buffer dependencies — survey git, diagnostics, diff, agent crates for buffer usage
- [ ] Decide trait vs wrapper — based on analysis
- [ ] Define `codon_buffer::Buffer` trait
- [ ] Implement for `helix_view::Document` or wrapper

### 3.2 Git pane
- [ ] Fork Zed's git crate — rewire buffer dependencies to Buffer trait
- [ ] Build `panes-git` — status, log, diff, hunk staging
- [ ] Hunk staging actions
- [ ] Selection::Hunks — first typed selection beyond text
- [ ] Cross-pane verb — `git.blame.show` accepting Text | File

---

## Phase 4 — Native UX Coverage

### 4.1 File manager polish
- [ ] Fuzzy filter mode — press `/` to filter entries by name (Insert mode)
- [ ] File operations — create (a), mkdir (A), delete (d), rename (r), copy (c), move (m)
- [ ] Marks — select multiple files with space, operate on marked set
- [ ] Git status in file listing
- [ ] Yank path — `y` copies path to clipboard
- [ ] Scrolling — scrollable columns for large directories

### 4.2 Diff viewer, Image preview, Diagnostics panes

---

## Phase 5 — Agent, Inline Assistant, Commit Editor

- [ ] Fork and rewire Zed's agent, inline_assistant, commit_editor crates
- [ ] Cross-pane `agent.explain` verb
- [ ] AI commit message generation

---

## Immediate next steps (priority order)

1. Terminal as default pane — new windows start with terminal
2. Command mode (:) — `:` in Normal mode opens command palette
3. File manager operations — create/delete/rename files
4. TOML keymap config — replace hardcoded bind_keys
5. Selection-first interfaces (types only, no implementation)
6. Unified pane mode switching
