---
id: REQ:codon/fm-shell-exec
type: requirement
status: draft
version: 0.0.1
level: SHOULD
summary: >
  Shell command execution from the file manager — `!` blocking,
  `;` async, both with path / cwd substitutions and stderr toast.
owners: [carlo]
categorized_under: [TOPIC:topics/phase-7]
---

# File manager shell exec

:::{requirement id="fm-shell-exec" level="SHOULD"}
The file manager SHOULD support shell execution against the
current selection / mark set:

- {#c-shell-blocking} `!` opens an input prompt; on Enter, runs
  the command in the codon terminal pane chosen by
  `c-shell-terminal-reuse`. The FM column visually grays until the
  command exits.
- {#c-shell-async} `;` runs the entered command non-blocking;
  control returns immediately to the FM. Output goes to the same
  terminal pane.
- {#c-shell-terminal-reuse} both `!` and `;` reuse the
  most-recently-active terminal pane in the active window when it
  is **idle** (no foreground process running, prompt visible).
  Otherwise spawn a new terminal pane. Idle detection: query
  alacritty's `Term::has_foreground_process()` or fall back to
  PTY shell-pid == foreground-pid.
- {#c-shell-substitutions} the command string accepts:
    `{path}`  — selected entry's absolute path (or `path` of cursor when no marks)
    `{paths}` — every marked entry, shell-escaped + space-separated
    `{name}`  — file name only (basename) of `{path}`
    `{names}` — file names of `{paths}`, shell-escaped + space-separated
    `{cwd}`   — current_dir
    `{parent}` — parent of `current_dir`
  Substitution happens before the shell sees the line.
- {#c-shell-stderr-toast} if the spawned process exits non-zero
  for `!` (blocking), surface its stderr via the existing
  `surface_error` toast. For `;` (async), failures are visible in
  the terminal output but don't toast (the user moved on).
:::
