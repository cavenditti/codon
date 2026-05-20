use std::{
    cell::Cell,
    path::PathBuf,
    time::{Duration, Instant},
};

use file_manager::{CacheOutcome, SwitchKind, record_switch_timing};
use gpui::{Action, App, AppContext as _, Context, Global, Window, actions};
use schemars::JsonSchema;
use serde::Deserialize;
use workspace::{
    CloseActiveItem, Workspace,
    notifications::{NotificationId, NotifyTaskExt as _, simple_message_notification::MessageNotification},
};

use crate::{
    break_pane,
    picker::SessionSwitchModal,
    registry::SessionRegistry,
    resize_sticky::{ResizeDir, ResizeStickyOverlay},
    runtime::{WindowRuntime, WindowRuntimeCache},
    session::{Session, SessionId},
    swap,
    window_rename::WindowRenameModal,
};

actions!(
    codon_session,
    [
        /// Create a new session named after the current workspace cwd.
        SessionNew,
        /// Open a fuzzy picker to switch sessions.
        SessionSwitch,
        /// Open the tmux-style overview, pre-positioned on the active
        /// session row. Sessions are top-level rows; windows nest under
        /// them. `j`/`k` move between visible rows, `h`/`l` collapse or
        /// expand, Enter attaches.
        SessionOverview,
        /// Open a fuzzy picker to switch windows within the active session.
        WindowSwitch,
        /// Open the tmux-style overview, pre-positioned on the active
        /// window row. Same modal as `SessionOverview`; differs only in
        /// where the initial selection lands.
        WindowOverview,
        /// Rename the active session.
        SessionRename,
        /// Close the active session (refuses to remove the last one).
        SessionClose,
        /// Add a new window to the active session.
        WindowNew,
        /// Move to the next window in the active session.
        WindowNext,
        /// Move to the previous window in the active session.
        WindowPrev,
        /// Toggle to the previously-active window in the active session
        /// (tmux `prefix l`). No-op when no previous window is recorded.
        WindowLast,
        /// Close the active window in the active session. Prompts to save
        /// or discard dirty items before destroying the pane group.
        WindowClose,
        /// Rename the active window. Opens a single-line text input modal
        /// seeded with the current name; empty input cancels (tmux
        /// `prefix ,`).
        WindowRename,
        /// Promote the active pane in the current window into a new window
        /// of its own (tmux `prefix !`).
        BreakPaneToWindow,
        /// Close the active tab. Falls back to closing the pane, then the
        /// codon-session window, then replacing the center with an empty
        /// pane. Never closes the OS window — that's reserved for
        /// `cmd-shift-w` / `cmd-shift-q`.
        ///
        /// Renamed from `SafeCloseActiveItem` in phase 20 — see
        /// REQ:codon/keymap-vocabulary#c-verb-collapse-close. The
        /// previous name remains registered as a deprecated alias
        /// (`SafeCloseActiveItem` below) for one release cycle so
        /// existing `~/.config/codon/codon.toml` overrides keep parsing;
        /// new defaults bind `codon_session::Close` everywhere.
        Close,
        /// Deprecated alias for [`Close`] — kept for one release cycle
        /// so user keymap overrides that still reference the old name
        /// keep parsing. Re-uses the same dispatch path.
        SafeCloseActiveItem,
        /// Bypass the close cascade and just close the active item.
        /// Unbound by default — exists only for the rare case where a
        /// user wants the raw `pane::CloseActiveItem` semantics without
        /// the codon item → pane → window → empty-pane fall-through.
        CloseForce,
        /// Hold-to-quit guard for `cmd-q`. Single tap shows a toast asking
        /// the user to hold; continued auto-repeat invocations over ~1.5s
        /// dispatch `zed::Quit`. Releasing before the threshold leaves the
        /// app running. `cmd-shift-q` remains an immediate-quit escape
        /// hatch.
        HoldQuit,
        /// Focus the most-recently-active terminal in the current window,
        /// or open a new terminal in the active pane if none exists.
        GotoOrOpenTerminal,
        /// Focus the most-recently-active file manager in the current
        /// window, or open a new one in the active pane if none exists.
        GotoOrOpenFileManager,
        /// Focus the most-recently-active editor in the current window,
        /// or open a new buffer in the active pane if none exists.
        GotoOrOpenEditor,
        /// Open a diff view. Today this is a thin wrapper that dispatches
        /// Zed's upstream `git::Diff` (working tree vs HEAD); arbitrary
        /// file-vs-file diffs are deferred until the phase-4 codon diff
        /// pane lands. See `diff_open.rs`.
        DiffOpen,
        /// Resize the active pane leftward by one cell and arm the
        /// sticky-resize overlay so bare `h/j/k/l` keep resizing for
        /// `STICKY_TIMEOUT` after the chord. See `resize_sticky.rs`.
        ResizePaneLeft,
        /// Resize the active pane downward and arm the sticky overlay.
        ResizePaneDown,
        /// Resize the active pane upward and arm the sticky overlay.
        ResizePaneUp,
        /// Resize the active pane rightward and arm the sticky overlay.
        ResizePaneRight,
    ]
);

/// Switch to the window at the given zero-based index in the active session.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = codon_session)]
#[serde(deny_unknown_fields)]
pub struct WindowGoto(pub usize);

