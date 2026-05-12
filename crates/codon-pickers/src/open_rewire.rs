use std::path::PathBuf;

use gpui::{App, Context, Window};
use workspace::{AppState, OpenOptions, Workspace};

use crate::dir_picker::DirPickerModal;

pub fn init(cx: &mut App) {
    cx.observe_new(|workspace: &mut Workspace, _window, _cx| {
        workspace.register_action(handle_open);
    })
    .detach();
}

fn handle_open(
    workspace: &mut Workspace,
    _: &workspace::Open,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let start = workspace
        .project()
        .read(cx)
        .worktrees(cx)
        .next()
        .map(|wt| wt.read(cx).abs_path().to_path_buf())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));

    let weak = workspace.weak_handle();
    workspace.toggle_modal(window, cx, move |window, cx| {
        DirPickerModal::new(
            start,
            move |path, _window, cx| {
                let app_state = AppState::global(cx);
                workspace::open_paths(&[path], app_state, OpenOptions::default(), cx)
                    .detach_and_log_err(cx);
            },
            weak,
            window,
            cx,
        )
    });
}
