---
id: TASK:phase-13/status-bar-pane-context-item
type: task
status: accepted
version: 0.0.1
summary: >
  New PaneContextLabel status item — renders file path / cwd / agent
  verb depending on the focused pane kind, replacing active_file_name.
owners: [carlo]
progress: done
refines:
  - REQ:codon/status-bar#c-pane-context-item
---

# Status-bar pane-context label

## What changes

The centre zone needs a single item that describes "what is this
pane" regardless of the pane's kind. Today
`workspace::active_file_name::ActiveFileName` covers editor panes
only — terminal, file-manager, and agent panes leave that slot
empty. The replacement is a codon-side item that asks the
`codon-mode` pane-kind tracker what the active pane is and renders
accordingly:

- **editor** → file path with middle-ellipsis truncation
  (reproducing `ActiveFileName`'s existing behaviour);
- **terminal** → `term: <cwd>` where `cwd` is the terminal's
  current working directory (read from `terminal::Terminal::working_directory`
  or the equivalent accessor on the pane item);
- **file-manager** → `fm: <cwd>` where `cwd` is the FM's active
  directory (read from `file_manager::FileManager::current_dir`
  or the equivalent);
- **agent** → `agent: <verb>` where `verb` is the agent pane's
  active verb name (read from `codon_agent::AgentPane::current_verb`
  or the equivalent).

Middle-ellipsis truncation applies uniformly across all four
variants so the
[REQ:codon/status-bar#c-collapse-policy](spec:REQ:codon/status-bar#c-collapse-policy)
step-1 clip is one render-time rule, not four bespoke ones.

## Approach

1. Pick a home: extend `crates/codon-session/` with a
   `pane_context_label.rs` module. Same rationale as
   `status-bar-git-branch-item`: keeping codon-side status items
   together.
2. Implement the item:
   ```rust
   pub struct PaneContextLabel {
       active_pane_kind: PaneKind,
       caption: SharedString,
       // …subscriptions to codon-mode + active pane item…
   }

   impl StatusItemView for PaneContextLabel {
       fn set_active_pane_item(&mut self, item, window, cx) {
           // Resolve kind via codon-mode tracker; recompute caption.
       }
   }

   impl Render for PaneContextLabel { … }
   ```
3. The caption update path subscribes to:
   - `codon_mode::CodonModeTracker` events for pane-kind changes;
   - per-kind item events for caption changes (editor file rename,
     terminal cwd change, FM directory change, agent verb change).
4. Middle-ellipsis: reuse `ActiveFileName`'s truncation helper if
   public; otherwise lift the relevant snippet into this module
   with a `Spec-Ref` trailer.
5. Register the item in the centre zone (via
   [TASK:phase-13/status-bar-layout-rewire](spec:TASK:phase-13/status-bar-layout-rewire)).

## Non-goals

- No icon prefix per kind. The `term: ` / `fm: ` / `agent: `
  textual prefix is enough — adds zero asset weight, reads cleanly
  in terminal-first contexts.
- No click handler. The label is read-only status; jumping between
  panes is a separate verb.
- No deletion of `workspace::active_file_name::ActiveFileName` from
  vendored Zed. Upstream still uses it; codon just stops
  registering it.

## Files touched

- `crates/codon-session/src/pane_context_label.rs` (new).
- `crates/codon-session/src/lib.rs` (export the new item).
- `apps/codon/src/zed.rs` — construct the item; the registration
  call lives in `status-bar-layout-rewire`. Also drop the
  `active_file_name` local binding (also handled in
  `status-bar-layout-rewire`).
- `crates/codon-session/Cargo.toml` — add `codon-mode`,
  `terminal`, `file-manager`, `codon-agent`, and any other
  per-kind crate dependencies needed to read captions.

## Verification

- `cargo run -p codon` shows:
  - editor pane focused → file path centred (with middle-ellipsis
    when narrow);
  - terminal pane focused → `term: <cwd>`;
  - FM pane focused → `fm: <cwd>`;
  - agent pane focused → `agent: <verb>`.
- Switching focus updates the caption within one event-loop tick
  without re-laying-out adjacent items.
- Renaming the active file (editor) updates the caption.
- `cd`-ing in the active terminal updates the caption.
