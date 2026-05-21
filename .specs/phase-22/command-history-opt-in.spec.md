---
id: TASK:phase-22/command-history-opt-in
type: task
status: accepted
version: 0.1.0
summary: >
  Default-off opt-in for command-history. The first time a user
  flips `[command_history] enabled = true` codon opens a one-pane
  onboarding describing what's stored, where, which redactor + model
  is used. Second-time enables are silent.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/command-history#c-opt-in
  - REQ:codon/project-knowledge-base#c-opt-in
aspects: [history-feature-flag, project-kb-shared-flag]
---

# Opt-in gate + first-run onboarding

## Plan

- Config:
  - `[command_history] enabled = false` (default) in
    `assets/config/codon.example.toml` with prominent comments
    explaining the LLM-call implication and the workspace-scope.
  - The flag is read at workspace open. When false: the subscriber
    is not registered, no rows are inserted, the picker shows an
    empty state with a hint at the binding to enable the feature.
- First-run onboarding pane:
  - A `meta` row tracks `command_history_acknowledged_at`.
  - When the user flips `enabled = true` and that row is NULL,
    open a workspace pane (full screen via `codon_panes::Open`)
    with a markdown-rendered description:
    - What we store (redacted command + output excerpt + summary).
    - Where (sqlite at the documented path).
    - Which redactor is the default (Presidio offline) + how to
      switch.
    - Which model summarizes commands (the harness's configured
      model) + the daily budget.
    - A one-keystroke "show me an example entry" button (uses a
      canned fixture so no real terminal data is exposed).
    - "I understand, enable command history" confirm; "Disable
      and remove the file" decline.
  - On confirm: write `command_history_acknowledged_at = now`;
    register the subscriber.
  - On decline: rewrite `codon.toml` to set `enabled = false` and
    delete the (empty) sqlite file if no rows yet exist.
- Second-time-on: when `command_history_acknowledged_at` is
  non-NULL and the user flips the flag back on, no pane opens.

## Acceptance

- Default config has the feature off — no sqlite file is created
  on a fresh workspace.
- Setting `enabled = true` for the first time opens the onboarding
  pane.
- Decline path leaves `enabled = false` in the config and removes
  the file.
- Confirm path sets the `acknowledged_at` timestamp and registers
  the subscriber.
- Toggling off → on → off → on on a workspace that's already
  acknowledged opens no onboarding pane.
