---
id: REQ:codon/packaging
type: requirement
status: accepted
version: 0.1.0
level: MUST
summary: >
  Codon installs as a proper macOS app bundle from a single repeatable
  script that builds, bundles, and updates /Applications/Codon.app.
owners: [carlo]
refines: []
categorized_under: []
---

# Packaging

## Context

Codon is used as a daily-driver app on macOS, not just via `cargo run`.
"Do a new build and update the release" must be one command that turns
the current checkout into an installed, Finder-visible `Codon.app` with
its own identity (name, icon, bundle id) — distinct from any real Zed
install so the two never fight over LaunchServices registrations or
single-instance detection. Bundling must not depend on Zed's patched
`cargo-bundle` fork; stock macOS tooling only.

:::{requirement id="packaging" level="MUST"}
The system MUST provide macOS packaging with:

- {#c-mac-bundle} a repeatable script `scripts/install-mac-app` that
  assembles a self-contained `Codon.app` (release `codon` binary,
  static `Info.plist` template, codon icon, document icon) using only
  stock macOS tools (`plutil`, `codesign`, `ditto`), and ad-hoc signs
  the result
- {#c-mac-install-update} install-and-update semantics: the script
  installs into `/Applications` (falling back to `~/Applications` when
  `/Applications` is not writable), and on re-run replaces the existing
  install by staging the new bundle first and swapping it in — never
  mutating the installed bundle in place — warning when Codon is
  currently running
- {#c-mac-identity} stable app identity: bundle identifier
  `dev.codon.Codon`, `CFBundleShortVersionString` taken from the
  `codon` package version, and `CFBundleVersion` plus a `CodonGitSha`
  key stamped at bundle time so an installed build is traceable to a
  commit
- {#c-mac-icon} a codon-branded icon: `scripts/gen-mac-icon` renders
  `assets/codon-logo.svg` into a macOS-style `.icns`, checked in at
  `assets/mac/Codon.icns` so bundling itself needs no SVG toolchain
:::

## Implementation

`scripts/install-mac-app` owns the whole flow (build → stage under
`target/mac-bundle/` → stamp → sign → swap into place). The
`Info.plist` template lives at `assets/mac/Info.plist` with
`__CODON_*__` placeholder tokens. Document types and permission
strings are adapted from `vendor/zed/crates/zed/resources/info/`.
