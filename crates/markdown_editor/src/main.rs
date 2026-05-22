#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(feature = "legacy-editor")]
mod legacy_editor;

#[cfg(all(feature = "md-editor", not(feature = "legacy-editor")))]
mod md_editor_app;

fn main() {
    #[cfg(feature = "legacy-editor")]
    {
        legacy_editor::run();
        return;
    }

    #[cfg(all(feature = "md-editor", not(feature = "legacy-editor")))]
    {
        md_editor_app::run();
        return;
    }

    #[cfg(all(not(feature = "legacy-editor"), not(feature = "md-editor")))]
    eprintln!("enable either the legacy-editor or md-editor feature");
}
