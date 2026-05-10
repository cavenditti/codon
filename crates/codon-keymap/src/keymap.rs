use gpui::{App, KeyBinding};
use serde::Deserialize;
use std::collections::HashMap;

/// Codon's TOML keymap format.
///
/// Example:
/// ```toml
/// [bindings.global]
/// "cmd-k h" = "workspace::ActivatePaneLeft"
/// "cmd-k t" = "workspace::NewTerminal"
///
/// [bindings.file_manager.normal]
/// "j" = "file_manager::NavigateDown"
///
/// [bindings.terminal.normal]
/// "j" = "terminal::ScrollLineDown"
/// ```
#[derive(Debug, Deserialize, Default)]
struct CodonKeymap {
    #[serde(default)]
    bindings: KeymapBindings,
}

#[derive(Debug, Deserialize, Default)]
struct KeymapBindings {
    #[serde(default)]
    global: HashMap<String, String>,
    #[serde(default)]
    editor: Option<ModeBindings>,
    #[serde(default)]
    terminal: Option<ModeBindings>,
    #[serde(default)]
    file_manager: Option<ModeBindings>,
}

#[derive(Debug, Deserialize, Default)]
struct ModeBindings {
    #[serde(default)]
    normal: HashMap<String, String>,
    #[serde(default)]
    insert: HashMap<String, String>,
}

const DEFAULT_KEYMAP: &str = r#"
# Codon default keybindings
# Override in ~/.config/codon/keymap.toml

[bindings.global]
# Pane focus (vim-style)
"ctrl-h" = "workspace::ActivatePaneLeft"
"ctrl-j" = "workspace::ActivatePaneDown"
"ctrl-k" = "workspace::ActivatePaneUp"
"ctrl-l" = "workspace::ActivatePaneRight"

# Pane move (swap with adjacent pane)
"ctrl-shift-h" = "workspace::SwapPaneLeft"
"ctrl-shift-j" = "workspace::SwapPaneDown"
"ctrl-shift-k" = "workspace::SwapPaneUp"
"ctrl-shift-l" = "workspace::SwapPaneRight"

# Pane resize (cmd-k prefix to avoid conflict)
"cmd-k shift-h" = "vim::ResizePaneLeft"
"cmd-k shift-j" = "vim::ResizePaneDown"
"cmd-k shift-k" = "vim::ResizePaneUp"
"cmd-k shift-l" = "vim::ResizePaneRight"

# Pane navigation (cmd-k h/j/k/l) — kept for back-compat, equivalent to ctrl-*
"cmd-k h" = "workspace::ActivatePaneLeft"
"cmd-k j" = "workspace::ActivatePaneDown"
"cmd-k k" = "workspace::ActivatePaneUp"
"cmd-k l" = "workspace::ActivatePaneRight"

# Pane splitting
"cmd-k |" = "pane::SplitRight"
"cmd-k -" = "pane::SplitDown"
"cmd-k \\" = "pane::SplitRight"

# Pane management
"cmd-k t" = "workspace::NewTerminal"
"cmd-k e" = "file_manager::Open"
"cmd-k w" = "pane::CloseActiveItem"

# Sessions (cmd-k s prefix)
"cmd-k s n" = "codon_session::SessionNew"
"cmd-k s s" = "codon_session::SessionSwitch"
"cmd-k s c" = "codon_session::SessionClose"

# Windows (cmd-k W prefix — capital W, lowercase w is "close active item")
"cmd-k shift-w n" = "codon_session::WindowNew"
"cmd-k shift-w l" = "codon_session::WindowNext"
"cmd-k shift-w h" = "codon_session::WindowPrev"
"cmd-k shift-w c" = "codon_session::WindowClose"

# Command palette
"cmd-shift-p" = "command_palette::Toggle"
"#;

/// Load Codon keybindings. Called from reload_keymaps so it survives keymap reloads.
pub fn load_codon_keymap(cx: &mut App) {
    if let Some(bindings) = parse_keymap(DEFAULT_KEYMAP) {
        apply_bindings(bindings, cx);
    }

    let config_dir = dirs::config_dir().map(|d| d.join("codon").join("keymap.toml"));
    if let Some(path) = config_dir {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Some(bindings) = parse_keymap(&content) {
                apply_bindings(bindings, cx);
            } else {
                log::warn!("Failed to parse Codon keymap at {}", path.display());
            }
        }
    }
}

