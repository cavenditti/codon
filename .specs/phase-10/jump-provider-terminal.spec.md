---
id: TASK:phase-10/jump-provider-terminal
type: task
status: accepted
version: 0.0.1
summary: >
  Terminal provider — yields visible-grid words and URLs (via
  the existing `terminal_hyperlinks::URL_REGEX` `RegexSearch`)
  as JumpCandidate entries with focus + alacritty-select / copy
  actions.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/jump-hints#c-pane-terminal
aspects: [terminal-word-provider, terminal-url-provider]
---

# Terminal jump provider

## What ships

New `TerminalJumpProvider` in
`vendor/zed/crates/terminal_view/src/codon_jump_provider.rs`.
Registered in `TerminalView::new`.

Implementation walks the alacritty visible grid:

```rust
impl JumpProvider for TerminalJumpProvider {
    fn collect(&self, ctx: &JumpContext, cx: &mut App) -> Vec<JumpCandidate> {
        let terminal = self.terminal.upgrade()?;
        terminal.read_with(cx, |term, cx| {
            let alacritty = term.last_content();
            let viewport = ...;  // visible cell rect in pixels
            let mut out = vec![];
            // Words: tokens of >= 2 word-chars between whitespace,
            // matching the helix word definition.
            for token in walk_visible_word_tokens(&alacritty) {
                out.push(JumpCandidate {
                    bounds: cell_bounds(token.start, &viewport),
                    kind: JumpKind::Word,
                    action: Box::new(|window, cx| {
                        terminal.update(cx, |t, cx| {
                            t.focus_handle(cx).focus(window);
                            t.select_word_at_cell(token.start, cx);
                        });
                    }),
                });
            }
            // URLs: reuse RegexSearch the hover system already uses.
            let hyperlinks = &term.hyperlinks;
            for hit in hyperlinks.find_all_in_viewport(&alacritty) {
                out.push(JumpCandidate {
                    bounds: cell_bounds(hit.start, &viewport),
                    kind: JumpKind::Url(hit.url.clone()),
                    action: Box::new(move |w, cx| {
                        terminal.update(cx, |t, _| t.focus_handle(cx).focus(w));
                    }),
                });
            }
            out
        })
    }
}
```

Three small helpers added to `vendor/zed/crates/terminal/`:

- `pub fn walk_visible_word_tokens(content: &Content) -> impl Iterator<...>`.
- `pub fn select_word_at_cell(&mut self, cell: Point, cx)`.
- `TerminalHyperlinks::find_all_in_viewport(content) -> Vec<UrlHit>`.

The third refactors the existing point-query
`hover_at_cell`-style API into a viewport scan.

## Verification

- Spawn a terminal, run `ls`; `cmd-k j`: every word in `ls`
  output is hinted. Two-key selection focuses the terminal and
  alacritty-selects the word.
- `curl https://example.com` history line; `cmd-k u`: the URL
  is hinted, two-key copies it.
- Scrollback off-screen: only visible lines hint, not
  scrolled-out lines.

## Where it slots in

- New: `vendor/zed/crates/terminal_view/src/codon_jump_provider.rs`.
- Edit: `vendor/zed/crates/terminal/src/terminal.rs` +
  `terminal_hyperlinks.rs` — small pub-export refactors.
- Edit: `vendor/zed/crates/terminal_view/src/terminal_view.rs` —
  `TerminalJumpProvider::register(self, cx)` in `TerminalView::new`.
- Vendor/zed submodule bump in the outer commit.
