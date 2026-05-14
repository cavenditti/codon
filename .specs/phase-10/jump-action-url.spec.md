---
id: TASK:phase-10/jump-action-url
type: task
status: accepted
version: 0.0.1
summary: >
  `codon_jump::JumpToUrl` action + default `cmd-k u` binding.
  Filters candidates to URL-only and copies the selected URL to
  the system clipboard with a `MessageNotification` toast.
owners: [carlo]
progress: done
refines:
  - REQ:codon/jump-hints#c-jump-urls
aspects: [url-action, clipboard-write, toast]
---

# JumpToUrl entry action

## What ships

- Action `codon_jump::JumpToUrl`.
- Handler: `JumpOverlay::open(JumpMode::Url, ...)`. The overlay's
  candidate-collection step filters to `JumpKind::Url(_)` for
  this mode.
- When a candidate fires, the overlay overrides the candidate's
  declared action with a clipboard write:
  ```rust
  let url = match candidate.kind { JumpKind::Url(s) => s, _ => return };
  cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
  workspace.show_notification(
      NotificationId::Named("jump-url-copied"),
      cx,
      |cx| cx.new(|_| MessageNotification::new(format!("Copied {url}"))),
  );
  ```
- One TOML line in `[bindings.global]`:
  ```toml
  "cmd-k u" = "codon_jump::JumpToUrl"
  ```
- One resolver arm.

The candidate's own action closure is dropped — for URL-mode the
clipboard-copy is uniform regardless of source pane.

## Verification

- Open a terminal pane with `curl https://example.com` history;
  `cmd-k u`: only URL chips appear; selecting one copies the URL
  and toasts.
- Open an editor with markdown containing `[Foo](https://...)`:
  the URL is hinted; copy works.
- Open fm: `cmd-k u` shows no chips (fm has no URLs) and dismisses
  immediately with a "No URLs visible" toast.

## Where it slots in

- Edit: `crates/codon-jump/src/codon_jump.rs` — `JumpMode::Url`
  branch in the dispatcher (~30 LOC).
- Edit: `crates/codon-keymap/src/keymap.rs` — TOML + resolver arm.
