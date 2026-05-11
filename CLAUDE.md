# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Codon is a **terminal-first, always-modal multiplexer editor** built as a Zed fork. It is a single Cargo workspace that combines codon-specific crates (under `crates/`) with a vendored copy of Zed (`vendor/zed/`, a git submodule on the `codon` branch) and a vendored copy of forge-spec for roadmap tracking (`vendor/forge-spec/`, also a submodule).

The binary is `apps/codon` (`codon-architecture.typ` has the long-form design doc; treat it as background reading).

## Commands

```sh
# Build / run the main binary
cargo build -p codon
cargo run   -p codon

# Per-crate check / test
cargo check -p codon-session
cargo test  -p codon-keymap [test_name]

# Lint: when editing inside vendor/zed/, use vendor/zed/script/clippy
#   (not `cargo clippy`) — this is required by Zed's own conventions.
( cd vendor/zed && ./script/clippy )

# Roadmap / task tracking — see .specs/AGENTS.md for the full vocabulary
SPEC=vendor/forge-spec/spec-cli/target/release/spec
$SPEC lint                              # 0 errors expected on a clean tree
$SPEC todo                              # what's open right now
$SPEC coverage REQ:codon/sessions       # per-clause + per-task progress
$SPEC start  TASK:phase-N/foo           # lifecycle: start/done/block/defer/wontdo/reset
$SPEC done   TASK:phase-N/foo

# Pre-commit hook (one-time, opt-in per clone) — runs spec lint when .spec.md files are staged
git config core.hooksPath .githooks
```

The `spec` binary must be built once after fresh clone: `( cd vendor/forge-spec/spec-cli && cargo build --release )`. The pre-commit hook will build it lazily if missing.

Rust toolchain is pinned in `rust-toolchain.toml` (currently 1.95.0).

## Repository layout

```
apps/codon/             entry binary; main.rs replaces Zed's main with codon's init order
crates/
  codon-mode/           PaneMode (Normal/Insert/Command) + Selection trait, force-on Helix mode
  codon-keymap/         TOML keymap loader + cmd-k F1 cheatsheet modal + chord-timeout setter
  codon-session/        tmux-style sessions + windows + in-memory pane stash for switching
  codon-agent/          cross-pane agent verbs (Explain/Summarize/Refactor) seeded from selections
  file-manager/         yazi-style three-column file manager (its own Item, not Zed's project_panel)
vendor/zed/             git submodule, branch `codon` — all upstream changes committed here
vendor/forge-spec/      git submodule — spec-cli for `.specs/` tracking
vendor/helix/           git submodule — vendored but not yet integrated (Phase 4 territory)
.specs/                 roadmap as forge-spec TOPIC/REQ/TASK files; see .specs/AGENTS.md
.githooks/pre-commit    optional spec-lint guard (enable with the command above)
assets/config/          user-facing example configs (e.g. keymap.example.toml)
```

The workspace `Cargo.toml` enumerates every vendored Zed crate as a workspace member — modifications to Zed internals compile and link directly without a separate publish step.

## Architecture — the bits that span files

**Modal layer.** `codon-mode` defines `PaneMode { Normal, Insert, Command }` and a global `CodonModeTracker`. Each codon pane (terminal, file-manager, agent) updates the tracker on focus; the status bar reads from it. The pane-mode model coexists with Vim mode — Helix mode is force-enabled in `vim` by default.

**Sessions + windows.** Codon is a single-OS-window multiplexer. Sessions are persisted to the global KVP (key `codon_sessions_v1`). Window-switching uses an **in-memory pane stash** (`WindowRuntimeCache` in `crates/codon-session/src/runtime.rs`) — cloned `Member` trees + active pane handles keep panes (and their workspace subscriptions) alive across switches. The persisted JSON `LayoutSnapshot` is the fallback for cross-restart restoration only. The `workspace::codon_bridge` module (in vendored Zed) exposes `capture_layout` / `apply_layout` / `replace_center_with_empty_pane` / `restore_center_root` to support this.

**Vendored helpers.** Several codon features required small public surfaces added to vendored Zed crates rather than full forks. Key examples:
- `vendor/zed/crates/workspace/src/codon_bridge.rs` — `LayoutSnapshot` types + capture/apply.
- `Workspace::replace_center_with_empty_pane`, `restore_center_root`, `serialize_workspace_now` — pane-tree manipulation primitives.
- `AgentPanel::seed_explain_with_selection` — entry point for cross-pane agent verbs.
- `gpui::set_keystroke_chord_timeout` — process-wide chord-timeout override; codon sets 5 s for multi-key chords.

When editing inside `vendor/zed/`, follow the upstream conventions in `vendor/zed/CLAUDE.md` (no `unwrap()`; no silent `let _ =`; never `mod.rs`; prefer additive changes to existing files; use `./script/clippy`).

**Keymap.** Default bindings are an embedded TOML string in `crates/codon-keymap/src/keymap.rs`. User overrides go to `~/.config/codon/keymap.toml`. Bindings are codon's only entry point for actions — every cross-cutting verb (`codon_session::*`, `codon_agent::*`, `codon_keymap::ShowKeymap`, etc.) is registered via TOML, not via Zed's JSON keymap files. `assets/config/keymap.example.toml` is the user-facing template.

## Workflow conventions

**Two repos to commit in.** Changes to `vendor/zed/` are submodule commits on the `codon` branch — commit there first, then commit the submodule-pointer bump in the outer repo. Same for `vendor/forge-spec/` when extending the spec tool itself.

**Conventional commit prefixes.** `feat(...)`, `fix(...)`, `docs(...)`, `chore(...)`. Scope = crate name when one is the clear subject (`feat(codon-session): ...`). Append `Spec-Ref:` trailers when the commit touches a clause or task:

```
Spec-Ref: TASK:phase-2/session-new (implements)
Spec-Ref: REQ:codon/sessions#c-create (touches)
```

`spec lint` checks that referenced TASK ids exist. The pre-commit hook (opt-in) runs this on `.spec.md` changes.

**Phase planning.** Roadmap is in `.specs/`, not `TODO.md`. `TODO.md` is a one-page pointer. To plan new work, write `TASK:phase-N/<slug>.spec.md` files refining clauses on existing `REQ:codon/<area>` specs — see `.specs/AGENTS.md` for the full vocabulary. `spec todo` is the single source of truth for what's open.

**Spec-first, always.** Any new feature — even a small one — follows the same order: (1) write or extend the `REQ:codon/<area>` spec with clauses, (2) author one `TASK:phase-N/<slug>.spec.md` per clause, (3) only then start the prototype. Never skip straight from "good idea" to code. The spec is the design conversation; the TASKs make scope reviewable; the prototype implements what's already agreed. `spec lint` must stay clean across the trio.

## When in doubt

- Reach for an existing codon crate before adding a new one — `codon-session` has the picker pattern, `codon-keymap` has the modal pattern, `codon-mode` has the selection/focus pattern.
- Reach for a vendored Zed primitive before writing a new one — `picker::Picker`, `ui::KeyBinding::from_keystrokes`, `workspace::ModalView`, `buffer_diff::DiffHunk`, `git::FileStatus` all already exist.
- Check `.specs/` for an existing task before starting work — `spec ancestors <id>` and `spec coverage REQ:codon/<area>` will tell you what's already planned.
