#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{env, fs, ops::Range, path::PathBuf};

use anyhow::{Context as _, Result};
use assets::Assets;
use clock::Global;
use editor::{
    Anchor, Editor, EditorEvent, RowHeightOverride,
    display_map::{DisplayRow, HighlightKey},
};
use gpui::{
    App, Context, Entity, Focusable as _, FontStyle, FontWeight, HighlightStyle, KeyBinding,
    MouseButton, PathPromptOptions, SharedString, StrikethroughStyle, Subscription, UnderlineStyle,
    Window, WindowOptions, actions, div, px,
};
use language::Buffer;
use markdown::{Markdown, MarkdownElement, MarkdownFont, MarkdownStyle};
use markdown_wysiwyg::{MarkdownBlockKind, MarkdownInlineKind, MarkdownSyntaxTree};
use multi_buffer::MultiBufferOffset;
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
        ToggleMode,
        ToggleCommandPalette
    ]
);

struct MarkdownEditorShell {
    document: Document,
    show_preview: bool,
    show_command_palette: bool,
    mode: MarkdownEditMode,
    preview: Entity<Markdown>,
    wysiwyg: MarkdownWysiwygController,
}

struct Document {
    editor: Entity<Editor>,
    path: Option<PathBuf>,
    title: Option<String>,
    status: DocumentStatus,
    _editor_subscription: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MarkdownEditMode {
    Source,
    Rendered,
}

impl MarkdownEditMode {
    fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Rendered => "Rendered",
        }
    }

    fn toggle(self) -> Self {
        match self {
            Self::Source => Self::Rendered,
            Self::Rendered => Self::Source,
        }
    }
}

#[derive(Default)]
struct RenderedRevealState {
    hover: Option<RevealTarget>,
    caret: Option<RevealTarget>,
    drag_frozen: bool,
}

impl RenderedRevealState {
    fn active_source_range(&self) -> Option<Range<usize>> {
        self.hover
            .as_ref()
            .or(self.caret.as_ref())
            .map(RevealTarget::source_range)
    }
}

#[derive(Clone)]
enum RevealTarget {
    Heading {
        row: u32,
        source_range: Range<usize>,
        content_range: Range<usize>,
    },
    Inline {
        source_range: Range<usize>,
        content_ranges: Vec<Range<usize>>,
    },
    Block {
        source_range: Range<usize>,
    },
}

impl RevealTarget {
    fn source_range(&self) -> Range<usize> {
        match self {
            RevealTarget::Heading { source_range, .. }
            | RevealTarget::Inline { source_range, .. }
            | RevealTarget::Block { source_range } => source_range.clone(),
        }
    }

    fn content_row(&self) -> Option<u32> {
        match self {
            RevealTarget::Heading { row, .. } => Some(*row),
            _ => None,
        }
    }

    fn content_ranges_len(&self) -> usize {
        match self {
            RevealTarget::Heading { content_range, .. } => content_range.len(),
            RevealTarget::Inline { content_ranges, .. } => content_ranges.len(),
            RevealTarget::Block { .. } => 0,
        }
    }
}

#[derive(Default)]
struct MarkdownWysiwygController {
    parse_tree: Option<MarkdownSyntaxTree>,
    parsed_version: Option<Global>,
    reveal_state: RenderedRevealState,
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
            mode: MarkdownEditMode::Source,
            preview,
            wysiwyg: MarkdownWysiwygController::default(),
        };

        this.sync_wysiwyg(cx);
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
                this.sync_wysiwyg(cx);
            }
            EditorEvent::SelectionsChanged { .. } => {
                let selection = this
                    .document
                    .editor
                    .update(cx, |editor, cx| editor.newest_selection_point_range(cx));
                if selection.start == selection.end {
                    this.sync_wysiwyg(cx);
                }
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
        self.wysiwyg = MarkdownWysiwygController::default();
        self.sync_wysiwyg(cx);
        self.focus_editor(window, cx);
        self.refresh_preview(cx);
        cx.notify();
    }

    fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.document = Self::build_document(Some(path), None, window, cx);
        self.wysiwyg = MarkdownWysiwygController::default();
        self.sync_wysiwyg(cx);
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

    fn toggle_mode(&mut self, _: &ToggleMode, _window: &mut Window, cx: &mut Context<Self>) {
        self.mode = self.mode.toggle();
        self.sync_wysiwyg(cx);
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

    fn sync_wysiwyg(&mut self, cx: &mut Context<Self>) {
        self.wysiwyg.sync(&self.document.editor, self.mode, cx);
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
                    .child(action_button("toggle-mode", self.mode.label(), ToggleMode))
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
            .child(command_row("Toggle Source/Rendered Mode", ToggleMode))
            .child(command_row("Toggle Markdown Preview", TogglePreview))
    }
}

