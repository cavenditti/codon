# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

Codon is a **terminal-first, always-modal multiplexer editor** built as a Zed fork. It is a single Cargo workspace that combines codon-specific crates (under `crates/`) with a vendored copy of Zed (`vendor/zed/`, a git submodule on the `codon` branch) and a vendored copy of forge-spec for roadmap tracking (`vendor/forge-spec/`, also a submodule).

**Keyboard-first, always.** Codon is driven entirely by the keyboard. Never add or preserve mouse-only affordances like tab close "x" buttons, hover-only icons, or click-to-do-anything controls when a keybinding already covers the action. When porting UI from Zed, strip those affordances and rely on the codon TOML keymap. If a verb has no binding yet, add the binding to the TOML defaults — do not fall back to leaving a mouse control in place.

The binary is `apps/codon` (`codon-architecture.typ` has the long-form design doc; treat it as background reading).

## Commands

```sh
# Build / run the main binary
cargo build -p codon
cargo run   -p codon

# Build + install/update /Applications/Codon.app (the release flow)
scripts/install-mac-app     # -n skip build, -o open after install
scripts/gen-mac-icon        # regen assets/mac/Codon.icns from the SVG logo

# Per-crate check / test
cargo check -p codon-session
cargo test  -p codon-keymap [test_name]

# Lint: when editing inside vendor/zed/, use vendor/zed/script/clippy
#   (not `cargo clippy`) — this is required by Zed's own conventions.
( cd vendor/zed && ./script/clippy )

# Roadmap / task tracking — see .specs/AGENTS.md for the full vocabulary.
# The `spec` binary is on PATH — invoke it as `spec`, not via the vendored
# release path. (Built once from vendor/forge-spec/spec-cli; the pre-commit
# hook will build it lazily if missing.)
spec lint                              # 0 errors expected on a clean tree
spec todo                              # what's open right now
spec coverage REQ:codon/sessions       # per-clause + per-task progress
spec start  TASK:phase-N/foo           # lifecycle: start/done/block/defer/wontdo/reset
spec done   TASK:phase-N/foo

# Pre-commit hooks (one-time, opt-in per clone) — managed by prek
# (https://prek.j178.dev). Installs fmt/clippy/test for codon crates + spec lint.
prek install

# Legacy shell hook (spec-lint only) — superseded by prek; keep for clones
# without prek installed.
git config core.hooksPath .githooks
```

Rust toolchain is pinned in `rust-toolchain.toml` (currently 1.95.0).

## Repository layout

