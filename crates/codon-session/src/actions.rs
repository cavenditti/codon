use std::path::PathBuf;

use gpui::{Action, App, AppContext as _, Context, Window, actions};
use schemars::JsonSchema;
use serde::Deserialize;
use workspace::{Workspace, notifications::NotifyTaskExt as _};

use crate::{
    picker::SessionSwitchModal,
    registry::SessionRegistry,
    runtime::{WindowRuntime, WindowRuntimeCache},
    session::Session,
    swap,
};

actions!(
    codon_session,
    [
        /// Create a new session named after the current workspace cwd.
        SessionNew,
        /// Open a fuzzy picker to switch sessions.
        SessionSwitch,
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
    workspace.register_action(handle_session_close);
    workspace.register_action(handle_window_new);
    workspace.register_action(handle_window_next);
    workspace.register_action(handle_window_prev);
    workspace.register_action(handle_window_close);
}

fn handle_session_new(
    workspace: &mut Workspace,
    _: &SessionNew,
    _window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    create_session_for_workspace(workspace, cx);
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
        workspace.restore_center_root(rt.root, rt.active_pane, window, cx);
    } else if let Some(layout) = incoming_layout {
        // Fall back to the persisted snapshot (cross-restart restore). If
        // the snapshot can't be hydrated (e.g. items missing from sqlite),
        // workspace::replace_center_with_snapshot now creates a fresh empty
        // pane instead of erroring.
        let weak = workspace.weak_handle();
        swap::apply(workspace, layout, window, cx).detach_and_notify_err(weak, window, cx);
    } else {
        workspace.replace_center_with_empty_pane(window, cx);
    }
}

fn capture_runtime(workspace: &Workspace, cx: &App) -> Option<WindowRuntime> {
    let root = workspace.center().root.clone();
    let active_pane = Some(workspace.active_pane().clone());
    let _ = cx;
    Some(WindowRuntime { root, active_pane })
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
