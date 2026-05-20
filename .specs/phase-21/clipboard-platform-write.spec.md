---
id: TASK:phase-21/clipboard-platform-write
type: task
status: draft
version: 0.1.0
summary: >
  Make GPUI's `write_to_clipboard` publish file URLs for the
  `ClipboardEntry::ExternalPaths` variant: `NSFilenamesPboardType`
  on macOS (currently a no-op match arm) and `text/uri-list`
  advertised on the `wl_data_source` on Wayland (currently
  unadvertised). X11 left as-is.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-os-clipboard#c-platform-write-paths
blocked_by: []
---

# Platform-layer write for `ExternalPaths`

## Plan

GPUI's `ClipboardEntry::ExternalPaths(ExternalPaths)` already exists
([`vendor/zed/crates/gpui/src/platform.rs`](spec:src:vendor/zed/crates/gpui/src/platform.rs)
`:1846`) and the read paths on both macOS and Wayland already produce
it. Only the write paths are missing — without this task, the
sibling FM-publish task can call `cx.write_to_clipboard(...)` all
day and Finder / Nautilus will see nothing.

### macOS — `gpui_macos::pasteboard::Pasteboard::write`

[`vendor/zed/crates/gpui_macos/src/pasteboard.rs`](spec:src:vendor/zed/crates/gpui_macos/src/pasteboard.rs)
`:178` currently matches `[ClipboardEntry::ExternalPaths(_)] => {}`.
Replace with a real branch that:

1. Clears `self.inner` (the `NSPasteboard`).
2. Builds an `NSArray<NSString>` of UTF-8 paths from
   `ExternalPaths(paths).0`.
3. Calls `setPropertyList:forType:NSFilenamesPboardType` with that
   array. The legacy type is what the existing `read` path consults
   at `:53`, so symmetry trumps the deprecation warning here.
4. Optionally *also* writes the modern `public.file-url` type for
   each path as a parallel offer. Out of scope for v1 — verify with
   a manual Finder paste first; only add it if Finder ignores the
   legacy type.

The mixed-entries arm at `:179` (`_ =>`) currently collapses to
plain text; leave it alone unless an `ExternalPaths + String` mix
becomes a real use-case.

### Wayland — `WaylandClient::write_to_clipboard`

[`vendor/zed/crates/gpui_linux/src/linux/wayland/client.rs`](spec:src:vendor/zed/crates/gpui_linux/src/linux/wayland/client.rs)
`:966`. Today the data-source offers `TEXT_MIME_TYPES` and
`self_mime` unconditionally; nothing offers `text/uri-list`
(constant already exists at
[`vendor/zed/crates/gpui_linux/src/linux/wayland/clipboard.rs`](spec:src:vendor/zed/crates/gpui_linux/src/linux/wayland/clipboard.rs)
`:19` as `FILE_LIST_MIME_TYPE`). Changes:

1. When the `ClipboardItem` contains an `ExternalPaths` entry,
   advertise `FILE_LIST_MIME_TYPE` on the `data_source.offer(...)`
   loop alongside the text/self offers.
2. Extend the data-source `send` handler so that requests for
   `text/uri-list` write `file:///abs/path\r\n` lines per RFC 2483.
   Match the existing per-MIME branch shape in
   `vendor/zed/crates/gpui_linux/src/linux/wayland/clipboard.rs`'s
   `Clipboard::set` write path — that's where Zed already keeps
   the staged buffer per MIME type for handing to the compositor.
3. Path encoding: use `percent_encoding::utf8_percent_encode` with
   `CONTROLS` plus `' '` / `'#'` / `'?'` / `'%'` so paths with
   spaces round-trip into Nautilus.

### Test path

Manual cross-app smoke test is the gate — there is no headless
NSPasteboard / wl_data_device to drive in unit tests. Add a tiny
integration helper that exercises `cx.write_to_clipboard(...)`
followed by `cx.read_from_clipboard()` to confirm same-process
round-trip works for `ExternalPaths` on both platforms; the real
cross-app verification lives in the [acceptance](#acceptance)
checklist below.

### Non-goals

- X11: out of scope for the acceptance gate. If the Wayland change
  has a trivial X11 analogue (target advertisement in
  [`vendor/zed/crates/gpui_linux/src/linux/x11/client.rs`](spec:src:vendor/zed/crates/gpui_linux/src/linux/x11/client.rs)
  `:1744`), do it; otherwise defer.
- Windows: codon doesn't ship there yet.

## Acceptance

- A unit test on macOS does
  `cx.write_to_clipboard(ClipboardItem { entries: vec![ExternalPaths(paths)] })`
  followed by `cx.read_from_clipboard()` and gets a matching
  `ExternalPaths` back (currently this returns `None`).
- A unit test on Linux/Wayland (under the existing gpui_linux test
  harness gate) does the same round-trip and passes.
- Manual smoke: a file written from codon into the OS clipboard is
  visible as a real file paste in Finder (macOS) and in
  Nautilus / Dolphin (Wayland).
- `vendor/zed/script/clippy` clean across `gpui_macos` and
  `gpui_linux`.
- `spec lint` clean.

## Files touched

- `vendor/zed/crates/gpui_macos/src/pasteboard.rs` — replace the
  `ExternalPaths` no-op match arm.
- `vendor/zed/crates/gpui_linux/src/linux/wayland/client.rs` —
  advertise `text/uri-list` when paths are present.
- `vendor/zed/crates/gpui_linux/src/linux/wayland/clipboard.rs`
  (or wherever the send-handler dispatch lives) — `text/uri-list`
  formatter.
- One submodule commit on the `codon` branch + the outer pointer
  bump (per `CLAUDE.md` workflow).