/// Move the active pane into the existing window at the given zero-based
/// index within the active session (mirrors tmux `join-pane -t :N`). The
/// pane is grafted onto the target window's layout as a new horizontal
/// split. Out-of-range or self-target invocations are silent no-ops
/// (with a toast on out-of-range). If moving the only pane out of the
/// source window leaves it empty, the source window is closed —
/// promoting the next window to active, same fallback that
/// `WindowClose` uses.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = codon_session)]
#[serde(deny_unknown_fields)]
pub struct MovePaneToWindow(pub usize);

/// Write the focused pane's [`codon_mode::SelectionSource::current_selection`]
/// to the register armed via a `"<char>` prefix, or — when no prefix
/// is armed — clear the slot. The action is the phase-19 proof point
/// that selection-producing verbs honour the register prefix; the
/// follow-up tasks add more producers (`MarkAll` in fm,
/// `SelectByPattern` in git, etc.).
///
/// No-op when no register is armed *and* no payload is supplied, so
/// binding it to a bare key (e.g. `Y`) without a prefix doesn't crash
/// — it just logs a debug line.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = codon_session)]
#[serde(deny_unknown_fields)]
pub struct YankSelection;

pub fn register(cx: &mut App) {
    // `YankSelection` is a selection-producer — when paired with a
    // `"<char>` arming, its output gets routed into the named
    // register. Producer kind is the pane's primary kind; the
    // file-manager produces `File` selections so we register that.
    // Future task work (when other panes implement SelectionSource +
    // YankSelection paths) will register additional producer kinds
    // — the registry holds one kind per action type, so polymorphic
    // producers will need a wrapper-per-pane or a dispatcher-side
    // override.
    let registry = cx.global_mut::<command_palette_hooks::ActionAcceptsRegistry>();
    registry.register_produces::<YankSelection>(command_palette_hooks::ObjectKind::File);
}

/// Wire workspace-scoped action handlers. Call from the workspace
/// initialization hook (e.g. `cx.observe_new(|workspace, _, cx| ...)`).
pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_session_new);
    workspace.register_action(handle_session_switch);
    workspace.register_action(handle_session_overview);
    workspace.register_action(handle_window_switch);
    workspace.register_action(handle_window_overview);
    workspace.register_action(handle_session_close);
    workspace.register_action(handle_window_new);
    workspace.register_action(handle_window_next);
    workspace.register_action(handle_window_prev);
    workspace.register_action(handle_window_last);
    workspace.register_action(handle_window_close);
    workspace.register_action(handle_window_rename);
    workspace.register_action(handle_break_pane_to_window);
    workspace.register_action(handle_move_pane_to_window);
    workspace.register_action(handle_close);
    workspace.register_action(handle_safe_close_active_item);
    workspace.register_action(handle_close_force);
    workspace.register_action(handle_hold_quit);
    workspace.register_action(handle_resize_pane_left);
    workspace.register_action(handle_resize_pane_down);
    workspace.register_action(handle_resize_pane_up);
    workspace.register_action(handle_resize_pane_right);
    crate::goto_or_open::register_for_workspace(workspace);
    crate::diff_open::register_for_workspace(workspace);
    crate::contextual_split::register_for_workspace(workspace);
    crate::registers::register_for_workspace(workspace);
    workspace.register_action(handle_yank_selection);
}

fn handle_yank_selection(
    workspace: &mut Workspace,
    _: &YankSelection,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    use codon_mode::SelectionSource as _;
    let store = crate::registers::RegisterStore::global(cx);
    let Some(pending) = store.take_pending() else {
        log::debug!("codon-session: YankSelection without armed register — noop");
        return;
    };
    // The only `SelectionSource` impl shipped this task is `file-manager`.
    // Look at the workspace's active pane and ask the trait for the
    // current selection if it's hosting an FM. Cross-pane SelectionSource
    // discovery (editor / git / diagnostics / agent) is the follow-up
    // task — see `phase-19/selection-registers-helix-interop` for the
    // editor side specifically.
    let active_item = workspace.active_pane().read(cx).active_item();
    let selection = active_item
        .and_then(|item| item.downcast::<file_manager::FileManager>())
        .map(|fm| fm.read(cx).current_selection());
    let Some(selection) = selection else {
        log::debug!(
            "codon-session: YankSelection target pane has no SelectionSource impl — \
             cross-pane lookup is a follow-up"
        );
        return;
    };
    if !store.write(pending.name, selection) {
        log::warn!(
            "codon-session: YankSelection wrote register '{}' but no active session — \
             call SessionRegistry::set_active first",
            pending.name
        );
    }
}

fn handle_resize_pane_left(
    workspace: &mut Workspace,
    _: &ResizePaneLeft,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    ResizeStickyOverlay::arm(ResizeDir::Left, workspace, window, cx);
}

fn handle_resize_pane_down(
    workspace: &mut Workspace,
    _: &ResizePaneDown,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    ResizeStickyOverlay::arm(ResizeDir::Down, workspace, window, cx);
}

fn handle_resize_pane_up(
    workspace: &mut Workspace,
    _: &ResizePaneUp,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    ResizeStickyOverlay::arm(ResizeDir::Up, workspace, window, cx);
}

fn handle_resize_pane_right(
    workspace: &mut Workspace,
    _: &ResizePaneRight,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    ResizeStickyOverlay::arm(ResizeDir::Right, workspace, window, cx);
}

fn handle_session_new(
    workspace: &mut Workspace,
    _: &SessionNew,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    // If a session is already attached, stash its center so the new session
    // gets a clean slate instead of inheriting the live pane tree (terminals
    // and all). On first launch — no active session — the existing center
    // becomes the new session's first window, matching `ensure_active_session`.
    let had_active = SessionRegistry::global(cx).active_id().is_some();
    if had_active {
        stash_outgoing(workspace, window, cx);
    }
    if create_session_for_workspace(workspace, cx).is_some() && had_active {
        workspace.replace_center_with_empty_pane(window, cx);
    }
}

