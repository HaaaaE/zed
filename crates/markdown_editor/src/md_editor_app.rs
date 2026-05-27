use std::{env, fs, path::PathBuf, sync::Arc};

use gpui::{
    App, Context, Entity, Focusable as _, IntoElement, KeyBinding, PathPromptOptions, Render,
    SharedString, Subscription, Window, WindowOptions, div, prelude::*,
};
use md_editor::{MarkdownEditor, MarkdownEditorEvent, MarkdownEditorMode, init_standalone};
use md_theme::{shell_palette, title_bar_height};

use crate::lightweight_http_client::MarkdownHttpClient;

gpui::actions!(
    markdown_editor,
    [NewDocument, OpenDocument, Save, SaveAs, ToggleMode]
);

struct MarkdownEditorShell {
    editor: Entity<MarkdownEditor>,
    path: Option<PathBuf>,
    is_dirty: bool,
    error_message: Option<String>,
    _editor_subscription: Subscription,
}

impl MarkdownEditorShell {
    fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mode = MarkdownEditorMode::Source;
        let (path, contents, error_message) = match path {
            Some(path) => read_markdown_file(&path)
                .map(|contents| (Some(path), contents, None))
                .unwrap_or_else(|message| {
                    eprintln!("{message}");
                    (None, String::new(), Some(message))
                }),
            None => (None, String::new(), None),
        };
        let (editor, editor_subscription) = Self::build_editor(contents, path.clone(), mode, cx);
        window.focus(&editor.focus_handle(cx));

        Self {
            editor,
            path,
            is_dirty: false,
            error_message,
            _editor_subscription: editor_subscription,
        }
    }

    fn build_editor(
        contents: String,
        path: Option<PathBuf>,
        mode: MarkdownEditorMode,
        cx: &mut Context<Self>,
    ) -> (Entity<MarkdownEditor>, Subscription) {
        let editor = cx.new(|cx| {
            let mut editor = MarkdownEditor::for_text_with_document_path(contents, path, cx);
            editor.set_mode(mode, cx);
            editor
        });
        let editor_subscription = cx.subscribe(
            &editor,
            |this, _, event: &MarkdownEditorEvent, cx| match event {
                MarkdownEditorEvent::DirtyChanged(is_dirty) => {
                    this.is_dirty = *is_dirty;
                    cx.notify();
                }
            },
        );
        (editor, editor_subscription)
    }

    fn replace_document(
        &mut self,
        path: Option<PathBuf>,
        contents: String,
        error_message: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mode = self.mode(cx);
        let (editor, editor_subscription) = Self::build_editor(contents, path.clone(), mode, cx);
        self.editor = editor;
        self._editor_subscription = editor_subscription;
        self.path = path;
        self.is_dirty = false;
        self.error_message = error_message;
        window.focus(&self.editor.focus_handle(cx));
        cx.notify();
    }

    fn new_document(&mut self, _: &NewDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_document(None, String::new(), None, window, cx);
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match read_markdown_file(&path) {
            Ok(contents) => {
                self.replace_document(Some(path), contents, None, window, cx);
            }
            Err(message) => {
                eprintln!("{message}");
                self.error_message = Some(message);
                cx.notify();
            }
        }
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

    fn title(&self) -> String {
        self.path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|file_name| file_name.to_str())
            .map(ToString::to_string)
            .unwrap_or_else(|| "Untitled.md".to_string())
    }

    fn path_label(&self) -> String {
        self.path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Unsaved Markdown document".to_string())
    }

    fn status_label(&self) -> &'static str {
        if self.is_dirty { "Unsaved" } else { "Saved" }
    }

    fn mode(&self, cx: &Context<Self>) -> MarkdownEditorMode {
        self.editor.read(cx).mode()
    }

    fn toggle_mode(&mut self, _: &ToggleMode, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| editor.toggle_mode(cx));
        cx.notify();
    }

    fn set_mode(&mut self, mode: MarkdownEditorMode, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.set_mode(mode, cx));
        cx.notify();
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if self.path.is_none() {
            self.save_as(&SaveAs, window, cx);
            return;
        }

        self.save_to_current_path(cx);
    }

    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        let suggested_name = self
            .path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|file_name| file_name.to_str())
            .map(ToString::to_string)
            .unwrap_or_else(|| "untitled.md".to_string());
        let directory = self
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
                    let message =
                        format!("expected a .md or .markdown file, got {}", path.display());
                    eprintln!("{message}");
                    this.error_message = Some(message);
                    cx.notify();
                    return;
                }

                this.path = Some(path.clone());
                this.editor.update(cx, |editor, cx| {
                    editor.set_document_path(Some(path.clone()), cx)
                });
                this.save_to_current_path(cx);
            })
            .ok();
        })
        .detach();
    }

    fn save_to_current_path(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else {
            return;
        };

        let contents = self.editor.read(cx).serialized_text();
        if let Err(error) = fs::write(&path, contents) {
            let message = format!("failed to save markdown file {}: {error:#}", path.display());
            eprintln!("{message}");
            self.error_message = Some(message);
            cx.notify();
            return;
        }

        self.editor.update(cx, |editor, cx| {
            editor.set_document_path(Some(path.clone()), cx);
            editor.mark_saved(cx);
        });
        self.error_message = None;
        self.is_dirty = false;
        cx.notify();
    }

    fn render_title_bar(&self, cx: &Context<Self>) -> impl IntoElement {
        let mode = self.mode(cx);
        let palette = shell_palette();
        div()
            .h(title_bar_height())
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px_3()
            .bg(palette.title_bar_background)
            .border_b_1()
            .border_color(palette.title_bar_border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(palette.title_text)
                            .child("Markdown Editor"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.secondary_text)
                            .child(SharedString::from(self.path_label())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .text_xs()
                            .text_color(if self.is_dirty {
                                palette.dirty_text
                            } else {
                                palette.secondary_text
                            })
                            .child(SharedString::from(self.status_label())),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(palette.secondary_text)
                            .child(SharedString::from(format!("{} · Ctrl/Cmd+S", self.title()))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .border_1()
                            .border_color(palette.title_bar_border)
                            .rounded_md()
                            .overflow_hidden()
                            .child(mode_button(
                                "Source",
                                mode == MarkdownEditorMode::Source,
                                palette,
                                MarkdownEditorMode::Source,
                                cx,
                            ))
                            .child(mode_button(
                                "Rendered",
                                mode == MarkdownEditorMode::Rendered,
                                palette,
                                MarkdownEditorMode::Rendered,
                                cx,
                            )),
                    ),
            )
    }
}

