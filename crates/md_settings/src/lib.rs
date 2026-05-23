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

/// Default editor keymap bindings (Zed-style key bindings).
///
/// Format: `"key combination" -> ActionName`
/// This is used as a reference; actual binding is done via `gpui::KeyBinding` in code.
pub const DEFAULT_EDITOR_KEYMAP: &str = "\
left        -> MoveLeft\
right       -> MoveRight\
up          -> MoveUp\
down        -> MoveDown\
home        -> MoveToBeginningOfLine\
end         -> MoveToEndOfLine\
shift-left  -> SelectLeft\
shift-right -> SelectRight\
shift-up    -> SelectUp\
shift-down  -> SelectDown\
shift-home  -> SelectToBeginningOfLine\
shift-end   -> SelectToEndOfLine\
ctrl-a      -> SelectAll\
cmd-a       -> SelectAll\
backspace   -> Backspace\
delete      -> Delete\
enter       -> InsertNewline\
tab         -> Tab\
ctrl-z      -> Undo\
cmd-z       -> Undo\
ctrl-shift-z -> Redo\
cmd-shift-z -> Redo\
";

/// Default application-level keymap bindings.
///
/// These control the outer shell (new/open/save/mode toggle).
pub const DEFAULT_APP_KEYMAP: &str = "\
ctrl-n           -> NewDocument\
cmd-n            -> NewDocument\
ctrl-o           -> OpenDocument\
cmd-o            -> OpenDocument\
ctrl-shift-m     -> ToggleMode\
cmd-shift-m      -> ToggleMode\
ctrl-s           -> Save\
cmd-s            -> Save\
ctrl-shift-s     -> SaveAs\
cmd-shift-s      -> SaveAs\
";
