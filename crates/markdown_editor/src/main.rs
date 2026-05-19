#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use assets::Assets;
use gpui::{Context, Window, WindowOptions};
use theme::{ActiveTheme, LoadThemes};
use ui::{div, prelude::*};

struct MarkdownEditorShell;

impl Render for MarkdownEditorShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(colors.editor_background)
            .text_color(colors.text)
            .child(
                div()
                    .flex()
                    .h_12()
                    .items_center()
                    .border_b_1()
                    .border_color(colors.border)
                    .px_4()
                    .child("Markdown Editor"),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .text_color(colors.text_muted)
                    .child("Markdown-only entry is ready. Editor integration comes next."),
            )
    }
}

fn main() {
    gpui_platform::application().with_assets(Assets).run(|cx| {
        settings::init(cx);
        theme_settings::init(LoadThemes::All(Box::new(Assets)), cx);

        if let Err(error) = Assets.load_fonts(cx) {
            eprintln!("failed to load bundled fonts: {error:#}");
        }

        match cx.open_window(WindowOptions::default(), |_, cx| {
            cx.new(|_| MarkdownEditorShell)
        }) {
            Ok(_) => cx.activate(true),
            Err(error) => {
                eprintln!("failed to open markdown editor window: {error:#}");
                cx.quit();
            }
        }
    });
}