/// Create a session anchored at the workspace's first visible worktree (or
/// `cwd` as a fallback) and mark it active. Returns the new session id.
fn create_session_for_workspace(
    workspace: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> Option<crate::session::SessionId> {
    let project = workspace.project().read(cx);
    let cwd: PathBuf = project
        .visible_worktrees(cx)
        .next()
        .map(|wt| wt.read(cx).abs_path().to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let base = cwd
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| format!("session-{}", chrono::Local::now().format("%H%M%S")));
    let unique = unique_name(&base, cx);
    let session = Session::new(unique.clone(), cwd);
    let id = session.id;

    let registry = SessionRegistry::global(cx);
    if let Err(err) = registry.upsert(session) {
        log::error!("could not create session: {err:?}");
        return None;
    }
    if let Err(err) = registry.set_active(id) {
        log::warn!("could not set active session: {err:?}");
    }
    workspace.set_session_id(Some(id.to_string()));
    persist_lifecycle(cx);
    cx.notify();
    log::info!("created session '{unique}' ({id})");
    Some(id)
}

/// Return the active session id, auto-creating one if none exists. Used by
/// commands like `WindowNew` that should always work — pressing a window key
/// from a fresh launch shouldn't be a no-op.
fn ensure_active_session(
    workspace: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> Option<crate::session::SessionId> {
    if let Some(id) = SessionRegistry::global(cx).active_id() {
        return Some(id);
    }
    create_session_for_workspace(workspace, cx)
}

fn handle_session_switch(
    workspace: &mut Workspace,
    _: &SessionSwitch,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let weak = workspace.weak_handle();
    workspace.toggle_modal(window, cx, move |window, cx| {
        SessionSwitchModal::new(weak, window, cx)
    });
}

fn handle_session_overview(
    workspace: &mut Workspace,
    _: &SessionOverview,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let weak = workspace.weak_handle();
    // c-overview-defer-capture: the modal's visual summary is sourced
    // from the active session's last persisted `Window::layout`. That
    // snapshot lags the runtime cache by at most one eviction (handled
    // by `WindowRuntimeCache::evict_and_persist`), which is fine for
    // the modal's pane-count / shorthand rendering. Skip the expensive
    // `swap::capture` walk so the modal-open chord stays responsive
    // even on 8+ pane layouts.
    workspace.toggle_modal(window, cx, move |window, cx| {
        crate::overview::OverviewModal::new(
            crate::overview::InitialFocus::Session,
            weak,
            None,
            window,
            cx,
        )
    });
}

fn handle_window_switch(
    workspace: &mut Workspace,
    _: &WindowSwitch,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let weak = workspace.weak_handle();
    workspace.toggle_modal(window, cx, move |window, cx| {
        crate::window_picker::WindowSwitchModal::new(weak, window, cx)
    });
}

fn handle_window_overview(
    workspace: &mut Workspace,
    _: &WindowOverview,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let weak = workspace.weak_handle();
    // c-overview-defer-capture: see `handle_session_overview` —
    // identical reasoning, only the initial-row focus differs.
    workspace.toggle_modal(window, cx, move |window, cx| {
        crate::overview::OverviewModal::new(
            crate::overview::InitialFocus::Window,
            weak,
            None,
            window,
            cx,
        )
    });
}

fn handle_session_close(
    workspace: &mut Workspace,
    _: &SessionClose,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let Some(active) = registry.active_id() else {
        return;
    };
    if let Err(err) = registry.remove(active) {
        log::warn!("could not close session: {err:?}");
        return;
    }
    WindowRuntimeCache::global(cx).drop_session(active);
    if let Some(next) = registry.active_id() {
        workspace.set_session_id(Some(next.to_string()));
    } else {
        workspace.set_session_id(None);
    }
    persist_lifecycle(cx);
    cx.notify();
}

fn handle_window_new(
    workspace: &mut Workspace,
    _: &WindowNew,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(active_id) = ensure_active_session(workspace, cx) else {
        log::warn!("could not establish a session, ignoring WindowNew");
        return;
    };
    let registry = SessionRegistry::global(cx);
    let Some(mut session) = registry.get(active_id) else {
        return;
    };

    let outgoing_id = session.active().map(|w| w.id);
    let snapshot = swap::capture(workspace, window, cx);
    let runtime = capture_runtime(workspace, cx);
    if let Some(active_window) = session.active_mut() {
        active_window.layout = Some(snapshot);
    }
    if let (Some(outgoing_window_id), Some(rt)) = (outgoing_id, runtime) {
        WindowRuntimeCache::global(cx).insert(active_id, outgoing_window_id, rt);
    }

    session.add_window(None);
    let new_index = session.windows.len() - 1;
    session.set_active_window(new_index);
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save new window: {err:?}");
    }
    persist_lifecycle(cx);

    workspace.replace_center_with_empty_pane(window, cx);
}

fn handle_window_next(
    workspace: &mut Workspace,
    _: &WindowNext,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    cycle_window(workspace, 1, window, cx);
}

fn handle_window_prev(
    workspace: &mut Workspace,
    _: &WindowPrev,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    cycle_window(workspace, -1, window, cx);
}

fn handle_window_last(
    workspace: &mut Workspace,
    _: &WindowLast,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let Some(active_id) = registry.active_id() else {
        return;
    };
    let Some(session) = registry.get(active_id) else {
        return;
    };
    let Some(prev_idx) = session.previous_window else {
        log::debug!("WindowLast: no previous window recorded");
        return;
    };
    let Some(target) = session.windows.get(prev_idx).map(|w| w.id) else {
        log::debug!(
            "WindowLast: previous_window index {prev_idx} out of range ({} windows)",
            session.windows.len()
        );
        return;
    };
    crate::window_indicator::switch_to_window(workspace, target, window, cx);
}

fn handle_window_close(
    workspace: &mut Workspace,
    _: &WindowClose,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let Some(active_id) = registry.active_id() else {
        return;
    };
    let Some(session) = registry.get(active_id) else {
        return;
    };
    if session.windows.len() <= 1 {
        return;
    }

    // `prompt_to_save_or_discard_dirty_items` short-circuits to Ok(true)
    // when no items are dirty, so calling it unconditionally costs
    // nothing on the clean path. When dirty items exist it shows the
    // upstream save/discard/cancel dialog; only proceed when the user
    // confirms.
    let prompt = workspace.prompt_to_save_or_discard_dirty_items(window, cx);
    cx.spawn_in(window, async move |workspace, cx| {
        match prompt.await {
            Ok(true) => {
                workspace
                    .update_in(cx, |workspace, window, cx| {
                        finish_window_close(workspace, active_id, window, cx);
                    })
                    .ok();
            }
            Ok(false) => {
                log::debug!("WindowClose cancelled by user");
            }
            Err(err) => {
                log::warn!("WindowClose save prompt failed: {err:?}");
            }
        }
    })
    .detach();
}

/// Actually remove the active window from the session and switch the
/// pane tree to the previous (or next) window's layout. Caller has
/// already cleared any dirty-item save prompts.
fn finish_window_close(
    workspace: &mut Workspace,
    active_id: crate::session::SessionId,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let Some(mut session) = registry.get(active_id) else {
        return;
    };
    if session.windows.len() <= 1 {
        return;
    }

    let removed_idx = session.active_window;
    let removed_id = session.windows[removed_idx].id;

    // Pick a target to land on after the removal. Prefer the last-active
    // window (tmux behavior) when valid; otherwise fall back to the
    // sibling at the same index (which post-removal points at the next
    // window) or the new last index if we were closing the tail.
    let target_after = match session.previous_window {
        Some(p) if p != removed_idx && p < session.windows.len() => {
            if p > removed_idx { p - 1 } else { p }
        }
        _ => removed_idx.min(session.windows.len().saturating_sub(2)),
    };

    // Drop the in-memory runtime cache entry for the doomed window so we
    // don't restore it later by accident. `take` returns the evicted
    // entry (if any) which we intentionally discard — the cache miss is
    // the desired outcome.
    let _evicted = WindowRuntimeCache::global(cx).take(active_id, removed_id);

    session.remove_window(removed_id);
    if target_after < session.windows.len() {
        session.active_window = target_after;
    }
    session.previous_window = None;

    let incoming_window_id = session.windows.get(session.active_window).map(|w| w.id);
    let incoming_layout = session
        .windows
        .get(session.active_window)
        .and_then(|w| w.layout.clone());

    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save after window close: {err:?}");
    }
    persist_lifecycle(cx);
    cx.notify();

    // Swap the visible pane tree to whatever the new active window has.
    let cache = WindowRuntimeCache::global(cx);
    let cached_runtime = incoming_window_id.and_then(|id| cache.take(active_id, id));
    if let Some(rt) = cached_runtime {
        workspace.restore_center_root(rt.root, rt.active_pane, window, cx);
    } else if let Some(layout) = incoming_layout {
        let weak = workspace.weak_handle();
        swap::apply(workspace, layout, window, cx).detach_and_notify_err(weak, window, cx);
    } else {
        workspace.replace_center_with_empty_pane(window, cx);
    }
}

