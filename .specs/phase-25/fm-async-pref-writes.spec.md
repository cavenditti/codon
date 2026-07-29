---
id: TASK:phase-25/fm-async-pref-writes
type: task
status: accepted
version: 0.1.0
summary: >
  Debounce and background preference/bookmark persistence so no
  keystroke blocks the UI thread on a disk write.
owners: [carlo]
progress: done
refines: ["REQ:codon/fm-op-responsiveness#c-async-config-writes"]
assignee:
eta:
blocked_by: []
---

# Fm async pref writes

## Plan

Refines `REQ:codon/fm-op-responsiveness#c-async-config-writes`.

[FmPrefs::save](spec:src:crates/file-manager/src/prefs.rs:168-194)
does synchronous `create_dir_all` + `write` on the UI thread, called
from every setter
([prefs.rs:143-166](spec:src:crates/file-manager/src/prefs.rs:143-166));
[nudge_preview_fraction](spec:src:crates/file-manager/src/file_manager.rs:4498-4503)
reaches it once per `+`/`-` keystroke under auto-repeat.
[BookmarkStore::save](spec:src:crates/file-manager/src/bookmarks.rs:71-95)
has the same shape.

- Move serialization + write to the background executor behind a
  trailing debounce (~300 ms), last-write-wins.
- Flush any pending write on entity drop / app quit so no change is
  lost.
- Apply the same treatment to both `FmPrefs` and `BookmarkStore`.

## Acceptance

- Holding `+` (auto-repeat) results in a small bounded number of disk
  writes rather than one per keystroke (test with an injected write
  counter).
- No `std::fs` write remains on the foreground path for prefs or
  bookmarks.
- A change made immediately before quit is persisted (flush-on-exit
  test).
