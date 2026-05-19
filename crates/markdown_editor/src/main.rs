#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{env, fs, path::PathBuf};

use anyhow::{Context as _, Result};
use assets::Assets;
use editor::{Editor, EditorEvent};
use gpui::{Context, Entity, Focusable as _, KeyBinding, Subscription, Window, WindowOptions, actions};
use language::Buffer;
use settings::{DEFAULT_KEYMAP_PATH, KeymapFile};
use theme::LoadThemes;
use ui::{div, prelude::*};

actions!(markdown_editor, [Save]);

struct MarkdownEditorShell {
    editor: Entity<Editor>,
    path: Option<PathBuf>,
    status: DocumentStatus,
    _editor_subscription: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DocumentStatus {
    Saved,
    Unsaved,
}

impl DocumentStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Saved => "Saved",
            Self::Unsaved => "Unsaved",
        }
    }
}

impl MarkdownEditorShell {
    fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let contents = path
            .as_ref()
            .map(read_markdown_file)
            .transpose()
            .unwrap_or_else(|error| {
                eprintln!("failed to open markdown file: {error:#}");
                None
            })
            .unwrap_or_default();
        let editor = cx.new(|cx| {
            let buffer = cx.new(|cx| Buffer::local(contents, cx));
            Editor::for_buffer(buffer, None, window, cx)
        });
        let editor_subscription = cx.subscribe(&editor, |this, _editor, event, cx| match event {
            EditorEvent::DirtyChanged | EditorEvent::Saved => {
                this.refresh_status(cx);
            }
            _ => {}
        });
        window.focus(&editor.focus_handle(cx), cx);
        let mut this = Self {
            editor,
            path,
            status: DocumentStatus::Saved,
            _editor_subscription: editor_subscription,
        };
        this.refresh_status(cx);
        this
    }

    fn save(&mut self, _: &Save, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.path.as_ref() else {
            eprintln!("cannot save untitled markdown document yet");
            return;
        };

        let contents = self.editor.read(cx).text(cx);
        if let Err(error) = fs::write(path, contents) {
            eprintln!("failed to save markdown file {}: {error:#}", path.display());
            return;
        }

        if let Some(buffer) = self.singleton_buffer(cx) {
            let version = buffer.read(cx).text_snapshot().version().clone();
            buffer.update(cx, |buffer, cx| buffer.did_save(version, None, cx));
        }
        self.refresh_status(cx);
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        let status = if self
            .editor
            .read(cx)
            .buffer()
            .read(cx)
            .is_dirty(cx)
        {
            DocumentStatus::Unsaved
        } else {
            DocumentStatus::Saved
        };

        if self.status != status {
            self.status = status;
            cx.notify();
        }
    }

    fn singleton_buffer(&self, cx: &Context<Self>) -> Option<Entity<Buffer>> {
        self.editor
            .read(cx)
            .buffer()
            .read(cx)
            .as_singleton()
    }

    fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|file_name| file_name.to_str())
            .unwrap_or("Untitled")
            .to_string()
    }

    fn path_label(&self) -> String {
        self.path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Unsaved Markdown document".to_string())
    }
}

impl Render for MarkdownEditorShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().colors().editor_background)
            .on_action(cx.listener(Self::save))
            .child(
                div()
                    .h_10()
                    .px_4()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(cx.theme().colors().border)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(div().text_sm().child(self.title()))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(cx.theme().colors().text_muted)
                                    .child(self.path_label()),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(self.status.label()),
                    ),
            )
            .child(div().flex_1().child(self.editor.clone()))
    }
}

fn main() {
    let path = env::args_os().nth(1).map(PathBuf::from);

    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            settings::init(cx);
            theme_settings::init(LoadThemes::All(Box::new(Assets)), cx);
            editor::init(cx);

            if let Ok(key_bindings) =
                KeymapFile::load_asset_allow_partial_failure(DEFAULT_KEYMAP_PATH, cx)
            {
                cx.bind_keys(key_bindings);
            }
            cx.bind_keys([KeyBinding::new("ctrl-s", Save, Some("Editor"))]);

            if let Err(error) = Assets.load_fonts(cx) {
                eprintln!("failed to load bundled fonts: {error:#}");
            }

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

fn read_markdown_file(path: &PathBuf) -> Result<String> {
    anyhow::ensure!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "md" | "markdown")),
        "expected a .md or .markdown file, got {}",
        path.display()
    );

    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}