fn handle_window_rename(
    workspace: &mut Workspace,
    _: &WindowRename,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if SessionRegistry::global(cx).active_id().is_none() {
        return;
    }
    let weak = workspace.weak_handle();
    workspace.toggle_modal(window, cx, move |window, cx| {
        WindowRenameModal::new(weak, window, cx)
    });
}

fn handle_break_pane_to_window(
    workspace: &mut Workspace,
    _: &BreakPaneToWindow,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let Some(active_id) = ensure_active_session(workspace, cx) else {
        return;
    };
    let registry = SessionRegistry::global(cx);
    let Some(mut session) = registry.get(active_id) else {
        return;
    };

    let snapshot = swap::capture(workspace, window, cx);
    let Some((remaining, broken)) = break_pane::split_off_active(snapshot) else {
        // Only one pane — `BreakPaneToWindow` is a no-op. Surface a brief
        // toast so the user knows the chord registered.
        workspace.show_notification(
            workspace::notifications::NotificationId::unique::<BreakPaneToWindow>(),
            cx,
            |cx| {
                cx.new(|cx| {
                    workspace::notifications::simple_message_notification::MessageNotification::new(
                        "Window already has only one pane.",
                        cx,
                    )
                })
            },
        );
        return;
    };

    // Stash the remaining layout on the *current* window before we move
    // off it — same pattern as cycle_window/switch_to_window.
    if let Some(active) = session.active_mut() {
        active.layout = Some(remaining.clone());
    }

    // Append the new window seeded with just the broken pane, and mark
    // it active. Reuses set_active_window so previous_window is set to
    // the outgoing index automatically.
    let new_window_id = session.add_window(None);
    if let Some(idx) = session.windows.iter().position(|w| w.id == new_window_id) {
        if let Some(slot) = session.windows.get_mut(idx) {
            slot.layout = Some(broken.clone());
        }
        let outgoing = session.active_window;
        session.set_active_window(idx);
        // set_active_window leaves previous_window pointing at the
        // outgoing window, which is what we want for `WindowLast`.
        let _ = outgoing;
    }
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save after break-pane: {err:?}");
    }
    persist_lifecycle(cx);

    // Apply the broken-pane layout in the visible center. Items
    // re-hydrate via SerializableItemRegistry.
    let weak = workspace.weak_handle();
    swap::apply(workspace, broken, window, cx).detach_and_notify_err(weak, window, cx);
}

