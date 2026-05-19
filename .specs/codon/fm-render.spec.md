---
id: REQ:codon/fm-render
type: requirement
status: accepted
version: 0.1.0
level: MUST
summary: >
  An aggressive file-manager rendering pipeline that bypasses GPUI's
  Div-based element tree (Interactivity, with_image_cache,
  with_text_style, taffy flexbox) in favour of custom Elements that
  paint glyphs and quads directly, so the per-frame cost of an FM
  redraw drops from ~30 ms to ~3 ms — well below terminal FMs like
  ranger and yazi.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-17]
---

# Aggressive file-manager rendering pipeline

## Motivation

Profiling the post-async-git-status build with `samply` shows the
file-manager logic itself is no longer on the main-thread hot path —
`collect_git_status`, `DirCache`, `read_dir_sync`, and the
label/icon memo all moved off-thread cleanly. The remaining
~30 ms / repaint baseline that makes directory-enter feel slower
than `yazi` lives entirely inside GPUI's `Div` pipeline:

- `Div::Interactivity::paint` runs ~6 nested closures per row
  (click / hover / action / focus / image-cache / text-style),
  even when the row has no per-row interactivity (selection is
  captured at view level via the codon TOML keymap).
- `Window::with_image_cache`, `with_text_style`,
  `with_optional_element_state`, `with_element_state` each install
  per-row state slots that the FM never reads.
- `taffy::compute_flexbox_layout` descends into per-row flex
  children to compute a layout that is in fact a fixed grid (icon,
  name, meta columns at fixed widths).
- `gpui::scene::Scene` receives per-row glyph and quad insertions
  that are never coalesced, and re-shapes the same text every
  frame even when only the selection moved.

Terminal FMs hit sub-millisecond redraws because they emit a diff
of escape sequences. We cannot match terminal escapes — but we can
bypass GPUI's per-row framework overhead the same way
`editor::EditorElement` does for the buffer view, and that is
sufficient to get within 1.5–3 ms / frame.

## Requirement

:::{requirement id="fm-render" level="MUST"}
The system MUST:

- {#c-custom-row-element} render each FM row through a custom
  `gpui::Element` impl (`crates/file-manager/src/render/row.rs`)
  that owns its own paint — no nested `Div`, no `Interactivity`,
  no `with_text_style` / `with_image_cache` / `with_element_state`
  wrappers. The Element issues one background `PaintQuad`, one
  icon glyph run, one name + meta glyph run, and one optional
  status decoration directly into `gpui::scene::Scene`.
  Hit-testing is captured at the view level via the codon TOML
  keymap and does not require per-row event handlers.
- {#c-custom-column-element} render each FM column (parent /
  current / preview) through a custom `gpui::Element` impl
  (`crates/file-manager/src/render/column.rs`) that owns its own
  virtualization, scrollbar, and inline row painting. Bypasses
  `UniformList`'s prepaint loop and the surrounding `Div`
  ancestors. The Element computes `first_visible_idx` /
  `last_visible_idx` from the column's scroll offset and a
  fixed row height, iterates that range, and calls the row
  Element's paint inline — no per-row Element instantiation
  cost.
- {#c-shaped-line-cache} maintain an LRU
  (`crates/file-manager/src/render/shaped_line_cache.rs`) keyed
  on `(font_id, font_size, string)` storing the resolved
  `ShapedLine` (glyph runs + advances). On paint, look up the
  cached glyphs and emit translated copies. The cache MUST be
  bounded (≤ 4 × visible rows × columns by default) and MUST
  invalidate entries whose `(font_id, font_size)` no longer
  matches the theme.
- {#c-row-glyph-cache} cache the entire row's pre-positioned
  `Vec<PaintGlyph>` keyed on `(path, display_state)` where
  `display_state` includes the LineMode, git-status decoration,
  and mark state. On scroll, translate by `(0, dy)` and emit
  the cached glyphs untouched. On selection move, only flip
  the background quad of the two affected rows — glyphs are
  reused.
- {#c-defer-editor-in-preview} defer the real `EditorElement` in
  the preview column. Until the active selection has dwelt
  ≥ 150 ms on a file, the preview MUST be rendered through the
  same custom row/column Elements (using the already-prepared
  syntax-highlighted snapshot). Only after the dwell threshold
  is reached does the FM swap in the real `EditorElement`. This
  preserves the rich preview when the user pauses and
  eliminates `CTLine::new_with_attributed_string` /
  `WindowTextSystem::shape_line` cost during fast j/k navigation.
- {#c-dirty-rect-repaint} on selection movement, repaint only the
  two affected rows (previously-selected and newly-selected) if
  GPUI's `Scene::replay` supports incremental scene composition;
  otherwise emit a full scene whose row glyph contributions
  come from the row-glyph cache. The custom column Element MUST
  expose a `mark_rows_dirty(&[usize])` API so the view can
  signal partial invalidation without forcing a full prepaint
  walk.
- {#c-frame-budget-harness} expose a feature-gated
  `--render-trace` mode (cli flag plus a corresponding
  `[diagnostics] render_trace = true` settings field) that logs
  `keypress_at → frame_painted_at` deltas and per-Element
  prepaint / paint durations to a structured JSON file under
  `$CODON_LOG_DIR/render-trace/`. The harness is the acceptance
  gate: the FM redraw cycle MUST measure ≤ 5 ms / frame at the
  **p95** of a 60-second navigation session (rapid
  `j`/`k`/`h`/`l` over a 500-entry tree on the reference
  Apple-Silicon device documented in the task), with ≤ 3 ms /
  frame typical when the row-glyph cache hits.
:::

## Trade-offs and constraints

- **Theme reactivity.** A custom Element that does not call
  `with_text_style` does not auto-react to theme changes. The
  Element MUST subscribe to the theme observer (codon already
  has `cx.observe_global::<SettingsStore>()` paths) and
  invalidate both caches on theme change.
- **Accessibility.** Skipping `Div::Interactivity` also forgoes
  any future a11y bridging that GPUI may layer onto `Div`. This
  is an accepted trade for codon's keyboard-first stance —
  recorded here so future maintainers see the choice.
- **Focus and keymap.** Custom Elements continue to participate
  in GPUI focus by virtue of being rendered inside a focusable
  view; the FM view's existing `focus_handle` and TOML keymap
  bindings keep working without per-Element changes.
- **`uniform_list` as fallback.** The custom column Element is
  feature-gated behind `[file_manager] custom_render = true`
  (default `true` once the harness reports parity). Setting it
  to `false` falls back to the existing `uniform_list` path —
  reversible escape hatch while the pipeline stabilises.
