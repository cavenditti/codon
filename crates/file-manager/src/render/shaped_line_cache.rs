//! FM-scoped LRU cache over `ShapedLine` results.
//!
//! GPUI's process-wide `LineLayoutCache` is bounded by `with_text_style`
//! boundaries and routinely re-shapes the same FM row labels (filenames,
//! meta strings) on every frame during fast `j`/`k` navigation. This
//! cache exists alongside it, keyed on
//! `(font_id, font_size, text, run_font, run_color)` — the keyset that
//! the FM column iterates over each paint — so that
//! a steady-state navigation reduces shaping to a handful of misses
//! (only when a new label scrolls into view) instead of one per row
//! per column per frame.
//!
//! Ownership: a `ShapedLineCache` is instantiated by the custom
//! column Element (`FmColumnElement`) and borrowed mutably by each
//! row's paint via `Rc<RefCell<_>>`. Capacity is sized for the
//! visible window plus a generous lookahead — see `default_capacity`.

use std::num::NonZeroUsize;
use std::sync::Arc;

use gpui::{Font, FontId, Hsla, Pixels, ShapedLine, SharedString, TextRun, WindowTextSystem};
use indexmap::IndexMap;

use crate::render::trace::COUNTERS;

/// Key for a cached shaped line. Font size is keyed as integer
/// hundredths of a pixel so equal-but-not-Eq `Pixels` values still
/// match — `Pixels(13.0)` and `Pixels(13.0)` are bit-equal but the
/// API doesn't promise it.
#[derive(Hash, Eq, PartialEq, Clone, Debug)]
pub(crate) struct ShapedLineKey {
    pub font_id: FontId,
    pub font_size_centi_px: u32,
    pub text: SharedString,
    /// `font_id` is resolved from the column's base font, while an
    /// individual run may override weight/style (selected rows are
    /// bold). Keep the run font and color in the key so a shaped line
    /// created for an unselected/default-color row cannot be replayed
    /// for a selected or marked row.
    pub run_font: Font,
    pub color: Hsla,
}

impl ShapedLineKey {
    fn from_parts(
        font_id: FontId,
        font_size: Pixels,
        text: SharedString,
        run_for_shaping: &TextRun,
    ) -> Self {
        // `Pixels` is a newtype over `f32`. Multiply by 100 and round so
        // 13.00 px and 13.00 px hash identically even after arithmetic.
        let centi = (f32::from(font_size) * 100.0).round() as u32;
        Self {
            font_id,
            font_size_centi_px: centi,
            text,
            run_font: run_for_shaping.font.clone(),
            color: run_for_shaping.color,
        }
    }
}

/// FM-scoped LRU over `ShapedLine`. Insertion order tracks recency:
/// the most-recently-accessed key is moved to the tail; eviction
/// drops the head.
#[allow(dead_code)] // hits/misses/reset_counters surfaced by fm-render-frame-budget
pub(crate) struct ShapedLineCache {
    inner: IndexMap<ShapedLineKey, Arc<ShapedLine>>,
    capacity: NonZeroUsize,
    hits: u64,
    misses: u64,
}

/// Default capacity heuristic: 4 × visible rows × columns × meta_columns.
/// A typical 30-row × 3-column × 5-meta-label layout lands near 600 —
/// enough to cover scroll headroom plus preview lookahead without
/// holding shaped data for offscreen sets indefinitely.
pub(crate) fn default_capacity(visible_rows: usize, columns: usize, meta_columns: usize) -> usize {
    let n = 4usize
        .saturating_mul(visible_rows.max(1))
        .saturating_mul(columns.max(1))
        .saturating_mul(meta_columns.max(1));
    n.max(64)
}

