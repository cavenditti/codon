//! In-memory store + helpers for surfacing long-running file-manager
//! operations as workspace notifications.
//!
//! Codon-wide model: every multi-entry fs operation (paste, bulk-delete,
//! bulk-rename, bulk-chmod, …) begins by calling
//! [`FmTaskStore::begin`] with a kind, a human-readable label, and the
//! total count of entries to process. The store returns an
//! [`FmTaskHandle`]; the per-entry loop bumps progress via
//! [`FmTaskHandle::tick`] and reports the terminal outcome via
//! [`FmTaskHandle::finish`].
//!
//! Each task owns a `NotificationId::Named("fm-task-<id>")`. Both
//! `tick` and `finish` call `workspace.show_notification(id, …)`, which
//! dismisses the previous frame and renders the new one — so the user
//! sees a single notification ticking forward and resolving in place.
//!
//! The store is a `gpui::Global` capped at [`HISTORY_CAP`] entries with
//! FIFO eviction. It is in-memory only — cleared on quit — and lives
//! beside the `WindowRuntimeCache` pattern in `codon-session`.

#![allow(dead_code)]

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

use gpui::{App, AppContext, Global, SharedString, WeakEntity};
use workspace::{
    Workspace,
    notifications::{NotificationId, simple_message_notification::MessageNotification},
};

/// Upper bound on the per-app history. Matches the spec's "last ~50
/// finished tasks" target — large enough that a typical session won't
/// roll a task off mid-use, small enough that the modal renders without
/// virtualisation in the worst case.
pub const HISTORY_CAP: usize = 50;

/// How often `tick` is allowed to re-emit a notification frame. The
/// store always carries the current `processed` count; only the
/// rendered notification is rate-limited so a 10 000-entry paste does
/// not push 10 000 frames through the workspace queue.
const TICK_THROTTLE: Duration = Duration::from_millis(100);

/// Verbs codon's file manager exposes as long-running tasks. The kind
/// drives the resolution-frame phrasing ("Trashed", "Pasted", …) and
/// the history-modal row label.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FmTaskKind {
    /// `D` — recursive trash of one or more entries.
    Delete,
    /// `p` / `P` — copy-or-move from the FM clipboard into the current
    /// directory.
    Paste,
    /// `R` / `cw` — rename every entry in the marked set via a pattern
    /// or `$EDITOR` round-trip.
    BulkRename,
    /// `cm` — apply a chmod mask to the marked set.
    Chmod,
}

impl FmTaskKind {
    fn verb_running(self) -> &'static str {
        match self {
            FmTaskKind::Delete => "Trashing",
            FmTaskKind::Paste => "Pasting",
            FmTaskKind::BulkRename => "Renaming",
            FmTaskKind::Chmod => "Chmoding",
        }
    }

    fn verb_done(self) -> &'static str {
        match self {
            FmTaskKind::Delete => "Trashed",
            FmTaskKind::Paste => "Pasted",
            FmTaskKind::BulkRename => "Renamed",
            FmTaskKind::Chmod => "Chmoded",
        }
    }
}

/// Lifecycle of a task. The store starts every task in `Running`,
/// `tick` mutates the `processed` count in place, and `finish` flips
/// to one of the three terminal variants. The history modal only ever
/// renders the terminal variant since live tasks already have a
/// notification on screen.
#[derive(Clone, Debug)]
pub enum FmTaskState {
    Running { processed: usize, total: usize },
    Done { processed: usize, total: usize },
    Failed { processed: usize, total: usize, errors: Vec<String> },
    Cancelled { processed: usize, total: usize },
}

impl FmTaskState {
    fn processed(&self) -> usize {
        match self {
            FmTaskState::Running { processed, .. }
            | FmTaskState::Done { processed, .. }
            | FmTaskState::Failed { processed, .. }
            | FmTaskState::Cancelled { processed, .. } => *processed,
        }
    }

    fn total(&self) -> usize {
        match self {
            FmTaskState::Running { total, .. }
            | FmTaskState::Done { total, .. }
            | FmTaskState::Failed { total, .. }
            | FmTaskState::Cancelled { total, .. } => *total,
        }
    }

