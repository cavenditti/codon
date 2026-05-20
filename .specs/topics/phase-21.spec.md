---
id: TOPIC:topics/phase-21
type: topic
status: draft
version: 0.1.0
summary: >
  OS clipboard integration. Today codon's file manager owns an
  in-process `FmClipboard` that the OS pasteboard never sees, and
  the GPUI platform layer's `write_to_clipboard` is a no-op for
  `ExternalPaths`. Phase 21 makes file copy/paste round-trip
  between codon's FM and the native file managers on macOS
  (Finder) and Wayland (Nautilus, Dolphin, Thunar), and lets the
  user paste images and file paths from arbitrary apps (Firefox,
  Slack, etc.) into the current FM directory.
owners: [carlo]
---

# Phase 21 — OS clipboard integration

By the end of phase 20 the file manager had a complete keyboard-
driven local clipboard (`y`, `d`, `p`, `P`, `Y`) but every operation
stopped at codon's process boundary. The user explicitly does not
want to fall back to Finder / Nautilus for cross-app file moves; the
FM should be a drop-in replacement.

Three things gate that today:

1. **Platform-layer write is a no-op for file URLs.** Upstream Zed's
   macOS [`Pasteboard::write`](spec:src:vendor/zed/crates/gpui_macos/src/pasteboard.rs)
   matches `[ClipboardEntry::ExternalPaths(_)] => {}` — files
   written to GPUI's clipboard are silently dropped. The Wayland
   client at
   [`vendor/zed/crates/gpui_linux/src/linux/wayland/client.rs`](spec:src:vendor/zed/crates/gpui_linux/src/linux/wayland/client.rs)
   has the same shape — `write_to_clipboard` only advertises
   `TEXT_MIME_TYPES` and `self_mime`, never `text/uri-list`.

2. **The FM never publishes to the OS clipboard.** `y` / `d` mutate
   `self.clipboard: FmClipboard` only. The OS pasteboard is
   deliberately untouched (see the comment at
   [`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
   `:120`) so the user can paste text into a terminal pane without
   collision. Phase 21 inverts that decision while keeping the
   internal state for the FM's private cut-vs-copy semantics.

3. **The FM never reads from the OS clipboard.** `paste_clipboard`
   reads `self.clipboard` only. Nothing in the FM consults
   `cx.read_from_clipboard()`, so a Finder / Nautilus "Copy files"
   is invisible to a subsequent `p` in codon, even though GPUI
   already decodes file URLs into `ClipboardEntry::ExternalPaths`
   on both platforms.

Refining requirement:

- [REQ:codon/fm-os-clipboard](spec:REQ:codon/fm-os-clipboard) — the
  bidirectional clipboard integration, broken into a platform-write
  task, an FM-publish task, an FM-paste-fallback task, and an
  image-paste task.

## Scope and explicit non-goals

In scope:

- macOS via `NSFilenamesPboardType` (and the modern `public.file-url`
  equivalent if it matters in practice — the existing read path uses
  the legacy type).
- Wayland via `text/uri-list` advertised on the `wl_data_source`.

Best-effort (no acceptance gate):

- X11 (most users run Wayland on Linux; X11 keeps working at whatever
  level upstream Zed provides).

Out of scope (explicit non-goals for phase 21):

- Pasting an arbitrary text string into the FM as a new file's
  contents. The cross-app cases the user named are file moves and
  image pastes; turning random text into a file would surprise
  more than it would help. Can be revisited if asked for.
- KDE / GNOME cut-vs-copy hint MIME types (e.g.
  `application/x-kde-cutselection`). Codon's FM keeps cut-vs-copy
  semantics private — the OS clipboard always carries a "copy", and
  internal `FmClipboard::Cut` decides whether the source is removed
  on our own paste. Cross-app paste therefore always copies; that
  matches the macOS Finder semantics already.
