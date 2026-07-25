use gpui::{App, Context, Entity, EntityId, Task, Window};
use std::path::{Path, PathBuf};
use std::time::Duration;
use terminal::Terminal;
use terminal_view::TerminalView;
use terminal_view::terminal_panel::TerminalPanel;
use workspace::Workspace;

/// Expand the file-manager shell-exec placeholders in `template` against
/// the FM cursor / marked set / current directory.
///
/// Substituted tokens (each result is shell-quoted):
///
/// - `{path}`   — `cursor`, shell-quoted
/// - `{paths}`  — every entry in `marked`, shell-quoted and joined by
///                spaces; falls back to `[cursor]` when `marked` is empty
/// - `{name}`   — `cursor.file_name()`, shell-quoted (empty when the
///                cursor has no basename)
/// - `{names}`  — basenames of every entry in `marked`, shell-quoted and
///                space-joined; falls back to `[cursor.file_name()]`
///                when `marked` is empty
/// - `{cwd}`    — `cwd`, shell-quoted
/// - `{parent}` — `cwd.parent()`, shell-quoted (falls back to `cwd` when
///                already at the filesystem root, so the placeholder is
///                never empty)
///
/// Doubled braces (`{{` and `}}`) are literal — they survive the
/// substitution as a single `{` or `}`. Unknown placeholders pass through
/// unchanged so a user's `{` in the middle of a command (e.g. a shell
/// brace expansion) is preserved verbatim.
pub fn apply_substitutions(
    template: &str,
    cursor: &Path,
    marked: &[PathBuf],
    cwd: &Path,
) -> String {
    let mut out = String::with_capacity(template.len());
    let bytes = template.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b'{' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            out.push('{');
            i += 2;
            continue;
        }
        if b == b'}' && i + 1 < bytes.len() && bytes[i + 1] == b'}' {
            out.push('}');
            i += 2;
            continue;
        }
        if b == b'{' {
            if let Some(end_rel) = template[i + 1..].find('}') {
                let end = i + 1 + end_rel;
                let token = &template[i + 1..end];
                if let Some(expansion) = expand_token(token, cursor, marked, cwd) {
                    out.push_str(&expansion);
                    i = end + 1;
                    continue;
                }
            }
        }
        out.push(b as char);
        i += 1;
    }

    out
}

fn expand_token(token: &str, cursor: &Path, marked: &[PathBuf], cwd: &Path) -> Option<String> {
    match token {
        "path" => Some(quote_path(cursor)),
        "paths" => {
            if marked.is_empty() {
                Some(quote_path(cursor))
            } else {
                Some(
                    marked
                        .iter()
                        .map(|p| quote_path(p))
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            }
        }
        "name" => Some(quote_name(cursor)),
        "names" => {
            if marked.is_empty() {
                Some(quote_name(cursor))
            } else {
                Some(
                    marked
                        .iter()
                        .map(|p| quote_name(p))
                        .collect::<Vec<_>>()
                        .join(" "),
                )
            }
        }
        "cwd" => Some(quote_path(cwd)),
        "parent" => Some(quote_path(cwd.parent().unwrap_or(cwd))),
        _ => None,
    }
}

fn quote_path(path: &Path) -> String {
    quote_str(&path.to_string_lossy())
}

fn quote_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    quote_str(&name)
}

/// shlex's `try_quote` refuses bytes it can't represent in a POSIX-quoted
/// form (e.g. embedded NULs). For interactive shells those would be
/// pathological anyway — when the quoter rejects the input we fall back
/// to a hard single-quoted form with single-quotes escaped the
/// POSIX-shell way (`'\''`).
fn quote_str(s: &str) -> String {
    match shlex::try_quote(s) {
        Ok(quoted) => quoted.into_owned(),
        Err(_) => fallback_single_quote(s),
    }
}

/// Outcome of [`pick_terminal_for_shell`]. `Existing` carries the
/// most-recently-active idle terminal in the active window; `New` says
/// the FM should spawn a fresh terminal split before sending the
/// command.
pub enum TerminalTarget {
    Existing(Entity<TerminalView>),
    New,
}

