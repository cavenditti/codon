//! In-app settings editor write-back.
//!
//! Zed's `SettingsStore::update_settings_file` flow (used by every
//! `settings_ui` page) is JSON-text-in / JSON-text-out: it reads
//! `~/.config/zed/settings.json`, hands the text to a tree-sitter-JSON
//! mutator, writes the result back, then re-applies via
//! `set_user_settings`. With codon's unified config we need the same
//! JSON-text round-trip but anchored on the `[settings]` sub-tree of
//! `~/.config/codon/codon.toml` — so the in-app editor edits the same
//! file the user reads.
//!
//! ## Approach (splice)
//!
//! Tree-sitter TOML would preserve comments on edited keys but is a
//! heavy dep. Instead we splice: locate the byte range that the
//! `[settings]` sub-tree occupies inside `codon.toml`, replace that
//! range with the freshly-serialised TOML rendering of the updated
//! JSON, leave every byte outside the range (notably `[bindings.*]`,
//! the header, and any user comments outside `[settings]`) untouched.
//!
//! Trade-off documented in `crates/codon-config/README.md`: comments
//! *inside* the `[settings]` sub-tree are dropped when the editor
//! writes a change. Comments anywhere else in the file are preserved.

use std::{fs as stdfs, io::ErrorKind, sync::Arc};

use anyhow::{Context as _, Result};
use fs::Fs;
use futures::FutureExt;
use serde_json::Value;
use settings::{UserSettingsIoOverride, set_user_settings_io_override};

use crate::{json_to_toml, toml_to_json, user_config_path};

/// Register the codon-side I/O hook against [`SettingsStore`]. Idempotent
/// — re-calling replaces the previous override.
pub fn install() {
    let hook = UserSettingsIoOverride {
        read: Arc::new(|fs: Arc<dyn Fs>| async move { read_settings(fs).await }.boxed_local()),
        write: Arc::new(|fs: Arc<dyn Fs>, new_text: String| {
            async move { write_settings(fs, new_text).await }.boxed_local()
        }),
    };
    set_user_settings_io_override(Some(hook));
}

async fn read_settings(_fs: Arc<dyn Fs>) -> Result<String> {
    let path = user_config_path().context("dirs::config_dir() unavailable")?;
    let content = match stdfs::read_to_string(&path) {
        Ok(content) => content,
        Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(err).with_context(|| format!("reading {}", path.display()));
        }
    };
    let toml_doc: toml::Value = if content.is_empty() {
        toml::Value::Table(toml::value::Table::new())
    } else {
        content
            .parse()
            .with_context(|| format!("parsing {} as TOML", path.display()))?
    };
    let settings_table = toml_doc
        .as_table()
        .and_then(|t| t.get("settings"))
        .cloned()
        .unwrap_or_else(|| toml::Value::Table(toml::value::Table::new()));
    let json_value = toml_to_json::translate(&settings_table);
    let json_text = serde_json::to_string_pretty(&json_value)
        .context("serialising [settings] to JSON for in-app editor")?;
    Ok(json_text)
}

