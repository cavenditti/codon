//! Per-column cache for fully-resolved FM row payloads.
//!
//! Sits one layer above the shaped-line cache: keyed on the `(path,
//! display_state, line_mode)` tuple — the full set of inputs that
//! decide what a row's painted pixels look like — and holds the
//! already-shaped name + meta lines plus the resolved background
//! colour.
//!
//! On a steady-state scroll, every row that survives in the viewport
//! is a cache hit and the `paint_inline` path skips both the shape
//! step (which the shaped-line cache already amortised) AND the
//! per-row state-derivation work (run construction, colour
//! resolution, meta-text lookup).
//!
//! On a selection move, only the two affected rows (previously- and
//! newly-selected) get fresh entries — every other visible row hits
//! the cache.

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::Arc;

use gpui::{Hsla, ShapedLine, SharedString};
use indexmap::IndexMap;

use crate::prefs::LineMode;
use crate::render::row::RowDisplayState;
use crate::render::trace::COUNTERS;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) struct RowGlyphKey {
    pub path: PathBuf,
    pub line_mode: LineMode,
    pub state: RowDisplayState,
    /// Resolved icon path — if the icon changes (e.g. after a
    /// `populate_icon_paths` backfill), the cached row payload
    /// must invalidate.
    pub icon_path: Option<SharedString>,
}

#[derive(Clone)]
pub(crate) struct CachedRow {
    pub background: Option<Hsla>,
    pub name_line: Option<Arc<ShapedLine>>,
    pub meta_line: Option<Arc<ShapedLine>>,
}

#[allow(dead_code)] // hits/misses/invalidate surfaces wired by frame-budget task
pub(crate) struct RowGlyphCache {
    inner: IndexMap<RowGlyphKey, Arc<CachedRow>>,
    capacity: NonZeroUsize,
    hits: u64,
    misses: u64,
}

impl RowGlyphCache {
    pub fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity.max(1))
            .expect("max(1) is at least 1");
        Self {
            inner: IndexMap::with_capacity(capacity.get()),
            capacity,
            hits: 0,
            misses: 0,
        }
    }

    /// Look up a row payload by key. Returns `None` on miss; the
    /// caller is responsible for building the payload + inserting
    /// via `insert`. Two-step API (rather than `get_or_build`)
    /// because building a row payload needs `&mut Window`, which
    /// can't be smuggled into a closure that also holds the cache
    /// borrow.
    pub fn get(&mut self, key: &RowGlyphKey) -> Option<Arc<CachedRow>> {
        if let Some(idx) = self.inner.get_index_of(key) {
            let (k, v) = self.inner.swap_remove_index(idx).expect("index just resolved");
            let value = v.clone();
            self.inner.insert(k, v);
            self.hits = self.hits.saturating_add(1);
            COUNTERS.add_row_glyph_hits(1);
            Some(value)
        } else {
            self.misses = self.misses.saturating_add(1);
            COUNTERS.add_row_glyph_misses(1);
            None
        }
    }

    pub fn insert(&mut self, key: RowGlyphKey, payload: Arc<CachedRow>) {
        self.inner.insert(key, payload);
        while self.inner.len() > self.capacity.get() {
            self.inner.shift_remove_index(0);
        }
    }

    /// Drop every entry. Used on theme change and when the column's
    /// entry set rotates wholesale.
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    /// Retain only entries whose path is in `visible_paths`. Useful
    /// when the column scrolls — keeps the working set bounded by
    /// the visible window plus headroom.
    #[allow(dead_code)]
    pub fn retain_visible(&mut self, visible_paths: &HashSet<PathBuf>) {
        self.inner.retain(|k, _| visible_paths.contains(&k.path));
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn hits(&self) -> u64 {
        self.hits
    }

    pub fn misses(&self) -> u64 {
        self.misses
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(path: &str, sel: bool) -> RowGlyphKey {
        RowGlyphKey {
            path: PathBuf::from(path),
            line_mode: LineMode::Size,
            state: RowDisplayState {
                is_selected: sel,
                is_marked: false,
                is_focused_row: sel,
                zebra_stripe: false,
            },
            icon_path: None,
        }
    }

    fn payload() -> Arc<CachedRow> {
        Arc::new(CachedRow {
            background: None,
            name_line: None,
            meta_line: None,
        })
    }

    #[test]
    fn row_glyph_cache_hits_after_insert() {
        let mut cache = RowGlyphCache::new(8);
        let k = key("/foo", false);
        assert!(cache.get(&k).is_none());
        cache.insert(k.clone(), payload());
        assert!(cache.get(&k).is_some());
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.misses(), 1);
    }

    #[test]
    fn row_glyph_cache_distinct_selection_state() {
        let mut cache = RowGlyphCache::new(8);
        let unselected = key("/foo", false);
        let selected = key("/foo", true);
        cache.insert(unselected.clone(), payload());
        assert!(cache.get(&unselected).is_some());
        // Selection-state change is a different key — miss.
        assert!(cache.get(&selected).is_none());
    }

    #[test]
    fn row_glyph_cache_evicts_lru() {
        let mut cache = RowGlyphCache::new(2);
        cache.insert(key("/a", false), payload());
        cache.insert(key("/b", false), payload());
        cache.insert(key("/c", false), payload());
        assert_eq!(cache.len(), 2);
        // First inserted (`/a`) should be evicted.
        assert!(cache.get(&key("/a", false)).is_none());
        assert!(cache.get(&key("/b", false)).is_some());
        assert!(cache.get(&key("/c", false)).is_some());
    }
}
