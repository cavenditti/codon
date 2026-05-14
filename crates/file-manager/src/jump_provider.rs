//! File-manager [`JumpProvider`] — one candidate per visible row.
//!
//! Wired into [`crate::FileManager::new`] via [`register`]. The
//! provider holds a [`WeakEntity<FileManager>`] so the
//! `codon-jump` registry's `is_alive` pruning drops it when the
//! panel is destroyed (e.g. on workspace teardown).
//!
//! For `JumpMode::Url`, the provider returns an empty list — the
//! file manager has no URLs to copy. The overlay handles the
//! "no candidates" path by showing the "No URLs visible" toast at
//! the action layer.
//!
//! Row bounds are computed analytically from the captured row
//! height + the uniform-list's bounds: every visible row is a
//! fixed-height slice of the list rectangle. If layout hasn't
//! happened yet (`last_item_size` is `None`), the helper returns
//! `None` and the overlay skips that candidate.

use std::sync::Arc;

use codon_jump::{JumpCandidate, JumpContext, JumpKind, JumpMode, JumpProvider, JumpRegistry};
use gpui::{App, Context, Focusable, WeakEntity};

use crate::file_manager::FileManager;

/// Register a freshly-constructed `FileManager`'s jump provider with
/// the global [`JumpRegistry`]. Called from
/// [`crate::FileManager::new`]; idempotent under the registry's
/// `is_alive` pruning — stale providers from prior panels are
/// dropped on the next collect.
pub(crate) fn register(cx: &mut Context<FileManager>) {
    let fm = cx.entity().downgrade();
    JumpRegistry::register(cx, Arc::new(FmJumpProvider { fm }));
}

struct FmJumpProvider {
    fm: WeakEntity<FileManager>,
}

impl JumpProvider for FmJumpProvider {
    fn collect(&self, ctx: &JumpContext, cx: &mut App) -> Vec<JumpCandidate> {
        // Url mode never has fm candidates — the file manager renders
        // names + metadata, none of which are URLs.
        if ctx.mode == JumpMode::Url {
            return Vec::new();
        }
        let Some(fm) = self.fm.upgrade() else {
            return Vec::new();
        };
        fm.read_with(cx, |fm, cx| {
            let first_visible = fm.first_visible_row();
            let row_count = fm.visible_row_count();
            (first_visible..first_visible + row_count)
                .filter_map(|row| {
                    let bounds = fm.row_screen_bounds(row, cx)?;
                    let fm_weak = self.fm.clone();
                    Some(JumpCandidate {
                        bounds,
                        kind: JumpKind::Word,
                        action: Box::new(move |window, cx| {
                            fm_weak
                                .update(cx, |fm, cx| {
                                    fm.set_cursor_index(row, cx);
                                    let handle = fm.focus_handle(cx);
                                    window.focus(&handle, cx);
                                })
                                .ok();
                        }),
                    })
                })
                .collect()
        })
    }

    fn is_alive(&self, _cx: &App) -> bool {
        self.fm.upgrade().is_some()
    }
}
