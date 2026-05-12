use editor::Editor;
use file_manager::FileManager;
use gpui::{Context, Window};
use terminal_view::TerminalView;
use workspace::{NewCenterTerminal, NewFile, Workspace};

use crate::actions::{GotoOrOpenEditor, GotoOrOpenFileManager, GotoOrOpenTerminal};

pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_goto_or_open_terminal);
    workspace.register_action(handle_goto_or_open_file_manager);
    workspace.register_action(handle_goto_or_open_editor);
}

fn handle_goto_or_open_terminal(
    workspace: &mut Workspace,
    _: &GotoOrOpenTerminal,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(item) = workspace.recent_active_item_by_type::<TerminalView>(cx) {
        workspace.activate_item(&item, true, true, window, cx);
        return;
    }
    window.dispatch_action(Box::new(NewCenterTerminal::default()), cx);
}

fn handle_goto_or_open_file_manager(
    workspace: &mut Workspace,
    _: &GotoOrOpenFileManager,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(item) = workspace.recent_active_item_by_type::<FileManager>(cx) {
        workspace.activate_item(&item, true, true, window, cx);
        return;
    }
    window.dispatch_action(Box::new(file_manager::Open), cx);
}

fn handle_goto_or_open_editor(
    workspace: &mut Workspace,
    _: &GotoOrOpenEditor,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    if let Some(item) = workspace.recent_active_item_by_type::<Editor>(cx) {
        workspace.activate_item(&item, true, true, window, cx);
        return;
    }
    window.dispatch_action(Box::new(NewFile), cx);
}
