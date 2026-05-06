use gpui::actions;

actions!(
    editor,
    [
        // Motion
        MoveCharLeft,
        MoveCharRight,
        MoveVisualLineUp,
        MoveVisualLineDown,
        MoveNextWordStart,
        MovePrevWordStart,
        MoveNextWordEnd,
        GotoFileStart,
        GotoLastLine,
        GotoLineStart,
        GotoLineEnd,
        // Mode switching
        InsertMode,
        AppendMode,
        NormalMode,
        // Editing
        DeleteSelection,
        ChangeSelection,
        DeleteCharBackward,
        DeleteCharForward,
        InsertNewline,
        Undo,
        Redo,
        // Line operations
        OpenBelow,
        OpenAbove,
        // Scrolling
        PageUp,
        PageDown,
        HalfPageUp,
        HalfPageDown,
    ]
);

actions!(
    pane,
    [FocusLeft, FocusRight]
);

actions!(
    workspace,
    [CommandPalette]
);