    /// Display tag used by the history modal's status column.
    pub fn status_label(&self) -> &'static str {
        match self {
            FmTaskState::Running { .. } => "running",
            FmTaskState::Done { .. } => "done",
            FmTaskState::Failed { .. } => "failed",
            FmTaskState::Cancelled { .. } => "cancelled",
        }
    }
}

/// Caller-visible terminal outcome. Translates into one of the three
/// terminal `FmTaskState` variants inside `finish`.
#[derive(Debug)]
pub enum FmTaskOutcome {
    Done,
    Failed { errors: Vec<String> },
    Cancelled,
}

/// Snapshot of one task. Cloned freely into the history modal and into
/// notification builders — the cancel flag is shared via `Arc`, so the
/// notification's Cancel button still flips the same bool the loop is
/// polling.
#[derive(Clone)]
pub struct FmTask {
    pub id: u64,
    pub kind: FmTaskKind,
    pub label: SharedString,
    pub state: FmTaskState,
    pub cancel: Arc<AtomicBool>,
    pub started_at: Instant,
    pub completed_at: Option<Instant>,
}

impl FmTask {
    pub fn notification_id(&self) -> NotificationId {
        notification_id_for(self.id)
    }

    pub fn is_terminal(&self) -> bool {
        !matches!(self.state, FmTaskState::Running { .. })
    }

    /// Human-readable summary used as the notification body. The
    /// terminal frames lead with the verb in past tense ("Trashed 12
    /// entries") so the user can glance at the toast and know the
    /// outcome without parsing the status label.
    pub fn summary(&self) -> String {
        match &self.state {
            FmTaskState::Running { processed, total } => {
                format!("{} {} of {} …", self.kind.verb_running(), processed, total)
            }
            FmTaskState::Done { processed, .. } => {
                format!("{} {} entr{}", self.kind.verb_done(), processed, plural_y(*processed))
            }
            FmTaskState::Failed { processed, total, errors } => {
                let first = errors.first().map(String::as_str).unwrap_or("see log");
                format!(
                    "{} {} of {} — {} failed ({})",
                    self.kind.verb_done(),
                    processed,
                    total,
                    total.saturating_sub(*processed),
                    first
                )
            }
            FmTaskState::Cancelled { processed, total } => {
                let skipped = total.saturating_sub(*processed);
                format!(
                    "{} cancelled after {} of {} ({} skipped)",
                    self.kind.verb_running().to_lowercase(),
                    processed,
                    total,
                    skipped
                )
            }
        }
    }
}

fn notification_id_for(task_id: u64) -> NotificationId {
    NotificationId::named(SharedString::from(format!("fm-task-{task_id}")))
}

fn plural_y(n: usize) -> &'static str {
    if n == 1 { "y" } else { "ies" }
}

/// `gpui::Global` holding the live + recent task records. Inserted by
/// [`init`] at startup so any `App` can call
/// `cx.global_mut::<FmTaskStore>()`.
#[derive(Default)]
pub struct FmTaskStore {
    tasks: Vec<FmTask>,
    next_id: AtomicU64,
}

impl Global for FmTaskStore {}

impl FmTaskStore {
    /// Snapshot of every recorded task, most-recent first. Used by the
    /// history modal.
    pub fn snapshot(&self) -> Vec<FmTask> {
        let mut out = self.tasks.clone();
        out.reverse();
        out
    }

    pub fn get(&self, id: u64) -> Option<&FmTask> {
        self.tasks.iter().find(|t| t.id == id)
    }

    fn allocate_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    fn insert(&mut self, task: FmTask) {
        self.tasks.push(task);
        while self.tasks.len() > HISTORY_CAP {
            self.tasks.remove(0);
        }
    }

    fn with_task_mut(&mut self, id: u64, f: impl FnOnce(&mut FmTask)) -> Option<FmTask> {
        let task = self.tasks.iter_mut().find(|t| t.id == id)?;
        f(task);
        Some(task.clone())
    }
}

