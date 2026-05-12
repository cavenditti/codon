//! Completer trait + registry + the four built-in completers.
//!
//! A `Completer` declares the argument shape for a single command verb
//! (`open`, `theme`, `goto`, `search`). Once the user types the verb's
//! alias followed by a space in the command palette, the palette enters
//! Argument mode and the completer drives the row list.
//!
//! Built-ins:
//!   - `file_path`  → opens a project-relative file (`codon_command_palette::OpenFile`)
//!   - `theme`      → opens the theme selector pre-filtered to the typed name
//!   - `line_number`→ opens the standard go-to-line modal (Layer A limitation;
//!     the typed number is shown in the row but not auto-applied yet)
//!   - `search`     → opens project search (the typed query is shown but
//!     not auto-applied yet)
//!
//! See `.specs/phase-5/command-palette-{completer-trait,builtin-completers}.spec.md`.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock},
};

use anyhow::Result;
use gpui::{Action, App, Task, WeakEntity};
use theme::ThemeRegistry;
use ui::SharedString;
use workspace::Workspace;

use crate::OpenFile;

#[derive(Clone, Debug)]
pub struct CompletionItem {
    /// What `Completer::build_action` receives on a terminal confirmation.
    pub value: String,
    /// Row label shown in the picker.
    pub label: SharedString,
    /// Secondary muted line below the label, if any.
    pub detail: Option<SharedString>,
    /// If `Some`, Enter on this row is a *navigation* — the modal sets
    /// the palette query to `<verb> <navigates_to>` and re-runs the
    /// completer, instead of dispatching `build_action`. Used by the
    /// file-path completer to drill into directories breadth-first.
    pub navigates_to: Option<String>,
}

pub trait Completer: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    /// Leading verbs the user might type; first entry is canonical.
    fn aliases(&self) -> &'static [&'static str];
    fn action_name(&self) -> &'static str;
    fn placeholder(&self) -> &'static str;
    fn complete(
        &self,
        query: &str,
        workspace: WeakEntity<Workspace>,
        cx: &mut App,
    ) -> Task<Result<Vec<CompletionItem>>>;
    fn build_action(&self, value: &str) -> Box<dyn Action>;
}

#[derive(Default)]
pub struct CompleterRegistry {
    by_alias: HashMap<&'static str, Arc<dyn Completer>>,
    by_action_name: HashMap<&'static str, Arc<dyn Completer>>,
}

impl CompleterRegistry {
    pub fn register(&mut self, completer: Arc<dyn Completer>) {
        for alias in completer.aliases() {
            self.by_alias.insert(*alias, completer.clone());
        }
        self.by_action_name
            .insert(completer.action_name(), completer.clone());
    }

    pub fn for_alias(&self, alias: &str) -> Option<Arc<dyn Completer>> {
        self.by_alias.get(alias).cloned()
    }

    pub fn for_action_name(&self, name: &str) -> Option<Arc<dyn Completer>> {
        self.by_action_name.get(name).cloned()
    }
}

static GLOBAL: OnceLock<RwLock<CompleterRegistry>> = OnceLock::new();

fn registry() -> &'static RwLock<CompleterRegistry> {
    GLOBAL.get_or_init(|| RwLock::new(CompleterRegistry::default()))
}

pub fn register(completer: Arc<dyn Completer>) {
    registry()
        .write()
        .expect("completer registry poisoned")
        .register(completer);
}

pub fn for_alias(alias: &str) -> Option<Arc<dyn Completer>> {
    registry()
        .read()
        .expect("completer registry poisoned")
        .for_alias(alias)
}

pub fn for_action_name(name: &str) -> Option<Arc<dyn Completer>> {
    registry()
        .read()
        .expect("completer registry poisoned")
        .for_action_name(name)
}

/// Register all built-in completers. Called once from `init` at codon startup.
pub fn register_builtins() {
    register(Arc::new(FilePathCompleter));
    register(Arc::new(ThemeCompleter));
    register(Arc::new(LineNumberCompleter));
    register(Arc::new(SearchCompleter));
}

// ─────────────────────────── file path ──────────────────────────────────

struct FilePathCompleter;