impl MarkdownWysiwygController {
    fn sync(
        &mut self,
        editor: &Entity<Editor>,
        mode: MarkdownEditMode,
        cx: &mut Context<MarkdownEditorShell>,
    ) {
        let Some(buffer) = singleton_buffer(editor, cx) else {
            return;
        };
        let snapshot = buffer.read(cx).text_snapshot();
        let text_version = snapshot.version().clone();
        if self.parsed_version.as_ref() != Some(&text_version) {
            let text = snapshot.as_rope().to_string();
            self.parse_tree = Some(MarkdownSyntaxTree::parse(&text));
            self.parsed_version = Some(text_version);
        }
        let Some(parse_tree) = self.parse_tree.as_ref() else {
            return;
        };

        let selection = editor.update(cx, |editor, cx| editor.newest_selection_point_range(cx));
        self.reveal_state.caret = markdown_caret_reveal_target(parse_tree, mode, selection.start.row as u32);
        self.reveal_state.drag_frozen = selection.start != selection.end;
        let _ = self
            .reveal_state
            .active_source_range()
            .zip(self.reveal_state.caret.as_ref().and_then(RevealTarget::content_row));
        let _ = self
            .reveal_state
            .caret
            .as_ref()
            .map(RevealTarget::content_ranges_len);

        let highlights = markdown_highlight_ranges(editor, parse_tree, cx);
        let row_height_overrides = markdown_row_height_overrides(parse_tree, mode);
        editor.update(cx, |editor, cx| {
            apply_markdown_highlights(editor, highlights, mode, cx);
            if row_height_overrides.is_empty() {
                editor.clear_row_height_overrides(cx);
            } else {
                editor.set_row_height_overrides(row_height_overrides, cx);
            }
        });
    }
}

fn markdown_caret_reveal_target(
    tree: &MarkdownSyntaxTree,
    mode: MarkdownEditMode,
    row: u32,
) -> Option<RevealTarget> {
    if mode == MarkdownEditMode::Source {
        return None;
    }

    tree.inline_spans()
        .iter()
        .find(|span| span.source_range.start < span.source_range.end)
        .map(|span| RevealTarget::Inline {
            source_range: span.source_range.clone(),
            content_ranges: span.content_ranges.clone(),
        })
        .or_else(|| {
            tree.blocks().iter().find_map(|block| {
                if !block.row_range.contains(&(row as usize)) {
                    return None;
                }
                match block.kind {
                    MarkdownBlockKind::AtxHeading { .. } => Some(RevealTarget::Heading {
                        row,
                        source_range: block.source_range.clone(),
                        content_range: block.content_range.clone(),
                    }),
                    MarkdownBlockKind::FencedCodeBlock => Some(RevealTarget::Block {
                        source_range: block.source_range.clone(),
                    }),
                    _ => None,
                }
            })
        })
}

