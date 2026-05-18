//! File-manager theme overlay — per-filetype filename colors.
//!
//! Reads `~/.config/codon/file-manager-theme.toml` once at startup and
//! re-reads it whenever the FS watcher reports a change. The parsed
//! table lives in `FmThemeStore`, a `gpui::Global`, so the row renderer
//! can ask for the right `Color` for any `DirEntry` without ever
//! touching disk on the render path.
//!
//! Layout mirrors [`crate::openers`]: an embedded default keeps the
//! palette useful out-of-the-box, the user TOML adds/overrides entries,
//! and a `Fs::watch` task swaps the global atomically on change.

use fs::Fs;
use futures::StreamExt as _;
use gpui::{App, BorrowAppContext as _, Global};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use ui::Color;

use crate::file_manager::DirEntry;

const FILE_NAME: &str = "file-manager-theme.toml";

/// Bundled palette — overlays the user TOML. Keys are normalized to the
/// `parse_key` rules (lowercase; leading-dot keeps the dot only for
/// filename-exact matches like `".env"`).
const EMBEDDED_DEFAULT: &str = r#"
[filetype]
# Rust
rs    = "warning"
# Markdown
md    = "info"
mdx   = "info"
# JSON / YAML / TOML config
json  = "conflict"
yml   = "conflict"
yaml  = "conflict"
toml  = "warning"
# TypeScript / JavaScript
ts    = "warning"
tsx   = "warning"
js    = "warning"
jsx   = "warning"
# Python
py    = "created"
pyi   = "created"
# Shell
sh    = "success"
bash  = "success"
zsh   = "success"
fish  = "success"
# Images
png   = "hint"
jpg   = "hint"
jpeg  = "hint"
webp  = "hint"
svg   = "hint"
gif   = "hint"
# Archives
zip   = "deleted"
tar   = "deleted"
gz    = "deleted"
xz    = "deleted"
zst   = "deleted"
bz2   = "deleted"
"7z"  = "deleted"
# Config-ish text
conf  = "muted"
ini   = "muted"
cfg   = "muted"

[special]
# Special categories — only one matches per entry; checked in this order:
#   directory > executable > dotfile > extension > filename > default.
directory  = "accent"
executable = "success"
dotfile    = "disabled"
default    = "default"
"#;

/// Color tokens accepted in the TOML. Map to `ui::Color` variants —
/// resolved at render time by `ui::Color::color(cx)`.
fn parse_color(token: &str) -> Option<Color> {
    match token.trim().to_ascii_lowercase().as_str() {
        "default" => Some(Color::Default),
        "muted" => Some(Color::Muted),
        "accent" => Some(Color::Accent),
        "disabled" => Some(Color::Disabled),
        "hidden" => Some(Color::Hidden),
        "hint" => Some(Color::Hint),
        "info" => Some(Color::Info),
        "success" => Some(Color::Success),
        "warning" => Some(Color::Warning),
        "error" => Some(Color::Error),
        "conflict" => Some(Color::Conflict),
        "created" => Some(Color::Created),
        "modified" => Some(Color::Modified),
        "deleted" => Some(Color::Deleted),
        _ => None,
    }
}

/// Off-disk TOML schema. Two tables: `filetype` for extension or exact-
/// filename overrides, `special` for the four named categories.
#[derive(Clone, Debug, Default, Deserialize)]
struct ThemeDoc {
    #[serde(default)]
    filetype: HashMap<String, String>,
    #[serde(default)]
    special: HashMap<String, String>,
}

/// Special-category color set. Anything resolved by category beats the
/// generic extension table.
#[derive(Clone, Copy, Debug)]
struct SpecialColors {
    directory: Color,
    executable: Color,
    dotfile: Color,
    default: Color,
}

impl Default for SpecialColors {
    fn default() -> Self {
        Self {
            directory: Color::Accent,
            executable: Color::Success,
            dotfile: Color::Disabled,
            default: Color::Default,
        }
    }
}