impl Completer for FilePathCompleter {
    fn id(&self) -> &'static str { "file_path" }
    fn aliases(&self) -> &'static [&'static str] { &["open", "edit", "e"] }
    fn action_name(&self) -> &'static str { "codon_command_palette::OpenFile" }
    fn placeholder(&self) -> &'static str { "file path" }

    /// Breadth-first navigation: parse the query as `<dir>/<partial>`,
    /// list only the entries of `<dir>` (relative to the first visible
    /// worktree), filter by `<partial>` as a substring, sort directories
    /// first then files alphabetically. Directory items are marked
    /// `navigates_to`, so Enter fills the input and re-runs the
    /// completer; file items dispatch via `build_action`.
    fn complete(
        &self,
        query: &str,
        workspace: WeakEntity<Workspace>,
        _cx: &mut App,
    ) -> Task<Result<Vec<CompletionItem>>> {
        let (subdir, partial) = split_dir_and_partial(query);
        let result = workspace.read_with(_cx, |workspace, cx| {
            let Some(worktree) = workspace.project().read(cx).visible_worktrees(cx).next() else {
                return Vec::new();
            };
            let snap = worktree.read(cx);
            let abs_root = snap.abs_path();
            let target_abs = abs_root.join(&subdir);
            let Ok(entries) = std::fs::read_dir(&target_abs) else {
                return Vec::new();
            };
            let partial_lc = partial.to_lowercase();
            let mut dirs: Vec<CompletionItem> = Vec::new();
            let mut files: Vec<CompletionItem> = Vec::new();
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') && partial.is_empty() {
                    continue; // hide dotfiles unless the user types something
                }
                if !partial_lc.is_empty() && !name.to_lowercase().contains(&partial_lc) {
                    continue;
                }
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let abs = target_abs.join(&name);
                let rel = if subdir.is_empty() {
                    name.clone()
                } else {
                    format!("{}/{}", subdir, name)
                };
                if is_dir {
                    let nav = if rel.ends_with('/') { rel.clone() } else { format!("{rel}/") };
                    dirs.push(CompletionItem {
                        value: abs.to_string_lossy().to_string(),
                        label: SharedString::from(format!("{name}/")),
                        detail: None,
                        navigates_to: Some(nav),
                    });
                } else {
                    files.push(CompletionItem {
                        value: abs.to_string_lossy().to_string(),
                        label: SharedString::from(name),
                        detail: None,
                        navigates_to: None,
                    });
                }
            }
            dirs.sort_by(|a, b| a.label.cmp(&b.label));
            files.sort_by(|a, b| a.label.cmp(&b.label));
            dirs.extend(files);
            dirs
        });
        Task::ready(Ok(result.unwrap_or_default()))
    }

    fn build_action(&self, value: &str) -> Box<dyn Action> {
        Box::new(OpenFile(PathBuf::from(value)))
    }
}

/// Split `"src/foo"` into `("src", "foo")`, `"src/"` into `("src", "")`,
/// `"foo"` into `("", "foo")`, and `""` into `("", "")`.
fn split_dir_and_partial(query: &str) -> (String, String) {
    match query.rfind('/') {
        Some(ix) => (query[..ix].to_string(), query[ix + 1..].to_string()),
        None => (String::new(), query.to_string()),
    }
}

// ─────────────────────────── theme ──────────────────────────────────────

struct ThemeCompleter;

impl Completer for ThemeCompleter {
    fn id(&self) -> &'static str { "theme" }
    fn aliases(&self) -> &'static [&'static str] { &["theme", "colorscheme", "colo"] }
    fn action_name(&self) -> &'static str { "theme_selector::Toggle" }
    fn placeholder(&self) -> &'static str { "theme name" }

    fn complete(
        &self,
        query: &str,
        _workspace: WeakEntity<Workspace>,
        cx: &mut App,
    ) -> Task<Result<Vec<CompletionItem>>> {
        let names = ThemeRegistry::global(cx).list_names();
        let q = query.to_lowercase();
        let items: Vec<CompletionItem> = names
            .into_iter()
            .filter(|n| q.is_empty() || n.to_lowercase().contains(&q))
            .map(|n| CompletionItem {
                value: n.to_string(),
                label: n,
                detail: None,
                navigates_to: None,
            })
            .collect();
        Task::ready(Ok(items))
    }

    fn build_action(&self, value: &str) -> Box<dyn Action> {
        Box::new(zed_actions::theme_selector::Toggle {
            themes_filter: Some(vec![value.to_string()]),
        })
    }
}

