---
id: REQ:codon/pane-ux
type: requirement
status: accepted
version: 0.1.0
level: MUST
summary: >
  Pane focus, move, and split keyboard ergonomics; default split is a
  terminal; never use OS-native dialogs from codon flows.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-2]
---

# Pane UX

## Context

Codon is keyboard-first. Pane navigation should feel like vim's
window-motion (`<C-w>hjkl`) without using `<C-w>` (reserved for terminal
input). New splits default to a terminal, and any path/file picker the
user encounters should be in-app, not a platform-native dialog.

:::{requirement id="pane-ux" level="MUST"}
The system MUST provide:

- {#c-focus} `ctrl-{h,j,k,l}` bound to
  `workspace::ActivatePane{Left,Down,Up,Right}`
- {#c-swap} `ctrl-shift-{h,j,k,l}` bound to
  `workspace::SwapPane{Left,Down,Up,Right}`
- {#c-default-split} new splits default to a terminal pane when no
  active item is suitable to clone
- {#c-no-native-dialogs} no codon-specific flow opens an OS-native
  file/save dialog. Audit results: 5 vendored Zed callsites still use
  `cx.prompt_for_paths` (`workspace.rs:2972`, `project_panel:3301`,
  `git_ui::clone:17`, `agent_ui::threads_archive_view:1237`,
  `agent_ui::message_editor:1423`); replacement requires a new in-app
  dir picker and is tracked under
  [REQ:codon/in-app-pickers](spec:REQ:codon/in-app-pickers)
- {#c-safe-close} closing the last tab MUST NOT close the OS window.
  The default close action falls back to (a) closing the pane if other
  panes exist, (b) closing the codon-session window if other windows
  exist, or (c) replacing the center with an empty pane. The OS window
  only closes via the explicit `cmd-shift-w` chord or `cmd-q`.
:::
