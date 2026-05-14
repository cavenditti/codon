---
id: TASK:phase-10/jump-provider-fm
type: task
status: accepted
version: 0.0.1
summary: >
  File-manager provider — one JumpCandidate per visible row.
  Action sets the cursor index. URL candidates skipped.
owners: [carlo]
progress: done
refines:
  - REQ:codon/jump-hints#c-pane-file-manager
aspects: [fm-row-provider]
---

# File-manager jump provider

## What ships

`FmJumpProvider` in
`crates/file-manager/src/jump_provider.rs`. Registered in
`FileManager::new`.

```rust
impl JumpProvider for FmJumpProvider {
    fn collect(&self, ctx: &JumpContext, cx: &mut App) -> Vec<JumpCandidate> {
        if ctx.mode == JumpMode::Url { return vec![]; }
        let fm = self.fm.upgrade()?;
        fm.read_with(cx, |fm, cx| {
            let first_visible = fm.first_visible_row();
            let row_count = fm.visible_row_count();
            (first_visible..first_visible + row_count)
                .filter_map(|row| {
                    let bounds = fm.row_screen_bounds(row, cx)?;
                    let fm_weak = fm.downgrade();
                    Some(JumpCandidate {
                        bounds,
                        kind: JumpKind::Word,
                        action: Box::new(move |window, cx| {
                            fm_weak.update(cx, |fm, cx| {
                                fm.set_cursor_index(row, cx);
                                fm.focus_handle(cx).focus(window);
                            }).ok();
                        }),
                    })
                })
                .collect()
        })
    }
}
```

Needs three new methods on `FileManager`:

- `pub fn first_visible_row(&self) -> usize` — read from the
  panel's scroll state (already tracked for `Ctrl-d`/`Ctrl-u`).
- `pub fn visible_row_count(&self) -> usize` — derived from
  height / row height.
- `pub fn row_screen_bounds(&self, row, cx) -> Option<Bounds<Pixels>>` —
  computed from the scroll position + row index. The panel
  doesn't currently track per-row bounds; the helper computes
  them analytically since rows are fixed-height.

## Verification

- Open fm with 12 entries visible; `cmd-k j` shows 12 chips,
  selecting one sets the cursor index and focuses the panel.
- `cmd-k u`: zero chips, "No URLs visible" toast (provider returns
  empty for URL mode).
- Scrolling between activations: hints follow the visible window
  exactly.

## Where it slots in

- New: `crates/file-manager/src/jump_provider.rs` (~80 LOC).
- Edit: `crates/file-manager/src/file_manager.rs` — three pub
  helpers (~30 LOC additive).
- Edit: `crates/file-manager/src/lib.rs` — declare module + init.
- Edit: `crates/file-manager/Cargo.toml` — `codon-jump.workspace = true`.
