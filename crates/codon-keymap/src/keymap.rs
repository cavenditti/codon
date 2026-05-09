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
# Pane navigation
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

# Command palette
"cmd-shift-p" = "command_palette::Toggle"
"#;

/// Load Codon keybindings. Called from reload_keymaps so it survives keymap reloads.
pub fn load_codon_keymap(cx: &mut App) {
    // Load and apply the default keymap
    if let Some(bindings) = parse_keymap(DEFAULT_KEYMAP) {
        apply_bindings(bindings, cx);
    }

    // Try to load user keymap from ~/.config/codon/keymap.toml
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

    // Global bindings (no context)
    for (keystroke, action) in &keymap.bindings.global {
        result.push((keystroke.clone(), action.clone(), None));
    }

    // Per-pane-kind bindings
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
    // Map action names to concrete action instances.
    macro_rules! bind {
        ($action:expr) => {
            Some(KeyBinding::new(keystroke, $action, context))
        };
    }
    match action_name {
        // Workspace
        "workspace::ActivatePaneLeft" => bind!(workspace::ActivatePaneLeft),
        "workspace::ActivatePaneRight" => bind!(workspace::ActivatePaneRight),
        "workspace::ActivatePaneUp" => bind!(workspace::ActivatePaneUp),
        "workspace::ActivatePaneDown" => bind!(workspace::ActivatePaneDown),
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
        // Command palette
        "command_palette::Toggle" => bind!(zed_actions::command_palette::Toggle),
        _ => {
            log::warn!("Unknown action in keymap: {}", action_name);
            None
        }
    }
}
