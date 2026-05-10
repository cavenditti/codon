---
id: REQ:codon/in-app-pickers
type: requirement
status: draft
version: 0.0.1
level: MUST
summary: >
  Replace OS-native dialogs (cx.prompt_for_paths) with an in-app dir
  picker built on the picker::Picker trait.
owners: [carlo]
refines: [REQ:codon/pane-ux#c-no-native-dialogs]
categorized_under: [TOPIC:topics/phase-5]
---

# In-app dir/file pickers

## Context

Phase 2 audited every native-dialog callsite in vendored Zed. Five
remain reachable from codon flows. Replacing them needs:

1. A reusable `DirPicker` `PickerDelegate` that lists directories
   from a starting path with type-to-filter
2. Re-routing each callsite to the in-app picker

:::{requirement id="in-app-pickers" level="MUST"}
The system SHOULD provide:

- {#c-picker-delegate} a `DirPicker` `PickerDelegate` reusable across
  callsites
- {#c-rewire-workspace} `workspace.rs:2972` (open-file dialog)
- {#c-rewire-project-panel} `project_panel:3301` (file destination)
- {#c-rewire-clone} `git_ui::clone:17`
- {#c-rewire-archive} `agent_ui::threads_archive_view:1237`
- {#c-rewire-attach} `agent_ui::message_editor:1423`
:::