/// Move the active pane into the existing window at the given index
/// (tmux `join-pane -t :N`). Grafts the detached pane onto the target
/// window's layout as a new horizontal split. If the source window is
/// left empty by the move, it is closed and the session promotes the
/// target to active.
fn handle_move_pane_to_window(
    workspace: &mut Workspace,
    action: &MovePaneToWindow,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let target_idx = action.0;

    let Some(active_id) = ensure_active_session(workspace, cx) else {
        return;
    };
    let registry = SessionRegistry::global(cx);
    let Some(mut session) = registry.get(active_id) else {
        return;
    };

    if target_idx == session.active_window {
        log::trace!("MovePaneToWindow: target index {target_idx} == active, no-op");
        return;
    }
    if target_idx >= session.windows.len() {
        workspace.show_notification(
            workspace::notifications::NotificationId::unique::<MovePaneToWindow>(),
            cx,
            move |cx| {
                cx.new(|cx| {
                    workspace::notifications::simple_message_notification::MessageNotification::new(
                        format!(
                            "Window {} does not exist (active session has {}).",
                            target_idx + 1,
                            registry_window_count(cx, active_id),
                        ),
                        cx,
                    )
                })
            },
        );
        return;
    }

    let snapshot = swap::capture(workspace, window, cx);
    let source_idx = session.active_window;
    let source_window_id = session.windows[source_idx].id;
    let target_window_id = session.windows[target_idx].id;
    let target_layout = session.windows[target_idx].layout.clone();

    // Detach the active pane from the source layout. `split_off_active`
    // returns None when the source has only one pane — in that case the
    // whole snapshot *is* the pane to move, and the source window will
    // be closed once the move lands.
    let (broken, source_remaining) = match break_pane::split_off_active(snapshot.clone()) {
        Some((remaining, broken)) => (broken, Some(remaining)),
        None => {
            let mut moved = snapshot;
            if let workspace::codon_bridge::LayoutSnapshot::Pane(p) = &mut moved {
                p.active = true;
            }
            (moved, None)
        }
    };

    let merged = break_pane::attach_pane_horizontally(target_layout, broken);
    if let Some(slot) = session.windows.get_mut(target_idx) {
        slot.layout = Some(merged.clone());
    }

    let cache = WindowRuntimeCache::global(cx);
    // Both windows' cached runtimes are now stale — drop them so the
    // persisted layouts are the source of truth on this switch.
    let _ = cache.take(active_id, target_window_id);
    let _ = cache.take(active_id, source_window_id);

    if let Some(remaining) = source_remaining {
        // Multi-pane source survives — update its layout and switch.
        // We can't use `switch_to_window` here because it would
        // re-capture the visible workspace (which still contains the
        // broken pane until we apply the new target layout below) and
        // clobber our `remaining` write. Inline the switch instead,
        // mirroring `finish_window_close`'s structure.
        if let Some(slot) = session.windows.get_mut(source_idx) {
            slot.layout = Some(remaining);
        }
        session.set_active_window(target_idx);
        if let Err(err) = registry.upsert(session) {
            log::warn!("could not save after move-pane: {err:?}");
        }
        persist_lifecycle(cx);
        cx.notify();

        let weak = workspace.weak_handle();
        swap::apply(workspace, merged, window, cx).detach_and_notify_err(weak, window, cx);
        return;
    }

    // Single-pane source — close it. `remove_window` shifts indexes
    // for us; resolve the target's new position by id afterwards.
    session.remove_window(source_window_id);
    if let Some(new_target_idx) = session.windows.iter().position(|w| w.id == target_window_id) {
        session.set_active_window(new_target_idx);
    }
    session.previous_window = None;

    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save after move-pane (single-pane source): {err:?}");
    }
    persist_lifecycle(cx);
    cx.notify();

    let weak = workspace.weak_handle();
    swap::apply(workspace, merged, window, cx).detach_and_notify_err(weak, window, cx);
}

fn registry_window_count(cx: &gpui::App, active_id: crate::session::SessionId) -> usize {
    SessionRegistry::global(cx)
        .get(active_id)
        .map(|s| s.windows.len())
        .unwrap_or(0)
}

/// Bypass the close cascade — just close the active item using Zed's
/// raw `pane::CloseActiveItem`. Wired for the optional
/// `codon_session::CloseForce` action; not bound by default. See
/// REQ:codon/keymap-vocabulary#c-verb-collapse-close.
fn handle_close_force(
    workspace: &mut Workspace,
    _: &CloseForce,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let active_pane = workspace.active_pane().clone();
    active_pane.update(cx, |pane, cx| {
        pane.close_active_item(
            &CloseActiveItem {
                save_intent: None,
                close_pinned: false,
            },
            window,
            cx,
        )
        .detach_and_log_err(cx);
    });
}

