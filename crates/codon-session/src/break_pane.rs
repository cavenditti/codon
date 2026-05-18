//! Snapshot-level pane surgery for `BreakPaneToWindow` and
//! `MovePaneToWindow(usize)`.
//!
//! Walks a `LayoutSnapshot`, finds the pane the user is currently focused
//! on, and returns a `(remaining, broken)` pair:
//!
//! - `broken` is a single-pane snapshot containing just that pane, with
//!   `active = true` so the destination window opens with it focused.
//! - `remaining` is the original tree with that pane removed and any
//!   `Group` nodes that have collapsed to a single child unwrapped.
//!
//! Returns `None` when the snapshot already contains only one pane —
//! there's nothing to break out, and the caller should surface a hint
//! to the user.
//!
//! The pane is identified by walking the tree in document order and
//! taking the first pane marked `active`. If no pane is marked active
//! (e.g. a freshly-applied snapshot before any pane has been focused),
//! the first pane in document order is used as a fallback so the action
//! still works.
//!
//! `attach_pane_horizontally` is the inverse — it grafts a detached pane
//! onto a target window's existing layout as a new horizontal split,
//! clearing every other pane's `active` flag so focus lands on the
//! moved-in pane after restore. Used by `MovePaneToWindow(usize)`.

use workspace::codon_bridge::{LayoutSnapshot, PaneSnapshot, SnapshotAxis};

/// Total number of pane leaves in this subtree.
fn count_panes(snap: &LayoutSnapshot) -> usize {
    match snap {
        LayoutSnapshot::Pane(_) => 1,
        LayoutSnapshot::Group { children, .. } => children.iter().map(count_panes).sum(),
        LayoutSnapshot::Stack { members, .. } => members.iter().map(count_panes).sum(),
    }
}

/// Returns `true` if the subtree contains a pane with `active = true`.
fn has_active_pane(snap: &LayoutSnapshot) -> bool {
    match snap {
        LayoutSnapshot::Pane(p) => p.active,
        LayoutSnapshot::Group { children, .. } => children.iter().any(has_active_pane),
        LayoutSnapshot::Stack { members, active, .. } => members
            .get(*active)
            .map(has_active_pane)
            .unwrap_or(false),
    }
}

/// Mutating walk that pulls out *some* pane and returns it. Used as a
/// post-step when no pane is marked active — pick the leftmost.
///
/// Returns the broken pane and whether the surrounding container has
/// collapsed (i.e. its child list went from 2 to 1). Caller is
/// responsible for unwrapping collapsed `Group`s.
fn take_first_pane(snap: &mut LayoutSnapshot) -> Option<PaneSnapshot> {
    match snap {
        LayoutSnapshot::Pane(_) => {
            // Can't take from a leaf — caller already handled that case.
            None
        }
        LayoutSnapshot::Group { children, .. } => take_pane_from_list(children),
        LayoutSnapshot::Stack { members, .. } => take_pane_from_list(members),
    }
}

/// Pull a pane out of a child list. Walks left-to-right; if a child is
/// itself a leaf pane, removes it. If a child is a container, recurses
/// and collapses the container if it ends up with a single child.
fn take_pane_from_list(children: &mut Vec<LayoutSnapshot>) -> Option<PaneSnapshot> {
    for ix in 0..children.len() {
        match &mut children[ix] {
            LayoutSnapshot::Pane(_) => {
                let LayoutSnapshot::Pane(pane) = children.remove(ix) else {
                    unreachable!()
                };
                return Some(pane);
            }
            inner @ LayoutSnapshot::Group { .. } | inner @ LayoutSnapshot::Stack { .. } => {
                if let Some(pane) = take_first_pane(inner) {
                    collapse_in_place(inner);
                    return Some(pane);
                }
            }
        }
    }
    None
}

/// Pull out the first pane marked `active`. Returns `Some(PaneSnapshot)`
/// if one was found and removed, `None` otherwise.
fn take_active_pane(snap: &mut LayoutSnapshot) -> Option<PaneSnapshot> {
    match snap {
        LayoutSnapshot::Pane(_) => None,
        LayoutSnapshot::Group { children, .. } => take_active_from_list(children),
        LayoutSnapshot::Stack { members, .. } => take_active_from_list(members),
    }
}

