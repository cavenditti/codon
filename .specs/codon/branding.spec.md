---
id: REQ:codon/branding
type: requirement
status: wontdo
version: 0.0.1
level: MAY
summary: >
  Wontdo placeholder — the Zed→Codon rename was a one-shot patch of
  the app-menu strings and the Settings window titlebar. No further
  branding requirements are planned; this file exists only to keep
  the historical commit 9e2a698 Spec-Ref trailer resolvable.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-14]
---

# Branding (wontdo)

## Why this file exists

Commit `9e2a698` ("chore: rename Zed to Codon in user-facing menus
and Settings window") used a `Spec-Ref:` trailer pointing at
`REQ:codon/branding`. The work was a single-shot rename of menu
strings (`About Codon`, `Hide Codon`, `Quit Codon`) and the
Settings-window titlebar (`"Codon — Settings"`). No further
branding requirements are planned — codon does not have a logo,
splash, marketing surface, or theming policy beyond what Zed
already exposes.

## Resolution

This placeholder satisfies `R013` for the legacy trailer. If
branding ever does grow into a real surface (icon set, splash,
themed prompt sigil), a new REQ with concrete clauses will be
authored at that time rather than retrofitting this one.