pub fn init(cx: &mut App) {
    cx.set_global(FmTaskStore::default());
}

/// Live handle returned by [`begin`]. Owns the cancel flag (so the
/// notification's Cancel button and the per-entry loop both observe
/// the same bool) and remembers the task id for tick / finish calls.
///
/// The handle deliberately does *not* implement `Drop` to auto-finish:
/// callers must explicitly call [`Self::finish`] so the terminal frame
/// reflects whether the run succeeded, failed, or was cancelled.
pub struct FmTaskHandle {
    id: u64,
    cancel: Arc<AtomicBool>,
    last_tick: Instant,
}

impl FmTaskHandle {
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        self.cancel.clone()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// Allocate a new task, push it into the store, and return a handle
/// usable from inside the async loop. Also emits the first
/// notification frame so the user sees the operation start immediately.
pub fn begin(
    workspace: WeakEntity<Workspace>,
    kind: FmTaskKind,
    label: impl Into<SharedString>,
    total: usize,
    cx: &mut App,
) -> FmTaskHandle {
    let label = label.into();
    let store = cx.global_mut::<FmTaskStore>();
    let id = store.allocate_id();
    let cancel = Arc::new(AtomicBool::new(false));
    let task = FmTask {
        id,
        kind,
        label,
        state: FmTaskState::Running { processed: 0, total },
        cancel: cancel.clone(),
        started_at: Instant::now(),
        completed_at: None,
    };
    store.insert(task.clone());

    emit_notification(workspace, task, cx);

    FmTaskHandle {
        id,
        cancel,
        last_tick: Instant::now(),
    }
}

/// Update the recorded `processed` count and — if the throttle allows —
/// re-emit the notification frame. Returns `true` when the
/// notification was actually re-rendered (callers can use this to
/// drive their own debug logging if needed).
pub fn tick(
    handle: &mut FmTaskHandle,
    processed: usize,
    workspace: WeakEntity<Workspace>,
    cx: &mut App,
) -> bool {
    let store = cx.global_mut::<FmTaskStore>();
    let updated = store.with_task_mut(handle.id, |task| {
        if let FmTaskState::Running { processed: p, .. } = &mut task.state {
            *p = processed;
        }
    });
    let Some(task) = updated else {
        return false;
    };
    let now = Instant::now();
    if now.duration_since(handle.last_tick) < TICK_THROTTLE {
        return false;
    }
    handle.last_tick = now;
    emit_notification(workspace, task, cx);
    true
}

/// Flip the task to its terminal state, emit the resolution frame, and
/// leave it in the store for the history modal. Consumes the handle.
pub fn finish(
    handle: FmTaskHandle,
    outcome: FmTaskOutcome,
    workspace: WeakEntity<Workspace>,
    cx: &mut App,
) {
    let store = cx.global_mut::<FmTaskStore>();
    let updated = store.with_task_mut(handle.id, |task| {
        let processed = task.state.processed();
        let total = task.state.total();
        task.completed_at = Some(Instant::now());
        task.state = match outcome {
            FmTaskOutcome::Done => FmTaskState::Done { processed, total },
            FmTaskOutcome::Failed { errors } => FmTaskState::Failed { processed, total, errors },
            FmTaskOutcome::Cancelled => FmTaskState::Cancelled { processed, total },
        };
    });
    let Some(task) = updated else {
        return;
    };
    emit_notification(workspace, task, cx);
}

/// Re-emit a task's notification from outside the running loop. Used
/// by the history modal so the user can pull a previously-dismissed
/// resolution frame back onto the screen.
pub fn emit_resolution(task: FmTask, workspace: WeakEntity<Workspace>, cx: &mut App) {
    emit_notification(workspace, task, cx);
}

fn emit_notification(workspace: WeakEntity<Workspace>, task: FmTask, cx: &mut App) {
    let id = task.notification_id();
    let body = SharedString::from(task.summary());
    let show_cancel = matches!(task.state, FmTaskState::Running { .. });
    let cancel_flag = task.cancel;
    workspace
        .update(cx, |workspace, cx| {
            workspace.show_notification(id, cx, move |cx| {
                cx.new(move |cx| {
                    let mut notif = MessageNotification::new(body, cx);
                    if show_cancel {
                        notif = notif
                            .primary_message("Cancel")
                            .primary_on_click(move |_, _| {
                                cancel_flag.store(true, Ordering::Relaxed);
                            });
                    }
                    notif
                })
            });
        })
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task(state: FmTaskState) -> FmTask {
        FmTask {
            id: 7,
            kind: FmTaskKind::Delete,
            label: SharedString::from("Trashing 12 entries"),
            state,
            cancel: Arc::new(AtomicBool::new(false)),
            started_at: Instant::now(),
            completed_at: None,
        }
    }

    #[test]
    fn running_summary_mentions_running_verb_and_counts() {
        let task = sample_task(FmTaskState::Running { processed: 4, total: 12 });
        let s = task.summary();
        assert!(s.contains("Trashing"), "{s}");
        assert!(s.contains("4 of 12"), "{s}");
    }

    #[test]
    fn cancelled_summary_names_skipped_count() {
        let task = sample_task(FmTaskState::Cancelled { processed: 7, total: 12 });
        let s = task.summary();
        assert!(s.contains("cancelled"), "{s}");
        assert!(s.contains("7 of 12"), "{s}");
        assert!(s.contains("5 skipped"), "{s}");
    }

    #[test]
    fn done_summary_uses_past_tense_verb() {
        let task = sample_task(FmTaskState::Done { processed: 12, total: 12 });
        assert!(task.summary().starts_with("Trashed"), "{}", task.summary());
    }

    #[test]
    fn failed_summary_surfaces_first_error() {
        let task = sample_task(FmTaskState::Failed {
            processed: 10,
            total: 12,
            errors: vec!["permission denied".into(), "io error".into()],
        });
        let s = task.summary();
        assert!(s.contains("permission denied"), "{s}");
        assert!(s.contains("10 of 12"), "{s}");
    }

    #[test]
    fn store_caps_history_at_fifty() {
        let mut store = FmTaskStore::default();
        for i in 0..60 {
            let task = FmTask {
                id: i,
                kind: FmTaskKind::Paste,
                label: SharedString::from(format!("task {i}")),
                state: FmTaskState::Done { processed: 1, total: 1 },
                cancel: Arc::new(AtomicBool::new(false)),
                started_at: Instant::now(),
                completed_at: Some(Instant::now()),
            };
            store.insert(task);
        }
        assert_eq!(store.tasks.len(), HISTORY_CAP);
        // FIFO eviction — the oldest ids (0..10) are gone, newest (10..60) survive.
        assert_eq!(store.tasks.first().map(|t| t.id), Some(10));
        assert_eq!(store.tasks.last().map(|t| t.id), Some(59));
    }

    #[test]
    fn store_with_task_mut_returns_updated_clone() {
        let mut store = FmTaskStore::default();
        let task = FmTask {
            id: 1,
            kind: FmTaskKind::Delete,
            label: SharedString::from("Trashing 3 entries"),
            state: FmTaskState::Running { processed: 0, total: 3 },
            cancel: Arc::new(AtomicBool::new(false)),
            started_at: Instant::now(),
            completed_at: None,
        };
        store.insert(task);
        let updated = store
            .with_task_mut(1, |task| {
                if let FmTaskState::Running { processed, .. } = &mut task.state {
                    *processed = 2;
                }
            })
            .expect("task exists");
        assert!(matches!(
            updated.state,
            FmTaskState::Running { processed: 2, total: 3 }
        ));
    }

    #[test]
    fn handle_observes_shared_cancel_flag() {
        let flag = Arc::new(AtomicBool::new(false));
        let handle = FmTaskHandle {
            id: 0,
            cancel: flag.clone(),
            last_tick: Instant::now(),
        };
        assert!(!handle.is_cancelled());
        flag.store(true, Ordering::Relaxed);
        assert!(handle.is_cancelled());
    }
}
