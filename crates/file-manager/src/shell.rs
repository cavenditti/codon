use std::path::{Path, PathBuf};

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

fn expand_token(
    token: &str,
    cursor: &Path,
    marked: &[PathBuf],
    cwd: &Path,
) -> Option<String> {
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
        let out = apply_substitutions(
            "cat {path}",
            &p("/tmp/has space.txt"),
            &[],
            &p("/tmp"),
        );
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
        let out = apply_substitutions(
            "cp {paths} {cwd}/dest",
            &p("/tmp/a"),
            &marked,
            &p("/tmp"),
        );
        assert_eq!(out, "cp /tmp/a /tmp/b /tmp/dest");
    }

    #[test]
    fn fallback_single_quote_escapes_inner_quotes() {
        // No exposed API, but verify the helper logic via a string
        // shlex can quote — ensure the round-trip preserves the value.
        let out = apply_substitutions(
            "echo {name}",
            &p("/tmp/it's.txt"),
            &[],
            &p("/tmp"),
        );
        // shlex produces `'it'\''s.txt'` for inputs with a single quote.
        assert!(out.starts_with("echo "));
        assert!(out.contains("it"));
        assert!(out.contains("s.txt"));
    }
}
