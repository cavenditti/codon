//! Unified codon configuration loader.
//!
//! Reads `~/.config/codon/codon.toml` and exposes:
//!
//! - the `[settings]` sub-table, translated to `serde_json::Value` shaped
//!   like Zed's [`settings_content::SettingsContent`], for hand-off to
//!   `settings::SettingsStore`.
//! - the `[bindings]` sub-table as a `toml::Value`, for hand-off to
//!   `codon-keymap`.
//!
//! See `README.md` (alongside this file) for the TOML schema mapping rules.

pub mod toml_to_json;

use gpui::App;

pub fn init(_cx: &mut App) {
    // Placeholder: subsequent tasks (`unified-config-config-crate`,
    // `unified-config-merge-keymap`, `unified-config-watch-reload`, …) wire
    // the actual loader and watcher here. Scaffolding-only commit keeps the
    // crate registered in the workspace without changing runtime behaviour.
}
