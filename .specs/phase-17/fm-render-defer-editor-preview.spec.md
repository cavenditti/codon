---
id: TASK:phase-17/fm-render-defer-editor-preview
type: task
status: draft
version: 0.0.1
summary: >
  Defer instantiating the real `EditorElement` in the FM preview
  column until the active selection has dwelt ≥ 150 ms on a file;
  meanwhile render a static syntax-highlighted snapshot via the
  custom row / column Elements. Eliminates per-keypress
  `CTLine::new_with_attributed_string` cost during fast j/k
  navigation.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-render#c-defer-editor-in-preview
aspects: [editor-deferral, dwell-timer, snapshot-preview]
---

# Defer editor in preview column

## What changes

The FM preview column today instantiates `EditorElement` on
every selection change. The render-trace profile shows ~5–10 ms
of per-frame cost in `EditorElement::prepaint` →
`WindowTextSystem::shape_line` → CoreText
`CTLine::new_with_attributed_string` for each preview swap.

Replace the instantiation flow:

1. On selection change, the column reads the cached preview
   (already prepared by phase-15's preview pipeline) and renders
   it via `FmRowElement`-equivalent static-text painting — one
   glyph run per visible preview line, no `EditorElement`.
2. The FM view starts a `dwell_timer` on selection change. If
   the selection stays on the same path for ≥ 150 ms, the
   timer fires and the preview column upgrades to the real
   `EditorElement` (with cursor, folding, gutter, etc.).
3. The next selection change cancels the timer and drops the
   `EditorElement` (back to static rendering).

Implementation sketch:

```rust
enum PreviewKind {
    Static(Arc<PreviewSnapshot>),       // syntax-highlighted lines
    Editor(Entity<editor::Editor>),     // upgraded view
}

impl FileManager {
    fn on_selection_changed(&mut self, ..., cx: &mut Context<Self>) {
        self.preview_kind = PreviewKind::Static(self.preview.clone());
        self.preview_upgrade_task = Some(
            cx.spawn(|this, mut cx| async move {
                cx.background_executor()
                    .timer(Duration::from_millis(150))
                    .await;
                this.update(&mut cx, |this, cx| {
                    this.upgrade_preview_to_editor(cx);
                })
            })
        );
        cx.notify();
    }
}
```

`PreviewSnapshot` already exists (or its rough shape does — it
holds the syntax-highlighted line vec produced by phase-15's
async preview pipeline). This task wires that snapshot to a
static renderer rather than to an editor instance.

## Why this clause

The 6.7–7.0 s burst in the post-async-git-status profile is
entirely the editor running text shaping in the preview column.
At ~5 ms per selection change in steady state and ~10 ms on
first instantiation, that cost is the most visible remaining
"every keystroke" lag. Deferring the real editor until the user
visibly pauses keeps the rich preview available *when it
matters* (when the user is reading), and kills it *when it
hurts* (during navigation).

150 ms is the threshold suggested by the original phase-15
debounce work — long enough to skip during j/k chains, short
enough to feel instant when the user stops.

## Verification

- New test `preview_dwell_upgrade_after_150ms` simulates a
  selection change, advances time by 100 ms (no upgrade),
  another 60 ms (upgrade fires), asserts that
  `preview_kind == Editor`.
- New test `preview_dwell_cancelled_on_rapid_navigation`
  simulates 10 selection changes in 20 ms each; asserts that
  the upgrade never fires, no `EditorElement` is ever
  constructed, and the static snapshot renderer was used for
  every paint.
- Render-trace harness during a rapid j/k session reports zero
  `CTLine::new_with_attributed_string` calls inside the
  preview column.

## Done when

- The preview column renders via the custom static path during
  navigation.
- The dwell upgrade fires after 150 ms of stable selection.
- Switching directories cancels and resets the timer.
- `spec lint` is at zero errors.
