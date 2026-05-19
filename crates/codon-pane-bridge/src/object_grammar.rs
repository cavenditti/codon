//! Error pattern: no fallible APIs — the trait is selection-in/selection-out and the dispatcher actions are infallible UI verbs.
//!
//! `ObjectGrammar` — the pane-level "movement / refinement" trait codon
//! routes Normal-mode keys (`w` / `b` / `mi<x>` / `ma<x>` / `%<x>`)
//! through.
//!
//! ## Why this lives in `codon-pane-bridge`
//!
//! Like [`PaneModeBridge`](crate::PaneModeBridge), this trait wants to
//! sit *below* the rest of codon in the dep graph so any pane crate
//! (editor / fm / git / diagnostics / agent / terminal) can implement
//! it without importing upstream codon crates. The trait, the
//! [`GrammarKind`] vocabulary, and the [`GrammarSelection`] data type
//! all live here so each pane crate depends only on
//! `codon-pane-bridge` to participate.
//!
//! ## Why a separate `GrammarKind` from `command_palette_hooks::ObjectKind`
//!
//! The Zed-side `command_palette_hooks::ObjectKind` describes the
//! kind of value a *palette verb accepts* (filter on the action
//! registry). The motion-grammar trait needs a finer vocabulary —
//! `Word` / `Paragraph` / `Function` / `BracketPair` / `Directory` /
//! `PromptBlock` aren't selection-shape kinds, they're motion-target
//! kinds. Keeping the two enums separate avoids polluting the
//! palette filter registry with motion-only variants. Where the two
//! overlap (`File`, `Hunk`, `Commit`, `Diagnostic`, `Message`,
//! `Block`) the names match so the conversion is obvious.
//!
//! ## What ships in phase 19 (this task)
//!
//! - The trait definition, `GrammarSelection`, `GrammarKind`.
//! - GPUI action structs `ObjectNext` / `ObjectPrev` /
//!   `InnerContainer(kind)` / `AroundContainer(kind)` /
//!   `SelectAll(kind)` that pane crates attach handlers for.
//! - One full impl in `file-manager` as the proof-of-concept pane.
//!
//! Follow-up tasks (`phase-19/object-grammar-predicate-lang`,
//! `phase-19/object-grammar-editor`, `phase-19/object-grammar-git`,
//! `phase-19/object-grammar-diagnostics`, `phase-19/object-grammar-agent`,
//! `phase-19/object-grammar-terminal`) layer in the rest.
//!
//! See `REQ:codon/object-grammar` for the full design.

use gpui::{Action, App};
use serde::Deserialize;
use std::path::PathBuf;

/// The grammar vocabulary every pane shares. Each pane declares which
/// `GrammarKind`s it owns; calls into the trait for unsupported kinds
/// return [`GrammarSelection::Empty`] (no-op, never panic).
///
/// New kinds are added here as new pane impls land. Kept as a plain
/// enum so it round-trips through JSON-deserialised action payloads
/// without ceremony.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GrammarKind {
    /// Editor pane: a word (matches Helix's `w` motion target).
    Word,
    /// Editor pane: a paragraph.
    Paragraph,
    /// Editor pane: a function definition.
    Function,
    /// Editor pane: a class / impl block.
    Class,
    /// Editor pane: a bracket pair (parens / brackets / braces).
    BracketPair,
    /// File-manager pane: a file row.
    File,
    /// File-manager pane: a directory row.
    Directory,
    /// Git pane: a diff hunk.
    Hunk,
    /// Git pane: a commit.
    Commit,
    /// Diagnostics pane: a single diagnostic.
    Diagnostic,
    /// Agent pane: an assistant message.
    Message,
    /// Agent pane / terminal: a tool-call block / prompt-output block.
    Block,
}

/// Typed selection — the value the grammar trait moves around. Lives
/// alongside `codon-mode`'s palette-side [`Selection`][codon_mode_sel]
/// type but is intentionally separate: this one carries the motion
/// dispatcher's view (just enough to apply a cursor / mark change on
/// the focused pane) and stays in `codon-pane-bridge` so any pane crate
/// can return one without depending on `codon-mode`.
///
/// `Empty` is the no-op return for kinds a pane doesn't own; callers
/// should prefer matching on the variant rather than treating `Empty`
/// as a failure — it's a deliberate "this pane has no opinion".
///
/// [codon_mode_sel]: <crate-doc-only>
#[derive(Clone, Debug, Default)]
pub enum GrammarSelection {
    /// No selection produced (default, also the "pane doesn't own
    /// this kind" return).
    #[default]
    Empty,
    /// Editor text: a list of `(start, end)` offset ranges.
    Text { ranges: Vec<(usize, usize)> },
    /// File-manager paths. Ordered by display order; deduplicated.
    Files(Vec<PathBuf>),
    /// File-manager indices into the focused FM's `entries` vec, when
    /// the consumer wants positional info rather than paths. Optional
    /// alternative to [`Self::Files`] — pane impls pick whichever fits
    /// their UX (the fm impl uses this for `next`/`prev` cursor moves
    /// where the consumer is the fm itself).
    FileIndices(Vec<usize>),
    /// Git hunks. Each entry pairs the buffer path with a hunk id.
    Hunks(Vec<(PathBuf, u32)>),
    /// Git commits, identified by sha.
    Commits(Vec<String>),
    /// Diagnostics — see [`DiagnosticRef`].
    Diagnostics(Vec<DiagnosticRef>),
}

/// Reference to a diagnostic — enough to identify and re-resolve it
/// without keeping the live `lsp::Diagnostic` value alive past the
/// dispatcher hop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticRef {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
    pub message: String,
}