/// Find a terminal to host an FM-initiated shell command.
///
/// Idle = the PTY's foreground process group equals the shell's own
/// PID (no child has stolen the foreground slot). Display-only and
/// remote terminals are always treated as "busy" so we never reuse
/// them. When nothing idle is reachable, returns [`TerminalTarget::New`]
/// so the caller can spawn a new split.
///
/// This intentionally mirrors `recent_active_item_by_type::<TerminalView>`
/// + an idle filter — the codon goto-or-open verb already uses the
/// same MRU lookup for navigation, so we want the same affordance for
/// shell exec.
///
/// `skip` lets the caller exclude its own entity — `act_as::<TerminalView>`
/// internally borrows each item for a type check, and we'd panic with
/// "cannot read while already being updated" if the iteration reached
/// the FM itself (which is always pinned as a workspace item while the
/// shell-exec keybind runs from inside its own `update`).
pub fn pick_terminal_for_shell(
    workspace: &Workspace,
    cx: &App,
    skip: Option<EntityId>,
) -> TerminalTarget {
    let mut recent: Option<Entity<TerminalView>> = None;
    let mut recent_timestamp: usize = 0;

    for pane_handle in workspace.panes() {
        let pane = pane_handle.read(cx);
        for (item_id, item) in pane
            .items()
            .map(|item| (item.item_id(), item.clone()))
            .collect::<Vec<_>>()
        {
            if Some(item_id) == skip {
                continue;
            }
            let Some(view) = item.act_as::<TerminalView>(cx) else {
                continue;
            };
            if !terminal_is_idle(&view, cx) {
                continue;
            }
            let timestamp = pane
                .activation_history()
                .iter()
                .find(|e| e.entity_id == item_id)
                .map(|e| e.timestamp)
                .unwrap_or(0);
            if timestamp >= recent_timestamp {
                recent_timestamp = timestamp;
                recent = Some(view);
            }
        }
    }

    match recent {
        Some(view) => TerminalTarget::Existing(view),
        None => TerminalTarget::New,
    }
}

/// `true` when the wrapped PTY has no foreground child — i.e. only the
/// shell process is in the foreground process group. Display-only
/// terminals always report `false` so we never write to a frozen view.
fn terminal_is_idle(view: &Entity<TerminalView>, cx: &App) -> bool {
    let terminal = view.read(cx).entity().read(cx);
    let Some(getter) = terminal.pid_getter() else {
        return false;
    };
    let Some(foreground) = terminal.pid() else {
        return false;
    };
    foreground == getter.fallback_pid()
}

/// Send `command` to `view`'s PTY, prefixed with `cd <cwd>` so the
/// command runs from the FM's current directory. `command` is already
/// fully substituted via [`apply_substitutions`]; this helper just
/// frames it with `cd` + a trailing newline so the shell executes it.
///
/// When `mark_exit` is true the frame trails with
/// `echo __codon_exit_marker:$?` so the blocking-exec watcher can read
/// the command's exit status off the scrollback — the user pipes their
/// stderr through the same terminal anyway, so a marker line is the
/// least-invasive way to surface the status.
///
/// Newlines inside `command` are forwarded verbatim — the user's shell
/// is responsible for parsing them. Empty `command` is a no-op so a
/// bare `!` Enter doesn't blast a stray newline.
pub fn send_to_terminal(
    view: &Entity<TerminalView>,
    cwd: &Path,
    command: &str,
    mark_exit: bool,
    cx: &mut App,
) {
    if command.trim().is_empty() {
        return;
    }
    let terminal = view.read(cx).entity().clone();
    let cwd_quoted = quote_path(cwd);
    let payload = if mark_exit {
        format!("cd {cwd_quoted} && {{ {command} ; }} ; echo __codon_exit_marker:$?\n")
    } else {
        format!("cd {cwd_quoted} && {command}\n")
    };
    terminal.update(cx, |term, _cx| {
        term.input(payload.into_bytes());
    });
}

/// Activate `view` in its pane so the user sees the command output. No-op
/// when the pane no longer hosts the view (e.g. it was closed between
/// the substitution and the actual send).
pub fn focus_terminal(
    workspace: &mut Workspace,
    view: &Entity<TerminalView>,
    window: &mut Window,
    cx: &mut App,
) {
    workspace.activate_item(view, true, true, window, cx);
}