/// `codon_session::Close` handler — the phase-20 rename of the close
/// cascade verb. Delegates to the shared implementation so the
/// deprecated `SafeCloseActiveItem` alias keeps the same semantics.
fn handle_close(
    workspace: &mut Workspace,
    _: &Close,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    close_cascade(workspace, window, cx);
}

/// Close the active item, falling back through increasingly broad scopes —
/// pane, codon-session window, then an empty-pane reset — so the OS window
/// never disappears as a side effect of a close-tab keystroke.
fn handle_safe_close_active_item(
    workspace: &mut Workspace,
    _: &SafeCloseActiveItem,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    log::info!(
        "codon_session::SafeCloseActiveItem is deprecated; rebind to codon_session::Close"
    );
    close_cascade(workspace, window, cx);
}

fn close_cascade(
    workspace: &mut Workspace,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let active_pane = workspace.active_pane().clone();
    let item_count = active_pane.read(cx).items_len();

    // (1) Plenty of items in the active pane — just close the one in front.
    if item_count > 1 {
        active_pane.update(cx, |pane, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
            .detach_and_log_err(cx);
        });
        return;
    }

    // (2) Last item in this pane, but the workspace has other panes —
    // close the entire pane via the existing Pane::Remove event, which
    // workspace::handle_pane_event already routes to remove_pane.
    if workspace.panes().len() > 1 {
        active_pane.update(cx, |_, cx| {
            cx.emit(workspace::pane::Event::Remove { focus_on_pane: None });
        });
        return;
    }

    // (3) Single-pane workspace, but the active session has multiple
    // windows — delegate to the existing window-close handler.
    let multi_window_session = SessionRegistry::global(cx)
        .active()
        .is_some_and(|s| s.windows.len() > 1);
    if multi_window_session {
        window.dispatch_action(Box::new(WindowClose), cx);
        return;
    }

    // (4) Truly the last pane in the last window. Close the active item
    // (preserving Zed's dirty-buffer save prompt) if there is one; the
    // pane lingers as empty rather than auto-closing the OS window
    // (vendored Zed gates that branch via Workspace::set_close_window_on_last_tab).
    if item_count == 1 {
        active_pane.update(cx, |pane, cx| {
            pane.close_active_item(
                &CloseActiveItem {
                    save_intent: None,
                    close_pinned: false,
                },
                window,
                cx,
            )
            .detach_and_log_err(cx);
        });
        return;
    }

    // item_count == 0: already-empty pane, last in the workspace. Replace
    // with a fresh empty pane so the user lands somewhere usable.
    workspace.replace_center_with_empty_pane(window, cx);
}

fn cycle_window(
    workspace: &mut Workspace,
    delta: i32,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let Some(active_id) = registry.active_id() else {
        return;
    };
    let Some(mut session) = registry.get(active_id) else {
        return;
    };
    if session.windows.len() < 2 {
        return;
    }
    let len = session.windows.len() as i32;
    let new_idx = ((session.active_window as i32 + delta).rem_euclid(len)) as usize;

    let outgoing_id = session.active().map(|w| w.id);
    let _ = take_restore_timing();
    // c-skip-capture-on-cache-hit: building a `LayoutSnapshot` here is
    // wasted work on the intra-session window-switch fast path — the
    // runtime cache below holds the live `Member` tree, which IS the
    // freshest copy. The snapshot is rebuilt lazily on eviction /
    // detach / shutdown via `WindowRuntimeCache::evict_and_persist`.
    let capture_ms = 0.0_f32;
    let runtime_started = Instant::now();
    let runtime = capture_runtime(workspace, cx);
    let runtime_capture_ms = elapsed_ms(runtime_started);
    if let Some(active) = session.active_mut() {
        active.layout_stale = true;
    }
    if let (Some(outgoing_window_id), Some(rt)) = (outgoing_id, runtime) {
        WindowRuntimeCache::global(cx).insert(active_id, outgoing_window_id, rt);
    }

    session.set_active_window(new_idx);
    let incoming_window_id = session.windows.get(new_idx).map(|w| w.id);
    let incoming_layout = session.windows.get(new_idx).and_then(|w| w.layout.clone());
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save window switch: {err:?}");
    }
    // Intra-session window cycle: debounce off the synchronous switch
    // path. Rapid `prefix Tab` mashing coalesces into a single flush
    // ~2 s after the last switch (`c-defer-persist`).
    let persist_started = Instant::now();
    persist_debounced(cx);
    let persist_scheduled_ms = elapsed_ms(persist_started);

    let cache = WindowRuntimeCache::global(cx);
    let cached_runtime = incoming_window_id.and_then(|id| cache.take(active_id, id));
    let restore_started = Instant::now();
    let cache_outcome = if let Some(rt) = cached_runtime {
        log::debug!(
            "restoring window {:?} from in-memory runtime cache",
            incoming_window_id
        );
        workspace.restore_center_root(rt.root, rt.active_pane, window, cx);
        CacheOutcome::Hit
    } else if let Some(layout) = incoming_layout {
        log::debug!(
            "restoring window {:?} from persisted snapshot (no runtime cache hit)",
            incoming_window_id
        );
        let weak = workspace.weak_handle();
        swap::apply(workspace, layout, window, cx).detach_and_notify_err(weak, window, cx);
        CacheOutcome::Miss
    } else {
        log::debug!(
            "no state for window {:?}; opening fresh empty pane",
            incoming_window_id
        );
        workspace.replace_center_with_empty_pane(window, cx);
        CacheOutcome::Cold
    };
    let restore_ms = take_restore_timing()
        .map(|(ms, _)| ms)
        .unwrap_or_else(|| elapsed_ms(restore_started));

    record_switch_timing(
        SwitchKind::Window,
        outgoing_id.map(|w| w.0),
        incoming_window_id.map(|w| w.0),
        capture_ms,
        runtime_capture_ms,
        restore_ms,
        persist_scheduled_ms,
        cache_outcome,
    );
}

