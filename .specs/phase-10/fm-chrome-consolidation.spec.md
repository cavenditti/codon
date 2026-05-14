---
id: TASK:phase-10/fm-chrome-consolidation
type: task
status: accepted
version: 0.0.1
summary: >
  Consolidate the file manager's four chrome bars into two — a top
  bar (path + chips) and a bottom bar (info / hints / shortcuts) —
  with contextual + Cmd-modifier overlays replacing the always-on
  help row.
owners: [carlo]
progress: done
refines:
  - REQ:codon/fm-chrome#c-top-bar
  - REQ:codon/fm-chrome#c-bottom-bar
  - REQ:codon/fm-chrome#c-contextual-hints
  - REQ:codon/fm-chrome#c-cmd-shortcuts
  - REQ:codon/fm-chrome#c-precedence
  - REQ:codon/fm-chrome#c-no-status-bar
  - REQ:codon/fm-chrome#c-no-help-toggle
aspects: [fm-chrome-top, fm-chrome-bottom, fm-chrome-hint-overlay]
---

# File manager chrome consolidation

## What ships

`crates/file-manager/src/view.rs` is restructured so the panel's
top-level `v_flex` has exactly two non-modal chrome children: the
top bar and the bottom bar. The center body (three columns) sits
between them.

**Top bar (`render_top_bar`)** — replaces the existing
`render_header_chips`:

```
[ /Users/carlo/Devel/personal/codon_v3      ] [ sort filter find . ]
```

- Left child: `Label::new(dir_display)`, `min_w_0` + `single_line`
  so a long path truncates rather than pushing the chips off-screen.
- Right child: the existing chip list (sort / filter / find /
  hidden), unchanged.

**Bottom bar (`render_bottom_bar`)** — extends the existing rich-info
bar with two overlay modes for the *left segment*. The right segment
always shows the listing totals **and the `position/total`
counter** that used to live in the removed status row.

The left segment resolves via this precedence:

1. `LeftSegment::ContextualHints(hints)` — when
   `contextual_help_hints(self)` returns a non-empty hint set (the
   existing helper, kept verbatim).
2. `LeftSegment::CmdShortcuts(rows)` — when `self.cmd_only_held` is
   `true` (Cmd is held and no other modifier is).
3. `LeftSegment::Info(segments)` — the default: the existing
   perms / owner / size / mtime / name segments produced from
   `RichInfoBarState`.

The view emits exactly one of the three; the bar's shell (padding,
border, bg) is identical across modes so toggling doesn't reflow.

**Cmd-tracking field on `FileManager`**:

```rust
pub(crate) cmd_only_held: bool,
```

Wired in `FileManager::new` via `window.on_modifiers_changed(...)`
on the focus handle: every modifier change updates the flag to
`m.modifiers.command && !m.modifiers.shift && !m.modifiers.alt &&
!m.modifiers.control && !m.modifiers.function`, then `cx.notify()`.

**Removals**

- The standalone status bar block in `Render::render` (currently
  ~lines 664-675, the `h_flex` with `Label::new(status_text)`).
- `render_help_bar` + the `show_help_bar` branch + the call site.
- `show_help_bar` / `show_rich_info` fields on `FmPrefs`, the
  `set_show_*` setters, and the `toggle_rich_info` / `toggle_help_bar`
  actions + `ToggleRichInfo` / `ToggleHelpBar` action types. Their
  keymap entries (if any) are dropped from
  `crates/codon-keymap/src/keymap.rs` and the example keymap.
- The `status_text` local in `Render::render` and its `dir | position
  | marks` join — the data moves to the two bars individually.

## Verification

- Top bar shows `/Users/...` on the left and the sort chip on the
  right with no other chrome between them.
- Bottom bar reads `<perms> <owner> <size> <mtime> <name>` on the
  left and `N entries (size) — i/N` on the right.
- Open the filter prompt (`/`): bottom-bar left flips to the
  contextual hints for filter mode.
- Hold Cmd alone: bottom-bar left flips to the general-shortcut
  row; release Cmd → back to info.
- Open the filter prompt AND hold Cmd: contextual hints win (per
  `#c-precedence`).
- Long path: the dir label truncates without pushing the chips
  off-screen.

## Where it slots in

- Edit: `crates/file-manager/src/view.rs` — collapse to two
  bars; new `LeftSegment` enum + render helpers; tear out
  `render_help_bar` + status block.
- Edit: `crates/file-manager/src/file_manager.rs` — add
  `cmd_only_held` field; register `on_modifiers_changed`; drop
  the toggle actions.
- Edit: `crates/file-manager/src/prefs.rs` — remove the two
  pref flags + setters + their tests.
- Edit: `crates/codon-keymap/src/keymap.rs` and
  `assets/config/keymap.example.toml` — drop any
  `ToggleRichInfo` / `ToggleHelpBar` bindings.
- No vendored-Zed changes; `Window::on_modifiers_changed` is
  already public.

## Out of scope

- A configurable Cmd-shortcuts surface (the row is a fixed,
  hard-coded list of the most common bindings). Future work can
  build it from the active keymap if motivated.
- Persisting "user pinned the contextual hint row on" — the user
  asked specifically for on-demand, no toggle.
