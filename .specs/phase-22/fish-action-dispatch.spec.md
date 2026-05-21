---
id: TASK:phase-22/fish-action-dispatch
type: task
status: accepted
version: 0.1.0
summary: >
  Implement `codon do <action_name> [json-args]` plus the
  convenience helpers (`codon edit`, `codon split`, `codon win`,
  `codon fm`) and tab-completion that reflects the live action
  registry.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fish-shell-integration#c-action-dispatch
  - REQ:codon/fish-shell-integration#c-action-convenience-helpers
aspects: [codon-do, convenience-helpers, tab-completion]
blocked_by:
  - TASK:phase-22/fish-rpc-socket
  - TASK:phase-22/fish-plugin-bootstrap
---

# `codon do` + convenience helpers + tab completion

## Plan

- Server side (in `codon-fish` crate):
  - `action.dispatch { action_name, payload? }` handler:
    1. Look up the typed action in Zed's global registry.
       Unknown → return
       `err { code: "action_unknown", message: "...",
       similar: [...] }` with a small Did-You-Mean list.
    2. Validate payload against the action's JSON schema if
       it has one (Zed action definitions can emit
       `serde_json` schemas via `serde_json::from_value` —
       reuse that path).
    3. Dispatch on the PTY's owning pane (stored at PTY
       spawn — see `fish-rpc-socket`'s window-id plumbing).
    4. Return `{ ok: { action: action_name } }`.
  - `action.list` handler returns the registry contents for
    tab-completion (cached server-side; refreshed on each
    `codon-keymap::Reload`).
- Client side (in `codon.fish`):
  - `codon do <name> [json]` — calls `__codon_rpc
    "action.dispatch" { action_name: $argv[1], payload: $argv[2] }`.
    Surface server errors to stderr with non-zero exit.
  - Convenience helpers (`codon edit`, `codon split`, …) are
    thin shims that translate positional args into typed
    payload JSON and call `codon do <typed-name> <payload>`.
  - Tab completion: `complete -c codon -n '__fish_seen_subcommand_from do' -xa '(__codon_action_list)'`
    where `__codon_action_list` calls
    `action.list` once per session and caches under
    `/tmp/codon-actions-$fish_pid.cache` (refreshed if older
    than 60 s).

## Acceptance

- `codon do file_manager::ToggleHidden` from a fish terminal
  toggles hidden visibility in the focused file manager pane.
- `codon do nonsense::Action` exits non-zero with a stderr
  message listing similar action names.
- `codon do <Tab>` completes to a filtered subset of the
  registry.
- `codon edit ./Cargo.toml` opens the file in a codon editor
  pane.
- `codon split right` splits the current terminal pane to the
  right.
- `codon win 3` jumps to window 3.
- `codon fm` opens an FM pane at cwd.
- `cargo test -p codon-fish` passes.
