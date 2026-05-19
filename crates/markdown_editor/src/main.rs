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
use ui::{prelude::*, Button, ButtonSize, ButtonStyle, Tab, TabBar, TabPosition};

actions!(
    markdown_editor,
    [
        NewDocument,
        OpenDocument,
        Save,
        SaveAs,
        CloseDocument,
        TogglePreview,
        ToggleCommandPalette,
        ActivatePreviousDocument,
        ActivateNextDocument
    ]
);

struct MarkdownEditorShell {
    documents: Vec<Document>,
    active_document: usize,
    next_untitled_id: usize,
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

    fn marker(self) -> &'static str {
        match self {
            Self::Saved => "",
            Self::Unsaved => "*",
        }
    }
}

impl MarkdownEditorShell {
    fn new(path: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let preview = cx.new(|cx| Markdown::new(SharedString::default(), None, None, cx));
        let mut this = Self {
            documents: Vec::new(),
            active_document: 0,
            next_untitled_id: 1,
            show_preview: false,
            show_command_palette: false,
            preview,
        };

        if let Some(path) = path {
            this.open_path(path, window, cx);
        }

        if this.documents.is_empty() {
            this.new_document(window, cx);
        }

        this.focus_active_editor(window, cx);
        this.refresh_preview(cx);
        this
    }

