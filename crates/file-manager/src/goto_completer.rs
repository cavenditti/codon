//! `:cd <path>` palette completer.
//!
//! Routes through the codon command palette so the same Helix-style `:`
//! palette that owns `:open`, `:theme`, `:search` also handles `:cd`.
//! Selecting a row dispatches `file_manager::GotoPath(<path>)`, which
//! opens the FM's input bar pre-seeded with the chosen path; Tab there
//! continues to extend the path against the filesystem.

use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use codon_command_palette::completer::{Completer, CompletionItem};
use gpui::{Action, App, Task, WeakEntity};
use ui::SharedString;
use workspace::Workspace;

use crate::file_manager::GotoPath;

pub fn register() {
    codon_command_palette::completer::register(Arc::new(CdCompleter));
}

struct CdCompleter;

impl Completer for CdCompleter {
    fn id(&self) -> &'static str {
        "cd"
    }
    fn aliases(&self) -> &'static [&'static str] {
        &["cd", "chdir"]
    }
    fn action_name(&self) -> &'static str {
        "codon_fm::GotoPath"
    }
    fn placeholder(&self) -> &'static str {
        "directory path"
    }

    fn complete(
        &self,
        query: &str,
        workspace: WeakEntity<Workspace>,
        cx: &mut App,
    ) -> Task<Result<Vec<CompletionItem>>> {
        let base = workspace
            .read_with(cx, |workspace, cx| {
                workspace
                    .project()
                    .read(cx)
                    .visible_worktrees(cx)
                    .next()
                    .map(|wt| wt.read(cx).abs_path().to_path_buf())
            })
            .ok()
            .flatten()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

        let expanded = expand_tilde(query);
        let (dir_part, leaf) = split_dir_leaf(&expanded);
        let target_abs = if dir_part.starts_with('/') {
            PathBuf::from(&*dir_part)
        } else if dir_part.is_empty() {
            base
        } else {
            base.join(&*dir_part)
        };

        let Ok(entries) = std::fs::read_dir(&target_abs) else {
            return Task::ready(Ok(Vec::new()));
        };
        let leaf_lc = leaf.to_lowercase();
        let mut dirs: Vec<CompletionItem> = entries
            .flatten()
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                if name.starts_with('.') && leaf.is_empty() {
                    return None;
                }
                if !leaf_lc.is_empty() && !name.to_lowercase().contains(&leaf_lc) {
                    return None;
                }
                let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
                if !is_dir {
                    return None;
                }
                let abs = target_abs.join(&name);
                let typed = if dir_part.is_empty() {
                    format!("{name}/")
                } else if dir_part.ends_with('/') {
                    format!("{dir_part}{name}/")
                } else {
                    format!("{dir_part}/{name}/")
                };
                Some(CompletionItem {
                    value: abs.to_string_lossy().to_string(),
                    label: SharedString::from(format!("{name}/")),
                    detail: None,
                    navigates_to: Some(typed),
                })
            })
            .collect();
        dirs.sort_by(|a, b| a.label.cmp(&b.label));
        Task::ready(Ok(dirs))
    }

    fn build_action(&self, value: &str) -> Box<dyn Action> {
        Box::new(GotoPath(value.to_string()))
    }
}

fn expand_tilde(query: &str) -> String {
    if let Some(rest) = query.strip_prefix("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            return format!("{}/{}", home.to_string_lossy(), rest);
        }
    } else if query == "~" {
        if let Some(home) = std::env::var_os("HOME") {
            return home.to_string_lossy().into_owned();
        }
    }
    query.to_string()
}

fn split_dir_leaf(query: &str) -> (std::borrow::Cow<'_, str>, std::borrow::Cow<'_, str>) {
    match query.rfind('/') {
        Some(ix) => (
            std::borrow::Cow::Borrowed(&query[..=ix]),
            std::borrow::Cow::Borrowed(&query[ix + 1..]),
        ),
        None => (
            std::borrow::Cow::Borrowed(""),
            std::borrow::Cow::Borrowed(query),
        ),
    }
}
