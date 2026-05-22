#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "legacy-editor")]
mod legacy_editor;

fn main() {
    #[cfg(feature = "legacy-editor")]
    legacy_editor::run();

    #[cfg(not(feature = "legacy-editor"))]
    {
        // R0 placeholder: md-editor path not yet implemented.
        // Full implementation follows in stage R3 (see REFACTOR_GOAL.md).
        eprintln!("md-editor path: not yet implemented (see REFACTOR_GOAL.md \u{00a7}R3)");
    }
}
