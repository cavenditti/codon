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
use fuzzy::StringMatchCandidate;
use gpui::{Action, App, AppContext as _, Task, WeakEntity};
use theme::ThemeRegistry;
use ui::SharedString;
use workspace::Workspace;

use crate::OpenFile;

#[derive(Clone, Debug)]
pub struct CompletionItem {
    pub value: String,
    pub label: SharedString,
    pub detail: Option<SharedString>,
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

    fn complete(
        &self,
        query: &str,
        workspace: WeakEntity<Workspace>,
        cx: &mut App,
    ) -> Task<Result<Vec<CompletionItem>>> {
        let query = query.to_string();
        // Snapshot every visible worktree's file paths up-front so the
        // background fuzzy match doesn't need to re-enter the App context.
        let paths: Vec<(PathBuf, String)> = workspace
            .read_with(cx, |workspace, cx| {
                let mut out = Vec::new();
                for worktree in workspace.project().read(cx).visible_worktrees(cx) {
                    let snap = worktree.read(cx);
                    let abs_root = snap.abs_path();
                    for entry in snap.entries(false, 0) {
                        if entry.is_file() {
                            let rel = entry.path.as_unix_str().to_string();
                            let abs = abs_root.join(entry.path.as_std_path());
                            out.push((abs.to_path_buf(), rel));
                        }
                    }
                    if out.len() > 25_000 {
                        break;
                    }
                }
                out
            })
            .unwrap_or_default();
        let executor = cx.background_executor().clone();
        cx.background_spawn(async move {
            if paths.is_empty() {
                return Ok(Vec::new());
            }
            let candidates: Vec<StringMatchCandidate> = paths
                .iter()
                .enumerate()
                .map(|(ix, (_, rel))| StringMatchCandidate::new(ix, rel))
                .collect();
            let matches = fuzzy::match_strings(
                &candidates,
                &query,
                false,
                true,
                100,
                &Default::default(),
                executor,
            )
            .await;
            let items = matches
                .into_iter()
                .filter_map(|m| {
                    paths.get(m.candidate_id).map(|(abs, rel)| CompletionItem {
                        value: abs.to_string_lossy().to_string(),
                        label: SharedString::from(rel.clone()),
                        detail: None,
                    })
                })
                .collect();
            Ok(items)
        })
    }

    fn build_action(&self, value: &str) -> Box<dyn Action> {
        Box::new(OpenFile(PathBuf::from(value)))
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
            }]
        } else {
            vec![CompletionItem {
                value: String::new(),
                label: SharedString::from("Enter a line number"),
                detail: None,
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
            }]
        } else {
            vec![CompletionItem {
                value: q.to_string(),
                label: SharedString::from(q.to_string()),
                detail: Some(SharedString::from(
                    "Press Enter to open project search",
                )),
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
