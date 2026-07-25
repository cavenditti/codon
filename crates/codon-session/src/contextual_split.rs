//! Contextual split actions: split the active pane, then open a fresh
//! terminal or file manager in the new pane seeded to the *caller's*
//! current path — whatever directory the active terminal is `cd`-ed
//! into, or the directory the active file manager is browsing. Falls
//! back to the project's first worktree (or the process cwd) when the
//! active item exposes no path.
//!
//! Bound from `crates/codon-keymap/src/keymap.rs`:
//!   `cmd-k \`  → SplitTerminalRight
//!   `cmd-k |`  → SplitFileManagerRight
//!   `cmd-k -`  → SplitTerminalDown
//!   `cmd-k _`  → SplitFileManagerDown

use std::path::PathBuf;
use std::sync::Arc;

use file_manager::FileManager;
use gpui::{AppContext as _, Context, Entity, Window, actions};
use terminal_view::TerminalView;
use workspace::{SplitDirection, Workspace};

actions!(
    codon_session,
    [
        /// Split the active pane horizontally and open a terminal in
        /// the new right-hand pane, seeded to the active pane's
        /// current directory.
        SplitTerminalRight,
        /// Split the active pane vertically and open a terminal in
        /// the new bottom pane, seeded to the active pane's current
        /// directory.
        SplitTerminalDown,
        /// Split the active pane horizontally and open a file manager
        /// in the new right-hand pane, seeded to the active pane's
        /// current directory.
        SplitFileManagerRight,
        /// Split the active pane vertically and open a file manager
        /// in the new bottom pane, seeded to the active pane's
        /// current directory.
        SplitFileManagerDown,
        /// Contextual split-right: the new pane's kind is picked from
        /// the active pane's focus (terminal → terminal, fm → fm,
        /// editor → editor / new buffer). Empty / unrecognised focus
        /// falls back to a terminal. See
        /// REQ:codon/keymap-vocabulary#c-verb-collapse-split.
        SplitRight,
        /// Contextual split-down — vertical sibling of [`SplitRight`].
        SplitDown,
        /// Contextual split-right, flipping the kind in the terminal ↔
        /// file-manager pair. Editor focus resolves to a terminal (no
        /// editor↔terminal pairing today). Used by the `prefix |`
        /// chord.
        SplitRightOther,
        /// Contextual split-down — vertical sibling of [`SplitRightOther`].
        SplitDownOther,
    ]
);

pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(|workspace, _: &SplitTerminalRight, window, cx| {
        split_with_terminal(workspace, SplitDirection::Right, window, cx);
    });
    workspace.register_action(|workspace, _: &SplitTerminalDown, window, cx| {
        split_with_terminal(workspace, SplitDirection::Down, window, cx);
    });
    workspace.register_action(|workspace, _: &SplitFileManagerRight, window, cx| {
        split_with_file_manager(workspace, SplitDirection::Right, window, cx);
    });
    workspace.register_action(|workspace, _: &SplitFileManagerDown, window, cx| {
        split_with_file_manager(workspace, SplitDirection::Down, window, cx);
    });
    workspace.register_action(|workspace, _: &SplitRight, window, cx| {
        contextual_split(
            workspace,
            SplitDirection::Right,
            /*flip=*/ false,
            window,
            cx,
        );
    });
    workspace.register_action(|workspace, _: &SplitDown, window, cx| {
        contextual_split(
            workspace,
            SplitDirection::Down,
            /*flip=*/ false,
            window,
            cx,
        );
    });
    workspace.register_action(|workspace, _: &SplitRightOther, window, cx| {
        contextual_split(
            workspace,
            SplitDirection::Right,
            /*flip=*/ true,
            window,
            cx,
        );
    });
    workspace.register_action(|workspace, _: &SplitDownOther, window, cx| {
        contextual_split(
            workspace,
            SplitDirection::Down,
            /*flip=*/ true,
            window,
            cx,
        );
    });
}

