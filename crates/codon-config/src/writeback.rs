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
//! ## Approach (`toml_edit` AST)
//!
//! Parse `codon.toml` as a `toml_edit::DocumentMut`, replace the
//! `settings` table in-place, render back. `toml_edit` preserves
//! comments, whitespace, and ordering on every key outside `[settings]`
//! (notably `[bindings.*]` and any user comments). The previous
//! line-by-line splicer dropped on three edge cases: commented
//! `# [settings]` headers earlier in the file, files without a trailing
//! newline, and sub-tables in awkward positions.
//!
//! Trade-off documented in `crates/codon-config/README.md`: comments
//! *inside* the `[settings]` sub-tree are dropped when the editor
//! writes a change. Comments anywhere else in the file are preserved.

use std::sync::Arc;

use anyhow::{Context as _, Result};
use fs::Fs;
use futures::FutureExt;
use serde_json::Value;
use settings::{UserSettingsIoOverride, set_user_settings_io_override};
use toml_edit::{DocumentMut, Item};

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

async fn read_settings(fs: Arc<dyn Fs>) -> Result<String> {
    let path = user_config_path().context("dirs::config_dir() unavailable")?;
    let content = load_or_empty(&fs, &path).await?;
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

async fn write_settings(fs: Arc<dyn Fs>, new_json: String) -> Result<()> {
    let path = user_config_path().context("dirs::config_dir() unavailable")?;
    if let Some(parent) = path.parent() {
        fs.create_dir(parent)
            .await
            .with_context(|| format!("creating {} for codon.toml", parent.display()))?;
    }
    let existing = load_or_empty(&fs, &path).await?;
    let new_settings_value: Value = serde_json::from_str(&new_json)
        .context("parsing JSON returned by settings_ui edit")?;
    let new_settings_toml = json_to_toml(&new_settings_value);

    // Round-trip the new settings sub-tree through `toml::to_string` →
    // `toml_edit::DocumentMut` so we get a properly-formatted
    // `toml_edit::Item` to splice into the existing document.
    let mut wrapper = toml::value::Table::new();
    wrapper.insert("settings".to_string(), new_settings_toml);
    let serialised = toml::to_string_pretty(&wrapper)
        .context("serialising updated [settings] to TOML")?;
    let new_doc: DocumentMut = serialised
        .parse()
        .context("re-parsing serialised [settings] as toml_edit")?;
    let new_settings_item = new_doc
        .get("settings")
        .cloned()
        .context("serialised wrapper unexpectedly missing [settings]")?;

    let mut doc: DocumentMut = if existing.is_empty() {
        DocumentMut::new()
    } else {
        existing
            .parse()
            .with_context(|| format!("parsing existing {} as toml_edit", path.display()))?
    };
    splice_settings_in_place(&mut doc, new_settings_item);
    let final_text = doc.to_string();
    fs.atomic_write(path.clone(), final_text)
        .await
        .with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

async fn load_or_empty(fs: &Arc<dyn Fs>, path: &std::path::Path) -> Result<String> {
    // `Fs::load` propagates io::ErrorKind::NotFound as an anyhow error
    // chain; rather than poking at the error variant, check metadata
    // first — cheaper too, since `load` would round-trip to the FS
    // anyway.
    let metadata = fs
        .metadata(path)
        .await
        .with_context(|| format!("statting {}", path.display()))?;
    if metadata.is_none() {
        return Ok(String::new());
    }
    fs.load(path)
        .await
        .with_context(|| format!("reading {}", path.display()))
}

/// Replace `doc["settings"]` with `new_settings`, preserving every other
/// table / key / comment in the document. If no `[settings]` table
/// exists, this inserts one. Any leading decor on the existing
/// `[settings]` header (typically a free-standing comment block above
/// the header line) is preserved and re-attached to the new table.
fn splice_settings_in_place(doc: &mut DocumentMut, mut new_settings: Item) {
    let existing_prefix = doc
        .get("settings")
        .and_then(Item::as_table)
        .and_then(|t| t.decor().prefix().cloned());
    if let Some(prefix) = existing_prefix
        && let Some(new_table) = new_settings.as_table_mut()
    {
        new_table.decor_mut().set_prefix(prefix);
    }
    doc["settings"] = new_settings;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn splice(existing: &str, new_settings_toml: &str) -> String {
        let new_doc: DocumentMut = new_settings_toml.parse().expect("new doc parses");
        let new_settings = new_doc
            .get("settings")
            .cloned()
            .expect("new doc has [settings]");
        let mut doc: DocumentMut = if existing.is_empty() {
            DocumentMut::new()
        } else {
            existing.parse().expect("existing parses")
        };
        splice_settings_in_place(&mut doc, new_settings);
        doc.to_string()
    }

    #[test]
    fn splices_into_middle_of_file() {
        let existing = "\
# header comment
[settings]
font_size = 12

[bindings.global]
\"cmd-w\" = \"foo\"
";
        let replacement = "[settings]\nfont_size = 14\n";
        let merged = splice(existing, replacement);
        assert!(merged.contains("# header comment"), "header comment kept");
        assert!(merged.contains("font_size = 14"), "new value written");
        assert!(!merged.contains("font_size = 12"), "old value gone");
        assert!(merged.contains("[bindings.global]"), "bindings preserved");
    }

    #[test]
    fn inserts_settings_when_absent() {
        let existing = "\
[bindings.global]
\"cmd-w\" = \"foo\"
";
        let replacement = "[settings]\nfont_size = 14\n";
        let merged = splice(existing, replacement);
        assert!(merged.contains("[bindings.global]"));
        assert!(merged.contains("font_size = 14"));
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
        let replacement =
            "[settings]\nfont_size = 14\n\n[settings.theme]\nmode = \"dark\"\n";
        let merged = splice(existing, replacement);
        assert!(merged.contains("font_size = 14"));
        assert!(merged.contains("mode = \"dark\""));
        assert!(!merged.contains("font_size = 12"));
        assert!(!merged.contains("mode = \"system\""));
        assert!(
            merged.contains("# bindings comment kept"),
            "bindings comment kept across the splice"
        );
        assert!(merged.contains("[bindings.global]"));
    }

    #[test]
    fn commented_settings_header_does_not_confuse_splicer() {
        // The previous line-by-line splicer matched any line starting
        // with `[settings]` after trim, including a commented-out
        // header. toml_edit only matches real headers.
        let existing = "\
# [settings] <-- old example in a comment, not the actual table
[bindings.global]
\"cmd-w\" = \"foo\"
";
        let replacement = "[settings]\nfont_size = 14\n";
        let merged = splice(existing, replacement);
        assert!(
            merged.contains("# [settings] <-- old example"),
            "comment kept verbatim"
        );
        assert!(merged.contains("font_size = 14"));
        assert!(merged.contains("[bindings.global]"));
    }

    #[test]
    fn no_trailing_newline_input_does_not_fuse_lines() {
        let existing = "[bindings.global]\n\"cmd-w\" = \"foo\"";
        let replacement = "[settings]\nfont_size = 14\n";
        let merged = splice(existing, replacement);
        assert!(merged.contains("[bindings.global]"));
        assert!(merged.contains("font_size = 14"));
        // toml_edit may add or omit a trailing newline; what matters
        // is that the two table headers stay on separate lines.
        assert!(!merged.contains("\"foo\"[settings]"));
    }

    #[test]
    fn writing_same_value_twice_is_idempotent() {
        let existing = "\
[settings]
font_size = 14

[bindings.global]
\"cmd-w\" = \"foo\"
";
        let replacement = "[settings]\nfont_size = 14\n";
        let once = splice(existing, replacement);
        let twice = splice(&once, replacement);
        assert_eq!(once, twice, "second write is a no-op on content");
    }
}