fn markdown_row_height_overrides(
    tree: &MarkdownSyntaxTree,
    mode: MarkdownEditMode,
) -> Vec<RowHeightOverride> {
    if mode == MarkdownEditMode::Source {
        return Vec::new();
    }

    tree.blocks()
        .iter()
        .filter_map(|block| {
            let MarkdownBlockKind::AtxHeading { level } = block.kind else {
                return None;
            };
            let height = match level {
                1 => px(42.),
                2 => px(34.),
                3 => px(28.),
                _ => px(24.),
            };
            Some(RowHeightOverride {
                row: DisplayRow(block.row_range.start as u32),
                height,
            })
        })
        .collect()
}

#[derive(Default)]
struct MarkdownHighlightRanges {
    marker: Vec<Range<Anchor>>,
    heading_1: Vec<Range<Anchor>>,
    heading_2: Vec<Range<Anchor>>,
    heading_3: Vec<Range<Anchor>>,
    heading_other: Vec<Range<Anchor>>,
    strong: Vec<Range<Anchor>>,
    emphasis: Vec<Range<Anchor>>,
    inline_code: Vec<Range<Anchor>>,
    link: Vec<Range<Anchor>>,
    strikethrough: Vec<Range<Anchor>>,
}

fn markdown_highlight_ranges(
    editor: &Entity<Editor>,
    tree: &MarkdownSyntaxTree,
    cx: &mut App,
) -> MarkdownHighlightRanges {
    let snapshot = editor.read(cx).buffer().read(cx).snapshot(cx);
    let mut ranges = MarkdownHighlightRanges::default();

    for block in tree.blocks() {
        if let MarkdownBlockKind::AtxHeading { level } = block.kind {
            for marker_range in &block.marker_ranges {
                push_byte_range(&snapshot, marker_range.clone(), &mut ranges.marker);
            }
            push_byte_range(
                &snapshot,
                block.content_range.clone(),
                match level {
                    1 => &mut ranges.heading_1,
                    2 => &mut ranges.heading_2,
                    3 => &mut ranges.heading_3,
                    _ => &mut ranges.heading_other,
                },
            );
        }
    }

    for span in tree.inline_spans() {
        let target = match span.kind {
            MarkdownInlineKind::Strong => &mut ranges.strong,
            MarkdownInlineKind::Emphasis => &mut ranges.emphasis,
            MarkdownInlineKind::InlineCode => &mut ranges.inline_code,
            MarkdownInlineKind::Link => &mut ranges.link,
            MarkdownInlineKind::Strikethrough => &mut ranges.strikethrough,
        };
        for content_range in &span.content_ranges {
            push_byte_range(&snapshot, content_range.clone(), target);
        }
        for marker_range in &span.marker_ranges {
            push_byte_range(&snapshot, marker_range.clone(), &mut ranges.marker);
        }
    }

    ranges
}

fn push_byte_range(
    snapshot: &multi_buffer::MultiBufferSnapshot,
    range: Range<usize>,
    target: &mut Vec<Range<Anchor>>,
) {
    if range.start < range.end {
        target.push(
            snapshot.anchor_before(MultiBufferOffset(range.start))
                ..snapshot.anchor_after(MultiBufferOffset(range.end)),
        );
    }
}

fn apply_markdown_highlights(
    editor: &mut Editor,
    ranges: MarkdownHighlightRanges,
    mode: MarkdownEditMode,
    cx: &mut Context<Editor>,
) {
    set_markdown_highlight(
        editor,
        markdown_marker_highlight_key(),
        ranges.marker,
        markdown_marker_highlight_style(mode, cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_heading_1_highlight_key(),
        ranges.heading_1,
        markdown_heading_highlight_style(1, mode, cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_heading_2_highlight_key(),
        ranges.heading_2,
        markdown_heading_highlight_style(2, mode, cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_heading_3_highlight_key(),
        ranges.heading_3,
        markdown_heading_highlight_style(3, mode, cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_heading_other_highlight_key(),
        ranges.heading_other,
        markdown_heading_highlight_style(4, mode, cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_strong_highlight_key(),
        ranges.strong,
        markdown_strong_highlight_style(cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_emphasis_highlight_key(),
        ranges.emphasis,
        markdown_emphasis_highlight_style(cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_inline_code_highlight_key(),
        ranges.inline_code,
        markdown_inline_code_highlight_style(cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_link_highlight_key(),
        ranges.link,
        markdown_link_highlight_style(cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_strikethrough_highlight_key(),
        ranges.strikethrough,
        markdown_strikethrough_highlight_style(cx),
        cx,
    );
}

fn set_markdown_highlight(
    editor: &mut Editor,
    key: HighlightKey,
    ranges: Vec<Range<Anchor>>,
    style: HighlightStyle,
    cx: &mut Context<Editor>,
) {
    if ranges.is_empty() {
        editor.clear_highlights(key, cx);
    } else {
        editor.highlight_text(key, ranges, style, cx);
    }
}

fn markdown_marker_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX)
}

fn markdown_heading_1_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 1)
}

fn markdown_heading_2_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 2)
}

fn markdown_heading_3_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 3)
}

fn markdown_heading_other_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 4)
}

