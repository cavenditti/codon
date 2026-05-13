//! Declarative file-opener configuration.
//!
//! Reads `~/.config/codon/openers.toml` once at startup and re-reads it
//! whenever the FS watcher reports a change. The parsed table is stored
//! as a `OpenerStore` global so the file manager can:
//!
//! - filter the set of openers matching the entry under the cursor (used
//!   by the `O` picker and the Enter / `l` route);
//! - retrieve a single chosen entry by its index for dispatch.
//!
//! Each `[[opener]]` row declares either a `glob` pattern or an explicit
//! `mime` essence string; the loader silently skips rows that declare
//! neither. `cmd` is the substitution template — substitutions live in
//! `crate::shell::apply_substitutions`, the same flow used by the `!`/`;`
//! shell-exec verbs.

use anyhow::{Context as _, Result};
use fs::Fs;
use futures::StreamExt as _;
use globset::{Glob, GlobMatcher};
use gpui::{App, BorrowAppContext as _, Global};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const FILE_NAME: &str = "openers.toml";

/// One opener entry as parsed off-disk. Both predicates are optional in
/// the TOML to keep the schema friendly, but the loader rejects rows
/// that declare neither (a row with nothing to match is unreachable).
#[derive(Clone, Debug, Default, Deserialize)]
struct OpenerRow {
    #[serde(default)]
    glob: Option<String>,
    #[serde(default)]
    mime: Option<String>,
    cmd: String,
    #[serde(default)]
    block: bool,
    #[serde(default)]
    description: String,
}

#[derive(Clone, Debug, Deserialize)]
struct OpenersDoc {
    #[serde(default)]
    opener: Vec<OpenerRow>,
}

/// A compiled opener: the glob pattern is parsed into a `GlobMatcher`
/// up front so the per-keystroke matching path stays cheap.
#[derive(Clone, Debug)]
pub struct Opener {
    /// Compiled glob — `None` when the row matched by mime essence only.
    glob: Option<GlobMatcher>,
    /// Mime essence string (e.g. `application/pdf`) — `None` when the
    /// row matched by glob only.
    pub mime: Option<String>,
    /// Shell-style template the FM hands to `apply_substitutions` before
    /// spawning. Empty strings are filtered out at load time.
    pub cmd: String,
    /// When `true` the opener is run via the FM's blocking shell-exec
    /// route (terminal overlay + foreground watcher). When `false` it
    /// goes through the async route (fire-and-forget).
    pub block: bool,
    /// Human-readable label shown in the `O` picker. Falls back to the
    /// cmd template when the user omitted `description`.
    pub description: String,
}

impl Opener {
    /// `true` when this opener should be offered for `path`. Mime
    /// matching uses `mime_guess::from_path` so it stays consistent with
    /// the FM preview pane's mime detection.
    pub fn matches(&self, path: &Path) -> bool {
        if let Some(matcher) = &self.glob {
            if matcher.is_match(path) {
                return true;
            }
            // Glob crates match against the full path by default; users
            // typically write `*.png`, expecting it to match the basename
            // — try the file name explicitly so both styles work.
            if let Some(name) = path.file_name() {
                if matcher.is_match(Path::new(name)) {
                    return true;
                }
            }
        }
        if let Some(expected) = &self.mime {
            let guessed = mime_guess::from_path(path)
                .first()
                .map(|m| m.essence_str().to_string())
                .unwrap_or_else(|| "application/octet-stream".to_string());
            if guessed.eq_ignore_ascii_case(expected) {
                return true;
            }
        }
        false
    }

    /// Compact label used in the picker — `<description> (<cmd>)`.
    pub fn label(&self) -> String {
        if self.description.is_empty() {
            self.cmd.clone()
        } else {
            format!("{}  ({})", self.description, self.cmd)
        }
    }
}

/// App-global store of compiled openers. The vec is replaced atomically
/// on every reload so callers that hold a `&App` see a consistent
/// snapshot.
#[derive(Clone, Debug, Default)]
pub struct OpenerStore {
    openers: Vec<Opener>,
}

impl Global for OpenerStore {}

impl OpenerStore {
    /// Every opener matching `path`, in the on-disk declaration order.
    /// The picker prepends a synthetic "Codon (default)" row so even
    /// when this returns empty the user still has an entry point.
    pub fn matches_for(&self, path: &Path) -> Vec<Opener> {
        self.openers
            .iter()
            .filter(|o| o.matches(path))
            .cloned()
            .collect()
    }

    /// Whole-store snapshot — exposed for tests and the cheatsheet
    /// so it can reflect the active opener set.
    pub fn all(&self) -> &[Opener] {
        &self.openers
    }

    /// Replace the store's contents with a freshly parsed document.
    /// Logs and ignores rows that compile to no predicate.
    fn install(&mut self, doc: OpenersDoc) {
        let mut openers = Vec::with_capacity(doc.opener.len());
        for row in doc.opener {
            if row.cmd.trim().is_empty() {
                log::warn!("openers.toml: skipping row with empty cmd");
                continue;
            }
            let glob = match row.glob.as_deref() {
                Some(pattern) if !pattern.is_empty() => match Glob::new(pattern) {
                    Ok(g) => Some(g.compile_matcher()),
                    Err(err) => {
                        log::warn!("openers.toml: invalid glob '{pattern}' ({err})");
                        continue;
                    }
                },
                _ => None,
            };
            let mime = row.mime.filter(|m| !m.is_empty());
            if glob.is_none() && mime.is_none() {
                log::warn!(
                    "openers.toml: skipping opener '{}' (neither glob nor mime declared)",
                    row.description
                );
                continue;
            }
            openers.push(Opener {
                glob,
                mime,
                cmd: row.cmd,
                block: row.block,
                description: row.description,
            });
        }
        self.openers = openers;
    }
}

