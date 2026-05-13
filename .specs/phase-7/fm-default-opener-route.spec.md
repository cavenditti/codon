---
id: TASK:phase-7/fm-default-opener-route
type: task
status: accepted
version: 0.0.1
summary: >
  Enter / `l` consults the opener config before falling through to
  workspace.open_abs_path. Unique-match opens directly; multi-match
  surfaces the `O` picker.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fm-openers#c-default-opener-route
---

# File-manager default Enter route through openers

## What ships

The existing `enter_directory` handler (the file branch — line
~380 in `file_manager.rs`) consults the `OpenerStore` first:

1. Find every opener whose glob / mime matches the entry.
2. If exactly one matches AND it's not the synthetic default,
   spawn its `cmd` (subject to `block`).
3. If multiple match, dispatch the `O` picker
   (TASK:phase-7/fm-choose-opener) instead of opening.
4. If zero match, the current `workspace.open_abs_path` path runs
   unchanged — this preserves today's behavior for files Zed
   already knows how to open (text, image via image_viewer, …).

## Where it slots in

[`crates/file-manager/src/file_manager.rs`](spec:src:crates/file-manager/src/file_manager.rs)
`enter_directory`. Depends on TASK:phase-7/fm-opener-config and
TASK:phase-7/fm-choose-opener. ~80 LOC.
