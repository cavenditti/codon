use gpui::actions;

// Motion commands
actions!(editor, [
    MoveCharLeft, MoveCharRight,
    MoveLineUp, MoveLineDown,
    MoveVisualLineUp, MoveVisualLineDown,
    MoveNextWordStart, MovePrevWordStart,
    MoveNextWordEnd, MovePrevWordEnd,
    MoveNextLongWordStart, MovePrevLongWordStart,
    MoveNextLongWordEnd, MovePrevLongWordEnd,
    // Extend (selection) variants
    ExtendCharLeft, ExtendCharRight,
    ExtendLineUp, ExtendLineDown,
    ExtendVisualLineUp, ExtendVisualLineDown,
    ExtendNextWordStart, ExtendPrevWordStart,
    ExtendNextWordEnd, ExtendPrevWordEnd,
    ExtendNextLongWordStart, ExtendPrevLongWordStart,
    ExtendNextLongWordEnd, ExtendPrevLongWordEnd,
    // Goto
    GotoFileStart, GotoFileEnd,
    GotoLastLine,
    GotoLineStart, GotoLineEnd,
    GotoFirstNonwhitespace,
    GotoWindowTop, GotoWindowCenter, GotoWindowBottom,
    GotoNextParagraph, GotoPrevParagraph,
    // Extend-to-goto
    ExtendToFileStart, ExtendToFileEnd, ExtendToLastLine,
    ExtendToLineStart, ExtendToLineEnd,
    ExtendToFirstNonwhitespace,
    // Find char (will need on_next_key)
    FindNextChar, FindTillChar, FindPrevChar, TillPrevChar,
    ExtendNextChar, ExtendTillChar, ExtendPrevChar, ExtendTillPrevChar,
    RepeatLastMotion,
    // Mode switching
    InsertMode, AppendMode, NormalMode, SelectMode, ExitSelectMode,
    InsertAtLineStart, InsertAtLineEnd,
    CommandMode,
    // Editing
    DeleteSelection, DeleteSelectionNoyank,
    ChangeSelection, ChangeSelectionNoyank,
    DeleteCharBackward, DeleteCharForward,
    DeleteWordBackward, DeleteWordForward,
    KillToLineStart, KillToLineEnd,
    InsertNewline, InsertTab,
    Replace,
    // Case
    SwitchCase, SwitchToUppercase, SwitchToLowercase,
    // Line operations
    OpenBelow, OpenAbove,
    AddNewlineBelow, AddNewlineAbove,
    JoinSelections, JoinSelectionsSpace,
    // Indentation
    Indent, Unindent,
    // Comments
    ToggleComments, ToggleLineComments, ToggleBlockComments,
    // Undo/Redo
    Undo, Redo, Earlier, Later,
    // Clipboard
    Yank, YankToClipboard, YankMainSelectionToClipboard,
    PasteAfter, PasteBefore,
    PasteClipboardAfter, PasteClipboardBefore,
    ReplaceWithYanked, ReplaceSelectionsWithClipboard,
    // Selection management
    SelectAll, CollapseSelection, FlipSelections, EnsureSelectionsForward,
    KeepPrimarySelection, RemovePrimarySelection,
    ExtendLine, ExtendLineBelow, ExtendLineAbove,
    ExtendToLineBounds, ShrinkToLineBounds,
    SelectCurrentLine,
    SplitSelectionOnNewline,
    MergeSelections, MergeConsecutiveSelections,
    RotateSelectionsForward, RotateSelectionsBackward,
    // Search
    Search, Rsearch, SearchNext, SearchPrev,
    ExtendSearchNext, ExtendSearchPrev,
    SearchSelection,
    // Match
    MatchBrackets,
    // Surround
    SurroundAdd, SurroundReplace, SurroundDelete,
    // Text objects
    SelectTextobjectAround, SelectTextobjectInner,
    // Expand/shrink (syntax tree)
    ExpandSelection, ShrinkSelection,
    SelectNextSibling, SelectPrevSibling,
    // Scroll/page
    PageUp, PageDown, HalfPageUp, HalfPageDown,
    ScrollUp, ScrollDown,
    AlignViewMiddle, AlignViewTop, AlignViewCenter, AlignViewBottom,
    // Jumps
    JumpForward, JumpBackward, SaveSelection,
    // Increment/decrement
    Increment, Decrement,
    // Misc
    NoOp,
]);

// Pane management
actions!(pane, [FocusLeft, FocusRight, FocusUp, FocusDown, SplitRight, SplitDown, Close]);

// Workspace
actions!(workspace, [CommandPalette]);
