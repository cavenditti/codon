---
id: TASK:phase-14/keymap-decouple
type: task
status: draft
version: 0.0.1
summary: >
  Decouple `codon-keymap` from `codon-agent`, `codon-command-palette`,
  `codon-config`, `codon-jump`, and `codon-session`. Action
  registration moves to each owning crate's `init(cx)`; keymap
  parses TOML and resolves actions through the GPUI action registry
  only.
owners: [carlo]
progress: done
refines:
  - REQ:codon/code-quality#c-keymap-decoupled
---

# Decouple `codon-keymap` from downstream codon crates

## What changes

`crates/codon-keymap/Cargo.toml` currently depends on:

```toml
codon-agent.workspace = true
codon-command-palette.workspace = true
codon-config.workspace = true
codon-jump.workspace = true
codon-session.workspace = true
```

Plus the (legitimate) `gpui`, `workspace`, `ui`, `vim`, `settings`,
`fs`, `toml`, `serde`, `log`, `anyhow`, `command_palette`,
`diagnostics`, `git`, `git_ui`, `menu`, `file-manager`,
`zed_actions`, `serde_json`. The downstream-codon group is the bug.

Commit `88dfb1a` ("resolve keymap actions through the GPUI
registry") already moved action resolution off compile-time
references; the remaining deps are likely from `init(cx)` calls
where keymap registers actions on behalf of the owning crate.

The fix: each owning crate exposes a `pub fn init(cx: &mut App)`
that registers its own actions; codon's main `init` in
`apps/codon/src/zed.rs` calls each of them once at startup; keymap
no longer references the downstream crates at all.

## Approach

1. Inventory `crates/codon-keymap/src/keymap.rs` for every place a
   per-crate action type is named or registered. Each entry maps to
   either:
   - "action already registered in <crate>'s `init(cx)`" — delete
     the reference in keymap and confirm the action resolves via
     GPUI registry at runtime.
   - "action registered ONLY in keymap" — move the registration to
     the owning crate's `init(cx)`; keymap loses the reference.

2. For each of the five crates, ensure it has a `pub fn init(cx: &mut App)`
   that registers all its actions. (Most already do; the audit will
   confirm.)

3. `apps/codon/src/zed.rs` calls each crate's `init(cx)`. Check that
   the call order matches any inter-crate registration ordering
   constraints (it should be lexical-order safe; if not, document).

4. Trim `crates/codon-keymap/Cargo.toml`:
   ```diff
   -codon-agent.workspace = true
   -codon-command-palette.workspace = true
   -codon-config.workspace = true
   -codon-jump.workspace = true
   -codon-session.workspace = true
   ```
   Keep `command_palette` (vendored Zed crate, different name).

5. `cargo build -p codon-keymap` — clean.
6. `cargo build -p codon` — clean. Runtime action resolution still
   works because the actions are registered in each crate's `init`.

## Risk

This is the largest single change in the phase. Action resolution
that works at compile time today (via the typed action reference)
moves to runtime registry lookup. If any keystroke fails to fire
after the change, the symptom is silent — the registry lookup
returns None and the action no-ops. Verification step 4 below
exists to catch that.

Mitigation:
- Do this in a worktree.
- Land it AFTER `hygiene-kill-silent-discards` so the registry-miss
  path (if it logs at all) shows the error instead of swallowing it.

## Non-goals

- Not changing the TOML format.
- Not changing the GPUI action-registry API.
- Not moving keymap defaults to a separate `codon-defaults` crate —
  the embedded TOML stays in `codon-keymap`. Decoupling is at the
  dependency layer, not the file layer.

## Files touched

- `crates/codon-keymap/Cargo.toml` — drop 5 deps.
- `crates/codon-keymap/src/keymap.rs` — remove per-crate type
  references; resolve via registry only.
- `crates/codon-agent/src/lib.rs` — confirm `pub fn init(cx)`
  registers all agent actions.
- `crates/codon-command-palette/src/lib.rs` — same.
- `crates/codon-config/src/lib.rs` — same (config may not register
  actions; if so, delete the dep without further work).
- `crates/codon-jump/src/lib.rs` — same.
- `crates/codon-session/src/lib.rs` (or `codon_session.rs`) — same.
- `apps/codon/src/zed.rs` — confirm each crate's `init(cx)` is
  called once.

## Verification

- `cargo build -p codon-keymap` — clean.
- `cargo build -p codon` — clean.
- `cargo tree -p codon-keymap | grep '^codon-' || true` — returns
  no `codon-*` entries.
- Manual smoke: every default keybinding in
  `crates/codon-keymap/src/keymap.rs` fires its action. Verify a
  representative sample: `codon_session::SessionSwitch`,
  `codon_agent::Explain`, `codon_command_palette::Toggle`,
  `codon_jump::Toggle`, and any cheatsheet-bound action.
- `git diff --stat crates/codon-keymap/Cargo.toml` — five lines
  removed under `[dependencies]`.