fn markdown_strong_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 5)
}

fn markdown_emphasis_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 6)
}

fn markdown_inline_code_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 7)
}

fn markdown_link_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 8)
}

fn markdown_strikethrough_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 9)
}

fn markdown_marker_highlight_style(mode: MarkdownEditMode, cx: &App) -> HighlightStyle {
    match mode {
        MarkdownEditMode::Source => HighlightStyle {
            color: Some(cx.theme().colors().text_muted.opacity(0.18)),
            fade_out: Some(0.85),
            ..Default::default()
        },
        MarkdownEditMode::Rendered => HighlightStyle {
            hide_text: true,
            ..Default::default()
        },
    }
}

fn markdown_heading_highlight_style(
    level: u8,
    mode: MarkdownEditMode,
    cx: &App,
) -> HighlightStyle {
    let colors = cx.theme().colors();
    HighlightStyle {
        color: Some(match level {
            1 => colors.text,
            2 => colors.text,
            3 => colors.text_accent,
            _ => colors.text_muted,
        }),
        font_weight: Some(match level {
            1 => FontWeight::BLACK,
            2 => FontWeight::EXTRA_BOLD,
            3 => FontWeight::BOLD,
            _ => FontWeight::SEMIBOLD,
        }),
        font_size: match mode {
            MarkdownEditMode::Source => None,
            MarkdownEditMode::Rendered => Some(match level {
                1 => px(28.).into(),
                2 => px(22.).into(),
                3 => px(18.).into(),
                _ => px(16.).into(),
            }),
        },
        ..Default::default()
    }
}

fn markdown_strong_highlight_style(_cx: &App) -> HighlightStyle {
    HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    }
}

fn markdown_emphasis_highlight_style(_cx: &App) -> HighlightStyle {
    HighlightStyle {
        font_style: Some(FontStyle::Italic),
        ..Default::default()
    }
}

fn markdown_inline_code_highlight_style(cx: &App) -> HighlightStyle {
    let colors = cx.theme().colors();
    HighlightStyle {
        color: Some(colors.text_accent),
        background_color: Some(colors.editor_foreground.opacity(0.08)),
        font_weight: Some(FontWeight::MEDIUM),
        ..Default::default()
    }
}

fn markdown_link_highlight_style(cx: &App) -> HighlightStyle {
    let colors = cx.theme().colors();
    HighlightStyle {
        color: Some(colors.link_text_hover),
        underline: Some(UnderlineStyle {
            thickness: px(1.),
            color: Some(colors.link_text_hover.opacity(0.7)),
            wavy: false,
        }),
        ..Default::default()
    }
}

fn markdown_strikethrough_highlight_style(cx: &App) -> HighlightStyle {
    HighlightStyle {
        strikethrough: Some(StrikethroughStyle {
            thickness: px(1.),
            color: Some(cx.theme().colors().text_muted),
        }),
        ..Default::default()
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
            .on_action(cx.listener(Self::toggle_mode))
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
                KeyBinding::new("ctrl-shift-m", ToggleMode, Some("Editor")),
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