fn parse_keymap(content: &str) -> Option<Vec<(String, String, Option<String>)>> {
    let keymap: CodonKeymap = toml::from_str(content).ok()?;
    let mut result = Vec::new();

    for (keystroke, action) in &keymap.bindings.global {
        result.push((keystroke.clone(), action.clone(), None));
    }

    if let Some(editor) = &keymap.bindings.editor {
        add_mode_bindings(&mut result, "Editor", editor);
    }
    if let Some(terminal) = &keymap.bindings.terminal {
        add_mode_bindings(&mut result, "Terminal", terminal);
    }
    if let Some(fm) = &keymap.bindings.file_manager {
        add_mode_bindings(&mut result, "FileManager", fm);
    }

    Some(result)
}

fn add_mode_bindings(
    result: &mut Vec<(String, String, Option<String>)>,
    pane_kind: &str,
    modes: &ModeBindings,
) {
    for (keystroke, action) in &modes.normal {
        result.push((
            keystroke.clone(),
            action.clone(),
            Some(format!("{} && pane_mode == normal", pane_kind)),
        ));
    }
    for (keystroke, action) in &modes.insert {
        result.push((
            keystroke.clone(),
            action.clone(),
            Some(format!("{} && pane_mode == insert", pane_kind)),
        ));
    }
}

fn apply_bindings(bindings: Vec<(String, String, Option<String>)>, cx: &mut App) {
    let mut key_bindings = Vec::new();

    for (keystroke, action_name, context) in bindings {
        if let Some(binding) = resolve_binding(&keystroke, &action_name, context.as_deref()) {
            key_bindings.push(binding);
        }
    }

    cx.bind_keys(key_bindings);
}

fn resolve_binding(
    keystroke: &str,
    action_name: &str,
    context: Option<&str>,
) -> Option<KeyBinding> {
    macro_rules! bind {
        ($action:expr) => {
            Some(KeyBinding::new(keystroke, $action, context))
        };
    }
    match action_name {
        // Workspace pane focus
        "workspace::ActivatePaneLeft" => bind!(workspace::ActivatePaneLeft),
        "workspace::ActivatePaneRight" => bind!(workspace::ActivatePaneRight),
        "workspace::ActivatePaneUp" => bind!(workspace::ActivatePaneUp),
        "workspace::ActivatePaneDown" => bind!(workspace::ActivatePaneDown),

        // Workspace pane swap
        "workspace::SwapPaneLeft" => bind!(workspace::SwapPaneLeft),
        "workspace::SwapPaneRight" => bind!(workspace::SwapPaneRight),
        "workspace::SwapPaneUp" => bind!(workspace::SwapPaneUp),
        "workspace::SwapPaneDown" => bind!(workspace::SwapPaneDown),

        // Vim pane resize
        "vim::ResizePaneLeft" => bind!(vim::ResizePaneLeft),
        "vim::ResizePaneRight" => bind!(vim::ResizePaneRight),
        "vim::ResizePaneUp" => bind!(vim::ResizePaneUp),
        "vim::ResizePaneDown" => bind!(vim::ResizePaneDown),

        // Workspace
        "workspace::NewTerminal" => bind!(workspace::NewTerminal { local: false }),

        // Pane
        "pane::SplitRight" => bind!(workspace::pane::SplitRight::default()),
        "pane::SplitDown" => bind!(workspace::pane::SplitDown::default()),
        "pane::SplitLeft" => bind!(workspace::pane::SplitLeft::default()),
        "pane::SplitUp" => bind!(workspace::pane::SplitUp::default()),
        "pane::CloseActiveItem" => bind!(workspace::CloseActiveItem {
            save_intent: None,
            close_pinned: false,
        }),

        // File manager
        "file_manager::Open" => bind!(file_manager::Open),

        // Codon session
        "codon_session::SessionNew" => bind!(codon_session::SessionNew),
        "codon_session::SessionSwitch" => bind!(codon_session::SessionSwitch),
        "codon_session::SessionClose" => bind!(codon_session::SessionClose),
        "codon_session::SessionRename" => bind!(codon_session::SessionRename),
        "codon_session::WindowNew" => bind!(codon_session::WindowNew),
        "codon_session::WindowNext" => bind!(codon_session::WindowNext),
        "codon_session::WindowPrev" => bind!(codon_session::WindowPrev),
        "codon_session::WindowClose" => bind!(codon_session::WindowClose),

        // Command palette
        "command_palette::Toggle" => bind!(zed_actions::command_palette::Toggle),

        _ => {
            log::warn!("Unknown action in keymap: {}", action_name);
            None
        }
    }
}
