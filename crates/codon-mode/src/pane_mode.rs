use gpui::{actions, Global, SharedString};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub enum PaneMode {
    #[default]
    Normal,
    Insert,
    Command,
}

impl std::fmt::Display for PaneMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaneMode::Normal => write!(f, "NORMAL"),
            PaneMode::Insert => write!(f, "INSERT"),
            PaneMode::Command => write!(f, "COMMAND"),
        }
    }
}

pub struct CodonModeTracker {
    pub mode: PaneMode,
    pub detail: Option<SharedString>,
    /// True while the codon command palette is open. The indicator forces
    /// `PaneMode::Command` whenever this is set, regardless of which pane
    /// or vim mode is otherwise focused — the modal owns the UI.
    pub command_active: bool,
}

impl Default for CodonModeTracker {
    fn default() -> Self {
        Self {
            mode: PaneMode::Normal,
            detail: None,
            command_active: false,
        }
    }
}

impl Global for CodonModeTracker {}

actions!(codon_mode, [SwitchToNormal, SwitchToInsert, SwitchToCommand]);