#[inline]
fn elapsed_ms(start: Instant) -> f32 {
    start.elapsed().as_secs_f64() as f32 * 1000.0
}

fn capture_runtime(workspace: &Workspace, cx: &App) -> Option<WindowRuntime> {
    let root = workspace.center().root.clone();
    let active_pane = Some(workspace.active_pane().clone());
    let _ = cx;
    Some(WindowRuntime { root, active_pane })
}

/// Capture the workspace's current center as the *currently-active*
/// session's active-window layout + runtime. Idempotent if there is no
/// active session. Caller is responsible for flipping the active session
/// afterwards.
fn stash_outgoing(
    workspace: &mut Workspace,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let _ = stash_outgoing_timed(workspace, cx);
}

#[derive(Default, Clone, Copy)]
struct StashTiming {
    outgoing_window: Option<u64>,
    capture_ms: f32,
    runtime_capture_ms: f32,
}

/// Same as [`stash_outgoing`], but returns per-phase timing for the
/// switch-trace harness. Inline timing is preferable to wrapping at the
/// call site because the conditional early-returns (no active session,
/// no active window) need to surface as zero values rather than an
/// outer-`Instant` measurement that misattributes the no-op cost.
fn stash_outgoing_timed(
    workspace: &mut Workspace,
    cx: &mut Context<Workspace>,
) -> StashTiming {
    let registry = SessionRegistry::global(cx);
    let Some(outgoing_session_id) = registry.active_id() else {
        return StashTiming::default();
    };
    let Some(mut session) = registry.get(outgoing_session_id) else {
        return StashTiming::default();
    };
    let Some(outgoing_window_id) = session.active().map(|w| w.id) else {
        return StashTiming::default();
    };

    // c-skip-capture-on-cache-hit: same fast-path reasoning as
    // `cycle_window` — we stash a runtime entry below that holds the
    // live `Member` tree, so a fresh `LayoutSnapshot` here would be
    // immediately stale. Mark the persisted layout as stale and rely
    // on `WindowRuntimeCache::evict_and_persist` to materialize a
    // snapshot at eviction / detach / shutdown.
    let capture_ms = 0.0_f32;
    let runtime_started = Instant::now();
    let runtime = capture_runtime(workspace, cx);
    let runtime_capture_ms = elapsed_ms(runtime_started);

    if let Some(active) = session.active_mut() {
        active.layout_stale = true;
    }
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not stash outgoing session layout: {err:?}");
    }
    if let Some(rt) = runtime {
        WindowRuntimeCache::global(cx).insert(outgoing_session_id, outgoing_window_id, rt);
    }

    StashTiming {
        outgoing_window: Some(outgoing_window_id.0),
        capture_ms,
        runtime_capture_ms,
    }
}

#[derive(Default, Clone, Copy)]
struct RestoreTiming {
    incoming_window: Option<u64>,
    restore_ms: f32,
    cache_outcome: Option<CacheOutcome>,
}