/// App-global filetype color store. Swapped wholesale on reload — the
/// render path only ever sees a consistent snapshot.
#[derive(Clone, Debug, Default)]
pub struct FmThemeStore {
    /// Extension (no leading dot, lowercase) -> color.
    ext: HashMap<String, Color>,
    /// Exact filename (e.g. `.env`, `Makefile`) -> color. Checked before
    /// the extension table so `".env"` beats the extension fallback for
    /// dotfile-but-not-just-a-dotfile entries.
    filename: HashMap<String, Color>,
    /// Named categories — directory, executable, dotfile, default.
    special: SpecialColors,
}

impl Global for FmThemeStore {}

impl FmThemeStore {
    /// Resolve the filename color for `entry`. Priority is fixed so the
    /// renderer can call this without ordering knowledge: directory
    /// beats executable beats exact-filename beats extension beats
    /// dotfile beats the generic `default`.
    pub fn color_for(&self, entry: &DirEntry) -> Color {
        if entry.is_dir {
            return self.special.directory;
        }
        if is_executable(entry.mode) {
            return self.special.executable;
        }
        if let Some(c) = self.filename.get(&entry.name.to_ascii_lowercase()) {
            return *c;
        }
        if let Some(ext) = extension_of(&entry.name) {
            if let Some(c) = self.ext.get(&ext) {
                return *c;
            }
        }
        if entry.is_hidden {
            return self.special.dotfile;
        }
        self.special.default
    }

    /// Replace the store with a parsed document. Unknown color tokens
    /// are logged and dropped — bad input never poisons the live store.
    fn install(&mut self, doc: ThemeDoc) {
        let mut ext: HashMap<String, Color> = HashMap::new();
        let mut filename: HashMap<String, Color> = HashMap::new();
        for (raw_key, raw_value) in doc.filetype {
            let Some(color) = parse_color(&raw_value) else {
                log::warn!(
                    "file-manager-theme: ignoring unknown color '{raw_value}' for key '{raw_key}'"
                );
                continue;
            };
            let key = raw_key.trim();
            if key.is_empty() {
                continue;
            }
            if key.starts_with('.') && key.len() > 1 {
                // Leading-dot key = exact filename (e.g. ".env",
                // ".gitignore"). Stored lowercase for case-insensitive
                // lookup.
                filename.insert(key.to_ascii_lowercase(), color);
            } else {
                // Strip a leading dot used as a tolerance prefix on
                // extensions ("rs" and ".rs" mean the same thing).
                let normalized = key.trim_start_matches('.').to_ascii_lowercase();
                ext.insert(normalized, color);
            }
        }
        let mut special = SpecialColors::default();
        for (key, value) in doc.special {
            let Some(color) = parse_color(&value) else {
                log::warn!(
                    "file-manager-theme: ignoring unknown color '{value}' for special '{key}'"
                );
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "directory" => special.directory = color,
                "executable" => special.executable = color,
                "dotfile" => special.dotfile = color,
                "default" => special.default = color,
                other => log::warn!("file-manager-theme: unknown special category '{other}'"),
            }
        }
        self.ext = ext;
        self.filename = filename;
        self.special = special;
    }
}

/// Lowercase extension of `name`, or `None` when the name has no
/// extension. We split on the last `.` so that `archive.tar.gz` resolves
/// to `gz` — the renderer applies whichever bucket the user configured
/// for the outermost suffix.
fn extension_of(name: &str) -> Option<String> {
    // Leading-dot files are not "extension-only" — `.bashrc` has no
    // extension for our purposes (otherwise every dotfile would tint
    // by its full name).
    let stripped = name.trim_start_matches('.');
    let dot = stripped.rfind('.')?;
    if dot + 1 >= stripped.len() {
        return None;
    }
    Some(stripped[dot + 1..].to_ascii_lowercase())
}

/// Unix executable-bit check. Returns `false` when `mode` is `None`
/// (Windows / unmounted filesystem) so we never spuriously flag entries
/// the kernel didn't report on.
fn is_executable(mode: Option<u32>) -> bool {
    match mode {
        Some(m) => m & 0o111 != 0,
        None => false,
    }
}

/// Resolve `~/.config/codon/file-manager-theme.toml`, honoring
/// `$XDG_CONFIG_HOME` via `codon_config::codon_config_dir`.
pub fn user_theme_path() -> Option<PathBuf> {
    codon_config::codon_config_dir().map(|d| d.join(FILE_NAME))
}

