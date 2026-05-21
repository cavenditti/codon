---
id: TASK:phase-22/fish-plugin-bootstrap
type: task
status: accepted
version: 0.1.0
summary: >
  Ship the fish plugin file (`codon.fish`) and the `codon fish-init`
  CLI subcommand that installs it idempotently to
  `~/.config/fish/conf.d/`. First-run toast in codon when a fish
  terminal opens without the plugin present.
owners: [carlo]
progress: pending
refines:
  - REQ:codon/fish-shell-integration#c-plugin-distribution
aspects: [plugin-file, installer-cli]
blocked_by:
  - TASK:phase-22/fish-rpc-socket
---

# Fish plugin file + installer

## Plan

- The plugin source lives at
  `crates/codon-fish/share/codon.fish`. Single file, no
  dependencies on plugin managers like fisher (we don't want to
  require one).
- The plugin's responsibilities:
  - Source-time: if `set -q CODON_SOCK`, set up the bindings
    (`Ctrl-G` for `#@`), define functions (`codon do`,
    `codon edit`, …), wire tab-completion.
  - If `CODON_SOCK` is unset, source-time is a near-no-op — the
    function definitions still get created (so `codon do` can
    print a helpful error), but no key bindings are installed.
- Helper RPC functions inside the plugin:
  - `__codon_rpc <method> <params-json>` — opens a UnixStream
    to `$CODON_SOCK`, writes one JSON-line, reads one
    JSON-line, closes. Implemented with `python3 -c` if fish
    can't manage Unix sockets natively without external tools;
    a future iteration may bundle a tiny `codon-rpc` binary
    instead of the python shim.
  - Note for the implementer: fish 4.0+ has improved
    networking, but a `socat` or `nc -U` fallback is the
    portable path. Pick the cheapest available.
- Installer subcommand:
  - `codon fish-init` writes
    `~/.config/fish/conf.d/codon.fish` from the bundled file.
    Asset embedded at build time via `include_str!`.
  - Idempotency: store a checksum of the last-written content
    at `~/.config/codon/.fish-plugin.sha256`. On re-run, if the
    target file's checksum matches the stored value, overwrite
    silently; if it doesn't match, prompt
    "codon.fish was modified locally; overwrite? [y/N]".
  - `codon fish-init --uninstall` removes the plugin file +
    checksum.
  - `codon fish-init --print` writes the plugin to stdout so a
    user can sourcegraph or pipe it elsewhere.
- First-run toast:
  - When codon spawns a fish PTY, send an `agent.complete`-
    shaped probe (a `__codon_plugin_probe` RPC that the plugin
    answers `{ installed: true, version: ... }`).
  - No response within 200 ms after the OSC 133 "shell ready"
    signal → display a toast: "fish plugin not installed; run
    `codon fish-init` from any terminal".
  - Only fires once per workspace open per shell binary
    (deduped by PTY pid).

## Acceptance

- `codon fish-init` writes the plugin file with `0644` mode;
  re-runs are silent when checksum matches.
- `codon fish-init --uninstall` removes the file.
- Opening a fresh fish terminal with the plugin present
  triggers the probe and answers cleanly (no toast).
- Opening fish without the plugin shows the toast exactly
  once per shell binary per session.
- A bash terminal (no plugin support yet) does NOT show the
  toast (it's fish-specific).
- `cargo test -p codon-fish` passes.
