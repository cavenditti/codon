---
id: TASK:phase-10/jump-action-target
type: task
status: accepted
version: 0.0.1
summary: >
  `codon_jump::JumpToTarget` action + default `cmd-k j` binding
  in codon-keymap + curated cheatsheet entry. Activates the
  overlay covering Word + Url + Clickable candidates.
owners: [carlo]
progress: done
refines:
  - REQ:codon/jump-hints#c-jump-targets
aspects: [action, keymap-binding, resolver-arm]
---

# JumpToTarget entry action

## What ships

- Action `codon_jump::JumpToTarget` (zero-data,
  `actions!(codon_jump, [JumpToTarget])`).
- Handler in `codon-jump`: `JumpOverlay::open(JumpMode::Target,
  window, cx)`.
- One TOML line in `crates/codon-keymap/src/keymap.rs`
  `DEFAULT_KEYMAP` under `[bindings.global]`:
  ```toml
  "cmd-k j" = "codon_jump::JumpToTarget"
  ```
- One resolver arm in `resolve_binding`:
  ```rust
  "codon_jump::JumpToTarget" => bind!(codon_jump::JumpToTarget),
  ```
- `codon-keymap`'s curated cheatsheet picks up the entry
  automatically once the TOML line is present.

`cmd-k j` is the canonical chord — under codon's `cmd-k` prefix
convention, `j` for "jump". `cmd-k u` is reserved for the URL
variant (separate task).

## Verification

- `cmd-k j` from any pane opens the overlay; pressing two label
  chars dispatches the candidate's action.
- `cmd-k F1` shows `cmd-k j → Jump to target` in the cheatsheet.
- User override in `~/.config/codon/codon.toml`:
  `"space j" = "codon_jump::JumpToTarget"` — resolver arm picks
  it up and the user chord works.

## Where it slots in

- New: handler fn in `crates/codon-jump/src/codon_jump.rs`.
- Edit: `crates/codon-keymap/src/keymap.rs` — TOML line + resolver
  arm.
- Edit: `apps/codon/src/main.rs` — `codon_jump::init(cx)` in
  startup so the action is registered.
- Edit: `apps/codon/Cargo.toml` — `codon-jump.workspace = true`.
