#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use assets::Assets;
use editor::Editor;
use gpui::{Context, Entity, Focusable as _, Window, WindowOptions};
use settings::{DEFAULT_KEYMAP_PATH, KeymapFile};
use theme::LoadThemes;
use ui::{div, prelude::*};

struct MarkdownEditorShell {
    editor: Entity<Editor>,
}

impl MarkdownEditorShell {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| Editor::multi_line(window, cx));
        window.focus(&editor.focus_handle(cx), cx);
        Self { editor }
    }
}

impl Render for MarkdownEditorShell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.editor.clone())
    }
}

fn main() {
    gpui_platform::application().with_assets(Assets).run(|cx| {
        settings::init(cx);
        theme_settings::init(LoadThemes::All(Box::new(Assets)), cx);
        editor::init(cx);

        if let Ok(key_bindings) =
            KeymapFile::load_asset_allow_partial_failure(DEFAULT_KEYMAP_PATH, cx)
        {
            cx.bind_keys(key_bindings);
        }

        if let Err(error) = Assets.load_fonts(cx) {
            eprintln!("failed to load bundled fonts: {error:#}");
        }

        match cx.open_window(WindowOptions::default(), |window, cx| {
            cx.new(|cx| MarkdownEditorShell::new(window, cx))
        }) {
            Ok(_) => cx.activate(true),
            Err(error) => {
                eprintln!("failed to open markdown editor window: {error:#}");
                cx.quit();
            }
        }
    });
}
