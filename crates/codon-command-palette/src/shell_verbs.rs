//! Palette path for Helix-style shell verbs.
//!
//! Registers five `Completer` impls so the codon command palette
//! recognises:
//!
//! - `:pipe <cmd>`           — equivalent to the `|` keyboard verb.
//! - `:pipe-to <cmd>`        — equivalent to `Alt-|`.
//! - `:insert-output <cmd>`  — equivalent to `!`.
//! - `:append-output <cmd>`  — equivalent to `Alt-!`.
//! - `:keep-pipe <cmd>`      — equivalent to `$`.
//! - `:sh <cmd>`             — standalone, no selection involvement;
//!   runs `<cmd>` in a new terminal pane and shows its output.
//!
//! Each free-form completer treats the user's typed text as the shell
//! command verbatim. There is no fuzzy-matching against a candidate
//! list; the row is a single passthrough item that surfaces the typed
//! text back to the user so the prompt feels live.
//!
//! See `TASK:phase-16/shell-palette-verbs` and the parent
//! `REQ:codon/shell-integration`.

use std::sync::Arc;

use anyhow::Result;
use gpui::{Action, App, Task, WeakEntity};
use ui::SharedString;
use vim::shell::{ShellMode, ShellRun};
use workspace::Workspace;

use crate::completer::{CompletionItem, Completer, register};

/// Register the six shell verbs (`pipe`, `pipe-to`, `insert-output`,
/// `append-output`, `keep-pipe`, `sh`) on the global completer
/// registry. Idempotent — re-registering the same alias overwrites the
/// previous entry.
pub fn register_builtins() {
    register(Arc::new(ShellVerbCompleter::pipe()));
    register(Arc::new(ShellVerbCompleter::pipe_to()));
    register(Arc::new(ShellVerbCompleter::insert_output()));
    register(Arc::new(ShellVerbCompleter::append_output()));
    register(Arc::new(ShellVerbCompleter::keep_pipe()));
    register(Arc::new(ShellVerbCompleter::sh()));
}

/// One verb's worth of completer state. The data is small enough
/// (three `&'static str`s + a mode tag) to inline rather than spread
/// across six separate struct types.
struct ShellVerbCompleter {
    id: &'static str,
    aliases: &'static [&'static str],
    action_name: &'static str,
    placeholder: &'static str,
    /// `None` is the marker for `:sh` — the standalone, no-selection
    /// form. `Some(mode)` is one of the four selection-aware verbs and
    /// dispatches `vim::ShellRun { mode, cmd }`.
    mode: Option<ShellMode>,
}

impl ShellVerbCompleter {
    const fn pipe() -> Self {
        Self {
            id: "shell_pipe",
            aliases: &["pipe"],
            action_name: "vim::ShellRun",
            placeholder: "shell command to pipe selection through",
            mode: Some(ShellMode::PipeReplace),
        }
    }
    const fn pipe_to() -> Self {
        Self {
            id: "shell_pipe_to",
            aliases: &["pipe-to"],
            action_name: "vim::ShellRun",
            placeholder: "shell command (stdout discarded)",
            mode: Some(ShellMode::PipeDiscard),
        }
    }
    const fn insert_output() -> Self {
        Self {
            id: "shell_insert_output",
            aliases: &["insert-output"],
            action_name: "vim::ShellRun",
            placeholder: "shell command (stdout inserted before each selection)",
            mode: Some(ShellMode::InsertBefore),
        }
    }
    const fn append_output() -> Self {
        Self {
            id: "shell_append_output",
            aliases: &["append-output"],
            action_name: "vim::ShellRun",
            placeholder: "shell command (stdout appended after each selection)",
            mode: Some(ShellMode::AppendAfter),
        }
    }
    const fn keep_pipe() -> Self {
        Self {
            id: "shell_keep_pipe",
            aliases: &["keep-pipe"],
            action_name: "vim::ShellRun",
            placeholder: "shell predicate (keep selections whose exit == 0)",
            mode: Some(ShellMode::KeepIfZero),
        }
    }
    const fn sh() -> Self {
        Self {
            id: "shell_sh",
            aliases: &["sh"],
            action_name: "codon_command_palette::RunShell",
            placeholder: "shell command (output to a new terminal pane)",
            mode: None,
        }
    }
}

