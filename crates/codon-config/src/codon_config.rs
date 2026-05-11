//! Unified codon configuration loader.
//!
//! Reads `~/.config/codon/codon.toml`. Translates the `[settings]` sub-tree
//! to JSON shaped like Zed's `SettingsContent` and hands it to
//! `SettingsStore::set_user_settings`. The `[bindings]` sub-tree is exposed
//! via [`bindings_toml`] for hand-off to `codon-keymap`.
//!
//! See `README.md` next to this file for the TOML schema mapping rules.

pub mod toml_to_json;

use std::path::PathBuf;

use anyhow::{Context as _, Result};
use gpui::{App, BorrowAppContext as _};
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
    apply_user_config(cx);
}
