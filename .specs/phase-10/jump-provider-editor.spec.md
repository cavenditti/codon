---
id: TASK:phase-10/jump-provider-editor
type: task
status: accepted
version: 0.0.1
summary: >
  Editor provider — yields visible-region words (via the existing
  helix-jump scan) and URLs (via `editor::find_url`) as
  JumpCandidate entries with cursor-set / copy actions.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/jump-hints#c-pane-editor
aspects: [editor-word-provider, editor-url-provider]
---

# Editor jump provider

## What ships

A new `EditorJumpProvider` in
`vendor/zed/crates/editor/src/codon_jump_provider.rs` (a sibling
to `codon_bridge.rs`, following the same vendored-helper
pattern). The editor crate exposes it as
`editor::EditorJumpProvider::register(editor, cx)`, called from
`Editor::new` so every editor entity self-registers with the
codon `JumpRegistry`.

Implementation:

```rust
impl JumpProvider for EditorJumpProvider {
    fn collect(&self, ctx: &JumpContext, cx: &mut App) -> Vec<JumpCandidate> {
        let editor = self.editor.upgrade()?;
        editor.update(cx, |editor, cx| {
            let snapshot = editor.snapshot(window, cx);
            let visible = editor.multi_buffer_visible_range(...);
            let mut out = vec![];
            // Words: reuse the helix-jump scan (extracted into a
            // pub helper `editor::visible_word_anchors`).
            for anchor in editor::visible_word_anchors(&snapshot, visible.clone()) {
                let bounds = editor.text_bounds_for_anchor(anchor, ...);
                out.push(JumpCandidate {
                    bounds,
                    kind: JumpKind::Word,
                    action: Box::new(move |window, cx| {
                        editor.update(cx, |e, cx| e.change_selections(...));
                    }),
                });
            }
            // URLs: existing `find_url` walking the visible range
            // returns (range, url) tuples.
            for (range, url) in editor::find_urls_in_range(&snapshot, visible) {
                let bounds = editor.text_bounds_for_range_start(range.start, ...);
                let url_string = url.to_string();
                out.push(JumpCandidate {
                    bounds,
                    kind: JumpKind::Url(url_string),
                    action: Box::new(move |window, cx| {
                        editor.update(cx, |e, cx| e.change_selections(...));
                    }),
                });
            }
            out
        })
    }
}
```

Two small helper extractions in `editor/src/`:

- `pub fn visible_word_anchors(snapshot, range)` — refactored
  from the body of `helix_jump_to_word`'s candidate-collection
  loop.
- `pub fn find_urls_in_range(snapshot, range)` — wraps the
  existing `hover_links::find_url` to yield all URLs in a range,
  not just at a point.

These extractions are additive; `helix_jump_to_word` continues
to call them.

## Verification

- Open an editor with 30 words visible: `cmd-k j` shows 30
  chips. Two-key selection moves the primary cursor to the
  word's first grapheme.
- Same editor with `https://example.com` text: `cmd-k u` shows
  one URL chip; pressing two keys copies the URL.

## Where it slots in

- New: `vendor/zed/crates/editor/src/codon_jump_provider.rs`.
- Edit: `vendor/zed/crates/editor/src/editor.rs` — export
  `visible_word_anchors`, `find_urls_in_range`, plus a one-line
  `EditorJumpProvider::register(editor, cx)` call in
  `Editor::new`.
- Vendor/zed submodule bump in the outer commit.

## Out of scope

- Terminal and fm providers — separate tasks.
- The overlay itself — depends on `jump-overlay-core`.
