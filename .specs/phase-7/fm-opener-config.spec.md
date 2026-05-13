---
id: TASK:phase-7/fm-opener-config
type: task
status: accepted
version: 0.0.1
summary: >
  `~/.config/codon/openers.toml` — declarative opener config loaded at
  startup and on FS-watcher notifications.
owners: [carlo]
progress: in-progress
refines:
  - REQ:codon/fm-openers#c-opener-config
---

# File-manager openers config

## What ships

A new config file `~/.config/codon/openers.toml`:

```toml
[[opener]]
glob = "*.{png,jpg,jpeg,gif,webp}"
cmd  = "qlmanage -p {path}"
block = false
description = "Quick Look"

[[opener]]
mime = "application/pdf"
cmd  = "open -a Preview {path}"
description = "Preview.app"
```

Each opener has: `glob` OR `mime` (one required), `cmd` (with
substitutions — see REQ:codon/fm-shell-exec#c-shell-substitutions),
`block: bool` (default false), `description: String`.

Loaded at startup; re-read on watcher events via the same FS
watcher codon-config uses. Writeback via the existing `toml_edit`
AST flow so in-app changes preserve user comments + formatting.

## Where it slots in

- New crate-level module or file under `crates/file-manager/src/`
  (e.g. `openers.rs`). ~200 LOC.
- Wire `OpenerStore` as a global via the same pattern
  `SessionRegistry` uses.