// ─────────────────────────── line number ────────────────────────────────

struct LineNumberCompleter;

impl Completer for LineNumberCompleter {
    fn id(&self) -> &'static str { "line_number" }
    fn aliases(&self) -> &'static [&'static str] { &["goto", "line"] }
    fn action_name(&self) -> &'static str { "editor::ToggleGoToLine" }
    fn placeholder(&self) -> &'static str { "line number" }

    fn complete(
        &self,
        query: &str,
        _workspace: WeakEntity<Workspace>,
        _cx: &mut App,
    ) -> Task<Result<Vec<CompletionItem>>> {
        let trimmed = query.trim();
        let items = if trimmed.parse::<u32>().is_ok() {
            vec![CompletionItem {
                value: trimmed.to_string(),
                label: SharedString::from(format!("Line {trimmed}")),
                detail: Some(SharedString::from(
                    "Press Enter to open the go-to-line prompt",
                )),
                navigates_to: None,
            }]
        } else {
            vec![CompletionItem {
                value: String::new(),
                label: SharedString::from("Enter a line number"),
                detail: None,
                navigates_to: None,
            }]
        };
        Task::ready(Ok(items))
    }

    fn build_action(&self, _value: &str) -> Box<dyn Action> {
        // Layer A: the standard editor go-to-line modal still owns the
        // numeric prompt. A future codon-owned `GoToLine(u32)` will skip
        // the second modal.
        Box::new(editor::actions::ToggleGoToLine)
    }
}

// ─────────────────────────── search ─────────────────────────────────────

struct SearchCompleter;

impl Completer for SearchCompleter {
    fn id(&self) -> &'static str { "search" }
    fn aliases(&self) -> &'static [&'static str] { &["search", "rg", "grep"] }
    fn action_name(&self) -> &'static str { "workspace::NewSearch" }
    fn placeholder(&self) -> &'static str { "search query" }

    fn complete(
        &self,
        query: &str,
        _workspace: WeakEntity<Workspace>,
        _cx: &mut App,
    ) -> Task<Result<Vec<CompletionItem>>> {
        let q = query.trim();
        let items = if q.is_empty() {
            vec![CompletionItem {
                value: String::new(),
                label: SharedString::from("Enter a search query"),
                detail: None,
                navigates_to: None,
            }]
        } else {
            vec![CompletionItem {
                value: q.to_string(),
                label: SharedString::from(q.to_string()),
                detail: Some(SharedString::from(
                    "Press Enter to open project search",
                )),
                navigates_to: None,
            }]
        };
        Task::ready(Ok(items))
    }

    fn build_action(&self, _value: &str) -> Box<dyn Action> {
        // Layer A: seeds the search panel but does not pre-fill the query.
        Box::new(workspace::NewSearch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubCompleter;
    impl Completer for StubCompleter {
        fn id(&self) -> &'static str { "stub" }
        fn aliases(&self) -> &'static [&'static str] { &["stub", "s"] }
        fn action_name(&self) -> &'static str { "stub::Run" }
        fn placeholder(&self) -> &'static str { "stub" }
        fn complete(
            &self,
            _q: &str,
            _w: WeakEntity<Workspace>,
            _cx: &mut App,
        ) -> Task<Result<Vec<CompletionItem>>> {
            Task::ready(Ok(Vec::new()))
        }
        fn build_action(&self, _v: &str) -> Box<dyn Action> {
            unimplemented!()
        }
    }

    #[test]
    fn registry_round_trips_aliases_and_action_name() {
        let mut reg = CompleterRegistry::default();
        reg.register(Arc::new(StubCompleter));
        assert!(reg.for_alias("stub").is_some());
        assert!(reg.for_alias("s").is_some());
        assert!(reg.for_alias("unknown").is_none());
        assert!(reg.for_action_name("stub::Run").is_some());
    }
}
