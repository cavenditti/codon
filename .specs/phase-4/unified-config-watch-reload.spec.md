---
id: TASK:phase-4/unified-config-watch-reload
type: task
status: accepted
version: 0.0.1
summary: >
  Watch ~/.config/codon/codon.toml for changes and reload both
  settings and bindings without restarting codon.
owners: [carlo]
progress: done
refines:
  - REQ:codon/unified-config#c-watch-reload
---

# Hot-reload for the unified config

## What ships

Two existing reload paths get pointed at the new file:

- **Settings:** `SettingsStore::watch_settings_files` watches a list
  of paths. Add `codon.toml` to that list (codon-config registers it
  during init). On change, re-translate TOML→JSON and call the same
  `set_user_settings` hook the initial load uses.
- **Bindings:** codon-keymap already has a `load_codon_keymap` that
  runs from `reload_keymaps`. Trigger `reload_keymaps` when the
  filesystem watcher fires on `codon.toml`.

Debounce to ~200 ms (matching Zed's existing settings debounce) so a
single editor save doesn't trigger two reloads in flight.

## Reference

- [`vendor/zed/crates/settings/src/settings_store.rs::watch_settings_files`](spec:src:vendor/zed/crates/settings/src/settings_store.rs)
- The existing `reload_keymaps` action in vendored Zed.

Small. ~80 LOC of glue. Depends on
[`TASK:phase-4/unified-config-config-crate`](spec:TASK:phase-4/unified-config-config-crate)
landing first.
