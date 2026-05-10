---
id: REQ:codon/selection-first
type: requirement
status: accepted
version: 1.0.0
level: MUST
summary: >
  Selection-first foundation: typed Selection enum, SelectionSource
  trait per pane kind, and an ActionAcceptsRegistry the command
  palette filters by.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-1]
---

# Selection-first action layer

## Context

Every pane has a notion of "what's currently selected": text ranges in
an editor, marked paths in the file manager, hunks in the git pane, etc.
Verbs (actions) declare which `ObjectKind`s they accept. The command
palette consults the registry and only shows applicable actions.

:::{requirement id="selection-first" level="MUST"}
The system MUST provide:

- {#c-object-kind} an `ObjectKind` enum covering Text / File / Dir /
  Hunk / Commit / Branch / Block / Url / Diagnostic / Message
- {#c-selection-enum} a `Selection` enum carrying typed payloads per
  ObjectKind
- {#c-source-trait} a `SelectionSource` trait every pane kind
  implements
- {#c-accepts-registry} an `ActionAcceptsRegistry` that the command
  palette consults to filter actions by current selection kind
:::
