use std::{env, fs, path::PathBuf};

use gpui::{
    Context, Entity, Focusable as _, IntoElement, KeyBinding, Render, SharedString, Window,
    WindowOptions, div, prelude::*, px,
};
use md_editor::{
    Backspace, Delete, InsertNewline, MarkdownEditor, MoveDown, MoveLeft, MoveRight,
    MoveToBeginningOfLine, MoveToEndOfLine, MoveUp, SelectAll, SelectDown, SelectLeft, SelectRight,
    SelectToBeginningOfLine, SelectToEndOfLine, SelectUp,
};

struct MarkdownEditorShell {
    editor: Entity<MarkdownEditor>,
    path: Option<PathBuf>,
    load_error: Option<String>,
}

impl MarkdownEditorShell {
    fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (contents, load_error) = match path.as_ref() {
            Some(path) => match fs::read_to_string(path) {
                Ok(contents) => (contents, None),
                Err(error) => {
                    let message =
                        format!("failed to open markdown file {}: {error:#}", path.display());
                    eprintln!("{message}");
                    (String::new(), Some(message))
                }
            },
            None => (String::new(), None),
        };

        let editor = cx.new(|cx| MarkdownEditor::for_text(contents, cx));
        window.focus(&editor.focus_handle(cx), cx);

        Self {
            editor,
            path,
            load_error,
        }
    }

    fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|file_name| file_name.to_str())
            .map(ToString::to_string)
            .unwrap_or_else(|| "Untitled".to_string())
    }

    fn path_label(&self) -> String {
        self.path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Unsaved Markdown document".to_string())
    }

    fn render_title_bar(&self) -> impl IntoElement {
        div()
            .h(px(36.))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .bg(gpui::rgb(0x202020))
            .border_b_1()
            .border_color(gpui::rgb(0x303030))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(gpui::rgb(0xf0f0f0))
                            .child("Markdown Editor"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(gpui::rgba(0xffffff99))
                            .child(SharedString::from(self.path_label())),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(gpui::rgba(0xffffff99))
                    .child(SharedString::from(format!("{} · R3 editing", self.title()))),
            )
    }
}

impl Render for MarkdownEditorShell {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(gpui::rgb(0x181818))
            .child(self.render_title_bar())
            .when_some(self.load_error.clone(), |this, load_error| {
                this.child(
                    div()
                        .px_3()
                        .py_2()
                        .bg(gpui::rgb(0x3a241f))
                        .text_color(gpui::rgb(0xffc7b8))
                        .text_xs()
                        .child(SharedString::from(load_error)),
                )
            })
            .child(div().flex_1().overflow_hidden().child(self.editor.clone()))
    }
}

pub fn run() {
    let path = env::args_os().nth(1).map(PathBuf::from);

    gpui_platform::application().run(move |cx| {
        cx.bind_keys([
            KeyBinding::new("left", MoveLeft, Some("MarkdownEditor")),
            KeyBinding::new("right", MoveRight, Some("MarkdownEditor")),
            KeyBinding::new("up", MoveUp, Some("MarkdownEditor")),
            KeyBinding::new("down", MoveDown, Some("MarkdownEditor")),
            KeyBinding::new("home", MoveToBeginningOfLine, Some("MarkdownEditor")),
            KeyBinding::new("end", MoveToEndOfLine, Some("MarkdownEditor")),
            KeyBinding::new("shift-left", SelectLeft, Some("MarkdownEditor")),
            KeyBinding::new("shift-right", SelectRight, Some("MarkdownEditor")),
            KeyBinding::new("shift-up", SelectUp, Some("MarkdownEditor")),
            KeyBinding::new("shift-down", SelectDown, Some("MarkdownEditor")),
            KeyBinding::new(
                "shift-home",
                SelectToBeginningOfLine,
                Some("MarkdownEditor"),
            ),
            KeyBinding::new("shift-end", SelectToEndOfLine, Some("MarkdownEditor")),
            KeyBinding::new("ctrl-a", SelectAll, Some("MarkdownEditor")),
            KeyBinding::new("cmd-a", SelectAll, Some("MarkdownEditor")),
            KeyBinding::new("backspace", Backspace, Some("MarkdownEditor")),
            KeyBinding::new("delete", Delete, Some("MarkdownEditor")),
            KeyBinding::new("enter", InsertNewline, Some("MarkdownEditor")),
        ]);

        let path = path.clone();
        match cx.open_window(WindowOptions::default(), move |window, cx| {
            cx.new(|cx| MarkdownEditorShell::new(path, window, cx))
        }) {
            Ok(_) => cx.activate(true),
            Err(error) => {
                eprintln!("failed to open markdown editor window: {error:#}");
                cx.quit();
            }
        }
    });
}
