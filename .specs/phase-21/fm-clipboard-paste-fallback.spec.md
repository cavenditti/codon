---
id: TASK:phase-21/fm-clipboard-paste-fallback
type: task
status: draft
version: 0.1.0
summary: >
  Teach FM `p` / `P` to consult the OS clipboard when the internal
  `FmClipboard` is empty. An `ExternalPaths` entry resolves into a
  copy into the current directory; a pure `String` entry surfaces
  a dismissal toast (writing text as a file is an explicit
  non-goal); the `Image` case is handled by the sibling
  image-paste task.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-os-clipboard#c-fm-paste-from-os
  - REQ:codon/fm-os-clipboard#c-cross-app-roundtrip
aspects: [external-paths-fallback, cross-app-paste]
blocked_by: []
---

# FM `p` / `P` falls back to OS clipboard

## Plan

`paste_clipboard` ([`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
`:2552`, going via `paste_clipboard_overwrite` `:2556` for `P`)
delegates to `execute_paste` which today returns a "Clipboard is
empty" toast at `:2812` when `self.clipboard.is_empty()`. Phase 21
turns that early-return into the OS-clipboard fallback.

### Changes

1. Refactor the "clipboard empty" branch in `execute_paste` (around
   `:2810`) to first call `cx.read_from_clipboard()`. The match on
   the returned `Option<ClipboardItem>` looks like:

   ```rust
   match cx.read_from_clipboard() {
       Some(item) if item.entries.iter().any(|e| matches!(e, ClipboardEntry::ExternalPaths(_))) => {
           // dispatch to a new fn paste_from_os_paths(paths, overwrite, …)
       }
       Some(item) if item.entries.iter().any(|e| matches!(e, ClipboardEntry::Image(_))) => {
           // handled by TASK:phase-21/fm-clipboard-image-paste
           //  — until that lands, surface a "not yet supported" toast
       }
       _ => self.surface_error("Clipboard is empty", cx),
   }
   ```

2. Add `paste_from_os_paths(paths, overwrite, window, cx)` next to
   `execute_paste` (`:2810+`). It reuses the existing copy plumbing
   — the same `fs.copy(src, dest, …)` calls and the same
   `crate::tasks::begin(... FmTaskKind::Paste, ...)` notification
   pipeline. Move semantics MUST NOT be inferred from cross-app
   sources (see REQ context) — always copy, never delete the
   source.

3. The internal `FmClipboard::Cut` flag is not consulted in this
   branch. It only applies when `self.clipboard` is non-empty;
   the OS fallback fires only when it *is* empty.

4. `Y` (path-text-only verb) is unaffected.

### Edge cases

- Mixed entries: GPUI's `ClipboardItem` is a `Vec<ClipboardEntry>`.
  Today the macOS read path always pairs an `ExternalPaths` with a
  string for downstream text-paste consumers. The match above
  treats *any* `ExternalPaths` entry as the file payload — the
  string sibling is ignored.
- Empty path list: `ExternalPaths(vec![])` is a paranoid no-op.
  Surface no toast; just return.
- Path doesn't exist: the existing `fs.copy` error path handles
  this — the per-entry failure is captured in the
  `FmTaskOutcome::Failed { errors }` payload, same as for an FM
  internal paste.

## Acceptance

- With `self.clipboard == FmClipboard::Empty`, copying files in
  Finder (macOS) or Nautilus (Wayland) and pressing `p` in codon's
  FM copies those files into the current directory.
- Same with `P` (overwrite-confirm flow goes through the existing
  collision UI).
- With `self.clipboard != Empty`, the OS clipboard is *not*
  consulted — internal state takes precedence so a user who just
  yanked in codon doesn't accidentally pick up a stale Finder
  selection.
- The bulk-task pipeline (`tasks::begin` → `tick` → `finish`) fires
  a progress notification for the OS-sourced paste, identical to
  the internal-source case.
- `spec lint` clean.

## Files touched

- `crates/file-manager/src/file_manager.rs` — `execute_paste` and
  one new `paste_from_os_paths` helper.

## Sequencing

Independent of the platform-write task — only reads the OS
clipboard, which already returns `ExternalPaths` on both macOS and
Wayland today. Can land before
[TASK:phase-21/clipboard-platform-write](spec:TASK:phase-21/clipboard-platform-write).