impl GrammarSelection {
    /// True iff the selection holds no items.
    pub fn is_empty(&self) -> bool {
        match self {
            GrammarSelection::Empty => true,
            GrammarSelection::Text { ranges } => ranges.is_empty(),
            GrammarSelection::Files(paths) => paths.is_empty(),
            GrammarSelection::FileIndices(indices) => indices.is_empty(),
            GrammarSelection::Hunks(hunks) => hunks.is_empty(),
            GrammarSelection::Commits(shas) => shas.is_empty(),
            GrammarSelection::Diagnostics(diags) => diags.is_empty(),
        }
    }
}

/// The per-pane "movement / refinement" trait the UX shell drives
/// from Normal-mode keys.
///
/// Every method takes the *current* selection (so motions can advance
/// from where the cursor is) and returns the *next* selection. Pane
/// impls that don't own a given [`GrammarKind`] return
/// [`GrammarSelection::Empty`] — never panic.
///
/// The trait is intentionally `&self` so an impl can compute the
/// motion target without taking exclusive access to the pane's state.
/// The pane crate's action handler is responsible for *applying* the
/// returned selection (moving the cursor, repainting, etc.); the
/// trait itself is pure.
pub trait ObjectGrammar {
    /// Selection one step forward of `from`, of the given `kind`.
    fn next(&self, kind: GrammarKind, from: &GrammarSelection) -> GrammarSelection {
        let _ = (kind, from);
        GrammarSelection::Empty
    }

    /// Selection one step backward of `from`, of the given `kind`.
    fn prev(&self, kind: GrammarKind, from: &GrammarSelection) -> GrammarSelection {
        let _ = (kind, from);
        GrammarSelection::Empty
    }

    /// "Inner container" — the contents of the container of kind `of`
    /// that encloses `from`. `mip` paragraph for editor; all files in
    /// the current directory for fm; all hunks in a file for git.
    fn inner_container(&self, of: GrammarKind, from: &GrammarSelection) -> GrammarSelection {
        let _ = (of, from);
        GrammarSelection::Empty
    }

    /// "Around container" — like [`inner_container`](Self::inner_container)
    /// but including the container's own delimiters (its name as an
    /// entry, its braces, etc.).
    fn around_container(&self, of: GrammarKind, from: &GrammarSelection) -> GrammarSelection {
        let _ = (of, from);
        GrammarSelection::Empty
    }

    /// `%<kind>` — every visible object of the given kind.
    fn select_all(&self, kind: GrammarKind) -> GrammarSelection {
        let _ = kind;
        GrammarSelection::Empty
    }

    /// The pane's *natural* object kind — what `w` / `b` / `%` operate
    /// on when no explicit kind suffix is given. File-manager =
    /// `File`, git = `Hunk`, diagnostics = `Diagnostic`, etc.
    fn primary_grammar_kind(&self) -> GrammarKind;
}

// ---------------------------------------------------------------------------
// Actions — the keymap-side surface
// ---------------------------------------------------------------------------

/// `codon_panes::ObjectNext` — `w`. Advance the focused pane's cursor
/// to the next object of its [`ObjectGrammar::primary_grammar_kind`].
///
/// Empty payload so it round-trips cleanly through codon's TOML
/// keymap (`"w" = "codon_panes::ObjectNext"`).
#[derive(Clone, Debug, Default, PartialEq, Deserialize, schemars::JsonSchema, Action)]
#[action(namespace = codon_panes)]
#[serde(deny_unknown_fields)]
pub struct ObjectNext;

/// `codon_panes::ObjectPrev` — `b`. Step backward.
#[derive(Clone, Debug, Default, PartialEq, Deserialize, schemars::JsonSchema, Action)]
#[action(namespace = codon_panes)]
#[serde(deny_unknown_fields)]
pub struct ObjectPrev;

/// `codon_panes::InnerContainer("file")` — `mi<kind>`.
///
/// Payload is the [`GrammarKind`] (snake-cased) the user wants the
/// inner container of: `mip` → `paragraph`, `mif` → `file`, `mih` →
/// `hunk`, etc. The TOML keymap binds the per-chord shapes; the
/// action carries the resolved kind so the dispatcher doesn't need to
/// know which key suffix produced it.
#[derive(Clone, Debug, PartialEq, Deserialize, schemars::JsonSchema, Action)]
#[action(namespace = codon_panes)]
#[serde(deny_unknown_fields)]
pub struct InnerContainer(pub GrammarKind);

/// `codon_panes::AroundContainer("file")` — `ma<kind>`.
#[derive(Clone, Debug, PartialEq, Deserialize, schemars::JsonSchema, Action)]
#[action(namespace = codon_panes)]
#[serde(deny_unknown_fields)]
pub struct AroundContainer(pub GrammarKind);

/// `codon_panes::SelectAll("file")` — `%<kind>`. Every visible object
/// of the given kind in the focused pane.
#[derive(Clone, Debug, PartialEq, Deserialize, schemars::JsonSchema, Action)]
#[action(namespace = codon_panes)]
#[serde(deny_unknown_fields)]
pub struct SelectAll(pub GrammarKind);

/// No-op registration hook kept for symmetry with the other codon
/// crates that wire their actions at app boot. The `Action` derive
/// macro registers each type with the global action registry as soon
/// as the type's symbol is referenced; pane crates attach
/// `on_action` listeners for these actions at render time, so the
/// trait impl is consulted from the listener (not from a focus-driven
/// dispatcher) and each pane keeps ownership of how it applies the
/// returned [`GrammarSelection`].
pub fn init(_cx: &mut App) {}
