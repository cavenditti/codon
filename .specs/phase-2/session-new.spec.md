---
id: TASK:phase-2/session-new
type: task
status: accepted
version: 0.1.0
summary: >
  Action SessionNew creates a session named after the current project's primary cwd.
owners: [carlo]
progress: done
refines:
  - REQ:codon/sessions#c-create
---

# Session creation action

Implemented in [codon_session::actions::handle_session_new](spec:src:crates/codon-session/src/actions.rs). Pulls cwd from the workspace's first visible worktree. Generates a unique name with numeric suffix on collision.
