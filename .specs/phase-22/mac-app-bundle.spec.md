---
id: TASK:phase-22/mac-app-bundle
type: task
status: accepted
version: 0.1.0
summary: >
  Install script that builds the release binary, assembles Codon.app
  with codon icon + stamped Info.plist, and installs/updates it in
  /Applications with stock macOS tooling only.
owners: [carlo]
progress: done
refines:
  - REQ:codon/packaging#c-mac-bundle
  - REQ:codon/packaging#c-mac-install-update
  - REQ:codon/packaging#c-mac-identity
  - REQ:codon/packaging#c-mac-icon
aspects: [mac-bundle, mac-install-update, mac-identity, mac-icon]
assignee:
eta:
blocked_by: []
---

# Mac app bundle + install script

## Plan

- `scripts/gen-mac-icon` — one-time / on-logo-change generator:
  render `assets/codon-logo.svg` with inkscape, recolor the glyph
  light, composite onto a dark rounded-rect tile (Big Sur geometry:
  824 px tile on a 1024 px canvas, ~185 px corner radius), emit the
  full iconset via `sips`, pack with `iconutil` into
  `assets/mac/Codon.icns` (checked in).
- `assets/mac/Info.plist` — static template with `__CODON_VERSION__`,
  `__CODON_BUILD__`, `__CODON_GIT_SHA__` tokens. Identity per
  `#c-mac-identity`; permission strings and document types adapted
  from Zed's `resources/info/` fragments (Codon-worded); URL scheme
  `codon`; `LSMinimumSystemVersion` 10.15.7; developer-tools category.
- `scripts/install-mac-app` — the repeatable entry point:
  1. `cargo build --release -p codon` (skippable with `-n`)
  2. stage `target/mac-bundle/Codon.app` (binary → `Contents/MacOS/`,
     icns + `Document.icns` from Zed resources → `Contents/Resources/`,
     stamped plist → `Contents/Info.plist`), `plutil -lint` the result
  3. `codesign --force --sign -` (ad-hoc; binary keeps its symbol
     table so the in-app hang traces stay symbolicated — no strip)
  4. swap into `/Applications` (or `~/Applications` fallback): move
     the old bundle aside, `ditto` the staged one in, delete the old;
     warn via `pgrep` when codon is running; `lsregister -f` at the end
- Document the command in CLAUDE.md's Commands block.

## Acceptance

- Running `scripts/install-mac-app` from a clean tree yields
  `/Applications/Codon.app` that launches via `open -a Codon`, shows
  "Codon" + the codon icon in the Dock, and `codesign --verify`
  passes on it.
- Re-running after new commits replaces the install; the installed
  `Info.plist` shows the new `CFBundleVersion` / `CodonGitSha`.
- `scripts/gen-mac-icon` regenerates `assets/mac/Codon.icns` from the
  SVG logo without manual steps.
- `spec lint` stays at 0 errors.
