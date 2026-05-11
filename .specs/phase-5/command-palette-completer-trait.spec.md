---
id: TASK:phase-5/command-palette-completer-trait
type: task
status: accepted
version: 0.0.1
summary: >
  Define the Completer trait + registry in codon-command-palette.
  Maps an action's registered name to a producer of (value, label)
  pairs filtered against the user's argument query.
owners: [carlo]
progress: done
refines:
  - REQ:codon/command-palette#c-completer-trait
---

# `Completer` trait + registry

## What ships

A new module `crates/codon-command-palette/src/completer.rs`:

```rust
pub struct CompletionItem {
    pub value: String,       // what the action receives
    pub label: SharedString, // what the user sees in the sub-picker
    pub detail: Option<SharedString>, // optional second line / path hint
}

pub trait Completer: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn placeholder(&self) -> &'static str;
    fn complete(
        &self,
        query: &str,
        cx: &mut App,
    ) -> Task<Result<Vec<CompletionItem>>>;
    fn build_action(&self, value: &str) -> Box<dyn Action>;
}

pub struct CompleterRegistry { ... }
impl CompleterRegistry {
    pub fn register(&mut self, action_name: &'static str, c: Arc<dyn Completer>);
    pub fn for_action(&self, action_name: &str) -> Option<Arc<dyn Completer>>;
}
```

Behaviour:

- Registry is process-wide, accessed via a `Global` on `App`.
- `for_action` keys on the qualified name returned by
  `Action::action_name()` (so `workspace::Open`,
  `theme_selector::Toggle`, etc.).
- `build_action(value)` is the only place we synthesise the typed
  action; downstream completers implement it however the action
  requires (e.g. `Open { paths: vec![PathBuf::from(value)] }`).

## Reference points

- [`vendor/zed/crates/gpui/src/action.rs`](spec:src:vendor/zed/crates/gpui/src/action.rs)
  — `Action::action_name`, `Action::build`. We mostly avoid `build`
  (raw JSON) and instead let each `Completer` construct the action
  directly.

## Tests

- Unit: registry round-trip — register a stub completer, look it up
  by action name.
- Unit: a stub `Completer` whose `complete` is synchronous; assert
  filter behaviour.

Effort: low. ~120 LOC including tests.
