---
id: TASK:phase-16/pickers-last-picker
type: task
status: draft
version: 0.0.1
summary: >
  Track the most recently dismissed picker in a workspace-scoped
  singleton and add a `codon_pickers::LastPicker` action that
  reopens it with the prior query intact. Bound to `prefix p '`.
owners: [carlo]
progress: done
refines:
  - REQ:codon/helix-pickers#c-last-picker
---

# Last-picker reopen

## What changes

Helix's `space '` reopens the most recent picker with its prior
query. To do the same in codon:

1. **Track the last picker.** Add a workspace-scoped singleton
   (similar pattern to `codon_command_palette::CodonPalette`) that
   stores `(action_name: SharedString, query: SharedString)`. Update
   it whenever a registered codon-side picker dismisses:
   - On dismiss, the picker's `PickerDelegate::dismissed` callback
     records its current `query` and the action that opened it.

2. **Add a stashing helper.** Expose a free function
   `codon_pickers::record_dismissed(action_name, query, cx)` that
   pickers call from their `dismissed` impl. Add it to all four
   codon pickers (jumplist, changed-files, plus any in-app pickers
   that already exist; vendored Zed pickers are not in scope —
   only codon-owned pickers track).

3. **Reopen action.** Register `codon_pickers::LastPicker`. The
   handler reads the singleton, dispatches the recorded action,
   then sets the picker's query post-toggle. Picker delegates
   need a small additive surface (`set_query(&str, cx)` or a
   constructor variant `with_initial_query(...)`) — add it to
   `codon-pickers::PickerDelegate` as a default-impl trait method
   so existing pickers don't have to opt in.

4. **Bind.** Add to `[bindings.global]`:

```toml
"prefix p '" = "codon_pickers::LastPicker"
```

Scope decisions:

- **Codon-owned pickers only**, not vendored Zed pickers. Wrapping
  every Zed picker would require touching ~10 separate
  PickerDelegate impls upstream; out of scope.
- **Per-workspace singleton**, not global. A user with two windows
  doesn't expect window B's "last picker" to be window A's.
- **No persistence across restarts.** The stash lives in memory.
  Cross-restart restoration is deferred to a future task if
  demand surfaces.

## Why this clause

`space '` is the cheap convenience that elevates Helix's space
mode from "set of separate pickers" to "fast retry". Users who run
a global search, dismiss to peek at one result, then want to
re-narrow rely on it constantly. The cost is a small singleton +
a tiny trait method; the convenience is high.

## Verification

- Open codon. Press `cmd-k p f`, type a partial name, hit escape.
- Press `cmd-k p '`. File finder reopens with the prior query.
- Open `cmd-k p g`, type a filter, escape. Press `cmd-k p '`. The
  changed-files picker reopens, query preserved.
- Try with a vendored Zed picker (e.g.,
  `command_palette::Toggle` — but codon's palette wraps it; verify
  the codon wrapper records). If the Zed picker doesn't record,
  the singleton stays at the most recent codon-owned picker. No
  crash, no silent confusion.
- Cheatsheet renders `LastPicker`.

## Done when

- `codon-pickers` exposes `record_dismissed` and the singleton.
- The two new pickers (jumplist, changed-files) call it on
  dismiss.
- `LastPicker` reopens the recorded picker with the recorded
  query.
- A unit test under `crates/codon-pickers/src/tests.rs` exercises
  "record → reopen restores query".
- `spec lint` is at zero errors.
