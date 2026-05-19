//! Settings for the codon which-key overlay.
//!
//! Read from `[which_key]` in `~/.config/codon/codon.toml`. The settings
//! cover three of the four knobs documented in
//! `REQ:codon/which-key-overlay`; `flip_threshold` (clause `c-auto-flip`)
//! is added by the follow-up `phase-16/which-key-auto-flip` commit.
//!
//! - `enabled` — global on/off switch (default `true`).
//! - `delay_ms` — milliseconds to wait after the prefix is held before
//!   the HUD appears (default `250`).
//! - `min_column_width` — minimum width in pixels per column when
//!   computing the multi-column layout (default `240`).
//!
//! The load is best-effort: a missing file, an unparsable file, or a
//! missing `[which_key]` table all fall back to the defaults below.
//! Out-of-range values are clamped rather than rejecting the whole
//! settings set.

use std::path::PathBuf;

use codon_config::codon_config_dir;
use serde::Deserialize;

/// Default delay before the HUD appears, in milliseconds. Matches the
/// codon overrides applied to vendored Zed's `which_key` settings in
/// `apps/codon/src/main.rs`.
pub const DEFAULT_DELAY_MS: u64 = 250;

/// Default minimum column width in pixels. Wide enough for most action
/// names (`codon_session::WindowGoto`) without truncating, narrow enough
/// to keep 3-4 columns on a typical pane.
pub const DEFAULT_MIN_COLUMN_WIDTH: f32 = 240.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CodonWhichKeySettings {
    pub enabled: bool,
    pub delay_ms: u64,
    pub min_column_width: f32,
}

impl Default for CodonWhichKeySettings {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_ms: DEFAULT_DELAY_MS,
            min_column_width: DEFAULT_MIN_COLUMN_WIDTH,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct CodonTomlFile {
    #[serde(default)]
    which_key: Option<WhichKeyTable>,
}

#[derive(Debug, Default, Deserialize)]
struct WhichKeyTable {
    enabled: Option<bool>,
    delay_ms: Option<u64>,
    min_column_width: Option<f32>,
}

/// Best-effort load of `[which_key]` from `~/.config/codon/codon.toml`.
///
/// Reads on every call — the overlay reads settings infrequently
/// (once per HUD open), so we don't bother caching. A missing config
/// file or missing `[which_key]` table both return [`CodonWhichKeySettings::default`].
pub fn load() -> CodonWhichKeySettings {
    let mut settings = CodonWhichKeySettings::default();
    let Some(path) = config_path() else {
        return settings;
    };
    let Ok(content) = std::fs::read_to_string(&path) else {
        return settings;
    };
    let parsed: CodonTomlFile = match toml::from_str(&content) {
        Ok(parsed) => parsed,
        Err(err) => {
            log::warn!(
                "codon-which-key: ignoring unparsable {}: {err}",
                path.display()
            );
            return settings;
        }
    };
    let Some(table) = parsed.which_key else {
        return settings;
    };
    if let Some(enabled) = table.enabled {
        settings.enabled = enabled;
    }
    if let Some(delay_ms) = table.delay_ms {
        settings.delay_ms = delay_ms;
    }
    if let Some(min_column_width) = table.min_column_width {
        // Clamp silently — a zero / negative width would divide-by-zero
        // in `compute_columns`; capping at one column for absurdly
        // large values is also fine.
        settings.min_column_width = min_column_width.max(60.0);
    }
    settings
}

fn config_path() -> Option<PathBuf> {
    codon_config_dir().map(|d| d.join("codon.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(content: &str) -> CodonWhichKeySettings {
        let parsed: CodonTomlFile = toml::from_str(content).expect("parse");
        let mut settings = CodonWhichKeySettings::default();
        if let Some(table) = parsed.which_key {
            if let Some(enabled) = table.enabled {
                settings.enabled = enabled;
            }
            if let Some(delay_ms) = table.delay_ms {
                settings.delay_ms = delay_ms;
            }
            if let Some(width) = table.min_column_width {
                settings.min_column_width = width.max(60.0);
            }
        }
        settings
    }

    #[test]
    fn defaults_when_no_table() {
        let settings = parse("");
        assert_eq!(settings, CodonWhichKeySettings::default());
    }

    #[test]
    fn parses_all_fields() {
        let settings = parse(
            r#"
            [which_key]
            enabled = false
            delay_ms = 800
            min_column_width = 300
            "#,
        );
        assert!(!settings.enabled);
        assert_eq!(settings.delay_ms, 800);
        assert!((settings.min_column_width - 300.0).abs() < f32::EPSILON);
    }

    #[test]
    fn clamps_min_column_width_low() {
        let settings = parse(
            r#"
            [which_key]
            min_column_width = 10
            "#,
        );
        assert!((settings.min_column_width - 60.0).abs() < f32::EPSILON);
    }
}
