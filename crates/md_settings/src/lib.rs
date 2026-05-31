//! Minimal Updraft Editor settings.
//! Ported from crates/settings (JSON load + keymap subset) in R4.

/// Editor behavior settings for the standalone markdown editor.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorSettings {
    pub tab_size: usize,
    pub use_soft_tabs: bool,
}

impl Default for EditorSettings {
    fn default() -> Self {
        Self {
            tab_size: DEFAULT_TAB_SIZE,
            use_soft_tabs: true,
        }
    }
}

/// Global Updraft Editor settings.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MarkdownSettings {
    pub editor: EditorSettings,
}

impl MarkdownSettings {
    /// Load hard-coded default settings.
    pub fn load_defaults() -> Self {
        Self::default()
    }
}

/// Default tab size (number of spaces per indentation level).
pub const DEFAULT_TAB_SIZE: usize = 4;

/// Default text size in pixels.
pub const DEFAULT_TEXT_SIZE: f32 = 14.0;

/// Default line height in pixels.
pub const DEFAULT_LINE_HEIGHT: f32 = 22.0;

/// Default caret height in pixels.
pub const DEFAULT_CARET_HEIGHT: f32 = 17.0;

/// Default minimum row height in pixels.
pub const DEFAULT_MIN_ROW_HEIGHT: f32 = 22.0;

/// Editor keybinding specification: (keystroke, action_name, key_context).
///
/// `action_name` is the string form of a `gpui::actions!`-generated action.
/// The consuming crate uses this to look up the concrete action type
/// and construct `gpui::KeyBinding` at runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyBindingSpec {
    pub keystroke: &'static str,
    pub action: &'static str,
    pub context: &'static str,
}

const fn key_binding(keystroke: &'static str, action: &'static str) -> KeyBindingSpec {
    KeyBindingSpec {
        keystroke,
        action,
        context: "MarkdownEditor",
    }
}

/// Default editor keymap bindings (Zed-style key bindings).
///
/// Each entry maps a keystroke to an action name within the
/// `MarkdownEditor` key context. The `md_editor` crate consumes
/// these to construct `gpui::KeyBinding` values.
pub const DEFAULT_EDITOR_KEYBINDINGS: &[KeyBindingSpec] = &[
    key_binding("left", "MoveLeft"),
    key_binding("right", "MoveRight"),
    key_binding("up", "MoveUp"),
    key_binding("down", "MoveDown"),
    key_binding("home", "MoveToBeginningOfLine"),
    key_binding("end", "MoveToEndOfLine"),
    key_binding("shift-left", "SelectLeft"),
    key_binding("shift-right", "SelectRight"),
    key_binding("shift-up", "SelectUp"),
    key_binding("shift-down", "SelectDown"),
    key_binding("shift-home", "SelectToBeginningOfLine"),
    key_binding("shift-end", "SelectToEndOfLine"),
    key_binding("ctrl-a", "SelectAll"),
    key_binding("cmd-a", "SelectAll"),
    key_binding("ctrl-c", "Copy"),
    key_binding("cmd-c", "Copy"),
    key_binding("ctrl-v", "Paste"),
    key_binding("cmd-v", "Paste"),
    key_binding("ctrl-x", "Cut"),
    key_binding("cmd-x", "Cut"),
    key_binding("backspace", "Backspace"),
    key_binding("delete", "Delete"),
    key_binding("enter", "InsertNewline"),
    key_binding("shift-enter", "InsertSoftBreak"),
    key_binding("tab", "Tab"),
    key_binding("shift-tab", "ShiftTab"),
    key_binding("ctrl-b", "ToggleBold"),
    key_binding("cmd-b", "ToggleBold"),
    key_binding("ctrl-i", "ToggleItalic"),
    key_binding("cmd-i", "ToggleItalic"),
    key_binding("ctrl-z", "Undo"),
    key_binding("cmd-z", "Undo"),
    key_binding("ctrl-shift-z", "Redo"),
    key_binding("cmd-shift-z", "Redo"),
];

/// Default application-level keymap bindings.
///
/// These control the outer shell (new/open/save/mode toggle)
/// within the `MarkdownEditor` key context.
pub const DEFAULT_APP_KEYBINDINGS: &[KeyBindingSpec] = &[
    key_binding("ctrl-n", "NewDocument"),
    key_binding("cmd-n", "NewDocument"),
    key_binding("ctrl-o", "OpenDocument"),
    key_binding("cmd-o", "OpenDocument"),
    key_binding("ctrl-shift-m", "ToggleMode"),
    key_binding("cmd-shift-m", "ToggleMode"),
    key_binding("ctrl-s", "Save"),
    key_binding("cmd-s", "Save"),
    key_binding("ctrl-shift-s", "SaveAs"),
    key_binding("cmd-shift-s", "SaveAs"),
];
