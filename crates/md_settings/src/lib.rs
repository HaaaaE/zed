//! Minimal markdown-editor settings.
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

/// Global markdown-editor settings.
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

/// Default editor keymap bindings (Zed-style key bindings).
///
/// Each entry maps a keystroke to an action name within the
/// `MarkdownEditor` key context. The `md_editor` crate consumes
/// these to construct `gpui::KeyBinding` values.
pub const DEFAULT_EDITOR_KEYBINDINGS: &[KeyBindingSpec] = &[
    KeyBindingSpec { keystroke: "left", action: "MoveLeft", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "right", action: "MoveRight", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "up", action: "MoveUp", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "down", action: "MoveDown", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "home", action: "MoveToBeginningOfLine", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "end", action: "MoveToEndOfLine", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "shift-left", action: "SelectLeft", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "shift-right", action: "SelectRight", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "shift-up", action: "SelectUp", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "shift-down", action: "SelectDown", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "shift-home", action: "SelectToBeginningOfLine", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "shift-end", action: "SelectToEndOfLine", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "ctrl-a", action: "SelectAll", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "cmd-a", action: "SelectAll", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "backspace", action: "Backspace", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "delete", action: "Delete", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "enter", action: "InsertNewline", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "tab", action: "Tab", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "ctrl-z", action: "Undo", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "cmd-z", action: "Undo", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "ctrl-shift-z", action: "Redo", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "cmd-shift-z", action: "Redo", context: "MarkdownEditor" },
];

/// Default application-level keymap bindings.
///
/// These control the outer shell (new/open/save/mode toggle)
/// within the `MarkdownEditor` key context.
pub const DEFAULT_APP_KEYBINDINGS: &[KeyBindingSpec] = &[
    KeyBindingSpec { keystroke: "ctrl-n", action: "NewDocument", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "cmd-n", action: "NewDocument", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "ctrl-o", action: "OpenDocument", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "cmd-o", action: "OpenDocument", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "ctrl-shift-m", action: "ToggleMode", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "cmd-shift-m", action: "ToggleMode", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "ctrl-s", action: "Save", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "cmd-s", action: "Save", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "ctrl-shift-s", action: "SaveAs", context: "MarkdownEditor" },
    KeyBindingSpec { keystroke: "cmd-shift-s", action: "SaveAs", context: "MarkdownEditor" },
];
