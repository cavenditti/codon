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

pub use migrate::{MigrationOutcome, migrate_if_needed};

use std::{path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context as _, Result};
use fs::Fs;
use futures::StreamExt as _;
use gpui::{App, AppContext as _, BorrowAppContext as _};
use settings::SettingsStore;

/// The on-disk location codon reads its unified config from. Resolved once
/// at startup; absence is not an error (defaults apply).
pub fn user_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("codon").join("codon.toml"))
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

    Ok(())
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
        Ok(MigrationOutcome::Already | MigrationOutcome::NoLegacy) => {}
        Err(err) => log::warn!("codon-config: migration failed: {err:#}"),
    }
    apply_user_config(cx);
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
    let (mut rx, watch_task) = settings::watch_config_file(&executor, fs, path.clone());
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
