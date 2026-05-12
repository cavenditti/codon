---
id: TASK:phase-4/git-panel-modal-integration
type: task
status: accepted
version: 0.0.1
summary: >
  Adopt Zed's existing GitPanel dock as codon's git surface. Patch
  the panel to publish `pane_mode` + sync `CodonModeTracker`, and
  add a Helix-style `[bindings.git_panel.*]` block in codon-keymap.
owners: [carlo]
progress: done
refines:
  - REQ:codon/git-pane#c-status
---

# GitPanel modal integration

## What ships

The dock is unchanged in behaviour; the keyboard surface around it
gets a codon overlay so it feels like every other pane (terminal,
file manager) in codon's modal model.

### Vendored Zed edits (`vendor/zed/crates/git_ui/src/git_panel.rs`)

1. **`dispatch_context()`** publishes `pane_mode` alongside the
   existing identifiers:
   - `pane_mode == "insert"` when the commit-message editor is
     focused (already gated by the `CommitEditor` branch).
   - `pane_mode == "normal"` when the panel's own `focus_handle`
     contains the focus (already the `menu` + `ChangesList`
     branch).
2. **`focus_in()`** writes `CodonModeTracker::mode` based on the
   same condition so the status-bar mode pill follows the dock.
3. A new `cx.on_focus(&commit_editor_focus_handle, …)` registered
   in the constructor flips the tracker to `PaneMode::Insert` when
   focus shifts *into* the commit editor (the panel's `focus_in`
   doesn't fire for inner-element focus).
4. New dep on `codon-mode` (precedent: `terminal_view`,
   `file-manager` already depend on `codon-mode`).

### Codon-side edits (`crates/codon-keymap/src/keymap.rs`)

1. `KeymapBindings` gains a `git_panel: Option<ModeBindings>` field
   and is wired into `parse_keymap`. The kind name maps to
   `KeyContext` "GitPanel" via the existing
   [`mode_predicates`](spec:src:crates/codon-keymap/src/keymap.rs)
   helper — no special-casing needed.
2. `cmd-k g s` is rebound from the old
   `codon_git::OpenStatusPane` to `git_panel::ToggleFocus` so the
   same muscle memory opens / focuses the dock.
3. New embedded `[bindings.git_panel.normal]` block:

   | key | action |
   |-----|--------|
   | `j` | `git_panel::NextEntry` |
   | `k` | `git_panel::PreviousEntry` |
   | `g g` | `git_panel::FirstEntry` |
   | `shift-g` | `git_panel::LastEntry` |
   | `enter` | `menu::Confirm` (open diff for entry) |
   | `s` | `git::StageFile` |
   | `u` | `git::UnstageFile` |
   | `space` | `git::ToggleStaged` |
   | `i` | `git_panel::FocusEditor` |
   | `:` | `codon_command_palette::Toggle` |

4. New `[bindings.git_panel.insert]` block:

   | key | action |
   |-----|--------|
   | `escape` | `git_panel::FocusChanges` |

5. `resolve_binding` gains arms for every action above.
6. `assets/config/codon.example.toml` mirrors both blocks
   (commented out) so the user-visible template documents the
   shape.

## Why this replaces TASK:phase-4/git-status-pane

See the wontdo note in that task. Net summary: the dock already has
~85% of the surface we'd need, the diff against upstream Zed is
small (~30 LOC), and the maintenance cost is one surface instead of
two.

## Known limitations

- **Esc inside the commit editor.** Vim mode owns Esc at the editor
  level (Helix is force-on). The `[bindings.git_panel.insert]` map
  binds `escape` to `FocusChanges` under predicate
  `GitPanel && pane_mode == insert`, but if vim's Esc handler fires
  first the user has to use `cmd-k g s` to toggle focus back out
  of the commit editor. Acceptable for v1; rebinding to a
  less-conflicting chord (`ctrl-w q`, `cmd-shift-i`) is one
  `codon.toml` edit away.
- **Stage / unstage semantics.** `s` stages, `u` unstages, `space`
  toggles. Three actions because GitPanel exposes them separately;
  users who'd rather treat both `s` and `u` as toggles can rebind.

## Files touched

- `vendor/zed/crates/git_ui/src/git_panel.rs` — dispatch_context,
  focus_in, constructor's commit-editor focus listener.
- `vendor/zed/crates/git_ui/Cargo.toml` — `codon-mode` dep.
- `crates/codon-keymap/src/keymap.rs` — TOML struct, parse,
  DEFAULT_KEYMAP, resolve_binding.
- `crates/codon-keymap/Cargo.toml` — `git_ui` dep.
- `assets/config/codon.example.toml` — git_panel sections.
- Tear-down: `crates/codon-git/` (gone), workspace + app dep
  removals.