/// On-disk location of the openers file. Honours `$XDG_CONFIG_HOME` via
/// `codon_config::codon_config_dir`, so it always sits next to
/// `codon.toml`.
pub fn user_openers_path() -> Option<PathBuf> {
    codon_config::codon_config_dir().map(|d| d.join(FILE_NAME))
}

/// Parse a TOML document into an `OpenersDoc`. Exposed so the watcher
/// task and the synchronous startup path share the same code.
fn parse_doc(content: &str) -> Result<OpenersDoc> {
    toml::from_str::<OpenersDoc>(content).context("parsing openers.toml")
}

/// Synchronous initial load. Missing file is a no-op (the store stays
/// empty so the default `open_abs_path` route applies to every entry).
/// Parse failures keep the previous contents and log a warning.
pub fn apply_user_openers(cx: &mut App) {
    let Some(path) = user_openers_path() else {
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => match parse_doc(&content) {
            Ok(doc) => {
                cx.update_global::<OpenerStore, _>(|store, _| store.install(doc));
                log::debug!("openers: loaded {}", path.display());
            }
            Err(err) => log::warn!(
                "openers: ignoring malformed {} ({err:#})",
                path.display()
            ),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            log::debug!("openers: {} not present", path.display());
        }
        Err(err) => log::warn!("openers: could not read {} ({err})", path.display()),
    }
}

/// Initialise the empty `OpenerStore` global, do the synchronous load,
/// then start the FS watcher so subsequent on-disk edits hot-reload.
pub fn init(fs: Arc<dyn Fs>, cx: &mut App) {
    cx.set_global(OpenerStore::default());
    apply_user_openers(cx);
    start_watcher(fs, cx);
}

/// Background watcher mirroring `codon_config::start_watcher` — same
/// 100 ms debounce from `fs::watch`, same trailing 50 ms yield so a
/// burst of writes coalesces into a single reload.
fn start_watcher(fs: Arc<dyn Fs>, cx: &mut App) {
    let Some(path) = user_openers_path() else {
        return;
    };
    let executor = cx.background_executor().clone();
    let (mut rx, watch_task) = settings::watch_config_file(&executor, fs, path);
    let mut saw_initial = false;
    cx.spawn(async move |cx| {
        while let Some(content) = rx.next().await {
            if !saw_initial {
                saw_initial = true;
                continue;
            }
            cx.update(|cx| match parse_doc(&content) {
                Ok(doc) => {
                    cx.update_global::<OpenerStore, _>(|store, _| store.install(doc));
                    log::debug!("openers: hot-reload applied");
                }
                Err(err) => log::warn!("openers: hot-reload failed ({err:#})"),
            });
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
        drop(watch_task);
    })
    .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(toml_str: &str) -> OpenersDoc {
        parse_doc(toml_str).expect("parse")
    }

    #[test]
    fn glob_row_matches_by_extension() {
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            glob = "*.png"
            cmd = "qlmanage -p {path}"
            description = "Quick Look"
            "#,
        ));
        let matches = store.matches_for(Path::new("/tmp/cat.png"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].description, "Quick Look");
    }

    #[test]
    fn brace_expansion_matches_multiple_extensions() {
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            glob = "*.{png,jpg}"
            cmd = "open {path}"
            "#,
        ));
        assert_eq!(store.matches_for(Path::new("/tmp/a.png")).len(), 1);
        assert_eq!(store.matches_for(Path::new("/tmp/a.jpg")).len(), 1);
        assert!(store.matches_for(Path::new("/tmp/a.txt")).is_empty());
    }

    #[test]
    fn mime_row_matches_known_extension() {
        // text/plain is what mime_guess returns for .txt — we rely on
        // that mapping to keep the test deterministic across platforms.
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            mime = "text/plain"
            cmd = "vim {path}"
            description = "Vim"
            "#,
        ));
        let matches = store.matches_for(Path::new("/tmp/notes.txt"));
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].description, "Vim");
    }

    #[test]
    fn rows_without_predicate_are_dropped() {
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            cmd = "echo hi"
            description = "No predicate"
            "#,
        ));
        assert!(store.all().is_empty());
    }

    #[test]
    fn empty_cmd_is_dropped() {
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            glob = "*.md"
            cmd = ""
            "#,
        ));
        assert!(store.all().is_empty());
    }

    #[test]
    fn invalid_glob_is_dropped_without_panicking() {
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            glob = "**unbalanced["
            cmd = "open {path}"
            "#,
        ));
        assert!(store.all().is_empty());
    }

    #[test]
    fn block_defaults_to_false() {
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            glob = "*.md"
            cmd = "open {path}"
            "#,
        ));
        assert_eq!(store.all().len(), 1);
        assert!(!store.all()[0].block);
    }

    #[test]
    fn block_true_round_trips() {
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            glob = "*.md"
            cmd = "less {path}"
            block = true
            "#,
        ));
        assert!(store.all()[0].block);
    }

    #[test]
    fn label_falls_back_to_cmd_without_description() {
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            glob = "*.zip"
            cmd = "unzip {path}"
            "#,
        ));
        assert_eq!(store.all()[0].label(), "unzip {path}");
    }

    #[test]
    fn ordering_preserved_across_reload() {
        let mut store = OpenerStore::default();
        store.install(doc(
            r#"
            [[opener]]
            glob = "*.png"
            cmd = "viewer-a {path}"
            description = "A"

            [[opener]]
            glob = "*.png"
            cmd = "viewer-b {path}"
            description = "B"
            "#,
        ));
        let matches = store.matches_for(Path::new("/tmp/x.png"));
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].description, "A");
        assert_eq!(matches[1].description, "B");
    }
}
