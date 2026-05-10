use anyhow::Result;
use gpui::{App, Context, Task, Window};
use workspace::{Workspace, codon_bridge};

pub use workspace::codon_bridge::LayoutSnapshot;

pub fn capture(workspace: &Workspace, window: &mut Window, cx: &mut App) -> LayoutSnapshot {
    codon_bridge::capture_layout(workspace, window, cx)
}

pub fn apply(
    workspace: &mut Workspace,
    snapshot: LayoutSnapshot,
    window: &mut Window,
    cx: &mut Context<Workspace>,
) -> Task<Result<()>> {
    codon_bridge::apply_layout(workspace, snapshot, window, cx)
}