impl Completer for ShellVerbCompleter {
    fn id(&self) -> &'static str {
        self.id
    }
    fn aliases(&self) -> &'static [&'static str] {
        self.aliases
    }
    fn action_name(&self) -> &'static str {
        self.action_name
    }
    fn placeholder(&self) -> &'static str {
        self.placeholder
    }

    /// Free-form completer: the entire query becomes the row's
    /// `value`, and `build_action` reads it back. Returning a single
    /// row keeps the picker out of "no candidates" empty-state mode.
    fn complete(
        &self,
        query: &str,
        _workspace: WeakEntity<Workspace>,
        _cx: &mut App,
    ) -> Task<Result<Vec<CompletionItem>>> {
        let trimmed = query.trim().to_string();
        let items = if trimmed.is_empty() {
            vec![CompletionItem {
                value: String::new(),
                label: SharedString::from(format!("Enter a {}", self.placeholder)),
                detail: None,
                navigates_to: None,
            }]
        } else {
            vec![CompletionItem {
                value: trimmed.clone(),
                label: SharedString::from(trimmed),
                detail: Some(SharedString::from("Press Enter to run")),
                navigates_to: None,
            }]
        };
        Task::ready(Ok(items))
    }

    fn build_action(&self, value: &str) -> Box<dyn Action> {
        let cmd = value.to_string();
        match self.mode {
            Some(mode) => Box::new(ShellRun { mode, cmd }),
            None => Box::new(crate::RunShell(cmd)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipe_builds_shell_run_with_pipe_replace_mode() {
        let completer = ShellVerbCompleter::pipe();
        let action = completer.build_action("sort | uniq");
        let action: &ShellRun = action
            .as_any()
            .downcast_ref::<ShellRun>()
            .expect("pipe verb dispatches vim::ShellRun");
        assert_eq!(action.mode, ShellMode::PipeReplace);
        assert_eq!(action.cmd, "sort | uniq");
    }

    #[test]
    fn pipe_to_uses_pipe_discard_mode() {
        let action = ShellVerbCompleter::pipe_to().build_action("pbcopy");
        let action = action
            .as_any()
            .downcast_ref::<ShellRun>()
            .expect("pipe-to dispatches vim::ShellRun");
        assert_eq!(action.mode, ShellMode::PipeDiscard);
    }

    #[test]
    fn insert_output_uses_insert_before_mode() {
        let action = ShellVerbCompleter::insert_output().build_action("date");
        let action = action
            .as_any()
            .downcast_ref::<ShellRun>()
            .expect("insert-output dispatches vim::ShellRun");
        assert_eq!(action.mode, ShellMode::InsertBefore);
    }

    #[test]
    fn append_output_uses_append_after_mode() {
        let action = ShellVerbCompleter::append_output().build_action("uuidgen");
        let action = action
            .as_any()
            .downcast_ref::<ShellRun>()
            .expect("append-output dispatches vim::ShellRun");
        assert_eq!(action.mode, ShellMode::AppendAfter);
    }

    #[test]
    fn keep_pipe_uses_keep_if_zero_mode() {
        let action = ShellVerbCompleter::keep_pipe().build_action("grep -q TODO");
        let action = action
            .as_any()
            .downcast_ref::<ShellRun>()
            .expect("keep-pipe dispatches vim::ShellRun");
        assert_eq!(action.mode, ShellMode::KeepIfZero);
        assert_eq!(action.cmd, "grep -q TODO");
    }

    #[test]
    fn sh_dispatches_run_shell_not_shell_run() {
        let action = ShellVerbCompleter::sh().build_action("ls -la");
        assert!(
            action.as_any().downcast_ref::<ShellRun>().is_none(),
            "sh must not dispatch vim::ShellRun"
        );
        let run_shell = action
            .as_any()
            .downcast_ref::<crate::RunShell>()
            .expect("sh dispatches codon_command_palette::RunShell");
        assert_eq!(run_shell.0, "ls -la");
    }
}