/// Restore (or initialize) the workspace center to the active window of
/// `target_id`. Prefers the in-memory runtime cache, falls back to the
/// persisted `LayoutSnapshot`, and finally drops in a fresh empty pane.
/// Returns timing + cache-outcome for the switch-trace harness.
fn restore_incoming_timed(
    workspace: &mut Workspace,
    target_id: SessionId,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> RestoreTiming {
    let registry = SessionRegistry::global(cx);
    let session = registry.get(target_id);
    let incoming_window_id = session.as_ref().and_then(|s| s.active().map(|w| w.id));
    let incoming_layout = session.as_ref().and_then(|s| s.active().and_then(|w| w.layout.clone()));

    let cached = incoming_window_id
        .and_then(|id| WindowRuntimeCache::global(cx).take(target_id, id));

    let _ = take_restore_timing();
    let restore_started = Instant::now();
    let outcome = if let Some(rt) = cached {
        log::debug!(
            "attach: restoring session {target_id} window {:?} from runtime cache",
            incoming_window_id
        );
        workspace.restore_center_root(rt.root, rt.active_pane, window, cx);
        CacheOutcome::Hit
    } else if let Some(layout) = incoming_layout {
        log::debug!(
            "attach: restoring session {target_id} window {:?} from persisted snapshot",
            incoming_window_id
        );
        let weak = workspace.weak_handle();
        swap::apply(workspace, layout, window, cx).detach_and_notify_err(weak, window, cx);
        CacheOutcome::Miss
    } else {
        log::debug!(
            "attach: no state for session {target_id} window {:?}; opening fresh empty pane",
            incoming_window_id
        );
        workspace.replace_center_with_empty_pane(window, cx);
        CacheOutcome::Cold
    };
    let restore_ms = take_restore_timing()
        .map(|(ms, _)| ms)
        .unwrap_or_else(|| elapsed_ms(restore_started));
    RestoreTiming {
        incoming_window: incoming_window_id.map(|w| w.0),
        restore_ms,
        cache_outcome: Some(outcome),
    }
}

/// Switch the workspace over to `target_id`, stashing the current center
/// under the outgoing session's active window first. No-op if already
/// attached.
pub fn attach_session(
    workspace: &mut Workspace,
    target_id: SessionId,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    if registry.active_id() == Some(target_id) {
        return;
    }
    let stash = stash_outgoing_timed(workspace, cx);
    if let Err(err) = registry.set_active(target_id) {
        log::warn!("could not activate session: {err:?}");
        return;
    }
    workspace.set_session_id(Some(target_id.to_string()));
    let restore = restore_incoming_timed(workspace, target_id, window, cx);
    let persist_started = Instant::now();
    persist_lifecycle(cx);
    let persist_scheduled_ms = elapsed_ms(persist_started);
    cx.notify();

    record_switch_timing(
        SwitchKind::Session,
        stash.outgoing_window,
        restore.incoming_window,
        stash.capture_ms,
        stash.runtime_capture_ms,
        restore.restore_ms,
        persist_scheduled_ms,
        restore.cache_outcome.unwrap_or(CacheOutcome::Cold),
    );
}

fn unique_name(base: &str, cx: &App) -> String {
    let registry = SessionRegistry::global(cx);
    let existing: std::collections::HashSet<String> =
        registry.sessions().into_iter().map(|s| s.name).collect();
    if !existing.contains(base) {
        return base.to_string();
    }
    for n in 2.. {
        let candidate = format!("{base}-{n}");
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
    base.to_string()
}

/// Persist the registry now. Used for lifecycle events
/// (attach/detach/create/delete/rename) where the on-disk view must be
/// consistent at the boundary. The returned task spawns immediately on
/// the background executor.
pub(crate) fn persist_lifecycle(cx: &App) {
    crate::registry::persist_scheduler(cx).flush_now(cx).detach();
}

/// Mark the registry dirty and let the persist scheduler debounce the
/// actual JSON write off the switch path
/// (`c-defer-persist`). Rapid `prefix Tab` cycles coalesce into a
/// single eventual flush rather than queuing one task per switch.
pub(crate) fn persist_debounced(cx: &App) {
    crate::registry::persist_scheduler(cx).mark_dirty(cx);
}

// ─── switch-timing trace harness ────────────────────────────────────────
//
// `Workspace::restore_center_root` (vendored Zed) measures its own wall
// clock and hands the result to the function pointer registered in
// `codon_session::init`. The pointer cannot pass values through GPUI's
// `cx` (the trampoline is sync-from-vendored-crate; there is no `cx`
// available to read globals), so the value lands in a thread-local
// cell. The codon-session action handler drives the rest of the switch
// path on the same foreground thread, so a `Cell<Option<f32>>` is
// race-free in practice — the cell is `take()`n right after the
// `restore_center_root` call returns.
thread_local! {
    static LAST_RESTORE_TIMING: Cell<Option<(f32, u32)>> = const { Cell::new(None) };
}

pub(crate) fn record_restore_timing_from_workspace(restore_ms: f32, new_pane_count: u32) {
    LAST_RESTORE_TIMING.with(|slot| slot.set(Some((restore_ms, new_pane_count))));
}

/// Drain the thread-local slot populated by the restore-timing callback.
/// Returns `(restore_ms, new_pane_count)` if a `restore_center_root` call
/// has fired since the last drain on this thread, `None` otherwise (e.g.
/// cache-miss path that took the `swap::apply` fallback instead).
fn take_restore_timing() -> Option<(f32, u32)> {
    LAST_RESTORE_TIMING.with(|slot| slot.take())
}

// ─── hold-to-quit ───────────────────────────────────────────────────────
//
// macOS's `cmd-q` defaults to an instantaneous app exit. Codon rebinds it
// to `codon_session::HoldQuit` and gates the actual `zed::Quit` dispatch
// behind a short hold — Chrome-style. The first press shows a toast and
// records a timestamp; while the user keeps the chord held, macOS
// auto-repeats the keystroke at ~30Hz, so subsequent action invocations
// arrive every ~33 ms. Once any of those arrive 1.5 s after the first
// press, we quit. Releasing before that leaves the app running; the
// stale state self-clears via a 250 ms idle threshold on the next press.

const HOLD_QUIT_THRESHOLD: Duration = Duration::from_millis(1500);
const HOLD_QUIT_IDLE_RESET: Duration = Duration::from_millis(250);

#[derive(Default, Clone, Copy)]
struct HoldQuitState {
    first_press: Option<Instant>,
    last_press: Option<Instant>,
}

impl Global for HoldQuitState {}

fn handle_hold_quit(
    workspace: &mut Workspace,
    _: &HoldQuit,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let now = Instant::now();
    let mut state = cx.try_global::<HoldQuitState>().copied().unwrap_or_default();

    // Stale state (released and pressed again later) → start fresh.
    let started = match state.last_press {
        Some(last) if now.duration_since(last) <= HOLD_QUIT_IDLE_RESET => state.first_press,
        _ => None,
    };

    let first_press = started.unwrap_or(now);
    state.first_press = Some(first_press);
    state.last_press = Some(now);
    cx.set_global(state);

    if now.duration_since(first_press) >= HOLD_QUIT_THRESHOLD {
        cx.remove_global::<HoldQuitState>();
        workspace.dismiss_notification(&NotificationId::unique::<HoldQuit>(), cx);
        window.dispatch_action(Box::new(zed_actions::Quit), cx);
        return;
    }

    if started.is_none() {
        let id = NotificationId::unique::<HoldQuit>();
        workspace.show_notification(id, cx, |cx| {
            cx.new(|cx| {
                MessageNotification::new(
                    "Hold ⌘Q to quit, or press ⇧⌘Q for an immediate quit.",
                    cx,
                )
            })
        });
    }
}