fn take_active_from_list(children: &mut Vec<LayoutSnapshot>) -> Option<PaneSnapshot> {
    for ix in 0..children.len() {
        if let LayoutSnapshot::Pane(p) = &children[ix] {
            if p.active {
                let LayoutSnapshot::Pane(pane) = children.remove(ix) else {
                    unreachable!()
                };
                return Some(pane);
            }
        }
    }
    for ix in 0..children.len() {
        match &mut children[ix] {
            LayoutSnapshot::Group { .. } | LayoutSnapshot::Stack { .. } => {
                if let Some(pane) = take_active_pane(&mut children[ix]) {
                    collapse_in_place(&mut children[ix]);
                    return Some(pane);
                }
            }
            LayoutSnapshot::Pane(_) => {}
        }
    }
    None
}

/// If `snap` is a `Group` or `Stack` with a single child, replace it
/// with that child. Otherwise leave untouched.
fn collapse_in_place(snap: &mut LayoutSnapshot) {
    let single_child = match snap {
        LayoutSnapshot::Group { children, .. } if children.len() == 1 => Some(children.remove(0)),
        LayoutSnapshot::Stack { members, .. } if members.len() == 1 => Some(members.remove(0)),
        _ => None,
    };
    if let Some(child) = single_child {
        *snap = child;
    }
}

/// Split `snapshot` into `(remaining, broken)`. Returns `None` if the
/// snapshot contains only one pane — break-pane is a no-op in that case.
///
/// `broken` is always a `LayoutSnapshot::Pane` with `active = true`.
/// `remaining` matches the input shape with the broken pane removed
/// and degenerate containers collapsed.
pub fn split_off_active(
    snapshot: LayoutSnapshot,
) -> Option<(LayoutSnapshot, LayoutSnapshot)> {
    if count_panes(&snapshot) <= 1 {
        return None;
    }

    let mut remaining = snapshot;
    let broken_pane = if has_active_pane(&remaining) {
        take_active_pane(&mut remaining)
    } else {
        take_first_pane(&mut remaining)
    }?;
    collapse_in_place(&mut remaining);

    let broken = LayoutSnapshot::Pane(PaneSnapshot {
        active: true,
        ..broken_pane
    });
    Some((remaining, broken))
}

/// Recursively clear `active` on every pane leaf so a newly-spliced pane
/// can claim focus exclusively. Stack containers retain their `active`
/// child-index (it controls which tab is visible, not which pane is
/// focused).
fn clear_active_flags(snap: &mut LayoutSnapshot) {
    match snap {
        LayoutSnapshot::Pane(p) => p.active = false,
        LayoutSnapshot::Group { children, .. } => {
            for child in children.iter_mut() {
                clear_active_flags(child);
            }
        }
        LayoutSnapshot::Stack { members, .. } => {
            for member in members.iter_mut() {
                clear_active_flags(member);
            }
        }
    }
}

