//! Error pattern: `anyhow::Result` with `.context()` at every `?` boundary. No custom error types.
//!
//! Codon command palette.
//!
//! Helix-style `:`-triggered palette built as a thin wrapper around Zed's
//! `Action` registry. The wrapper adds two things on top of the existing
//! Zed `command_palette`:
//!
//! 1. An always-visible description aside (`PickerDelegate::documentation_aside`)
//!    so a keyboard user sees what each command does without hovering.
//! 2. A typed-argument sub-picker driven by registered
//!    [`Completer`](completer::Completer) impls — type `open ` and the row
//!    list becomes project file paths; type `theme ` and it becomes theme
//!    names.
//!
//! See `.specs/codon/command-palette.spec.md`.

pub mod completer;
mod modal;
mod shell_verbs;

use std::path::PathBuf;

use std::any::TypeId;

use command_palette_hooks::CommandPaletteFilter;
use gpui::{Action, App, AppContext as _, Context, Window, actions};
use schemars::JsonSchema;
use serde::Deserialize;
use task::{HideStrategy, RevealStrategy, SaveStrategy, Shell, SpawnInTerminal, TaskId};
use workspace::Workspace;
use zed_actions::RevealTarget;

pub use modal::CodonPalette;

actions!(
    codon_command_palette,
    [
        /// Open the codon command palette. Bound to `:` in codon Normal
        /// mode and to `cmd-shift-p` globally.
        Toggle,
    ]
);

/// Open an absolute file path in the active workspace.
///
/// Dispatched by the `file_path` completer when the user confirms a path
/// in argument mode. Holds an absolute path so the handler doesn't have to
/// resolve it against any specific worktree.
#[derive(Clone, Debug, PartialEq, Default, Deserialize, JsonSchema, Action)]
#[action(namespace = codon_command_palette)]
#[serde(deny_unknown_fields)]
pub struct OpenFile(pub PathBuf);

/// Run a shell command and surface its output in a new terminal pane.
///
/// Dispatched by the `sh` completer (Helix's `:sh` verb — no selection
/// involvement). The command is run via the workspace task runner so
/// it inherits the project's first-worktree cwd, the same as the
/// existing `:!<cmd>` flow in `vim/src/command.rs::ShellExec::run`.
#[derive(Clone, Debug, PartialEq, Default, Deserialize, JsonSchema, Action)]
#[action(namespace = codon_command_palette)]
#[serde(deny_unknown_fields)]
pub struct RunShell(pub String);

/// Process-wide setup. Call once during app init.
pub fn init(cx: &mut App) {
    completer::register_builtins();
    shell_verbs::register_builtins();
    hide_destructive_actions_from_palette(cx);
    codon_mode::register_pane_mode_bridge::<CodonPalette>(cx);
}

/// Hide actions whose only effect is to quit the app or kill the OS window
/// from the codon command palette. The palette's fuzzy match for short
/// queries like `q` would otherwise pin `zed::Quit` to the top — one stray
/// Enter and the whole app is gone. `cmd-shift-q` and the macOS menu
/// remain available for users who actually want to quit.
fn hide_destructive_actions_from_palette(cx: &mut App) {
    let hidden: [TypeId; 2] = [
        TypeId::of::<zed_actions::Quit>(),
        TypeId::of::<workspace::CloseWindow>(),
    ];
    CommandPaletteFilter::update_global(cx, |filter, _| {
        filter.hide_action_types(&hidden);
    });
}

/// Register workspace-scoped action handlers. Invoked from `apps/codon`
/// once per workspace, mirroring `codon_session::actions::register_for_workspace`.
pub fn register_for_workspace(workspace: &mut Workspace) {
    workspace.register_action(handle_toggle);
    workspace.register_action(handle_open_file);
    workspace.register_action(handle_run_shell);
}

fn handle_toggle(
    workspace: &mut Workspace,
    _: &Toggle,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    CodonPalette::toggle(workspace, window, cx);
}

fn handle_open_file(
    workspace: &mut Workspace,
    action: &OpenFile,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let path = action.0.clone();
    if path.as_os_str().is_empty() {
        return;
    }
    workspace
        .open_abs_path(path, workspace::OpenOptions::default(), window, cx)
        .detach_and_log_err(cx);
}

fn handle_run_shell(
    workspace: &mut Workspace,
    action: &RunShell,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) {
    let command = action.0.trim().to_string();
    if command.is_empty() {
        return;
    }
    let cwd = workspace.project().read(cx).first_project_directory(cx);
    let spawn = SpawnInTerminal {
        id: TaskId("codon-sh".to_string()),
        full_label: command.clone(),
        label: command.clone(),
        command: Some(command.clone()),
        args: Vec::new(),
        command_label: command,
        cwd,
        env: Default::default(),
        use_new_terminal: true,
        allow_concurrent_runs: true,
        reveal: RevealStrategy::Always,
        reveal_target: RevealTarget::Dock,
        hide: HideStrategy::Never,
        shell: Shell::System,
        show_summary: true,
        show_command: true,
        show_rerun: false,
        save: SaveStrategy::default(),
    };
    let task = workspace.spawn_in_terminal(spawn, window, cx);
    cx.background_spawn(async move {
        match task.await {
            Some(Ok(_)) | None => {}
            Some(Err(e)) => log::error!("codon :sh failed: {e}"),
        }
    })
    .detach();
}
