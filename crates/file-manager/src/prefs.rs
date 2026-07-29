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

use crate::debounced_writer::DebouncedWriter;

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

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineMode {
    None,
    #[default]
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

    /// Stable index into a per-variant array. Used by the entry-label
    /// cache to look up the precomputed `SharedString` for whichever
    /// mode is active without a match-on-every-render.
    pub fn idx(self) -> usize {
        match self {
            LineMode::None => 0,
            LineMode::Size => 1,
            LineMode::Mtime => 2,
            LineMode::Permissions => 3,
            LineMode::Owner => 4,
        }
    }

    pub const COUNT: usize = 5;
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
    /// Opt into the phase-17 custom-render pipeline
    /// (`REQ:codon/fm-render`). Off by default while the harness
    /// stabilises — flip to `true` to exercise the
    /// `FmRowElement` / `FmColumnElement` paint path. Reversible
    /// escape hatch.
    #[serde(default)]
    pub custom_render: bool,
    #[serde(skip)]
    writer: Option<DebouncedWriter<FmPrefsSnapshot>>,
}

#[derive(Clone, Debug, Serialize)]
struct FmPrefsSnapshot {
    sort: SortMode,
    reverse: bool,
    line_mode: LineMode,
    show_gitignored: bool,
    preview_fraction: f32,
    custom_render: bool,
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

impl Default for FmPrefs {
    fn default() -> Self {
        Self {
            sort: SortMode::default(),
            reverse: false,
            line_mode: default_line_mode(),
            show_gitignored: true,
            preview_fraction: PREVIEW_FRACTION_DEFAULT,
            custom_render: false,
            writer: None,
        }
    }
}

impl FmPrefs {
    pub fn load() -> Self {
        let Some(path) = Self::file_path() else {
            return Self::default();
        };
        let mut prefs = match std::fs::read_to_string(&path) {
            Ok(content) => match toml::from_str::<FmPrefs>(&content) {
                Ok(mut prefs) => {
                    prefs.preview_fraction = clamp_fraction(prefs.preview_fraction);
                    prefs
                }
                Err(err) => {
                    log::warn!(
                        "file-manager: ignoring malformed {} ({err})",
                        path.display()
                    );
                    Self::default()
                }
            },
            Err(_) => Self::default(),
        };
        prefs.writer = Some(DebouncedWriter::toml(path, "prefs"));
        prefs
    }

    fn snapshot(&self) -> FmPrefsSnapshot {
        FmPrefsSnapshot {
            sort: self.sort,
            reverse: self.reverse,
            line_mode: self.line_mode,
            show_gitignored: self.show_gitignored,
            preview_fraction: self.preview_fraction,
            custom_render: self.custom_render,
        }
    }

    pub(crate) fn flush(&self) {
        if let Some(writer) = &self.writer {
            writer.flush();
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

    fn save(&self) {
        if let Some(writer) = &self.writer {
            writer.schedule(self.snapshot());
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
        assert!((p.preview_fraction - PREVIEW_FRACTION_DEFAULT).abs() < f32::EPSILON);
        assert!(!p.custom_render);
    }

    #[test]
    fn roundtrip_through_toml() {
        let p = FmPrefs {
            sort: SortMode::Size,
            reverse: true,
            line_mode: LineMode::Permissions,
            show_gitignored: false,
            preview_fraction: 0.5,
            custom_render: true,
            writer: None,
        };
        let s = toml::to_string_pretty(&p).expect("serialise");
        let parsed: FmPrefs = toml::from_str(&s).expect("parse");
        assert_eq!(parsed.sort, SortMode::Size);
        assert!(parsed.reverse);
        assert_eq!(parsed.line_mode, LineMode::Permissions);
        assert!(!parsed.show_gitignored);
        assert!((parsed.preview_fraction - 0.5).abs() < f32::EPSILON);
        assert!(parsed.custom_render);
    }
}
