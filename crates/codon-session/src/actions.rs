use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use gpui::{Action, App, AppContext as _, Context, Global, Window, actions};
use schemars::JsonSchema;
use serde::Deserialize;
use workspace::{
    CloseActiveItem, Workspace,
    notifications::{NotificationId, NotifyTaskExt as _, simple_message_notification::MessageNotification},
};

use crate::{
    picker::SessionSwitchModal,
    registry::SessionRegistry,
    runtime::{WindowRuntime, WindowRuntimeCache},
    session::{Session, SessionId},
    swap,
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
        /// Close the active window in the active session.
        WindowClose,
        /// Close the active tab. Falls back to closing the pane, then the
        /// codon-session window, then replacing the center with an empty
        /// pane. Never closes the OS window — that's reserved for
        /// `cmd-shift-w` / `cmd-shift-q`.
        SafeCloseActiveItem,
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
    ]
);

/// Switch to the window at the given zero-based index in the active session.
#[derive(Clone, PartialEq, Debug, Deserialize, JsonSchema, Default, Action)]
#[action(namespace = codon_session)]
#[serde(deny_unknown_fields)]
pub struct WindowGoto(pub usize);

pub fn register(_cx: &mut App) {}

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
    workspace.register_action(handle_window_close);
    workspace.register_action(handle_safe_close_active_item);
    workspace.register_action(handle_hold_quit);
    crate::goto_or_open::register_for_workspace(workspace);
    crate::diff_open::register_for_workspace(workspace);
    crate::contextual_split::register_for_workspace(workspace);
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
    persist_async(cx);
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
    workspace.toggle_modal(window, cx, move |window, cx| {
        crate::overview::OverviewModal::new(
            crate::overview::InitialFocus::Session,
            weak,
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
    workspace.toggle_modal(window, cx, move |window, cx| {
        crate::overview::OverviewModal::new(
            crate::overview::InitialFocus::Window,
            weak,
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
    persist_async(cx);
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
    session.active_window = new_index;
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save new window: {err:?}");
    }
    persist_async(cx);

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

fn handle_window_close(
    workspace: &mut Workspace,
    _: &WindowClose,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let Some(active_id) = registry.active_id() else {
        return;
    };
    let Some(mut session) = registry.get(active_id) else {
        return;
    };
    if session.windows.len() <= 1 {
        return;
    }
    let removed = session.windows[session.active_window].id;
    session.remove_window(removed);
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save after window close: {err:?}");
    }
    persist_async(cx);
    cx.notify();
    let _ = workspace;
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
    let snapshot = swap::capture(workspace, window, cx);
    let runtime = capture_runtime(workspace, cx);
    if let Some(active) = session.active_mut() {
        active.layout = Some(snapshot);
    }
    if let (Some(outgoing_window_id), Some(rt)) = (outgoing_id, runtime) {
        WindowRuntimeCache::global(cx).insert(active_id, outgoing_window_id, rt);
    }

    session.active_window = new_idx;
    let incoming_window_id = session.windows.get(new_idx).map(|w| w.id);
    let incoming_layout = session.windows.get(new_idx).and_then(|w| w.layout.clone());
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not save window switch: {err:?}");
    }
    persist_async(cx);

    let cache = WindowRuntimeCache::global(cx);
    let cached_runtime = incoming_window_id.and_then(|id| cache.take(active_id, id));
    if let Some(rt) = cached_runtime {
        log::debug!(
            "restoring window {:?} from in-memory runtime cache",
            incoming_window_id
        );
        workspace.restore_center_root(rt.root, rt.active_pane, window, cx);
    } else if let Some(layout) = incoming_layout {
        log::debug!(
            "restoring window {:?} from persisted snapshot (no runtime cache hit)",
            incoming_window_id
        );
        let weak = workspace.weak_handle();
        swap::apply(workspace, layout, window, cx).detach_and_notify_err(weak, window, cx);
    } else {
        log::debug!(
            "no state for window {:?}; opening fresh empty pane",
            incoming_window_id
        );
        workspace.replace_center_with_empty_pane(window, cx);
    }
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
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let Some(outgoing_session_id) = registry.active_id() else {
        return;
    };
    let Some(mut session) = registry.get(outgoing_session_id) else {
        return;
    };
    let Some(outgoing_window_id) = session.active().map(|w| w.id) else {
        return;
    };

    let snapshot = swap::capture(workspace, window, cx);
    let runtime = capture_runtime(workspace, cx);

    if let Some(active) = session.active_mut() {
        active.layout = Some(snapshot);
    }
    if let Err(err) = registry.upsert(session) {
        log::warn!("could not stash outgoing session layout: {err:?}");
    }
    if let Some(rt) = runtime {
        WindowRuntimeCache::global(cx).insert(outgoing_session_id, outgoing_window_id, rt);
    }
}

/// Restore (or initialize) the workspace center to the active window of
/// `target_id`. Prefers the in-memory runtime cache, falls back to the
/// persisted `LayoutSnapshot`, and finally drops in a fresh empty pane.
fn restore_incoming(
    workspace: &mut Workspace,
    target_id: SessionId,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let registry = SessionRegistry::global(cx);
    let session = registry.get(target_id);
    let incoming_window_id = session.as_ref().and_then(|s| s.active().map(|w| w.id));
    let incoming_layout = session.as_ref().and_then(|s| s.active().and_then(|w| w.layout.clone()));

    let cached = incoming_window_id
        .and_then(|id| WindowRuntimeCache::global(cx).take(target_id, id));

    if let Some(rt) = cached {
        log::debug!(
            "attach: restoring session {target_id} window {:?} from runtime cache",
            incoming_window_id
        );
        workspace.restore_center_root(rt.root, rt.active_pane, window, cx);
    } else if let Some(layout) = incoming_layout {
        log::debug!(
            "attach: restoring session {target_id} window {:?} from persisted snapshot",
            incoming_window_id
        );
        let weak = workspace.weak_handle();
        swap::apply(workspace, layout, window, cx).detach_and_notify_err(weak, window, cx);
    } else {
        log::debug!(
            "attach: no state for session {target_id} window {:?}; opening fresh empty pane",
            incoming_window_id
        );
        workspace.replace_center_with_empty_pane(window, cx);
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
    stash_outgoing(workspace, window, cx);
    if let Err(err) = registry.set_active(target_id) {
        log::warn!("could not activate session: {err:?}");
        return;
    }
    workspace.set_session_id(Some(target_id.to_string()));
    restore_incoming(workspace, target_id, window, cx);
    persist_async(cx);
    cx.notify();
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

pub(crate) fn persist_async(cx: &App) {
    let snapshot = SessionRegistry::global(cx).snapshot();
    cx.background_spawn(async move {
        if let Err(err) = snapshot.write().await {
            log::warn!("failed to persist session registry: {err:?}");
        }
    })
    .detach();
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
