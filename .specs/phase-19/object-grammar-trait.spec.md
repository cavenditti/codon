---
id: TASK:phase-19/object-grammar-trait
type: task
status: draft
version: 0.0.1
summary: >
  Define the `ObjectGrammar` trait in `codon-pane-bridge`, wire
  the UX shell to route `w` / `b` / `mi…` / `%<kind>` through
  it, and ship one full pane implementation (file manager) as
  proof. Other pane impls + the predicate mini-language follow
  in separate tasks.
owners: [carlo]
progress: done
refines:
  - REQ:codon/object-grammar#c-grammar-trait
  - REQ:codon/object-grammar#c-shell-driven-keys
  - REQ:codon/object-grammar#c-pane-grammar-fm
aspects: [grammar-trait, shell-key-dispatch, fm-impl]
---

# ObjectGrammar trait + UX-shell wiring + fm impl

## What ships

The trait definition, the dispatcher wiring, and one pane impl
end-to-end. Follow-up tasks
(`phase-19/object-grammar-predicate-lang`,
`phase-19/object-grammar-editor`,
`phase-19/object-grammar-git`,
`phase-19/object-grammar-diagnostics`,
`phase-19/object-grammar-agent`,
`phase-19/object-grammar-terminal`) fill out the rest.

1. **`ObjectGrammar` trait** in `codon-pane-bridge`:

   ```rust
   pub trait ObjectGrammar {
       fn next(&self, kind: ObjectKind, from: &Selection) -> Selection;
       fn prev(&self, kind: ObjectKind, from: &Selection) -> Selection;
       fn inner_container(&self, of: ObjectKind, from: &Selection) -> Selection;
       fn around_container(&self, of: ObjectKind, from: &Selection) -> Selection;
       fn select_all(&self, kind: ObjectKind) -> Selection;
       // filter / intersect deferred to predicate-lang task
   }
   ```

   Default impls on the trait return `Selection::Empty` so panes
   only implement the kinds they own.

2. **UX-shell key dispatch** in `codon-keymap`. Normal-mode `w`
   / `b` / `mi<key>` / `ma<key>` / `%<key>` look up the focused
   pane's `ObjectGrammar` impl via a new trait object reference
   stored alongside `SelectionSource` on the pane.

3. **`SelectionSource::primary_object_kind()`** — new method
   declaring the pane's "natural" object kind so `w` / `b` /
   `%` know what to motion over without an explicit key suffix.
   File manager = `ObjectKind::File`, git = `ObjectKind::Hunk`,
   diagnostics = `ObjectKind::Diagnostic`, etc.

4. **File-manager `ObjectGrammar` impl** in `file-manager`:
   - `next(File, ...)` / `prev(File, ...)` — move cursor to
     next / previous file row in the current directory.
   - `inner_container(File, ...)` — select all files in the
     containing directory.
   - `around_container(File, ...)` — select all files in the
     containing directory plus its name as an entry.
   - `select_all(File)` — select every visible row.
   - Other kinds return `Selection::Empty`.

5. **Bindings** in the embedded TOML defaults under
   `[bindings.file_manager.normal]`:

   ```toml
   "w"      = "codon_panes::ObjectNext"
   "b"      = "codon_panes::ObjectPrev"
   "mip"    = "codon_panes::InnerContainer(file)"
   "map"    = "codon_panes::AroundContainer(file)"
   "%f"     = "codon_panes::SelectAll(file)"
   ```

   (Action names provisional; reconcile with naming convention
   before merge.)

## Out of scope

- The predicate mini-language (`s '<pat>'` / `K '<pat>'`).
- Editor / git / diagnostics / agent / terminal pane impls.
- Cross-pane motion (codon-jump already covers).

## Verification

- Unit tests in `file-manager`: a fixture directory with N rows
  + given cursor → `next(File, sel)` returns Selection at row
  cursor+1; wraps at end (or saturates — pick one and document).
- Integration test: focus fm, press `w` → cursor advances; press
  `mip` → all rows in current dir selected; press `%f` → all
  visible rows selected.

## Files touched

- `crates/codon-pane-bridge/src/`: trait definition +
  `SelectionSource::primary_object_kind`.
- `crates/codon-keymap/src/`: dispatcher wiring for `w` / `b` /
  `mi…` / `ma…` / `%…`.
- `crates/file-manager/src/`: `ObjectGrammar` impl.
- `crates/codon-keymap/src/keymap.rs`: embedded TOML defaults.
