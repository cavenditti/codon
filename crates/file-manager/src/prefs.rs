//! Persisted file-manager preferences.
//!
//! Bundles the sort/display state that lives across launches:
//! sort mode + direction, per-entry line-mode column, gitignore
//! visibility, and preview-column fraction. Stored at
//! `~/.local/state/codon/file-manager-prefs.toml` (or wherever
//! `codon_config::codon_state_dir()` resolves to).

use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const FILE_NAME: &str = "file-manager-prefs.toml";

pub(crate) const PREVIEW_FRACTION_MIN: f32 = 0.10;
pub(crate) const PREVIEW_FRACTION_MAX: f32 = 0.80;
pub(crate) const PREVIEW_FRACTION_DEFAULT: f32 = 0.333;
pub(crate) const PREVIEW_FRACTION_STEP: f32 = 0.05;

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortMode {
    #[default]
    Name,
    Size,
    Mtime,
    Btime,
    Extension,
    Random,
    Natural,
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineMode {
    #[default]
    None,
    Size,
    Mtime,
    Permissions,
    Owner,
}

impl LineMode {
    pub fn next(self) -> Self {
        match self {
            LineMode::None => LineMode::Size,
            LineMode::Size => LineMode::Mtime,
            LineMode::Mtime => LineMode::Permissions,
            LineMode::Permissions => LineMode::Owner,
            LineMode::Owner => LineMode::None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FmPrefs {
    #[serde(default)]
    pub sort: SortMode,
    #[serde(default)]
    pub reverse: bool,
    #[serde(default = "default_line_mode")]
    pub line_mode: LineMode,
    #[serde(default = "default_show_gitignored")]
    pub show_gitignored: bool,
    #[serde(default = "default_preview_fraction")]
    pub preview_fraction: f32,
    /// Show the ranger-style info bar (perms / owner / size / mtime
    /// for the focused entry + totals) above the status line.
    #[serde(default = "default_true")]
    pub show_rich_info: bool,
    /// Show the contextual key-hints row under the status line.
    #[serde(default = "default_true")]
    pub show_help_bar: bool,
}

fn default_show_gitignored() -> bool {
    true
}

fn default_preview_fraction() -> f32 {
    PREVIEW_FRACTION_DEFAULT
}

/// Default meta column shows file sizes — denser and closer to ranger
/// than the old `None` default. Cycling `M` still rotates through the
/// other modes.
fn default_line_mode() -> LineMode {
    LineMode::Size
}

fn default_true() -> bool {
    true
}

impl Default for FmPrefs {
    fn default() -> Self {
        Self {
            sort: SortMode::default(),
            reverse: false,
            line_mode: default_line_mode(),
            show_gitignored: true,
            preview_fraction: PREVIEW_FRACTION_DEFAULT,
            show_rich_info: true,
            show_help_bar: true,
        }
    }
}

impl FmPrefs {
    pub fn load() -> Self {
        let Some(path) = Self::file_path() else {
            return Self::default();
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str::<FmPrefs>(&content) {
            Ok(mut p) => {
                p.preview_fraction = clamp_fraction(p.preview_fraction);
                p
            }
            Err(err) => {
                log::warn!(
                    "file-manager: ignoring malformed {} ({err})",
                    path.display()
                );
                Self::default()
            }
        }
    }

    pub fn set_sort(&mut self, mode: SortMode) {
        self.sort = mode;
        self.save();
    }

    pub fn set_reverse(&mut self, reverse: bool) {
        self.reverse = reverse;
        self.save();
    }

    pub fn set_line_mode(&mut self, mode: LineMode) {
        self.line_mode = mode;
        self.save();
    }

    pub fn set_show_gitignored(&mut self, value: bool) {
        self.show_gitignored = value;
        self.save();
    }

    pub fn set_preview_fraction(&mut self, value: f32) {
        self.preview_fraction = clamp_fraction(value);
        self.save();
    }

    pub fn set_show_rich_info(&mut self, value: bool) {
        self.show_rich_info = value;
        self.save();
    }

    pub fn set_show_help_bar(&mut self, value: bool) {
        self.show_help_bar = value;
        self.save();
    }

    fn save(&self) {
        let Some(path) = Self::file_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            if let Err(err) = std::fs::create_dir_all(parent) {
                log::warn!(
                    "file-manager: could not create {} ({err})",
                    parent.display()
                );
                return;
            }
        }
        let serialized = match toml::to_string_pretty(self) {
            Ok(s) => s,
            Err(err) => {
                log::warn!("file-manager: serialising prefs failed: {err}");
                return;
            }
        };
        if let Err(err) = std::fs::write(&path, serialized) {
            log::warn!(
                "file-manager: could not persist prefs to {} ({err})",
                path.display()
            );
        }
    }

    fn file_path() -> Option<PathBuf> {
        codon_config::codon_state_dir().map(|d| d.join(FILE_NAME))
    }
}

impl Global for FmPrefs {}

pub fn clamp_fraction(value: f32) -> f32 {
    if value.is_nan() {
        return PREVIEW_FRACTION_DEFAULT;
    }
    value.clamp(PREVIEW_FRACTION_MIN, PREVIEW_FRACTION_MAX)
}

pub fn init(cx: &mut App) {
    cx.set_global(FmPrefs::load());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_mode_cycle_wraps() {
        let mut m = LineMode::None;
        m = m.next();
        assert_eq!(m, LineMode::Size);
        m = m.next();
        assert_eq!(m, LineMode::Mtime);
        m = m.next();
        assert_eq!(m, LineMode::Permissions);
        m = m.next();
        assert_eq!(m, LineMode::Owner);
        m = m.next();
        assert_eq!(m, LineMode::None);
    }

    #[test]
    fn clamp_fraction_bounds() {
        assert_eq!(clamp_fraction(0.0), PREVIEW_FRACTION_MIN);
        assert_eq!(clamp_fraction(1.0), PREVIEW_FRACTION_MAX);
        assert_eq!(clamp_fraction(0.5), 0.5);
        assert_eq!(clamp_fraction(f32::NAN), PREVIEW_FRACTION_DEFAULT);
    }

    #[test]
    fn default_prefs_match_today() {
        let p = FmPrefs::default();
        assert_eq!(p.sort, SortMode::Name);
        assert!(!p.reverse);
        // Default flipped from `None` to `Size` so the meta column
        // reads dense out of the box (ranger-style).
        assert_eq!(p.line_mode, LineMode::Size);
        assert!(p.show_gitignored);
        assert!(p.show_rich_info);
        assert!(p.show_help_bar);
        assert!((p.preview_fraction - PREVIEW_FRACTION_DEFAULT).abs() < f32::EPSILON);
    }

    #[test]
    fn roundtrip_through_toml() {
        let p = FmPrefs {
            sort: SortMode::Size,
            reverse: true,
            line_mode: LineMode::Permissions,
            show_gitignored: false,
            preview_fraction: 0.5,
            show_rich_info: false,
            show_help_bar: false,
        };
        let s = toml::to_string_pretty(&p).expect("serialise");
        let parsed: FmPrefs = toml::from_str(&s).expect("parse");
        assert_eq!(parsed.sort, SortMode::Size);
        assert!(parsed.reverse);
        assert_eq!(parsed.line_mode, LineMode::Permissions);
        assert!(!parsed.show_gitignored);
        assert!(!parsed.show_rich_info);
        assert!(!parsed.show_help_bar);
        assert!((parsed.preview_fraction - 0.5).abs() < f32::EPSILON);
    }
}