fn mode_button(
    label: &'static str,
    is_active: bool,
    palette: md_theme::ShellPalette,
    mode: MarkdownEditorMode,
    cx: &Context<MarkdownEditorShell>,
) -> impl IntoElement {
    div()
        .id(match mode {
            MarkdownEditorMode::Source => "markdown-mode-source-button",
            MarkdownEditorMode::Rendered => "markdown-mode-rendered-button",
        })
        .px_2()
        .py_1()
        .text_xs()
        .cursor_pointer()
        .bg(if is_active {
            gpui::rgba(0xffffff1c)
        } else {
            gpui::rgba(0xffffff00)
        })
        .text_color(if is_active {
            palette.title_text
        } else {
            palette.secondary_text
        })
        .child(label)
        .on_click(cx.listener(move |this, _, _window, cx| {
            this.set_mode(mode, cx);
        }))
}

impl Render for MarkdownEditorShell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = shell_palette();
        div()
            .size_full()
            .flex()
            .flex_col()
            .on_action(cx.listener(Self::new_document))
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::toggle_mode))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .bg(palette.window_background)
            .child(self.render_title_bar(cx))
            .when_some(self.error_message.clone(), |this, error_message| {
                this.child(
                    div()
                        .px_3()
                        .py_2()
                        .bg(palette.error_background)
                        .text_color(palette.error_text)
                        .text_xs()
                        .child(SharedString::from(error_message)),
                )
            })
            .child(div().flex_1().overflow_hidden().child(self.editor.clone()))
    }
}

/// Construct app-level keybindings from the single-source-of-truth in `md_settings`.
///
/// This reads `md_settings::DEFAULT_APP_KEYBINDINGS` and dispatches on
/// the action name to produce typed `gpui::KeyBinding` values.
fn app_keybindings() -> Vec<KeyBinding> {
    md_settings::DEFAULT_APP_KEYBINDINGS
        .iter()
        .map(|spec| {
            let context = Some(spec.context);
            match spec.action {
                "NewDocument" => KeyBinding::new(spec.keystroke, NewDocument, context),
                "OpenDocument" => KeyBinding::new(spec.keystroke, OpenDocument, context),
                "ToggleMode" => KeyBinding::new(spec.keystroke, ToggleMode, context),
                "Save" => KeyBinding::new(spec.keystroke, Save, context),
                "SaveAs" => KeyBinding::new(spec.keystroke, SaveAs, context),
                _ => panic!(
                    "unknown app action in DEFAULT_APP_KEYBINDINGS: {}",
                    spec.action
                ),
            }
        })
        .collect()
}

pub fn run() {
    let path = env::args_os().nth(1).map(PathBuf::from);

    gpui::Application::new().run(move |cx| {
        init_standalone(cx);
        configure_http_client(cx);
        cx.bind_keys(app_keybindings());

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

fn configure_http_client(cx: &mut App) {
    const USER_AGENT: &str = concat!("markdown-editor/", env!("CARGO_PKG_VERSION"));

    match MarkdownHttpClient::new(USER_AGENT) {
        Ok(client) => cx.set_http_client(Arc::new(client)),
        Err(error) => eprintln!("failed to initialize HTTP client: {error:#}"),
    }
}

fn is_markdown_path(path: &PathBuf) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension, "md" | "markdown"))
}

fn read_markdown_file(path: &PathBuf) -> Result<String, String> {
    if !is_markdown_path(path) {
        return Err(format!(
            "expected a .md or .markdown file, got {}",
            path.display()
        ));
    }

    fs::read_to_string(path)
        .map_err(|error| format!("failed to open markdown file {}: {error:#}", path.display()))
}
