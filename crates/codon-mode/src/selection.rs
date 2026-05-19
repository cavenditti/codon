use command_palette_hooks::ObjectKind;
use std::path::PathBuf;

/// A typed selection representing the currently-targeted nouns
/// in the focused pane. Each pane kind produces selections of
/// its own types.
#[derive(Clone, Debug, Default)]
pub enum Selection {
    #[default]
    Empty,
    Text {
        ranges: Vec<std::ops::Range<usize>>,
    },
    Files(Vec<PathBuf>),
    Hunks(Vec<HunkRef>),
    Commits(Vec<String>),
    Blocks(Vec<BlockRef>),
    Diagnostics(Vec<DiagnosticRef>),
    Messages(Vec<MessageRef>),
}

impl Selection {
    pub fn kind(&self) -> Option<ObjectKind> {
        match self {
            Selection::Empty => None,
            Selection::Text { .. } => Some(ObjectKind::Text),
            Selection::Files(_) => Some(ObjectKind::File),
            Selection::Hunks(_) => Some(ObjectKind::Hunk),
            Selection::Commits(_) => Some(ObjectKind::Commit),
            Selection::Blocks(_) => Some(ObjectKind::Block),
            Selection::Diagnostics(_) => Some(ObjectKind::Diagnostic),
            Selection::Messages(_) => Some(ObjectKind::Message),
        }
    }

    pub fn is_empty(&self) -> bool {
        matches!(self, Selection::Empty)
    }
}

/// Trait that every pane kind implements to expose its current selection.
pub trait SelectionSource {
    fn current_selection(&self) -> Selection;
    fn object_kinds(&self) -> &'static [ObjectKind];

    /// The pane's *natural* object kind — what `w` / `b` / `%` operate on
    /// when no explicit kind suffix is given. Used by the
    /// `codon-pane-bridge::ObjectGrammar` dispatcher so it knows which
    /// kind to pass to `ObjectNext` / `ObjectPrev` / `SelectAll` from
    /// the focused pane. File-manager = `File`, git = `Hunk`,
    /// diagnostics = `Diagnostic`, etc.
    ///
    /// Default returns the first entry of `object_kinds()`, falling
    /// back to `ObjectKind::Text` for impls that haven't yet been
    /// updated. Override in pane impls that own multiple kinds where
    /// the *first listed* isn't the natural target (e.g. an editor
    /// pane returning `Text` even though it also produces hunks).
    fn primary_object_kind(&self) -> ObjectKind {
        self.object_kinds().first().copied().unwrap_or(ObjectKind::Text)
    }
}

/// Reference to a git hunk (placeholder — will be fleshed out in Phase 3).
#[derive(Clone, Debug)]
pub struct HunkRef {
    pub file: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
}

/// Reference to a terminal block (command + output).
#[derive(Clone, Debug)]
pub struct BlockRef {
    pub command: String,
    pub output_start: usize,
    pub output_end: usize,
}

/// Reference to a diagnostic.
#[derive(Clone, Debug)]
pub struct DiagnosticRef {
    pub file: PathBuf,
    pub line: usize,
    pub message: String,
}

/// Reference to an agent message.
#[derive(Clone, Debug)]
pub struct MessageRef {
    pub index: usize,
}
