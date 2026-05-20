---
id: TASK:phase-21/fm-clipboard-publish
type: task
status: draft
version: 0.1.0
summary: >
  Mirror FM `y` (yank) and `d` (cut) into the OS clipboard as
  `ClipboardEntry::ExternalPaths` in addition to the existing
  `FmClipboard::{Yank,Cut}` internal state. The OS side always
  carries a "copy"; cut-vs-copy stays a private FM concern decided
  on the FM's own paste.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-os-clipboard#c-fm-publishes-to-os
blocked_by:
  - TASK:phase-21/clipboard-platform-write
---

# FM publishes yank / cut to OS clipboard

## Plan

The FM today owns a self-contained `FmClipboard` enum
([`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
`:124`) that never touches the OS pasteboard. The comment at `:120`
justifies the isolation by appealing to terminal/agent paste
non-collision. Phase 21 retires that justification: codon will own
the clipboard outright when the FM publishes to it, and the user
can still paste *path text* into a terminal pane via the existing
`Y` (`copy_path_to_os_clipboard` at `:1643`), which stays a
distinct verb.

### Changes

1. **`yank_to_clipboard`** (`:1614`):
   - Keep the existing `self.clipboard = FmClipboard::Yank(paths)`.
   - After updating internal state, additionally write to the OS
     clipboard:
     ```rust
     cx.write_to_clipboard(ClipboardItem {
         entries: vec![ClipboardEntry::ExternalPaths(ExternalPaths(paths.clone().into()))],
     });
     ```
     Use the path set verbatim — no string-form fallback in this
     entry. The OS clipboard read path on macOS already pairs an
     `ExternalPaths` entry with a string representation for
     downstream text consumers
     ([`vendor/zed/crates/gpui_macos/src/pasteboard.rs`](spec:src:vendor/zed/crates/gpui_macos/src/pasteboard.rs)
     `:64-69`); the *write* side after `clipboard-platform-write`
     does not need to do that itself.

2. **`cut_to_clipboard`** (`:1629`):
   - Keep the existing `self.clipboard = FmClipboard::Cut(paths)`.
   - Publish the *same* `ExternalPaths` payload as yank — no cut
     hint MIME type. The reasoning is in
     [REQ:codon/fm-os-clipboard](spec:REQ:codon/fm-os-clipboard)'s
     context: KDE / GNOME `application/x-kde-cutselection` is not
     a universal protocol, and macOS Finder has no cut at all.
     The FM's internal `Cut` flag still drives source removal on
     codon's own paste.

3. **`Y` (`copy_path_to_os_clipboard`, `:1643`)**:
   - No behavioural change. Today it writes the newline-joined
     path string for terminal pasting. After this task it stays
     a separate, intentional verb the user can reach when they
     want path *text* (e.g. paste into a `cd ` in a terminal pane)
     without disturbing the file payload published by `y` / `d`.
     Optional: extend it to also include the `ExternalPaths`
     entry so a single chord publishes both representations — but
     that conflates verbs and is left as a follow-up.

### Why mirror, not move

A cleaner-looking design would be "let the OS clipboard *be* the
storage". It loses information codon needs:

- macOS has no cut-vs-copy bit on the pasteboard; without an
  internal `FmClipboard::Cut`, codon couldn't honour `d` → `p`
  (move) versus `y` → `p` (copy) symmetrically.
- The OS clipboard's contents can change asynchronously (another
  app stomping on it); the FM's progress notifications and
  bulk-task tracking need a stable view of the operation it's
  about to perform.

Mirror semantics keep the internal state authoritative and use the
OS clipboard as a publishing channel, which matches what every
mainstream native file manager does.

### Test path

A GPUI test exercising `yank_to_clipboard` after
`clipboard-platform-write` lands: assert the read-back
`ClipboardItem` from `cx.read_from_clipboard()` contains an
`ExternalPaths` whose path set equals `current_targets()`.

## Acceptance

- After `y` on a marked set of paths, `cx.read_from_clipboard()`
  in the same `App` returns `Some(ClipboardItem)` whose `entries`
  include an `ExternalPaths` with the same path set.
- Same for `d` (cut).
- A `Y` (shift-y) still writes path text only — no `ExternalPaths`
  entry — preserving the "paste path into terminal" workflow.
- `FmClipboard::{Yank,Cut}` internal state is unchanged in shape;
  the only delta is the additional `cx.write_to_clipboard(...)`
  call after the assignment.
- `spec lint` clean.

## Files touched

- `crates/file-manager/src/file_manager.rs` — `yank_to_clipboard`
  and `cut_to_clipboard` gain a trailing
  `cx.write_to_clipboard(...)` call.

## Sequencing

Blocked by [TASK:phase-21/clipboard-platform-write](spec:TASK:phase-21/clipboard-platform-write).
Until that lands, the `write_to_clipboard(ExternalPaths)` calls
added here are silent no-ops on macOS — which is harmless
(internal `FmClipboard` keeps working) but means the cross-app
half of the acceptance gate is unreachable.