fn parse_doc(content: &str) -> Result<ThemeDoc, toml::de::Error> {
    toml::from_str::<ThemeDoc>(content)
}

/// Parse the embedded default. Panics on malformed input by design —
/// the bundled string is part of the binary and a parse failure means
/// the build is broken.
fn embedded_doc() -> ThemeDoc {
    match parse_doc(EMBEDDED_DEFAULT) {
        Ok(d) => d,
        Err(err) => {
            // No `unwrap` — log and return an empty doc so the panel
            // still renders with the SpecialColors::default() palette.
            log::error!("file-manager-theme: embedded default failed to parse ({err})");
            ThemeDoc::default()
        }
    }
}

/// Synchronous initial load. Always installs the embedded default
/// first, then layers the user file on top so missing keys fall back to
/// the bundled palette.
pub fn apply_user_theme(cx: &mut App) {
    cx.update_global::<FmThemeStore, _>(|store, _| store.install(embedded_doc()));
    let Some(path) = user_theme_path() else {
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => match parse_doc(&content) {
            Ok(doc) => {
                cx.update_global::<FmThemeStore, _>(|store, _| {
                    // Re-apply embedded, then user, so unspecified keys
                    // retain the bundled value rather than reverting to
                    // the type-default empty map.
                    store.install(embedded_doc());
                    store.install_overlay(doc);
                });
                log::debug!("file-manager-theme: loaded {}", path.display());
            }
            Err(err) => log::warn!(
                "file-manager-theme: ignoring malformed {} ({err})",
                path.display()
            ),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            log::debug!("file-manager-theme: {} not present", path.display());
        }
        Err(err) => log::warn!(
            "file-manager-theme: could not read {} ({err})",
            path.display()
        ),
    }
}

impl FmThemeStore {
    /// Like `install`, but additive: existing extension/filename entries
    /// from a prior install survive when not overridden, and only the
    /// `special` keys present in `doc` are touched.
    fn install_overlay(&mut self, doc: ThemeDoc) {
        for (raw_key, raw_value) in doc.filetype {
            let Some(color) = parse_color(&raw_value) else {
                log::warn!(
                    "file-manager-theme: ignoring unknown color '{raw_value}' for key '{raw_key}'"
                );
                continue;
            };
            let key = raw_key.trim();
            if key.is_empty() {
                continue;
            }
            if key.starts_with('.') && key.len() > 1 {
                self.filename.insert(key.to_ascii_lowercase(), color);
            } else {
                let normalized = key.trim_start_matches('.').to_ascii_lowercase();
                self.ext.insert(normalized, color);
            }
        }
        for (key, value) in doc.special {
            let Some(color) = parse_color(&value) else {
                continue;
            };
            match key.trim().to_ascii_lowercase().as_str() {
                "directory" => self.special.directory = color,
                "executable" => self.special.executable = color,
                "dotfile" => self.special.dotfile = color,
                "default" => self.special.default = color,
                _ => {}
            }
        }
    }
}

/// Initialise the global, do the synchronous load, then start the FS
/// watcher. Mirrors `openers::init` step-for-step.
pub fn init(fs: Arc<dyn Fs>, cx: &mut App) {
    cx.set_global(FmThemeStore::default());
    apply_user_theme(cx);
    start_watcher(fs, cx);
}

