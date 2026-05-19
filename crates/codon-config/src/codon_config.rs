//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.
//!
//! Unified codon configuration loader.
//!
//! Reads `~/.config/codon/codon.toml`. Translates the `[settings]` sub-tree
//! to JSON shaped like Zed's `SettingsContent` and hands it to
//! `SettingsStore::set_user_settings`. The `[bindings]` sub-tree is exposed
//! via [`bindings_toml`] for hand-off to `codon-keymap`.
//!
//! See `README.md` next to this file for the TOML schema mapping rules.

pub mod migrate;
pub mod toml_to_json;
pub mod writeback;

pub use migrate::{MigrationOutcome, json_to_toml, migrate_if_needed};

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use fs::Fs;
use futures::StreamExt as _;
use gpui::{App, BorrowAppContext as _};
use settings::SettingsStore;

/// Resolve the codon config directory. Always `~/.config/codon` regardless
/// of platform convention — codon is a terminal-first multiplexer editor
/// where dotfile expectations dominate, so we deliberately diverge from
/// `dirs::config_dir()` (which returns `~/Library/Application Support` on
/// macOS). Honours `$XDG_CONFIG_HOME` first, then falls back to
/// `$HOME/.config`.
pub fn codon_config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(xdg).join("codon"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("codon"))
}

/// The on-disk location codon reads its unified config from. Resolved once
/// at startup; absence is not an error (defaults apply).
pub fn user_config_path() -> Option<PathBuf> {
    codon_config_dir().map(|d| d.join("codon.toml"))
}

/// Resolve codon's user state directory — `~/.local/state/codon` by
/// default, overridable via `$XDG_STATE_HOME`. Distinct from the config
/// dir: state holds machine-generated artefacts (bookmarks, history,
/// session caches) that a user typically does not check into dotfiles.
pub fn codon_state_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(xdg).join("codon"));
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state").join("codon"))
}

/// Read the user config from disk and apply the `[settings]` sub-tree to
/// the workspace `SettingsStore`. Idempotent — safe to call from a file
/// watcher on every change. Missing file is a no-op; parse errors are
/// logged and the previous settings stay active.
pub fn apply_user_config(cx: &mut App) {
    let Some(path) = user_config_path() else {
        log::debug!("codon-config: dirs::config_dir() unavailable; skipping load");
        return;
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => match apply_from_str(&content, cx) {
            Ok(()) => log::debug!("codon-config: loaded {}", path.display()),
            Err(err) => log::warn!(
                "codon-config: failed to apply {}: {err:#}",
                path.display()
            ),
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            log::debug!(
                "codon-config: {} not present; using defaults",
                path.display()
            );
        }
        Err(err) => {
            log::warn!(
                "codon-config: could not read {}: {err}",
                path.display()
            );
        }
    }
}

/// Apply a `codon.toml` document already in memory. Exposed for tests and
/// for the migration step (which writes a fresh document and then re-applies
/// it without round-tripping through disk).
pub fn apply_from_str(content: &str, cx: &mut App) -> Result<()> {
    let toml_doc: toml::Value = content
        .parse()
        .context("parsing codon.toml as TOML")?;

    if let Some(settings_table) = toml_doc.get("settings") {
        let json_value = toml_to_json::translate(settings_table);
        let json_string = serde_json::to_string(&json_value)
            .context("serialising translated settings to JSON")?;
        cx.update_global::<SettingsStore, _>(|store, cx| {
            let result = store.set_user_settings(&json_string, cx);
            match result.parse_status {
                settings::ParseStatus::Success | settings::ParseStatus::Unchanged => {}
                settings::ParseStatus::Failed { error } => {
                    log::warn!("codon-config: SettingsStore rejected settings: {error}");
                }
            }
        });
    }

    apply_window_chrome(&toml_doc, cx);

    Ok(())
}

/// Parsed `[diagnostics]` sub-tree. Codon's host (apps/codon) reads
/// this after `codon-config` loads the user's `codon.toml`, then
/// hands `render_trace_path` off to `file_manager::install_render_trace`.
/// This indirection keeps `codon-config` independent of `file-manager`
/// (the dependency only goes the other way).
#[derive(Default, Debug, Clone)]
pub struct DiagnosticsConfig {
    pub render_trace: bool,
    pub render_trace_path: Option<PathBuf>,
}

/// Read the `[diagnostics]` table from a codon.toml document already
/// in memory. Missing table / fields produce the [`Default`] config.
pub fn diagnostics_config(content: &str) -> DiagnosticsConfig {
    let Ok(toml_doc) = content.parse::<toml::Value>() else {
        return DiagnosticsConfig::default();
    };
    let Some(table) = toml_doc.get("diagnostics").and_then(|v| v.as_table()) else {
        return DiagnosticsConfig::default();
    };
    DiagnosticsConfig {
        render_trace: table
            .get("render_trace")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        render_trace_path: table
            .get("render_trace_path")
            .and_then(|v| v.as_str())
            .map(PathBuf::from),
    }
}

