---
id: REQ:codon/object-grammar
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  The `ObjectGrammar` trait every `SelectionSource` pane also
  implements, exposing `next` / `prev` / `inner_container` /
  `around_container` / `filter_by_predicate` over the pane's
  native object kinds. The UX shell drives Normal-mode keys
  (`w` / `b` / `mi<kind>` / `s` / `K`) through the trait so the
  same alphabet, same muscle memory works in every pane.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-19]
---

# Pane-level object grammar

## Context

Helix's text-Normal-mode movements aren't text-specific in their
shape — they generalise to *selection refinement operators*
parameterised by the pane's object grammar:

| Helix in text         | Generalised refinement                          |
|-----------------------|-------------------------------------------------|
| `w` next word         | next item of same kind                          |
| `b` previous word     | previous item of same kind                      |
| `mip` inner paragraph | inner container of given kind                   |
| `s` select-regex      | predicate filter over the current selection     |
| `K` keep matching     | intersect current selection with a predicate    |
| `%` whole-buffer      | select all of the pane's primary objects        |
| `n` / `N` repeat      | next / prev predicate match                     |

The same alphabet learned once, working across seven panes. This
REQ defines the trait the UX shell calls into and the per-pane
impls that make it real.

:::{requirement id="object-grammar" level="MUST"}
The system MUST provide:

- {#c-grammar-trait} an `ObjectGrammar` trait in
  `codon-pane-bridge`:

  ```rust
  pub trait ObjectGrammar {
      fn next(&self, kind: ObjectKind, from: &Selection) -> Selection;
      fn prev(&self, kind: ObjectKind, from: &Selection) -> Selection;
      fn inner_container(&self, of: ObjectKind, from: &Selection) -> Selection;
      fn around_container(&self, of: ObjectKind, from: &Selection) -> Selection;
      fn filter_by_predicate(&self, sel: &Selection, p: &Predicate) -> Selection;
      fn intersect_with_predicate(&self, sel: &Selection, p: &Predicate) -> Selection;
      fn select_all(&self, kind: ObjectKind) -> Selection;
  }
  ```

  Each pane kind that implements `SelectionSource` also
  implements `ObjectGrammar` for the object kinds it owns. Pane
  kinds that don't own a given `ObjectKind` return
  `Selection::Empty` from the corresponding call (no-op rather
  than panic).

- {#c-predicate-lang} a small `Predicate` mini-language used by
  `s` and `K`:

  ```
  '<glob>'                       — name / path glob (fm, git files)
  <field>=<value>                — equality on a typed field
                                  (severity=error, kind=function,
                                   author=carlo)
  <field>~<regex>                — regex match on a typed field
  '<text>'                       — substring match (block output,
                                  message body, diagnostic message)
  <p1> and <p2>                  — conjunction
  <p1> or  <p2>                  — disjunction
  not <p>                        — negation
  ```

  Pane impls declare which fields exist on their grammar via
  `Predicate::supported_fields(&self, ObjectKind) -> &[&str]`;
  unsupported fields produce a clear error message from the `:s`
  / `:K` command-line entry, not a silent empty result.

- {#c-shell-driven-keys} the UX shell routes Normal-mode keys
  through the focused pane's `ObjectGrammar` impl:

  - `w` / `b` → `next` / `prev` with `kind` = the focused pane's
    *primary* object kind (declared on `SelectionSource`).
  - `mi<key>` / `ma<key>` → `inner_container` / `around_container`
    with `kind` selected by the `<key>` (e.g. `mip` paragraph in
    editor, mid file in fm, mih hunk in git).
  - `s '<pat>'` (command-line entry) →
    `filter_by_predicate`.
  - `K '<pat>'` (command-line entry) →
    `intersect_with_predicate`.
  - `%<key>` → `select_all` with the given kind.

  Keys are bound in the embedded TOML defaults; per-pane-kind
  bindings (`[bindings.git.normal]`, etc.) can override.

- {#c-pane-grammar-editor} the editor pane's `ObjectGrammar`
  delegates to the existing vim+Helix code path for text /
  word / paragraph / function / class / bracket-pair —
  `mi(` / `mip` / etc. already work and the trait impl is a thin
  bridge. Predicate fields: `kind`, `text`, `regex`.

- {#c-pane-grammar-fm} the file manager pane: next/prev file in
  the current dir, inner directory (selects all files in the
  containing dir), around directory (selects all files in the
  containing dir *and* its name as a directory entry), filter by
  glob / kind (file/dir/symlink) / git-status / size.

- {#c-pane-grammar-git} the git pane: next/prev hunk, inner
  file (all hunks in the same file as cursor hunk), around file
  (hunks + file header), filter by author / path / commit-sha
  prefix.

- {#c-pane-grammar-diagnostics} the diagnostics pane: next/prev
  diagnostic, inner file (all diagnostics in the same file),
  inner severity-bucket (all errors / warnings / info), filter by
  code / severity / source / message.

- {#c-pane-grammar-agent} the agent pane: next/prev message,
  inner thread (all messages in the same thread / branch), filter
  by role (user/assistant/tool) / contains-text.

- {#c-pane-grammar-terminal} the terminal pane (gated on
  [REQ:codon/terminal-blocks](spec:REQ:codon/terminal-blocks)):
  next/prev block, inner block, around block (block + prompt
  line), filter by exit-status / command-contains.
:::

## Out of scope

- Cross-pane refinement (`w` jumping from editor word to fm file
  when they happen to be adjacent on-screen). The grammar is
  per-pane; cross-pane motion stays in `codon-jump`.
- Custom user-defined object kinds. The kind set is the one
  declared in `selection-first.spec.md`; extending it is a
  separate REQ.
