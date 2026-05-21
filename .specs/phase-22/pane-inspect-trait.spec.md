---
id: TASK:phase-22/pane-inspect-trait
type: task
status: accepted
version: 0.1.0
summary: >
  Add the `PaneInspect` trait to `codon-pane-bridge` and implement it
  for every pane kind that publishes a `PaneModeBridge` impl today.
  Read-only methods: `kind_label`, `summary`, `read_visible`,
  `read_scrollback`, `search`. Default no-op for kinds without content.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/agent-pane-tools#c-pane-inspect-trait
  - REQ:codon/agent-pane-tools#c-read-only
  - REQ:codon/agent-pane-tools#c-workspace-scope
aspects: [trait-shape, read-only, workspace-scope]
---

# PaneInspect trait + per-kind impls

## Plan

- Add a new module
  `crates/codon-pane-bridge/src/pane_inspect.rs` with:
  - `pub struct PaneSlice { bytes: String, byte_offset: usize,
    truncated: bool, next_offset: Option<usize> }`
  - `pub struct SearchHit { line: u32, col: u32, snippet: String }`
  - `pub struct PaneSummary { kind: PaneKind, label: String,
    cwd_or_path: Option<PathBuf>, content_lines: u64 }`
  - `pub trait PaneInspect { fn kind_label(&self) -> &'static str;
    fn summary(&self, cx: &App) -> PaneSummary; fn read_visible(...)
    -> PaneSlice; fn read_scrollback(...) -> PaneSlice; fn
    search(...) -> Vec<SearchHit>; }` — default impls return an
    empty slice / empty Vec so kinds without content can opt out
    cheaply.
- Per-kind impls (each in the owning crate, mirroring the bridge
  pattern):
  - **Terminal** in
    [crates/file-manager/src/shell.rs](spec:src:crates/file-manager/src/shell.rs)
    neighbour — actually new file
    `vendor/zed/crates/terminal_view/src/pane_inspect.rs`. Use
    alacritty's grid + scrollback.
  - **Editor** in
    `vendor/zed/crates/editor/src/codon_pane_inspect.rs`. Read
    visible region from `EditorSnapshot::display_text`; scrollback
    = full multibuffer text. Search uses
    `editor::search::SearchQuery::regex_or_text`.
  - **FileManager** in `crates/file-manager/src/pane_inspect.rs`.
    Visible = currently rendered rows formatted as a tree-style
    listing; scrollback = full directory listing; search = substring
    over entry names.
  - **Agent / Outline / Git / Debug / Peek** — no-op default for
    phase 22 (no scrollback to expose). Their `summary` still returns
    a meaningful label.
- Workspace-scope guard: a separate `PaneInspectRegistry` keyed on
  the entity's window + pane id; cross-window lookups return `None`.

## Acceptance

- Unit tests per impl assert non-empty `summary` and correct
  `truncated` semantics at the byte budget boundary.
- `PaneInspect::search` over a terminal scrollback returns the
  expected hits for a literal string injected via the test PTY.
- `cargo test -p codon-pane-bridge` passes; vendored Zed clippy
  clean.