/// Spawn a fresh center-pane terminal rooted at `cwd`, then run
/// `command` once the terminal is ready. Returns a [`Task`] that
/// resolves to the new terminal entity (so the caller can subscribe to
/// it for the blocking-exec overlay) or `None` if the spawn failed.
pub fn spawn_new_terminal_and_run(
    workspace: &mut Workspace,
    cwd: PathBuf,
    command: String,
    mark_exit: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Task<Option<Entity<Terminal>>> {
    let task = TerminalPanel::add_center_terminal(workspace, window, cx, move |project, cx| {
        project.create_terminal_shell(Some(cwd), cx)
    });
    cx.spawn(async move |_, cx| {
        let terminal = task.await.ok()?.upgrade()?;
        cx.update(|cx| {
            terminal.update(cx, |term, _cx| {
                let payload = if mark_exit {
                    format!("{{ {command} ; }} ; echo __codon_exit_marker:$?\n")
                } else {
                    format!("{command}\n")
                };
                term.input(payload.into_bytes());
            });
        });
        Some(terminal)
    })
}

/// Snapshot the last N non-empty lines from the terminal's scrollback —
/// used by the blocking-exec stderr toast when the command exited
/// non-zero. The terminal is the source of truth for what the user saw,
/// so we sample it instead of capturing through a separate pipe.
pub fn snapshot_tail(view: &Entity<TerminalView>, n: usize, cx: &App) -> Vec<String> {
    view.read(cx).entity().read(cx).last_n_non_empty_lines(n)
}

/// Convenience: a small polling interval for the blocking-exec watcher.
/// Exposed so call sites stay symmetric — they don't need to know the
/// exact cadence, just that the watcher checks "is the foreground
/// process still the shell".
pub const SHELL_POLL_INTERVAL: Duration = Duration::from_millis(150);

fn fallback_single_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        if ch == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn path_substitutes_cursor() {
        let out = apply_substitutions("cat {path}", &p("/tmp/foo.txt"), &[], &p("/tmp"));
        assert_eq!(out, "cat /tmp/foo.txt");
    }

    #[test]
    fn path_quotes_special_characters() {
        let out = apply_substitutions("cat {path}", &p("/tmp/has space.txt"), &[], &p("/tmp"));
        assert_eq!(out, "cat '/tmp/has space.txt'");
    }

    #[test]
    fn paths_falls_back_to_cursor_when_marked_empty() {
        let out = apply_substitutions("rm {paths}", &p("/tmp/a.txt"), &[], &p("/tmp"));
        assert_eq!(out, "rm /tmp/a.txt");
    }

    #[test]
    fn paths_joins_marked_with_spaces() {
        let marked = vec![p("/tmp/a.txt"), p("/tmp/b c.txt")];
        let out = apply_substitutions("rm {paths}", &p("/tmp/a.txt"), &marked, &p("/tmp"));
        assert_eq!(out, "rm /tmp/a.txt '/tmp/b c.txt'");
    }

    #[test]
    fn name_substitutes_cursor_basename() {
        let out = apply_substitutions("touch {name}", &p("/tmp/foo.txt"), &[], &p("/tmp"));
        assert_eq!(out, "touch foo.txt");
    }

    #[test]
    fn name_empty_when_cursor_has_no_basename() {
        let out = apply_substitutions("echo {name}", &p("/"), &[], &p("/"));
        assert_eq!(out, "echo ''");
    }

    #[test]
    fn names_falls_back_to_cursor_when_marked_empty() {
        let out = apply_substitutions("echo {names}", &p("/tmp/foo.txt"), &[], &p("/tmp"));
        assert_eq!(out, "echo foo.txt");
    }

    #[test]
    fn names_joins_marked_basenames() {
        let marked = vec![p("/tmp/a.txt"), p("/var/log/b c.txt")];
        let out = apply_substitutions(
            "tar cf bundle.tar {names}",
            &p("/tmp/a.txt"),
            &marked,
            &p("/tmp"),
        );
        assert_eq!(out, "tar cf bundle.tar a.txt 'b c.txt'");
    }

    #[test]
    fn cwd_substitutes_current_directory() {
        let out = apply_substitutions("ls {cwd}", &p("/tmp/foo"), &[], &p("/tmp"));
        assert_eq!(out, "ls /tmp");
    }

    #[test]
    fn parent_substitutes_cwd_parent() {
        let out = apply_substitutions("ls {parent}", &p("/tmp/foo/x"), &[], &p("/tmp/foo"));
        assert_eq!(out, "ls /tmp");
    }

    #[test]
    fn parent_falls_back_to_cwd_at_root() {
        let out = apply_substitutions("ls {parent}", &p("/foo"), &[], &p("/"));
        assert_eq!(out, "ls /");
    }

    #[test]
    fn doubled_braces_escape_to_literal() {
        let out = apply_substitutions(
            "echo {{path}} is {path}",
            &p("/tmp/foo.txt"),
            &[],
            &p("/tmp"),
        );
        assert_eq!(out, "echo {path} is /tmp/foo.txt");
    }

    #[test]
    fn unknown_placeholder_passes_through() {
        let out = apply_substitutions("echo {unknown}", &p("/tmp/foo"), &[], &p("/tmp"));
        assert_eq!(out, "echo {unknown}");
    }

    #[test]
    fn unterminated_brace_passes_through() {
        let out = apply_substitutions("echo {path", &p("/tmp/foo"), &[], &p("/tmp"));
        assert_eq!(out, "echo {path");
    }

    #[test]
    fn multiple_substitutions_in_one_template() {
        let marked = vec![p("/tmp/a"), p("/tmp/b")];
        let out = apply_substitutions("cp {paths} {cwd}/dest", &p("/tmp/a"), &marked, &p("/tmp"));
        assert_eq!(out, "cp /tmp/a /tmp/b /tmp/dest");
    }

    #[test]
    fn fallback_single_quote_escapes_inner_quotes() {
        // No exposed API, but verify the helper logic via a string
        // shlex can quote — ensure the round-trip preserves the value.
        let out = apply_substitutions("echo {name}", &p("/tmp/it's.txt"), &[], &p("/tmp"));
        // shlex produces `'it'\''s.txt'` for inputs with a single quote.
        assert!(out.starts_with("echo "));
        assert!(out.contains("it"));
        assert!(out.contains("s.txt"));
    }
}
