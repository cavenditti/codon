//! Vi-style global file-manager bookmarks.
//!
//! 26 slots indexed by `a`..`z`. `m<letter>` saves the current directory
//! into the slot; `'<letter>` jumps there. Persisted to
//! `~/.local/state/codon/fm-bookmarks.toml` so a slot keeps its value
//! across codon launches.

use std::path::{Path, PathBuf};

use gpui::{App, Global};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "fm-bookmarks.toml";

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct BookmarkFile {
    #[serde(default)]
    slots: std::collections::BTreeMap<String, PathBuf>,
}

pub struct BookmarkStore {
    slots: [Option<PathBuf>; 26],
}

impl Default for BookmarkStore {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }
}

impl BookmarkStore {
    pub fn load() -> Self {
        let Some(path) = Self::file_path() else {
            return Self::default();
        };
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        let parsed: BookmarkFile = match toml::from_str(&content) {
            Ok(v) => v,
            Err(err) => {
                log::warn!(
                    "file-manager: ignoring malformed {} ({err})",
                    path.display()
                );
                return Self::default();
            }
        };
        let mut store = Self::default();
        for (key, value) in parsed.slots {
            if let Some(idx) = slot_index(&key) {
                store.slots[idx] = Some(value);
            }
        }
        store
    }

    pub fn set(&mut self, letter: char, dir: PathBuf) {
        if let Some(idx) = slot_index(&letter.to_string()) {
            self.slots[idx] = Some(dir);
            self.save();
        }
    }

    pub fn get(&self, letter: char) -> Option<&Path> {
        slot_index(&letter.to_string()).and_then(|idx| self.slots[idx].as_deref())
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
        let mut file = BookmarkFile::default();
        for (idx, slot) in self.slots.iter().enumerate() {
            if let Some(dir) = slot {
                let letter = (b'a' + idx as u8) as char;
                file.slots.insert(letter.to_string(), dir.clone());
            }
        }
        let serialized = match toml::to_string_pretty(&file) {
            Ok(s) => s,
            Err(err) => {
                log::warn!("file-manager: serialising bookmarks failed: {err}");
                return;
            }
        };
        if let Err(err) = std::fs::write(&path, serialized) {
            log::warn!(
                "file-manager: could not persist bookmarks to {} ({err})",
                path.display()
            );
        }
    }

    fn file_path() -> Option<PathBuf> {
        codon_config::codon_state_dir().map(|d| d.join(FILE_NAME))
    }
}

impl Global for BookmarkStore {}

/// Install the shared bookmark store as a process-wide global. Called
/// once during `file_manager::init`.
pub fn init(cx: &mut App) {
    cx.set_global(BookmarkStore::load());
}

fn slot_index(key: &str) -> Option<usize> {
    let mut chars = key.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    if !first.is_ascii_alphabetic() {
        return None;
    }
    Some((first.to_ascii_lowercase() as u8 - b'a') as usize)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slot_index_lowercase_and_uppercase() {
        assert_eq!(slot_index("a"), Some(0));
        assert_eq!(slot_index("A"), Some(0));
        assert_eq!(slot_index("z"), Some(25));
        assert_eq!(slot_index("Z"), Some(25));
    }

    #[test]
    fn slot_index_rejects_multichar_and_non_alpha() {
        assert_eq!(slot_index(""), None);
        assert_eq!(slot_index("ab"), None);
        assert_eq!(slot_index("1"), None);
        assert_eq!(slot_index("é"), None);
    }

    #[test]
    fn set_and_get_round_trip_in_memory() {
        let mut store = BookmarkStore::default();
        let dir = PathBuf::from("/tmp/example");
        store.slots[0] = Some(dir.clone());
        assert_eq!(store.get('a'), Some(dir.as_path()));
        assert_eq!(store.get('b'), None);
    }
}
