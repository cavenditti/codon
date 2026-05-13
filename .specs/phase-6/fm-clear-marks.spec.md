---
id: TASK:phase-6/fm-clear-marks
type: task
status: accepted
version: 0.0.1
summary: >
  `uv` (un-visual, vim-style) clears the entire marked set.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-selection#c-clear-marks
---

# File-manager clear-marks

## What ships

A chord-style binding `u` then `v` that empties `marked`. Distinct
from the single-entry mark toggle on `v` so the user retains
existing muscle memory.

## Where it slots in

Chord state machine — same `pending_chord: Option<char>` field
the bookmarks task (TASK:phase-6/fm-bookmarks) introduces. If
that task hasn't shipped yet, introduce the field here and let
bookmarks build on it.

~30 LOC.
