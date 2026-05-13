---
id: TASK:phase-7/fm-search-by-name
type: task
status: accepted
version: 0.0.1
summary: >
  `s` opens a name-search picker rooted at current_dir. Uses `fd` if
  installed; `walkdir` fallback otherwise.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-find-search#c-search-by-name
---

# File-manager search-by-name

## What ships

`s` opens a `Picker` modal. The query is type-as-you-go fuzzy
filter. The candidate set is:

- Primary: `fd --type f --type d --hidden --no-ignore <query>`
  rooted at `current_dir`, results streamed in. Output parsed
  lazily so a huge tree doesn't block the modal.
- Fallback (no `fd` binary): synchronous `walkdir` with a hard
  cap of 5000 entries, surfaced via toast when truncated.

Enter on a result reveals it via the `codon_fm::Reveal` action
(TASK:phase-6/fm-reveal-action).

## Approach

- New module `crates/file-manager/src/search.rs` — defines a
  `SearchSource` trait with `Fd` and `Walkdir` implementations.
- Reuse the existing `codon-pickers` `Picker` shape for
  rendering and key handling.
- Spawn the search as a background task; the picker fills
  incrementally.

~250 LOC.
