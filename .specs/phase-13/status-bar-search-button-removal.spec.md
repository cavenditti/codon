---
id: TASK:phase-13/status-bar-search-button-removal
type: task
status: accepted
version: 0.0.1
summary: >
  Remove the magnifying-glass SearchButton from the codon status
  bar — search is reachable from the keymap and command palette,
  not from a status indicator.
owners: [carlo]
progress: done
refines:
  - REQ:codon/status-bar#c-no-search-button
---

# Drop the SearchButton from the status bar

## What changes

`apps/codon/src/zed.rs:543` constructs a
`search::search_status_button::SearchButton` and `:588` registers it
via `add_left_item`. The button duplicates an action that is
already on the keymap and discoverable through the command palette;
it earns a permanent slot on the bar for no signal value.

Removing it:

- deletes the `search_button` local binding at line 543;
- deletes the `add_left_item(search_button, ...)` call at line 588;
- removes the now-unused dependency line from `apps/codon/Cargo.toml`
  if no other codon-side consumer of `search::search_status_button`
  exists (check before deleting — `search` is used by the editor
  internals and likely stays).

## Approach

1. Edit `apps/codon/src/zed.rs`: drop the two referenced lines.
2. Run `cargo check -p codon`; expect no unused-import warnings in
   the wider `use` block at the top of the file (the `search`
   crate is brought in via path, not a `use`, so this should be
   clean).
3. Confirm the search keybindings still resolve — `cmd-f`, `cmd-shift-f`,
   and any codon TOML override remain unchanged because the button
   was a UI shortcut, not the action source.

## Non-goals

- No removal of the `SearchButton` type itself from `vendor/zed`.
  Upstream consumers (the standalone Zed editor) still register
  it; deleting it would be an unnecessary upstream divergence.
- No new "search status" indicator. There is intentionally no
  replacement.

## Files touched

- `apps/codon/src/zed.rs` — drop the two referenced lines and any
  surrounding whitespace.

## Verification

- `cargo run -p codon` launches; the magnifying-glass icon is
  absent from the left zone.
- `cmd-f` and `cmd-shift-f` still open the editor's find / project
  search overlays.
- The command palette still lists "buffer search" and "project
  search" entries.