    fn new_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let title = format!("Untitled {}", self.next_untitled_id);
        self.next_untitled_id += 1;
        self.add_document(None, String::new(), Some(title), window, cx);
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match read_markdown_file(&path) {
            Ok(contents) => self.add_document(Some(path), contents, None, window, cx),
            Err(error) => eprintln!("failed to open markdown file: {error:#}"),
        }
    }

    fn add_document(
        &mut self,
        path: Option<PathBuf>,
        contents: String,
        title: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editor = cx.new(|cx| {
            let buffer = cx.new(|cx| Buffer::local(contents, cx));
            Editor::for_buffer(buffer, None, window, cx)
        });
        let editor_subscription = cx.subscribe(&editor, |this, editor, event, cx| match event {
            EditorEvent::DirtyChanged | EditorEvent::Saved | EditorEvent::BufferEdited => {
                this.refresh_document_status(&editor, cx);
                this.refresh_preview(cx);
            }
            _ => {}
        });

        self.documents.push(Document {
            editor,
            path,
            title,
            status: DocumentStatus::Saved,
            _editor_subscription: editor_subscription,
        });
        self.active_document = self.documents.len() - 1;
        self.refresh_active_status(cx);
        self.refresh_preview(cx);
        self.focus_active_editor(window, cx);
        cx.notify();
    }

    fn open_document(&mut self, _: &OpenDocument, window: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Open Markdown Files".into()),
        });

        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };

            this.update_in(cx, |this, window, cx| {
                for path in paths {
                    this.open_path(path, window, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    fn save(&mut self, _: &Save, window: &mut Window, cx: &mut Context<Self>) {
        if self.active_document().and_then(|document| document.path.as_ref()).is_none() {
            self.save_as(&SaveAs, window, cx);
            return;
        }

        self.save_active_to_current_path(cx);
    }

    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        let suggested_name = self
            .active_document()
            .map(Document::title)
            .unwrap_or_else(|| "untitled.md".to_string());
        let directory = self
            .active_document()
            .and_then(|document| document.path.as_ref())
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

                if let Some(document) = this.active_document_mut() {
                    document.path = Some(path);
                }
                this.save_active_to_current_path(cx);
            })
            .ok();
        })
        .detach();
    }

    fn save_active_to_current_path(&mut self, cx: &mut Context<Self>) {
        let Some(document) = self.active_document() else {
            return;
        };
        let Some(path) = document.path.clone() else {
            return;
        };

        let contents = document.editor.read(cx).text(cx);
        if let Err(error) = fs::write(&path, contents) {
            eprintln!("failed to save markdown file {}: {error:#}", path.display());
            return;
        }

        let editor = document.editor.clone();
        if let Some(buffer) = singleton_buffer(&editor, cx) {
            let version = buffer.read(cx).text_snapshot().version().clone();
            buffer.update(cx, |buffer, cx| buffer.did_save(version, None, cx));
        }
        self.refresh_active_status(cx);
        self.refresh_preview(cx);
    }

    fn close_document(&mut self, _: &CloseDocument, window: &mut Window, cx: &mut Context<Self>) {
        if self.documents.len() <= 1 {
            self.documents.clear();
            self.new_document(window, cx);
            return;
        }

        self.documents.remove(self.active_document);
        if self.active_document >= self.documents.len() {
            self.active_document = self.documents.len() - 1;
        }
        self.focus_active_editor(window, cx);
        self.refresh_preview(cx);
        cx.notify();
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

    fn activate_previous_document(
        &mut self,
        _: &ActivatePreviousDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.documents.is_empty() {
            return;
        }
        self.active_document = if self.active_document == 0 {
            self.documents.len() - 1
        } else {
            self.active_document - 1
        };
        self.focus_active_editor(window, cx);
        self.refresh_preview(cx);
        cx.notify();
    }

    fn activate_next_document(
        &mut self,
        _: &ActivateNextDocument,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.documents.is_empty() {
            return;
        }
        self.active_document = (self.active_document + 1) % self.documents.len();
        self.focus_active_editor(window, cx);
        self.refresh_preview(cx);
        cx.notify();
    }

    fn activate_document(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.documents.len() {
            return;
        }
        self.active_document = index;
        self.focus_active_editor(window, cx);
        self.refresh_preview(cx);
        cx.notify();
    }

    fn close_document_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index >= self.documents.len() {
            return;
        }

        self.active_document = index;
        self.close_document(&CloseDocument, window, cx);
    }

    fn refresh_document_status(&mut self, editor: &Entity<Editor>, cx: &mut Context<Self>) {
        if let Some(document) = self
            .documents
            .iter_mut()
            .find(|document| document.editor == *editor)
        {
            document.status = document_status(document, cx);
        }
        cx.notify();
    }

    fn refresh_active_status(&mut self, cx: &mut Context<Self>) {
        if let Some(document) = self.active_document_mut() {
            document.status = document_status(document, cx);
        }
        cx.notify();
    }

    fn refresh_preview(&mut self, cx: &mut Context<Self>) {
        if !self.show_preview {
            return;
        }
        let text = self
            .active_document()
            .map(|document| document.editor.read(cx).text(cx))
            .unwrap_or_default();
        self.preview.update(cx, |preview, cx| {
            preview.reset(SharedString::from(text), cx);
        });
    }

    fn focus_active_editor(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(document) = self.active_document() {
            window.focus(&document.editor.focus_handle(cx), cx);
        }
    }

    fn active_document(&self) -> Option<&Document> {
        self.documents.get(self.active_document)
    }

    fn active_document_mut(&mut self) -> Option<&mut Document> {
        self.documents.get_mut(self.active_document)
    }

    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .h_10()
            .px_2()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().tab_bar_background)
            .child(action_button("new-document", "New", NewDocument))
            .child(action_button("open-document", "Open", OpenDocument))
            .child(action_button("save-document", "Save", Save))
            .child(action_button("save-as-document", "Save As", SaveAs))
            .child(action_button("toggle-preview", "Preview", TogglePreview))
            .child(action_button(
                "toggle-command-palette",
                "Commands",
                ToggleCommandPalette,
            ))
    }

    fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_document;
        let shell = cx.entity();
        TabBar::new("markdown-tabs").children(self.documents.iter().enumerate().map(
            move |(index, document)| {
                let shell_for_tab = shell.clone();
                let shell_for_close = shell.clone();
                Tab::new(format!("markdown-tab-{index}"))
                    .position(tab_position(index, active, self.documents.len()))
                    .toggle_state(index == active)
                    .child(format!("{}{}", document.status.marker(), document.title()))
                    .end_slot(
                        Button::new(format!("close-markdown-tab-{index}"), "x")
                            .size(ButtonSize::Compact)
                            .style(ButtonStyle::Subtle)
                            .on_click(move |_event, window, cx| {
                                cx.stop_propagation();
                                shell_for_close.update(cx, |shell, cx| {
                                    shell.close_document_at(index, window, cx)
                                });
                            }),
                    )
                    .on_click(move |_event, window, cx| {
                        shell_for_tab.update(cx, |shell, cx| {
                            shell.activate_document(index, window, cx)
                        });
                    })
            },
        ))
    }

    fn render_document_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let title = self
            .active_document()
            .map(Document::title)
            .unwrap_or_else(|| "Untitled".to_string());
        let path = self
            .active_document()
            .map(Document::path_label)
            .unwrap_or_else(|| "Unsaved Markdown document".to_string());
        let status = self
            .active_document()
            .map(|document| document.status.label())
            .unwrap_or("Saved");

        h_flex()
            .h_10()
            .px_4()
            .justify_between()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                v_flex()
                    .child(div().text_sm().child(title))
                    .child(div().text_xs().text_color(cx.theme().colors().text_muted).child(path)),
            )
            .child(div().text_xs().text_color(cx.theme().colors().text_muted).child(status))
    }

    fn render_preview(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .overflow_y_hidden()
            .p_4()
            .border_l_1()
            .border_color(cx.theme().colors().border)
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
            .w(rems(34.))
            .p_2()
            .border_1()
            .border_color(cx.theme().colors().border)
            .bg(cx.theme().colors().elevated_surface_background)
            .shadow_lg()
            .child(div().text_sm().mb_2().child("Command Palette"))
            .child(command_row("New Markdown Document", NewDocument))
            .child(command_row("Open Markdown File...", OpenDocument))
            .child(command_row("Save", Save))
            .child(command_row("Save As...", SaveAs))
            .child(command_row("Toggle Markdown Preview", TogglePreview))
            .child(command_row("Close Document", CloseDocument))
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

    fn path_label(&self) -> String {
        self.path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "Unsaved Markdown document".to_string())
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
            .on_action(cx.listener(|this, _: &NewDocument, window, cx| {
                this.new_document(window, cx)
            }))
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::close_document))
            .on_action(cx.listener(Self::toggle_preview))
            .on_action(cx.listener(Self::toggle_command_palette))
            .on_action(cx.listener(Self::activate_previous_document))
            .on_action(cx.listener(Self::activate_next_document))
            .child(self.render_toolbar(cx))
            .child(self.render_tabs(cx))
            .child(self.render_document_bar(cx))
            .child(
                h_flex()
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex_1()
                            .size_full()
                            .child(self.active_document().unwrap().editor.clone()),
                    )
                    .when(self.show_preview, |this| {
                        this.child(div().w_1_2().h_full().child(self.render_preview(window, cx)))
                    }),
            )
            .when(self.show_command_palette, |this| this.child(self.render_command_palette(cx)))
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
                KeyBinding::new("ctrl-w", CloseDocument, Some("Editor")),
                KeyBinding::new("ctrl-shift-p", ToggleCommandPalette, None),
                KeyBinding::new("ctrl-shift-v", TogglePreview, Some("Editor")),
                KeyBinding::new("ctrl-pageup", ActivatePreviousDocument, Some("Editor")),
                KeyBinding::new("ctrl-pagedown", ActivateNextDocument, Some("Editor")),
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
        .hover(|style| style.bg(gpui::transparent_black()))
        .child(label)
        .on_mouse_down(MouseButton::Left, move |_event, window, cx| {
            window.dispatch_action(action.boxed_clone(), cx)
        })
}

fn tab_position(index: usize, active: usize, len: usize) -> TabPosition {
    if index == 0 {
        TabPosition::First
    } else if index + 1 == len {
        TabPosition::Last
    } else {
        TabPosition::Middle(index.cmp(&active))
    }
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
