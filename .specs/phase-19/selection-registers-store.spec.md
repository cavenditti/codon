---
id: TASK:phase-19/selection-registers-store
type: task
status: draft
version: 0.0.1
summary: >
  `RegisterStore` gpui::Global + per-session register map +
  `"<char>` read/write prefix wired through the action
  dispatcher. Helix-text-register interop and named-persistent
  registers are follow-up tasks.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/selection-registers#c-register-store
  - REQ:codon/selection-registers#c-write-prefix
  - REQ:codon/selection-registers#c-read-prefix
  - REQ:codon/selection-registers#c-session-scope
aspects: [register-store, write-prefix, read-prefix, session-scope]
---

# RegisterStore + per-session scope + read/write prefix

## What ships

The minimum surface to make typed-selection registers usable.
Follow-ups (`phase-19/selection-registers-persistent`,
`phase-19/selection-registers-helix-interop`,
`phase-19/selection-registers-overview`) build on this.

1. **`RegisterStore`** as a `gpui::Global` in `codon-session`:

   ```rust
   pub struct RegisterStore {
       active_session: SessionId,
       by_session: HashMap<SessionId, HashMap<RegisterName, Selection>>,
   }
   ```

   API: `write(name, selection)`, `read(name) -> Option<&Selection>`,
   `clear(name)`, `swap_session(id)`. The active-session swap drops
   no data — sessions hold their registers; closing a session clears
   that session's map.

2. **`Session::registers` field** on the session struct. Persisted
   alongside other session state (KVP write), though *contents* of
   per-session registers are not yet serialised in this task —
   `swap_session` rebuilds an empty register map on rehydrate. (The
   named-persistent variant is the follow-up; the per-session
   contents persistence comes with it.)

3. **`"<char>` prefix parsing** in the codon-keymap dispatcher. A
   `"a` prefix followed by a known selection-producing verb routes
   the verb's output into register `a`; followed by a known
   selection-consuming verb, supplies register `a` as the action's
   selection input.

4. **Verb annotation** — extend `Action::accepts` with a sibling
   `produces: Option<ObjectKind>` so the dispatcher knows which
   side of the read/write split a given verb sits on. Most verbs
   are `produces: None`; explicit selection-producing verbs
   (yank-equivalent in editor, `MarkAll` in fm, `SelectByPattern`
   in git, etc.) set it.

5. **Minimal verb coverage** in this task — wire register
   read/write into three verbs as proof:
   - `codon_session::YankSelection` (write the focused pane's
     current selection into the named register).
   - `codon_panes::OpenFromRegister` (consume `Selection::Files`
     from a named register and open each file).
   - `codon_agent::Explain` (already accepts multi-kind selection
     — extend the entry point to read from a register when a
     prefix is present).

## Out of scope

- `[registers]` TOML section + named-persistent across-restart
  storage (separate task).
- Helix text-register interop (vim crate cache invalidation,
  cross-side reads).
- `RegisterOverview` modal picker.
- Default-register (`""`) semantics.

## Verification

- Unit tests in `codon-session/src/registers.rs`: write+read
  round-trip, session swap clears live view but preserves the
  inactive session's map, clear removes the entry.
- Integration test: a fixture with two sessions confirms that
  writing `"f` in session A doesn't leak into session B.
- Smoke: in a real codon window, in the file manager, mark three
  files, `"f` then mark-all → palette `OpenFromRegister "f"` →
  three editor panes open.

## Files touched

- `crates/codon-session/src/`: new `registers.rs` module + field
  on `Session`.
- `crates/codon-keymap/src/`: `"<char>` prefix parsing in the
  dispatcher.
- `crates/codon-pane-bridge/src/`: `Action::produces` annotation.
- `crates/codon-panes/src/`: `OpenFromRegister` action wiring.
- `crates/codon-agent/src/`: register-prefix entry point.