/// Pick the new pane's kind from the active pane's focus, then split.
/// `flip` swaps terminal ↔ file-manager (editor → terminal); used for
/// the `prefix | / _` "other primary kind" chords. Empty / unrecognised
/// focus falls back to a terminal so `prefix \` always succeeds.
fn contextual_split(
    workspace: &mut Workspace,
    direction: SplitDirection,
    flip: bool,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    enum Kind {
        Terminal,
        FileManager,
    }
    let mut kind = Kind::Terminal;
    if let Some(item) = workspace.active_pane().read(cx).active_item() {
        if item.act_as::<TerminalView>(cx).is_some() {
            kind = Kind::Terminal;
        } else if item.act_as::<FileManager>(cx).is_some() {
            kind = Kind::FileManager;
        }
        // Editor / other items fall through to the Terminal default;
        // codon doesn't have an editor↔terminal pairing convention.
    }
    if flip {
        kind = match kind {
            Kind::Terminal => Kind::FileManager,
            Kind::FileManager => Kind::Terminal,
        };
    }
    match kind {
        Kind::Terminal => split_with_terminal(workspace, direction, window, cx),
        Kind::FileManager => split_with_file_manager(workspace, direction, window, cx),
    }
}

/// Resolve the directory that a freshly spawned terminal / file
/// manager should land in. Priority order:
///   1. The active terminal's shell cwd (live PTY-tracked dir).
///   2. The active file manager's `current_dir`.
///   3. The project's active-entry directory (e.g. parent of an open
///      buffer's file).
///   4. The project's first worktree root.
///   5. The process working directory.
fn resolve_seed_directory(workspace: &Workspace, cx: &Context<Workspace>) -> PathBuf {
    if let Some(item) = workspace.active_pane().read(cx).active_item() {
        if let Some(terminal_view) = item.act_as::<TerminalView>(cx)
            && let Some(dir) = terminal_view
                .read(cx)
                .terminal()
                .read(cx)
                .working_directory()
        {
            return dir;
        }
        if let Some(fm) = item.act_as::<FileManager>(cx) {
            return fm.read(cx).current_directory().to_path_buf();
        }
    }

    let project = workspace.project().read(cx);
    if let Some(dir) = project.active_entry_directory(cx) {
        return dir;
    }
    if let Some(wt) = project.visible_worktrees(cx).next() {
        return wt.read(cx).abs_path().to_path_buf();
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn split_with_terminal(
    workspace: &mut Workspace,
    direction: SplitDirection,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let dir = resolve_seed_directory(workspace, cx);
    let active_pane = workspace.active_pane().clone();
    let new_pane: Entity<workspace::pane::Pane> =
        workspace.split_pane(active_pane, direction, window, cx);

    let project = workspace.project().downgrade();
    cx.spawn_in(window, async move |workspace, cx| {
        let terminal = project
            .update(cx, |project, cx| {
                project.create_terminal_shell(Some(dir), cx)
            })?
            .await?;

        workspace.update_in(cx, |workspace, window, cx| {
            let terminal_view = cx.new(|cx| {
                TerminalView::new(
                    terminal,
                    workspace.weak_handle(),
                    workspace.database_id(),
                    workspace.project().downgrade(),
                    window,
                    cx,
                )
            });
            workspace.add_item(
                new_pane,
                Box::new(terminal_view),
                None,
                true,
                true,
                window,
                cx,
            );
        })?;
        anyhow::Ok(())
    })
    .detach_and_log_err(cx);
}

fn split_with_file_manager(
    workspace: &mut Workspace,
    direction: SplitDirection,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let dir = resolve_seed_directory(workspace, cx);
    let active_pane = workspace.active_pane().clone();
    let new_pane = workspace.split_pane(active_pane, direction, window, cx);

    let fs = workspace.app_state().fs.clone();
    let languages = Some(workspace.app_state().languages.clone());
    let weak_workspace = workspace.weak_handle();
    let project = workspace.project().clone();

    // Ensure the worktree exists so per-entry git status resolves —
    // mirrors `file_manager::open_file_manager`.
    let needs_worktree = project
        .read(cx)
        .worktree_store()
        .read(cx)
        .find_worktree(&dir, cx)
        .is_none();
    if needs_worktree {
        let dir_arc: Arc<std::path::Path> = Arc::from(dir.as_path());
        project
            .update(cx, |project, cx| {
                project.find_or_create_worktree(dir_arc, false, cx)
            })
            .detach_and_log_err(cx);
    }

    let file_manager =
        cx.new(|cx| FileManager::new(dir, weak_workspace, fs, languages, window, cx));
    workspace.add_item(
        new_pane,
        Box::new(file_manager),
        None,
        true,
        true,
        window,
        cx,
    );
}