/// Read [`diagnostics_config`] straight from the on-disk
/// `codon.toml`. Returns the [`Default`] config when the file is
/// missing or unreadable; parse errors are logged at `warn` and the
/// default is returned.
pub fn diagnostics_config_from_disk() -> DiagnosticsConfig {
    let Some(path) = user_config_path() else {
        return DiagnosticsConfig::default();
    };
    match std::fs::read_to_string(&path) {
        Ok(content) => diagnostics_config(&content),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => DiagnosticsConfig::default(),
        Err(err) => {
            log::warn!(
                "codon-config: could not read {} for diagnostics: {err}",
                path.display()
            );
            DiagnosticsConfig::default()
        }
    }
}

/// Translate the `[window]` sub-tree of codon.toml to the
/// `platform_title_bar::WindowChromeConfig` global. Both fields default
/// to false (vanilla Zed behavior) so an absent `[window]` table or
/// missing keys leave the title bar fully draggable.
fn apply_window_chrome(toml_doc: &toml::Value, cx: &mut App) {
    let table = toml_doc.get("window").and_then(|v| v.as_table());
    let read_bool = |key: &str| -> bool {
        table
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    let config = platform_title_bar::WindowChromeConfig {
        disable_drag: read_bool("disable_drag"),
        disable_double_click_zoom: read_bool("disable_double_click_zoom"),
    };
    cx.set_global(config);
}

/// The raw `[bindings]` sub-tree as a `toml::Value`. Codon-keymap consumes
/// this via [`load_bindings`] once the merge-keymap task wires the path.
pub fn bindings_toml(content: &str) -> Result<Option<toml::Value>> {
    let toml_doc: toml::Value = content
        .parse()
        .context("parsing codon.toml as TOML")?;
    Ok(toml_doc.get("bindings").cloned())
}

pub fn init(cx: &mut App) {
    match migrate::migrate_if_needed() {
        Ok(MigrationOutcome::Migrated {
            from_zed_settings,
            from_codon_keymap,
        }) => log::info!(
            "codon-config: wrote codon.toml from legacy files \
             (zed settings.json={from_zed_settings}, keymap.toml={from_codon_keymap})",
        ),
        Ok(MigrationOutcome::Scaffolded) => {
            log::info!("codon-config: wrote starter codon.toml on first launch");
        }
        Ok(MigrationOutcome::Already) => {}
        Err(err) => log::warn!("codon-config: migration failed: {err:#}"),
    }
    apply_user_config(cx);
    // Reroute Zed's in-app settings editor through codon.toml — every
    // settings_ui edit reads / writes the [settings] sub-tree instead of
    // ~/.config/zed/settings.json.
    writeback::install();
}

/// Start a background watcher on `~/.config/codon/codon.toml`. On every
/// content change, re-applies the `[settings]` sub-tree via
/// [`apply_from_str`] and invokes `on_keymap_reload` so the host app can
/// re-bind keymaps. The 100 ms debounce comes from `fs::watch` itself.
///
/// The watcher Task is detached — it lives for the App's lifetime. Calling
/// `start_watcher` more than once spawns multiple watchers; the host app
/// should call it exactly once during init.
pub fn start_watcher<F>(fs: Arc<dyn Fs>, cx: &mut App, on_keymap_reload: F)
where
    F: Fn(&mut App) + 'static,
{
    let Some(path) = user_config_path() else {
        return;
    };
    let executor = cx.background_executor().clone();
    let (mut rx, watch_task) = settings::watch_config_file(&executor, fs, path);
    // First message arrives immediately (initial load) — we already applied
    // settings during init(), so drop it without re-applying to avoid
    // double-firing the keymap reload.
    let mut saw_initial = false;
    cx.spawn(async move |cx| {
        while let Some(content) = rx.next().await {
            if !saw_initial {
                saw_initial = true;
                continue;
            }
            cx.update(|cx| {
                if let Err(err) = apply_from_str(&content, cx) {
                    log::warn!("codon-config: live reload failed: {err:#}");
                }
                on_keymap_reload(cx);
            });
            // Yield briefly so a quick burst of fs events coalesces into one
            // reload — the watch_config_file utility doesn't debounce on
            // its own.
            cx.background_executor()
                .timer(Duration::from_millis(50))
                .await;
        }
        drop(watch_task);
    })
    .detach();
}
