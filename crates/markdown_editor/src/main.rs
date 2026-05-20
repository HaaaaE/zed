#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{env, fs, path::PathBuf};

use anyhow::{Context as _, Result};
use assets::Assets;
use editor::{Editor, EditorEvent};
use gpui::{
    App, Context, Entity, Focusable as _, KeyBinding, MouseButton, PathPromptOptions, SharedString,
    Subscription, Window, WindowOptions, actions,
};
use language::Buffer;
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownStyle};
use settings::{DEFAULT_KEYMAP_PATH, KeymapFile};
use theme::LoadThemes;
use ui::{
    Button, ButtonSize, ButtonStyle, LabelSize, prelude::*, utils::platform_title_bar_height,
};

actions!(
    markdown_editor,
    [
        NewDocument,
        OpenDocument,
        Save,
        SaveAs,
        TogglePreview,
        ToggleCommandPalette
    ]
);

struct MarkdownEditorShell {
    document: Document,
    show_preview: bool,
    show_command_palette: bool,
    preview: Entity<Markdown>,
}

struct Document {
    editor: Entity<Editor>,
    path: Option<PathBuf>,
    title: Option<String>,
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
        let preview = cx.new(|cx| Markdown::new(SharedString::default(), None, None, cx));
        let document = Self::build_document(path, None, window, cx);
        let mut this = Self {
            document,
            show_preview: false,
            show_command_palette: false,
            preview,
        };

        this.focus_editor(window, cx);
        this.refresh_preview(cx);
        this
    }

    fn build_document(
        path: Option<PathBuf>,
        title: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Document {
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
            EditorEvent::DirtyChanged | EditorEvent::Saved | EditorEvent::BufferEdited => {
                this.refresh_status(cx);
                this.refresh_preview(cx);
            }
            _ => {}
        });

        Document {
            editor,
            path,
            title,
            status: DocumentStatus::Saved,
            _editor_subscription: editor_subscription,
        }
    }

    fn new_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.document = Self::build_document(None, Some("Untitled".to_string()), window, cx);
        self.focus_editor(window, cx);
        self.refresh_preview(cx);
        cx.notify();
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.document = Self::build_document(Some(path), None, window, cx);
        self.focus_editor(window, cx);
        self.refresh_preview(cx);
        cx.notify();
    }

    fn open_document(&mut self, _: &OpenDocument, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open Markdown File".into()),
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };

            this.update_in(cx, |this, window, cx| {
                this.open_path(path, window, cx);
            })
            .ok();
        })
        .detach();
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if self.document.path.as_ref().is_none() {
            self.save_as(&SaveAs, window, cx);
            return;
        }

        self.save_to_current_path(cx);
    }

    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        let suggested_name = self.document.title();
        let directory = self
            .document
            .path
            .as_ref()
            .and_then(|path| path.parent())
            .map(PathBuf::from)
            .or_else(|| env::current_dir().ok())
            .unwrap_or_default();
        let path = cx.prompt_for_new_path(&directory, Some(&suggested_name));

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = path.await else {
                return;
            };

            this.update(cx, |this, cx| {
                if !is_markdown_path(&path) {
                    eprintln!("expected a .md or .markdown file, got {}", path.display());
                    return;
                }

                this.document.path = Some(path);
                this.save_to_current_path(cx);
            })
            .ok();
        })
        .detach();
    }

    fn save_to_current_path(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.document.path.clone() else {
            return;
        };

        let contents = self.document.editor.read(cx).text(cx);
        if let Err(error) = fs::write(&path, contents) {
            eprintln!("failed to save markdown file {}: {error:#}", path.display());
            return;
        }

        let editor = self.document.editor.clone();
        if let Some(buffer) = singleton_buffer(&editor, cx) {
            let version = buffer.read(cx).text_snapshot().version().clone();
            buffer.update(cx, |buffer, cx| buffer.did_save(version, None, cx));
        }
        self.refresh_status(cx);
        self.refresh_preview(cx);
    }

    fn toggle_preview(&mut self, _: &TogglePreview, _window: &mut Window, cx: &mut Context<Self>) {
        self.show_preview = !self.show_preview;
        self.refresh_preview(cx);
        cx.notify();
    }

    fn toggle_command_palette(
        &mut self,
        _: &ToggleCommandPalette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_command_palette = !self.show_command_palette;
        cx.notify();
    }

    fn refresh_status(&mut self, cx: &mut Context<Self>) {
        self.document.status = document_status(&self.document, cx);
        cx.notify();
    }

    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        if !self.show_preview {
            return;
        }
        let text = self.document.editor.read(cx).text(cx);
        self.preview.update(cx, |preview, cx| {
            preview.reset(SharedString::from(text), cx);
        });
    }

    fn focus_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.document.editor.focus_handle(cx), cx);
    }

    fn render_title_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self.document.title();
        let path = self
            .document
            .path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Unsaved Markdown document".to_string());
        let status = self.document.status.label();

        h_flex()
            .h(platform_title_bar_height(window))
            .w_full()
            .px_2()
            .items_center()
            .justify_between()
            .bg(cx.theme().colors().title_bar_background)
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        Button::new("markdown-editor-title", "Markdown Editor")
                            .label_size(LabelSize::Small)
                            .style(ButtonStyle::Subtle),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(path),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(status),
                    )
                    .child(action_button("new-document", "New", NewDocument))
                    .child(action_button("open-document", "Open", OpenDocument))
                    .child(action_button("save-document", "Save", Save))
                    .child(action_button("toggle-preview", "Preview", TogglePreview))
                    .child(action_button(
                        "toggle-command-palette",
                        "Commands",
                        ToggleCommandPalette,
                    )),
            )
    }

    fn render_preview(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .overflow_y_hidden()
            .p_3()
            .border_l_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(cx.theme().colors().editor_background)
            .child(MarkdownElement::new(
                self.preview.clone(),
                MarkdownStyle::themed(MarkdownFont::Preview, window, cx),
            ))
    }

    fn render_command_palette(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .absolute()
            .top_12()
            .left_1_2()
            .w(rems(30.))
            .ml(rems(-15.))
            .p_2()
            .rounded_lg()
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().elevated_surface_background)
            .shadow_lg()
            .child(
                div()
                    .px_2()
                    .pb_2()
                    .text_xs()
                    .text_color(cx.theme().colors().text_muted)
                    .child("Command Palette"),
            )
            .child(command_row("New Markdown Document", NewDocument))
            .child(command_row("Open Markdown File...", OpenDocument))
            .child(command_row("Save", Save))
            .child(command_row("Save As...", SaveAs))
            .child(command_row("Toggle Markdown Preview", TogglePreview))
    }
}