async fn write_settings(_fs: Arc<dyn Fs>, new_json: String) -> Result<()> {
    let path = user_config_path().context("dirs::config_dir() unavailable")?;
    if let Some(parent) = path.parent() {
        stdfs::create_dir_all(parent)
            .with_context(|| format!("creating {} for codon.toml", parent.display()))?;
    }
    let existing = match stdfs::read_to_string(&path) {
        Ok(s) => s,
        Err(err) if err.kind() == ErrorKind::NotFound => String::new(),
        Err(err) => {
            return Err(err).with_context(|| format!("reading {}", path.display()));
        }
    };
    let new_settings_value: Value = serde_json::from_str(&new_json)
        .context("parsing JSON returned by settings_ui edit")?;
    let new_settings_toml = json_to_toml(&new_settings_value);

    // Serialise just the new [settings] sub-tree (with the `settings.`
    // prefix on every key) so the line-block looks identical to what a
    // user would write by hand.
    let mut wrapper = toml::value::Table::new();
    wrapper.insert("settings".to_string(), new_settings_toml);
    let mut serialised = toml::to_string_pretty(&wrapper)
        .context("serialising updated [settings] to TOML")?;
    if !serialised.ends_with('\n') {
        serialised.push('\n');
    }

    let merged = splice_settings_block(&existing, &serialised);
    stdfs::write(&path, merged.as_bytes())
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Replace the contiguous range that the `[settings]` (and `[settings.*]`)
/// sections occupy in `existing` with `replacement`. If no `[settings]`
/// section exists, append the replacement (prefixed with a blank line)
/// to the end of the file.
fn splice_settings_block(existing: &str, replacement: &str) -> String {
    let (start, end) = match find_settings_range(existing) {
        Some(range) => range,
        None => {
            let mut out = existing.to_string();
            if !out.is_empty() && !out.ends_with("\n\n") {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
            out.push_str(replacement);
            return out;
        }
    };
    let mut out = String::with_capacity(existing.len() + replacement.len());
    out.push_str(&existing[..start]);
    out.push_str(replacement);
    out.push_str(&existing[end..]);
    out
}

/// Find the byte range occupied by `[settings]` and every `[settings.*]`
/// child table in `content`. Returns `None` if no such section exists.
fn find_settings_range(content: &str) -> Option<(usize, usize)> {
    let mut start: Option<usize> = None;
    let mut end: Option<usize> = None;
    let mut cursor = 0;
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let is_settings_header = trimmed.starts_with("[settings]")
            || trimmed.starts_with("[settings.")
            || trimmed.starts_with("[[settings.")
            || trimmed.starts_with("[[settings]]");
        let is_other_header = !is_settings_header
            && trimmed.starts_with('[')
            && !trimmed.starts_with("[[")
            && !trimmed.starts_with("[ ");
        let is_other_array_header = !is_settings_header && trimmed.starts_with("[[");

        if is_settings_header && start.is_none() {
            start = Some(cursor);
        }
        if start.is_some() && (is_other_header || is_other_array_header) {
            end = Some(cursor);
            break;
        }
        cursor += line.len();
    }
    let start = start?;
    Some((start, end.unwrap_or(content.len())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splices_into_middle_of_file() {
        let existing = "\
# header comment
[settings]
font_size = 12

[bindings.global]
\"cmd-w\" = \"foo\"
";
        let replacement = "[settings]\nfont_size = 14\n\n";
        let merged = splice_settings_block(existing, replacement);
        assert!(merged.starts_with("# header comment\n"));
        assert!(merged.contains("font_size = 14"));
        assert!(merged.contains("[bindings.global]"));
        assert!(!merged.contains("font_size = 12"));
    }

    #[test]
    fn appends_when_no_settings_block() {
        let existing = "\
[bindings.global]
\"cmd-w\" = \"foo\"
";
        let replacement = "[settings]\nfont_size = 14\n\n";
        let merged = splice_settings_block(existing, replacement);
        assert!(merged.contains("[bindings.global]"));
        assert!(merged.ends_with("font_size = 14\n\n"));
    }

    #[test]
    fn preserves_bindings_when_settings_has_subsections() {
        let existing = "\
[settings]
font_size = 12

[settings.theme]
mode = \"system\"

[bindings.global]
\"cmd-w\" = \"foo\"
# bindings comment kept
";
        let replacement = "[settings]\nfont_size = 14\n\n[settings.theme]\nmode = \"dark\"\n\n";
        let merged = splice_settings_block(existing, replacement);
        assert!(merged.contains("font_size = 14"));
        assert!(merged.contains("mode = \"dark\""));
        assert!(!merged.contains("font_size = 12"));
        assert!(!merged.contains("mode = \"system\""));
        assert!(merged.contains("# bindings comment kept"));
        assert!(merged.contains("[bindings.global]"));
    }
}
