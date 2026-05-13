---
id: REQ:codon/fm-symlinks
type: requirement
status: draft
version: 0.0.1
level: MAY
summary: >
  Create symlink, create hardlink, follow / resolve symlinks from the
  file manager.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-8]
---

# File manager symlink operations

:::{requirement id="fm-symlinks" level="MAY"}
The file manager SHOULD support:

- {#c-make-symlink} `ln` (chord: `l`-then-`n`) creates a symlink in
  `current_dir` pointing at the marked target(s) (or the cursor's
  target if no marks). Link name defaults to the target's basename;
  conflicts use the same numbered-suffix logic as paste.
  Implementation: `fs::Fs::create_link` if available; else add it.
- {#c-make-hardlink} `Ln` (chord: `L`-then-`n`) creates a hardlink
  instead. Surfaces a toast when the source and `current_dir` are
  on different filesystems (hardlinks fail there).
- {#c-follow-symlink} pressing Enter on a symlinked directory follows
  the link. Pressing `F` on any symlinked entry resolves the link
  and reveals the target via `codon_fm::Reveal`. Symlink-loop
  protection: cap traversal depth at 16.
:::
