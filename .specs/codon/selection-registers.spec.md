---
id: REQ:codon/selection-registers
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Typed-selection registers. Any `Selection` (Text / Files /
  Hunks / Commits / Blocks / Messages / Diagnostics / …) can be
  stored in a named register; selection-producing verbs can write
  to a register; selection-consuming verbs can read from one.
  Per-session by default, named-persistent across sessions when
  declared in codon.toml. Interops with Helix's text registers
  from the vendored vim crate.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-19]
---

# Selection registers

## Context

Helix's text registers (`"a yy`, `"a p`) are the pipeline-
composition mechanism that makes "the user's working set" an
inspectable, named, reusable artifact instead of a fleeting
yank. Codon's typed `Selection` enum is the same idea generalized
beyond text — but the register layer is missing. Today selections
exist only as transient state on the focused pane.

Generalized registers let the Section 6 cross-pane workflows
compose:

```
"f          "files I always review together"  → Selection::Files(...)
"e          last-error set                    → Selection::Diagnostics(...)
"c          cherry-pick candidates            → Selection::Commits(...)
```

Verbs that consume selections read from `"f`-style prefixes
(`"f open` opens all files in register `f`, `"e fix` fixes all
diagnostics in register `e`). Verbs that produce selections write
to them.

:::{requirement id="selection-registers" level="MUST"}
The system MUST provide:

- {#c-register-store} a `RegisterStore` `gpui::Global` keyed by
  register name (single char `'a'..='z'` plus a small set of
  named slots) holding the current `Selection` value. Stored per
  active session — switching sessions swaps the active register
  view. The store is the single source of truth; pane crates
  never own register state.

- {#c-write-prefix} a Normal-mode `"<char>` register prefix that,
  when followed by a *selection-producing* verb (yank-equivalent
  in editor; mark-set verbs in fm / git / diagnostics), targets
  the produced selection at the named register. Matches Helix's
  `"a y` shape exactly.

- {#c-read-prefix} a Normal-mode `"<char>` register prefix that,
  when followed by a *selection-consuming* verb, supplies the
  register's value as the action's selection input instead of the
  focused pane's current selection. `"f open` opens all files in
  register `f` regardless of which pane is focused.

- {#c-session-scope} registers are per-session by default.
  Sessions hold a `registers: HashMap<RegisterName, Selection>`
  field; switching sessions swaps the live `RegisterStore` view.
  Closing a session clears its registers; reopening starts empty.

- {#c-named-persistent} `[registers]` section in
  `~/.config/codon/codon.toml` declares persistent named
  registers:

  ```toml
  [registers.review-bundle]
  kind = "Files"

  [registers.cherry-picks]
  kind = "Commits"
  ```

  Named registers survive across sessions and across app
  restarts. Persisted to the global KVP alongside
  `codon_sessions_v1` under `codon_registers_v1`. Single-char
  registers stay per-session.

- {#c-text-register-compat} the existing Helix text registers
  (from `vim::register`) interop with the typed-selection store —
  yanking text into register `a` from the editor writes a
  `Selection::Text { buffer, ranges }` into `RegisterStore`; a
  typed write into the same register from outside the editor
  invalidates the vim-side text-register cache so subsequent
  Helix-text reads return the new value (rendered as buffer
  contents when the register holds non-text, with a clear
  "register f holds 3 files" status message rather than a panic).

- {#c-overview-modal} `codon_session::RegisterOverview` — a modal
  picker listing every occupied register with its name, kind,
  count, and a one-line preview. `j`/`k` navigate, Enter inserts
  a `"<name>` prefix at the focused pane's command line for the
  user to follow with a verb. Esc dismisses.

- {#c-default-register} the unnamed default register (Helix's
  `"`) holds the most-recent selection-producing verb's output.
  Reads with no `"<char>` prefix consume the focused pane's
  current selection, *not* the default register — the default
  register is opt-in via explicit `""` prefix to avoid surprising
  users coming from the existing selection-first flow.
:::

## Out of scope

- Per-window register scope (registers are per-session, not
  per-window — switching windows within a session keeps the
  same registers).
- Register history (only the current value lives in the store;
  no undo / redo of register writes).
- Cross-machine register sharing (registers are local;
  persistence is to local KVP only).