impl ShapedLineCache {
    pub fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity.max(1))
            .expect("max(1) is at least 1, so NonZeroUsize::new succeeds");
        Self {
            inner: IndexMap::with_capacity(capacity.get()),
            capacity,
            hits: 0,
            misses: 0,
        }
    }

    /// Look up a cached shaped line, shaping on miss. Marks the entry
    /// as most-recently-used by moving it to the tail of the index.
    pub fn get_or_shape(
        &mut self,
        text: &SharedString,
        font_id: FontId,
        font_size: Pixels,
        text_system: &WindowTextSystem,
        run_for_shaping: &TextRun,
    ) -> Arc<ShapedLine> {
        let key = ShapedLineKey::from_parts(font_id, font_size, text.clone(), run_for_shaping);

        if let Some(idx) = self.inner.get_index_of(&key) {
            // Hit: bump recency by moving the entry to the tail.
            // `move_index` preserves the order of every other entry —
            // a `swap_remove` + reinsert would teleport the previous
            // tail entry into the vacated slot and corrupt LRU order,
            // letting head-eviction drop recently used lines.
            let last = self.inner.len() - 1;
            self.inner.move_index(idx, last);
            let (_, value) = self.inner.get_index(last).expect("entry just moved");
            let value_clone = value.clone();
            self.hits = self.hits.saturating_add(1);
            COUNTERS.add_shaped_line_hits(1);
            return value_clone;
        }

        // Miss: shape and insert. `shape_line` panics on '\n' so callers
        // must pre-strip newlines (FM labels are single-line by
        // construction).
        let shaped = text_system.shape_line(
            text.clone(),
            font_size,
            std::slice::from_ref(run_for_shaping),
            None,
        );
        let arc = Arc::new(shaped);
        self.inner.insert(key, arc.clone());
        self.misses = self.misses.saturating_add(1);
        COUNTERS.add_shaped_line_misses(1);

        // Evict from the head until we're at-or-under capacity.
        while self.inner.len() > self.capacity.get() {
            self.inner.shift_remove_index(0);
        }
        arc
    }

    /// Drop every cached entry whose key doesn't match the supplied
    /// `(font_id, font_size)` pair. Called on theme/font changes so
    /// stale glyphs don't leak across reconfigurations.
    #[allow(dead_code)]
    pub fn invalidate_for_font(&mut self, font_id: FontId, font_size: Pixels) {
        let centi = (f32::from(font_size) * 100.0).round() as u32;
        self.inner
            .retain(|k, _| k.font_id == font_id && k.font_size_centi_px == centi);
    }

    /// Drop every entry. Used when the visible set rotates wholesale
    /// (e.g. directory change).
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.inner.clear();
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Cumulative hit count since construction (or last `reset_counters`).
    #[allow(dead_code)]
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// Cumulative miss count since construction (or last `reset_counters`).
    #[allow(dead_code)]
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Reset the hit/miss counters — used by the render-trace harness
    /// to attribute hit-rate per frame.
    #[allow(dead_code)]
    pub fn reset_counters(&mut self) {
        self.hits = 0;
        self.misses = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::px;

    fn dummy_key(font_id: usize, font_size: f32, text: &str) -> ShapedLineKey {
        let run = TextRun {
            len: text.len(),
            ..Default::default()
        };
        ShapedLineKey {
            font_id: FontId(font_id),
            font_size_centi_px: (font_size * 100.0).round() as u32,
            text: SharedString::from(text.to_string()),
            run_font: run.font,
            color: run.color,
        }
    }

    /// Construct a cache and pre-populate `inner` directly without
    /// shaping — exercises LRU semantics without needing a real
    /// `WindowTextSystem`.
    fn populate(cache: &mut ShapedLineCache, keys: &[ShapedLineKey]) {
        for k in keys {
            // Dummy shaped line — we only check key behavior, not paint.
            let shaped = Arc::new(ShapedLine::default());
            cache.inner.insert(k.clone(), shaped);
            while cache.inner.len() > cache.capacity.get() {
                cache.inner.shift_remove_index(0);
            }
        }
    }

    #[test]
    fn shaped_line_cache_lru_eviction() {
        let mut cache = ShapedLineCache::new(3);
        let a = dummy_key(0, 13.0, "a");
        let b = dummy_key(0, 13.0, "b");
        let c = dummy_key(0, 13.0, "c");
        let d = dummy_key(0, 13.0, "d");

        populate(&mut cache, &[a.clone(), b.clone(), c.clone()]);
        assert_eq!(cache.len(), 3);

        // Touch `a` to mark it most-recently-used — same move-to-tail
        // the production hit path performs.
        let idx = cache.inner.get_index_of(&a).expect("a present");
        let last = cache.inner.len() - 1;
        cache.inner.move_index(idx, last);

        // Insert `d` — should evict `b` (the new least-recently-used).
        populate(&mut cache, std::slice::from_ref(&d));

        assert_eq!(cache.len(), 3);
        assert!(cache.inner.contains_key(&a), "a was just touched");
        assert!(!cache.inner.contains_key(&b), "b was the LRU");
        assert!(cache.inner.contains_key(&c));
        assert!(cache.inner.contains_key(&d));
    }

    #[test]
    fn shaped_line_cache_font_invalidation() {
        let mut cache = ShapedLineCache::new(16);
        let small = dummy_key(0, 13.0, "small");
        let large = dummy_key(0, 18.0, "large");
        let other_font = dummy_key(1, 13.0, "other");
        populate(
            &mut cache,
            &[small.clone(), large.clone(), other_font.clone()],
        );
        assert_eq!(cache.len(), 3);

        cache.invalidate_for_font(FontId(0), px(13.0));
        assert_eq!(cache.len(), 1, "only the (0, 13.0) entry should survive");
        assert!(cache.inner.contains_key(&small));
        assert!(!cache.inner.contains_key(&large));
        assert!(!cache.inner.contains_key(&other_font));
    }

    #[test]
    fn default_capacity_scales_with_layout() {
        // 30 rows × 3 columns × 5 meta labels × 4 = 1800, well above
        // the floor of 64. Small layouts still get a sane floor.
        assert!(default_capacity(30, 3, 5) >= 600);
        assert_eq!(default_capacity(0, 0, 0), 64);
    }

    #[test]
    fn shaped_line_key_distinguishes_run_style() {
        let text = SharedString::from("same");
        let base = TextRun {
            len: text.len(),
            ..Default::default()
        };
        let mut bold = base.clone();
        bold.font.weight = gpui::FontWeight::BOLD;
        let mut colored = base.clone();
        colored.color = gpui::Hsla {
            h: 0.5,
            s: 0.8,
            l: 0.6,
            a: 1.0,
        };

        let a = ShapedLineKey::from_parts(FontId(0), px(13.0), text.clone(), &base);
        let b = ShapedLineKey::from_parts(FontId(0), px(13.0), text.clone(), &bold);
        let c = ShapedLineKey::from_parts(FontId(0), px(13.0), text, &colored);
        assert_ne!(a, b);
        assert_ne!(a, c);
    }
}