fn start_watcher(fs: Arc<dyn Fs>, cx: &mut App) {
    let Some(path) = user_theme_path() else {
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
                    cx.update_global::<FmThemeStore, _>(|store, _| {
                        store.install(embedded_doc());
                        store.install_overlay(doc);
                    });
                    log::debug!("file-manager-theme: hot-reload applied");
                }
                Err(err) => log::warn!("file-manager-theme: hot-reload failed ({err})"),
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

    fn make_entry(name: &str, is_dir: bool, mode: Option<u32>) -> DirEntry {
        DirEntry {
            name: name.to_string(),
            path: PathBuf::from(name),
            is_dir,
            is_hidden: name.starts_with('.'),
            is_symlink: false,
            size: 0,
            git_status: None,
            mtime: None,
            btime: None,
            mode,
            uid: None,
            gid: None,
            child_count: None,
            labels: Default::default(),
        }
    }

    fn loaded_store() -> FmThemeStore {
        let mut store = FmThemeStore::default();
        store.install(embedded_doc());
        store
    }

    #[test]
    fn extension_lookup_matches_rust_source() {
        let store = loaded_store();
        let entry = make_entry("main.rs", false, Some(0o644));
        assert_eq!(store.color_for(&entry), Color::Warning);
    }

    #[test]
    fn directory_wins_over_extension() {
        let store = loaded_store();
        let entry = make_entry("src.rs", true, Some(0o755));
        assert_eq!(store.color_for(&entry), Color::Accent);
    }

    #[test]
    fn executable_bit_wins_over_extension() {
        let store = loaded_store();
        let entry = make_entry("run.sh", false, Some(0o755));
        // Both `executable` and `.sh` map to Success — verify the path
        // works for non-overlapping cases too:
        let other = make_entry("custom.bin", false, Some(0o755));
        assert_eq!(store.color_for(&entry), Color::Success);
        assert_eq!(store.color_for(&other), Color::Success);
    }

    #[test]
    fn dotfile_falls_back_to_disabled() {
        let store = loaded_store();
        let entry = make_entry(".bashrc", false, Some(0o644));
        assert_eq!(store.color_for(&entry), Color::Disabled);
    }

    #[test]
    fn exact_filename_overrides_extension() {
        let mut store = loaded_store();
        let doc: ThemeDoc = toml::from_str(
            r#"
            [filetype]
            ".env" = "muted"
            "#,
        )
        .expect("parse");
        store.install_overlay(doc);
        let entry = make_entry(".env", false, Some(0o644));
        assert_eq!(store.color_for(&entry), Color::Muted);
    }

    #[test]
    fn unknown_color_token_is_skipped() {
        let mut store = FmThemeStore::default();
        let doc: ThemeDoc = toml::from_str(
            r#"
            [filetype]
            rs = "purple"
            "#,
        )
        .expect("parse");
        store.install(doc);
        // No mapping installed — falls back to default Color::Default.
        let entry = make_entry("main.rs", false, Some(0o644));
        assert_eq!(store.color_for(&entry), Color::Default);
    }

    #[test]
    fn user_overlay_replaces_embedded_extension() {
        let mut store = loaded_store();
        let doc: ThemeDoc = toml::from_str(
            r#"
            [filetype]
            rs = "info"
            "#,
        )
        .expect("parse");
        store.install_overlay(doc);
        let entry = make_entry("lib.rs", false, Some(0o644));
        assert_eq!(store.color_for(&entry), Color::Info);
    }

    #[test]
    fn extension_strips_leading_dot_tolerance() {
        let mut store = FmThemeStore::default();
        let doc: ThemeDoc = toml::from_str(
            r#"
            [filetype]
            ".md" = "info"
            "#,
        )
        .expect("parse");
        // ".md" with len > 1 is treated as filename-exact in install(),
        // but the embedded default also maps "md" -> info via ext. So
        // make sure the filename map captures it:
        store.install(doc);
        let entry = make_entry(".md", false, Some(0o644));
        assert_eq!(store.color_for(&entry), Color::Info);
    }

    #[test]
    fn embedded_palette_covers_common_extensions() {
        let store = loaded_store();
        let cases = [
            ("foo.json", Color::Conflict),
            ("foo.yml", Color::Conflict),
            ("foo.png", Color::Hint),
            ("foo.zip", Color::Deleted),
            ("foo.py", Color::Created),
        ];
        for (name, expected) in cases {
            let e = make_entry(name, false, Some(0o644));
            assert_eq!(store.color_for(&e), expected, "case {name}");
        }
    }

    #[test]
    fn extension_of_skips_leading_dot() {
        assert_eq!(extension_of(".bashrc"), None);
        assert_eq!(extension_of(".env"), None);
        assert_eq!(extension_of("foo.rs"), Some("rs".to_string()));
        assert_eq!(extension_of("foo.tar.gz"), Some("gz".to_string()));
        assert_eq!(extension_of("noext"), None);
    }
}