/// Graft `pane` onto `target` as the right-hand child of a new horizontal
/// split. Every pane in `target` has its `active` flag cleared so focus
/// lands on `pane` after restore. When `target` is empty, `pane` becomes
/// the layout.
///
/// `pane` is expected to be a single-pane snapshot returned by
/// [`split_off_active`] (i.e. its only pane is already `active = true`).
/// Passing a multi-pane `pane` would still work mechanically but the
/// active-pane invariant in the result is only guaranteed for the
/// single-pane case.
pub fn attach_pane_horizontally(
    target: Option<LayoutSnapshot>,
    pane: LayoutSnapshot,
) -> LayoutSnapshot {
    match target {
        None => pane,
        Some(mut existing) => {
            clear_active_flags(&mut existing);
            LayoutSnapshot::Group {
                axis: SnapshotAxis::Horizontal,
                flexes: None,
                children: vec![existing, pane],
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use workspace::codon_bridge::ItemSnapshot;

    fn pane(name: &str, active: bool) -> LayoutSnapshot {
        LayoutSnapshot::Pane(PaneSnapshot {
            items: vec![ItemSnapshot {
                kind: name.to_string(),
                item_id: 1,
                active: true,
                preview: false,
            }],
            active,
            pinned_count: 0,
        })
    }

    fn group(children: Vec<LayoutSnapshot>) -> LayoutSnapshot {
        LayoutSnapshot::Group {
            axis: SnapshotAxis::Horizontal,
            flexes: None,
            children,
        }
    }

    #[test]
    fn single_pane_returns_none() {
        assert!(split_off_active(pane("only", true)).is_none());
    }

    #[test]
    fn two_pane_split_collapses_group() {
        let layout = group(vec![pane("left", false), pane("right", true)]);
        let (remaining, broken) = split_off_active(layout).expect("two panes split");

        // Remaining is just the left leaf — Group with one child collapsed.
        match &remaining {
            LayoutSnapshot::Pane(p) => assert_eq!(p.items[0].kind, "left"),
            other => panic!("expected leaf, got {other:?}"),
        }

        // Broken is the right pane, re-flagged active.
        match &broken {
            LayoutSnapshot::Pane(p) => {
                assert_eq!(p.items[0].kind, "right");
                assert!(p.active);
            }
            other => panic!("expected leaf, got {other:?}"),
        }
    }

    #[test]
    fn three_pane_split_keeps_group() {
        let layout = group(vec![pane("a", false), pane("b", true), pane("c", false)]);
        let (remaining, _broken) = split_off_active(layout).expect("three-way split");
        match &remaining {
            LayoutSnapshot::Group { children, .. } => assert_eq!(children.len(), 2),
            other => panic!("expected group of two, got {other:?}"),
        }
    }

    #[test]
    fn no_active_falls_back_to_first() {
        let layout = group(vec![pane("a", false), pane("b", false)]);
        let (_remaining, broken) = split_off_active(layout).expect("fallback path");
        match &broken {
            LayoutSnapshot::Pane(p) => assert_eq!(p.items[0].kind, "a"),
            other => panic!("expected leaf, got {other:?}"),
        }
    }

    /// End-to-end on the snapshot layer: detach the active pane from
    /// the source layout and graft it onto a target layout. The result
    /// should be a horizontal split with the moved pane on the right
    /// and focus, with every original target pane deactivated. Mirrors
    /// what `MovePaneToWindow(usize)` does to the persisted layouts
    /// before applying them to the visible workspace.
    #[test]
    fn move_pane_end_to_end_grafts_onto_target() {
        let source = group(vec![pane("src-a", false), pane("src-b", true), pane("src-c", false)]);
        let target = group(vec![pane("tgt-a", true), pane("tgt-b", false)]);

        let (source_remaining, broken) = split_off_active(source).expect("multi-pane source");
        let target_after = attach_pane_horizontally(Some(target), broken);

        // Source loses src-b — group of two remains (src-a, src-c).
        let LayoutSnapshot::Group { children, .. } = &source_remaining else {
            panic!("expected source to remain a group");
        };
        assert_eq!(children.len(), 2);
        let kinds: Vec<_> = children
            .iter()
            .filter_map(|c| {
                if let LayoutSnapshot::Pane(p) = c {
                    Some(p.items[0].kind.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(kinds, vec!["src-a", "src-c"]);

        // Target wrapped into a horizontal group; src-b is on the right
        // and is the only active pane.
        let LayoutSnapshot::Group { axis, children, .. } = &target_after else {
            panic!("expected target wrapped in group");
        };
        assert!(matches!(axis, SnapshotAxis::Horizontal));
        let LayoutSnapshot::Pane(moved) = &children[1] else {
            panic!("expected moved pane on right of target");
        };
        assert!(moved.active);
        assert_eq!(moved.items[0].kind, "src-b");

        let LayoutSnapshot::Group { children: tgt_inner, .. } = &children[0] else {
            panic!("expected original target group on left");
        };
        for child in tgt_inner {
            if let LayoutSnapshot::Pane(p) = child {
                assert!(!p.active, "every original target pane must be deactivated");
            }
        }
    }

    /// Single-pane source: `split_off_active` returns None — the
    /// caller (MovePaneToWindow handler) takes the whole snapshot as
    /// the pane-to-move and closes the source window after the move.
    /// This test exercises the snapshot-level half of that path.
    #[test]
    fn move_single_pane_source_uses_whole_snapshot() {
        let source = pane("only", true);
        assert!(split_off_active(source.clone()).is_none());

        // The handler in actions.rs sets active=true on the cloned pane
        // and attaches it to the target. Simulate that here.
        let target = pane("tgt-only", true);
        let merged = attach_pane_horizontally(Some(target), source);
        let LayoutSnapshot::Group { children, .. } = &merged else {
            panic!("expected merged group");
        };
        // Target on left now deactivated; source on right active.
        if let LayoutSnapshot::Pane(left) = &children[0] {
            assert!(!left.active);
            assert_eq!(left.items[0].kind, "tgt-only");
        }
        if let LayoutSnapshot::Pane(right) = &children[1] {
            assert!(right.active);
            assert_eq!(right.items[0].kind, "only");
        }
    }

    #[test]
    fn attach_into_empty_target_replaces_layout() {
        let detached = pane("moved", true);
        let attached = attach_pane_horizontally(None, detached);
        match &attached {
            LayoutSnapshot::Pane(p) => {
                assert_eq!(p.items[0].kind, "moved");
                assert!(p.active);
            }
            other => panic!("expected leaf, got {other:?}"),
        }
    }

    #[test]
    fn attach_into_existing_wraps_in_group_with_active_cleared() {
        let existing = group(vec![pane("a", true), pane("b", false)]);
        let detached = pane("moved", true);
        let attached = attach_pane_horizontally(Some(existing), detached);

        match &attached {
            LayoutSnapshot::Group { axis, children, .. } => {
                assert!(matches!(axis, SnapshotAxis::Horizontal));
                assert_eq!(children.len(), 2);
                // Every pane in the existing subtree must have active=false
                // — only the moved-in pane on the right keeps focus.
                let LayoutSnapshot::Group { children: inner, .. } = &children[0] else {
                    panic!("expected inner group");
                };
                for child in inner {
                    if let LayoutSnapshot::Pane(p) = child {
                        assert!(!p.active, "existing pane should be deactivated");
                    }
                }
                let LayoutSnapshot::Pane(moved) = &children[1] else {
                    panic!("expected moved pane on right");
                };
                assert!(moved.active);
                assert_eq!(moved.items[0].kind, "moved");
            }
            other => panic!("expected horizontal group, got {other:?}"),
        }
    }

    #[test]
    fn clear_active_flags_recurses_into_stacks_and_groups() {
        let mut layout = LayoutSnapshot::Group {
            axis: SnapshotAxis::Vertical,
            flexes: None,
            children: vec![
                pane("a", true),
                LayoutSnapshot::Stack {
                    members: vec![pane("b", true), pane("c", true)],
                    active: 0,
                },
            ],
        };
        clear_active_flags(&mut layout);
        let LayoutSnapshot::Group { children, .. } = &layout else {
            panic!()
        };
        if let LayoutSnapshot::Pane(p) = &children[0] {
            assert!(!p.active);
        }
        let LayoutSnapshot::Stack { members, active } = &children[1] else {
            panic!()
        };
        assert_eq!(*active, 0, "stack active-tab index unchanged");
        for m in members {
            if let LayoutSnapshot::Pane(p) = m {
                assert!(!p.active);
            }
        }
    }

    #[test]
    fn nested_group_collapses_after_break() {
        let inner = group(vec![pane("inner-l", false), pane("inner-r", true)]);
        let layout = group(vec![pane("outer-l", false), inner]);
        let (remaining, _broken) = split_off_active(layout).expect("nested split");
        // After taking inner-r, the inner group collapses to inner-l. The
        // outer group still has two children (outer-l, inner-l) so it
        // stays a Group.
        match &remaining {
            LayoutSnapshot::Group { children, .. } => {
                assert_eq!(children.len(), 2);
                match &children[1] {
                    LayoutSnapshot::Pane(p) => assert_eq!(p.items[0].kind, "inner-l"),
                    other => panic!("expected collapsed leaf, got {other:?}"),
                }
            }
            other => panic!("expected outer group, got {other:?}"),
        }
    }
}