impl Document {
    fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|file_name| file_name.to_str())
            .map(ToString::to_string)
            .or_else(|| self.title.clone())
            .unwrap_or_else(|| "Untitled".to_string())
    }
}

impl Render for MarkdownEditorShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(cx.theme().colors().editor_background)
            .on_action(
                cx.listener(|this, _: &NewDocument, window, cx| this.new_document(window, cx)),
            )
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::toggle_preview))
            .on_action(cx.listener(Self::toggle_command_palette))
            .child(self.render_title_bar(window, cx))
            .child(
                h_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex_1()
                            .size_full()
                            .child(self.document.editor.clone()),
                    )
                    .when(self.show_preview, |this| {
                        this.child(
                            div()
                                .w_1_2()
                                .h_full()
                                .child(self.render_preview(window, cx)),
                        )
                    }),
            )
            .when(self.show_command_palette, |this| {
                this.child(self.render_command_palette(cx))
            })
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
            cx.bind_keys([
                KeyBinding::new("ctrl-n", NewDocument, Some("Editor")),
                KeyBinding::new("ctrl-o", OpenDocument, Some("Editor")),
                KeyBinding::new("ctrl-s", Save, Some("Editor")),
                KeyBinding::new("ctrl-shift-s", SaveAs, Some("Editor")),
                KeyBinding::new("ctrl-shift-p", ToggleCommandPalette, None),
                KeyBinding::new("ctrl-shift-v", TogglePreview, Some("Editor")),
            ]);

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

fn action_button(label_id: &'static str, label: &'static str, action: impl gpui::Action) -> Button {
    Button::new(label_id, label)
        .size(ButtonSize::Compact)
        .style(ButtonStyle::Subtle)
        .on_click(move |_event, window, cx| window.dispatch_action(action.boxed_clone(), cx))
}

fn command_row(label: &'static str, action: impl gpui::Action) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .rounded_md()
        .text_xs()
        .hover(|style| style.bg(gpui::transparent_black()))
        .child(label)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(action.boxed_clone(), cx)
        })
}

fn document_status(document: &Document, cx: &App) -> DocumentStatus {
    if document.editor.read(cx).buffer().read(cx).is_dirty(cx) {
        DocumentStatus::Unsaved
    } else {
        DocumentStatus::Saved
    }
}

fn singleton_buffer(editor: &Entity<Editor>, cx: &App) -> Option<Entity<Buffer>> {
    editor.read(cx).buffer().read(cx).as_singleton()
}

fn read_markdown_file(path: &PathBuf) -> Result<String> {
    anyhow::ensure!(
        is_markdown_path(path),
        "expected a .md or .markdown file, got {}",
        path.display()
    );

    fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))
}

fn is_markdown_path(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "md" | "markdown"))
}