```
apps/codon/             entry binary; main.rs replaces Zed's main with codon's init order
crates/
  codon-pane-bridge/    PaneMode enum + CodonModeTracker global + PaneModeBridge trait (cycle-free base)
  codon-mode/           re-exports pane-bridge + mode_indicator that translates vim::state to PaneMode
  codon-keymap/         TOML keymap loader + cheatsheet modal + configurable chord prefix + chord-timeout setter
  codon-session/        tmux-style sessions + windows + in-memory pane stash for switching
  codon-panes/          adapts agent/git/outline/debug/peek panels into pane-kind splits (phase 12)
  codon-pickers/        shared ModalScaffold for codon modals/pickers (focus/dismiss/mode triplet)
  codon-command-palette/ Helix-style `:` palette over Zed's command registry
  codon-jump/           Vimium-style jump-hint overlay (`prefix j` / `prefix u`)
  codon-agent/          cross-pane agent verbs (Explain/Summarize/Refactor) seeded from selections
  codon-config/         unified ~/.config/codon/codon.toml loader + writeback (toml_edit)
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

**Modal layer.** `codon-pane-bridge` owns the `PaneMode { Normal, Insert, Command }` enum, the global `CodonModeTracker`, and the `PaneModeBridge` trait every codon pane / modal implements. A single focus subscriber installed via `install_pane_mode_dispatcher` picks the focused entity, calls its bridge impl, and writes the tracker — no crate updates the tracker directly. `codon-mode` re-exports the types and hosts `mode_indicator`, which translates `vim::state` per-pane into a `PaneMode` for the status bar. The pane-mode model coexists with Vim mode — Helix mode is force-enabled in `vim` by default.

**Sessions + windows.** Codon is a single-OS-window multiplexer. Sessions are persisted to the global KVP (key `codon_sessions_v1`). Window-switching uses an **in-memory pane stash** (`WindowRuntimeCache` in `crates/codon-session/src/runtime.rs`) — cloned `Member` trees + active pane handles keep panes (and their workspace subscriptions) alive across switches. The persisted JSON `LayoutSnapshot` is the fallback for cross-restart restoration only. The `workspace::codon_bridge` module (in vendored Zed) exposes `capture_layout` / `apply_layout` / `replace_center_with_empty_pane` / `restore_center_root` to support this, plus a single unified registry surface — `codon_register_pane_kind(spec)` / `codon_pane_kind_spec(kind)` — that codon crates use to teach Zed how to serialize, restore, and seed their pane kinds. The previous two-shape registry (function-pointer `OnceLock` + closure `HashMap`) collapsed into that single API in phase 14.

**Vendored helpers.** Several codon features required small public surfaces added to vendored Zed crates rather than full forks. Key examples:

- `vendor/zed/crates/workspace/src/codon_bridge.rs` — `LayoutSnapshot` types + capture/apply, plus the unified `codon_register_pane_kind` / `codon_pane_kind_spec` registry for codon-injected pane kinds (one shape, not two).
- `Workspace::replace_center_with_empty_pane`, `restore_center_root`, `serialize_workspace_now` — pane-tree manipulation primitives.
- `AgentPanel::seed_explain_with_selection` — entry point for cross-pane agent verbs.
- `gpui::set_keystroke_chord_timeout` — process-wide chord-timeout override; codon sets 5 s for multi-key chords.

When editing inside `vendor/zed/`, follow the upstream conventions in `vendor/zed/CLAUDE.md` (no `unwrap()`; no silent `let _ =`; never `mod.rs`; prefer additive changes to existing files; use `./script/clippy`).

**Keymap.** Default bindings are an embedded TOML string in `crates/codon-keymap/src/keymap.rs`. User overrides live in `~/.config/codon/codon.toml` (the unified config file; legacy `~/.config/codon/keymap.toml` is still read with a deprecation hint). The tmux-style chord prefix is configurable via `[keymap] prefix = "<chord>"` in `codon.toml` (default `cmd-k`); the loader expands the literal sentinel `prefix` in every keystroke string (defaults *and* user bindings) to the resolved value at bind time — so `"prefix s s"` in the embedded defaults binds as `"cmd-k s s"` out of the box and as `"ctrl-x s s"` once the user sets `prefix = "ctrl-x"`. Bindings are codon's only entry point for actions — every cross-cutting verb (`codon_session::*`, `codon_agent::*`, `codon_keymap::ShowKeymap`, etc.) is registered via TOML, not via Zed's JSON keymap files. `codon-keymap` itself does NOT depend on any downstream codon crate; each owning crate registers its own GPUI actions from its own `init(cx)` (called in turn from `apps/codon/src/main.rs`), and the keymap resolves names through the global action registry only. `assets/config/codon.example.toml` is the user-facing template (the old `keymap.example.toml` is now a one-release-cycle redirect stub).

## Workflow conventions

**Two repos to commit in.** Changes to `vendor/zed/` are submodule commits on the `codon` branch — commit there first, then commit the submodule-pointer bump in the outer repo. Same for `vendor/forge-spec/` when extending the spec tool itself.

**Conventional commit prefixes.** `feat(...)`, `fix(...)`, `docs(...)`, `chore(...)`. Scope = crate name when one is the clear subject (`feat(codon-session): ...`). Append `Spec-Ref:` trailers when the commit touches a clause or task:

```
Spec-Ref: TASK:phase-2/session-new (implements)
Spec-Ref: REQ:codon/sessions#c-create (touches)
```

`spec lint` checks that referenced TASK ids exist. The pre-commit hook (opt-in) runs this on `.spec.md` changes.

**Phase planning.** Roadmap is in `.specs/`, not `TODO.md`. `TODO.md` is a one-page pointer. To plan new work, scaffold `TASK:phase-N/<slug>` files refining clauses on existing `REQ:codon/<area>` specs — see `.specs/AGENTS.md` for the full vocabulary. `spec todo` is the single source of truth for what's open.

**Always use `spec` — never touch `.specs/` files directly.** Treat the spec CLI as the only sanctioned way to interact with the roadmap. Reading raw `.spec.md` files or hand-editing frontmatter bypasses validation, breaks refinement links, and produces commits that `spec lint` will reject. Use these subcommands instead:

```sh
spec todo                                        # what's open right now (start here)
spec coverage REQ:codon/<area>                   # per-clause + per-task progress for an area
spec ancestors TASK:phase-N/<slug>               # parent REQ + refined clauses
spec children  REQ:codon/<area>                  # tasks refining this REQ
spec render --target agent REQ:codon/<area>      # read a spec (agent-rendered)
spec render --target agent TASK:phase-N/<slug>   # read a task (agent-rendered)
spec new task   phase-N/<slug>                   # scaffold a new TASK from the template
spec new req    codon/<area>                     # scaffold a new REQ
spec start  TASK:phase-N/<slug>                  # lifecycle: start → done (or block/defer/wontdo/reset)
spec done   TASK:phase-N/<slug>
spec lint                                        # must stay at 0 errors
```

**Always pass `--target agent` to `spec render`.** The default `human` target strips structure that you rely on for traceability; the `agent` target preserves clause IDs, refinement links, and machine-readable framing. If you need ancestors/descendants in the same call, add `--ancestors full --descendants full` (or `--include-source` when you need resolved source references). Never `cat` / `Read` a `.spec.md` file to get its content — go through `spec render --target agent`.

When `spec new` scaffolds a file, edit it to fill in clauses/acceptance criteria — that body editing is the only direct file write that's appropriate, and it must be followed by `spec lint`. Never use `Read`/`Write`/`Edit` to discover what tasks exist, to enumerate clauses, to read a spec, or to flip lifecycle state — those go through `spec render --target agent` / `spec todo` / `spec coverage` / `spec start|done|block|defer|wontdo|reset`.

**Spec-first, always.** Any new feature — even a small one — follows the same order: (1) `spec new req codon/<area>` (or extend an existing REQ) and fill in clauses, (2) `spec new task phase-N/<slug>` per clause and fill in acceptance criteria, (3) `spec start` and only then start the prototype, (4) `spec done` when shipped. Never skip straight from "good idea" to code. The spec is the design conversation; the TASKs make scope reviewable; the prototype implements what's already agreed. `spec lint` must stay clean across the trio.

**Code quality**: Always prefer clarity over cleverness. Use descriptive names, break complex functions into smaller ones, and add comments where necessary to explain non-obvious logic. Follow Rust's idiomatic practices and leverage the type system to prevent bugs. Always use `cargo clippy` (or `vendor/zed/script/clippy` when editing Zed) to catch common mistakes and enforce code quality standards. Write tests for new features and bug fixes to ensure reliability and maintainability.

**Naming conventions.** Three rules; existing names are grandfathered when a rename would break user keymaps.

- **Action names.** New actions follow `codon_<area>::<Verb>` (matching the GPUI `actions!` macro form). The action registry is keyed on the typed name, so existing namespaces stay as they are — the rule applies to actions added from here on.
- **Enum / struct names.** Types exported from a codon crate carry the crate's vocabulary in the name (`KeymapCheatTab`, not `CheatTab`; `JumpKind`, not `Kind`). Crate-private types are exempt but benefit from the same prefix once they appear in more than one file.
- **Lib-root file.** Each codon crate's lib-root source file matches the Cargo package's underscored form — `crates/codon-session/src/codon_session.rs` for package `codon-session`. New crates follow the same shape; `lib.rs` is a hold-over.

## When in doubt

- Reach for an existing codon crate before adding a new one — `codon-session` has the picker pattern, `codon-keymap` has the modal pattern, `codon-mode` has the selection/focus pattern.
- Reach for a vendored Zed primitive before writing a new one — `picker::Picker`, `ui::KeyBinding::from_keystrokes`, `workspace::ModalView`, `buffer_diff::DiffHunk`, `git::FileStatus` all already exist.
- Check the roadmap for an existing task before starting work via `spec todo` / `spec ancestors <id>` / `spec coverage REQ:codon/<area>` — never grep or read `.specs/` files to figure out what's already planned.
