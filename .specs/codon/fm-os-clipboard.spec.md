---
id: REQ:codon/fm-os-clipboard
type: requirement
status: draft
version: 0.1.0
level: MUST
summary: >
  Codon's file manager MUST round-trip file copy/paste with the
  native file managers on macOS (Finder) and Wayland (Nautilus,
  Dolphin, Thunar), and MUST accept image / file-list pastes from
  arbitrary OS-clipboard sources (Firefox, Slack, …) into the
  current directory. Today the FM's clipboard is process-local
  and the GPUI platform write path is a no-op for `ExternalPaths`;
  this requirement closes both gaps.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-21]
---

# File manager — OS clipboard integration

## Context

The file manager already has a polished keyboard-driven clipboard
(`y`, `d`, `p`, `P`, `Y`) backed by a process-local `FmClipboard`
enum at
[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
`:124`. The comment at `:120` makes the design choice explicit:
keep the OS pasteboard untouched so a yank in the FM doesn't
collide with terminal/agent pastes. Phase 21 walks back the
"untouched" half — the FM should mirror to the OS pasteboard while
still using the internal state for its private cut-vs-copy
semantics — and closes the read direction so a `Copy files` in
Finder / Nautilus is reachable from a subsequent `p` in codon.

GPUI already models file-URL clipboard entries (`ClipboardEntry::ExternalPaths`
at
[`vendor/zed/crates/gpui/src/platform.rs`](spec:src:vendor/zed/crates/gpui/src/platform.rs)
`:1846`) and reads them on both platforms. Only the *write* side
is missing: macOS `Pasteboard::write` matches the variant and
returns silently
([`vendor/zed/crates/gpui_macos/src/pasteboard.rs`](spec:src:vendor/zed/crates/gpui_macos/src/pasteboard.rs)
`:178`); the Wayland data-source only advertises text and
`self_mime` types
([`vendor/zed/crates/gpui_linux/src/linux/wayland/client.rs`](spec:src:vendor/zed/crates/gpui_linux/src/linux/wayland/client.rs)
`:966`).

The cut-vs-copy semantics question is answered by the upstream
landscape: macOS Finder has no "cut" on the clipboard at all (the
move-on-paste is a Cmd-Opt-V *paste-time* decision), and the
KDE / GNOME `application/x-kde-cutselection` MIME hint isn't
universal. Codon resolves it by keeping cut-vs-copy private:
the OS clipboard always carries a "copy", and the FM's internal
`FmClipboard::Cut` flag decides whether the source is removed
when we paste from our own clipboard. Cross-app pastes therefore
always copy — matching Finder semantics and avoiding the
half-supported Linux hint protocols.

:::{requirement id="fm-os-clipboard" level="MUST"}
The system MUST:

- {#c-platform-write-paths} Make GPUI's platform-layer
  `write_to_clipboard` publish `ClipboardEntry::ExternalPaths`
  as the native file-list pasteboard type: `NSFilenamesPboardType`
  on macOS (using the same legacy type the read path already
  speaks), and `text/uri-list` advertised on the `wl_data_source`
  on Wayland. The existing string/image branches MUST keep
  working unchanged. X11 is best-effort and is not part of the
  acceptance gate.

- {#c-fm-publishes-to-os} The FM `y` (yank) and `d` (cut)
  handlers MUST publish the target path set to the OS clipboard
  *in addition to* updating the internal `FmClipboard`. The OS
  side always carries the paths as `ExternalPaths` (copy); the
  internal `FmClipboard::Cut` flag continues to drive
  move-vs-copy when codon's own paste consumes the clipboard.
  The newline-joined string form remains reachable via `Y`
  (shift-y), unchanged.

- {#c-fm-paste-from-os} The FM `p` / `P` handlers MUST consult
  the OS clipboard when `self.clipboard` is empty. An
  `ExternalPaths` entry resolves into a copy into the current
  directory (move semantics are intentionally not inferred from
  cross-app sources — see context). An `Image` entry resolves
  into "save as a timestamped file in the current directory"
  with a reasonable extension picked from the image format. A
  pure `String` entry MUST NOT be written as a new file's
  contents (explicit non-goal); the FM surfaces a toast
  instructing the user to paste into a terminal or editor pane.

- {#c-cross-app-roundtrip} A `y` in codon's FM followed by a
  "Paste" in the native file manager (Finder on macOS;
  Nautilus / Dolphin / Thunar on Wayland) MUST copy the file(s)
  into the target directory. A "Copy" in the native file manager
  followed by `p` in codon's FM MUST copy the file(s) into
  codon's current directory. Both directions are part of the
  acceptance gate.
:::
