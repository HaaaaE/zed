use std::{env, fs, ops::Range, path::PathBuf, sync::Arc};

use anyhow::{Context as _, Result};
use assets::Assets;
use clock::Global;
use collections::HashSet;
use editor::{
    Anchor, Editor, EditorEvent, RowHeightOverride,
    display_map::{
        BlockPlacement, BlockProperties, BlockStyle, DisplayRow, HighlightKey,
    },
};
use gpui::{
    App, Context, Entity, Focusable as _, FontStyle, FontWeight, HighlightStyle,
    KeyBinding, MouseButton, PathPromptOptions, SharedString, StrikethroughStyle,
    Subscription, UnderlineStyle, Window, WindowOptions, actions, div, img, px,
};
use http_client::{AsyncBody, HttpClient, Url};
use md_buffer::Buffer as MdBuffer;
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
    file_buffer: MdBuffer,
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
    revealed_marker_ranges: Vec<Range<usize>>,
    drag_frozen: bool,
}

impl RenderedRevealState {
    fn is_marker_revealed(&self, marker_range: &Range<usize>) -> bool {
        self.revealed_marker_ranges
            .iter()
            .any(|revealed| revealed.start == marker_range.start && revealed.end == marker_range.end)
    }
}

struct MarkdownRenderer {
    block_ids: Vec<editor::display_map::CustomBlockId>,
}

impl Default for MarkdownRenderer {
    fn default() -> Self {
        Self {
            block_ids: Vec::new(),
        }
    }
}

impl MarkdownRenderer {
    fn clear(&mut self, editor: &mut Editor, cx: &mut Context<Editor>) {
        if self.block_ids.is_empty() {
            return;
        }
        let ids = self.block_ids.drain(..).collect::<HashSet<_>>();
        editor.remove_blocks(ids, None, cx);
    }

    fn sync(
        &mut self,
        editor: &Entity<Editor>,
        tree: &MarkdownSyntaxTree,
        mode: MarkdownEditMode,
        cx: &mut Context<MarkdownEditorShell>,
    ) {
        let snapshot = editor.read(cx).buffer().read(cx).snapshot(cx);
        editor.update(cx, |editor, cx| {
            self.clear(editor, cx);
        });

        if mode != MarkdownEditMode::Rendered {
            return;
        }

        let mut new_blocks = Vec::new();

        for span in tree.inline_spans() {
            if span.kind != MarkdownInlineKind::Image {
                continue;
            }
            let Some(url) = &span.url else {
                continue;
            };
            if !url.starts_with("http://") && !url.starts_with("https://") {
                continue;
            }
            let start_anchor = snapshot.anchor_before(MultiBufferOffset(span.source_range.start));
            let end_anchor = snapshot.anchor_after(MultiBufferOffset(span.source_range.end));
            let url = url.clone();
            new_blocks.push(BlockProperties {
                placement: BlockPlacement::Replace(start_anchor..=end_anchor),
                height: Some(6),
                style: BlockStyle::Flex,
                render: Arc::new(move |cx| {
                    let max_width = cx.max_width;
                    let gutter = cx.margins.gutter.width;
                    let _display_url = url.split('?').next().unwrap_or(&url).to_string();
                    img(url.clone())
                        .with_fallback(move || {
                            div()
                                .max_w(px(600.))
                                .h(px(120.))
                                .rounded_md()
                                .border_1()
                                .border_color(gpui::white())
                                .bg(gpui::hsla(0., 0., 0.15, 1.))
                                .into_any_element()
                        })
                        .object_fit(gpui::ObjectFit::Contain)
                        .max_w(max_width)
                        .pl(gutter)
                        .pt(px(4.))
                        .pb(px(4.))
                        .into_any_element()
                }),
                priority: 0,
            });
        }

        if !new_blocks.is_empty() {
            editor.update(cx, |editor, cx| {
                let ids = editor.insert_blocks(new_blocks, None, cx);
                self.block_ids.extend(ids);
            });
        }
    }
}

struct MarkdownWysiwygController {
    parse_tree: Option<MarkdownSyntaxTree>,
    parsed_version: Option<Global>,
    reveal_state: RenderedRevealState,
    renderer: MarkdownRenderer,
}

impl Default for MarkdownWysiwygController {
    fn default() -> Self {
        Self {
            parse_tree: None,
            parsed_version: None,
            reveal_state: RenderedRevealState::default(),
            renderer: MarkdownRenderer::default(),
        }
    }
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
        let file_buffer = MdBuffer::local(contents.clone());
        let editor = cx.new(move |cx| {
            let buffer = cx.new(move |cx| Buffer::local(contents, cx));
            Editor::for_buffer(buffer, None, window, cx)
        });
        editor.update(cx, |editor, _cx| editor.set_soft_wrap());
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
            file_buffer,
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
        self.document.file_buffer.set_text(contents);
        let contents = self.document.file_buffer.serialized_text();
        if let Err(error) = fs::write(&path, contents) {
            eprintln!("failed to save markdown file {}: {error:#}", path.display());
            return;
        }

        self.document.file_buffer.did_save_at_current_version();
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
        let caret_offset = snapshot.point_to_offset(selection.start);
        self.reveal_state.drag_frozen = selection.start != selection.end;
        self.reveal_state.revealed_marker_ranges =
            caret_revealed_marker_ranges(parse_tree, mode, caret_offset);

        let highlights =
            markdown_highlight_ranges(editor, parse_tree, &self.reveal_state, cx);
        let row_height_overrides = markdown_row_height_overrides(parse_tree, mode);
        editor.update(cx, |editor, cx| {
            apply_markdown_highlights(editor, highlights, mode, cx);
            if row_height_overrides.is_empty() {
                editor.clear_row_height_overrides(cx);
            } else {
                editor.set_row_height_overrides(row_height_overrides, cx);
            }
        });

        self.renderer.sync(editor, parse_tree, mode, cx);
    }
}

fn caret_revealed_marker_ranges(
    tree: &MarkdownSyntaxTree,
    mode: MarkdownEditMode,
    caret_offset: usize,
) -> Vec<Range<usize>> {
    if mode == MarkdownEditMode::Source {
        return Vec::new();
    }

    let mut revealed = Vec::new();

    for block in tree.blocks() {
        if is_offset_near_range(caret_offset, &block.source_range, 0) {
            revealed.extend(block.marker_ranges.iter().cloned());
        }
    }

    for span in tree.inline_spans() {
        if is_offset_near_range(caret_offset, &span.source_range, 0) {
            revealed.extend(span.marker_ranges.iter().cloned());
        }
    }

    revealed
}

fn is_offset_near_range(offset: usize, range: &Range<usize>, margin: usize) -> bool {
    offset >= range.start.saturating_sub(margin) && offset < range.end.saturating_add(margin)
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
    revealed_marker: Vec<Range<Anchor>>,
    heading_1: Vec<Range<Anchor>>,
    heading_2: Vec<Range<Anchor>>,
    heading_3: Vec<Range<Anchor>>,
    heading_other: Vec<Range<Anchor>>,
    strong: Vec<Range<Anchor>>,
    emphasis: Vec<Range<Anchor>>,
    inline_code: Vec<Range<Anchor>>,
    link: Vec<Range<Anchor>>,
    strikethrough: Vec<Range<Anchor>>,
    image: Vec<Range<Anchor>>,
    inline_math: Vec<Range<Anchor>>,
    fenced_code_content: Vec<Range<Anchor>>,
    pipe_table_content: Vec<Range<Anchor>>,
}

fn markdown_highlight_ranges(
    editor: &Entity<Editor>,
    tree: &MarkdownSyntaxTree,
    reveal_state: &RenderedRevealState,
    cx: &mut App,
) -> MarkdownHighlightRanges {
    let snapshot = editor.read(cx).buffer().read(cx).snapshot(cx);
    let mut ranges = MarkdownHighlightRanges::default();

    for block in tree.blocks() {
        match block.kind {
            MarkdownBlockKind::AtxHeading { level } => {
                for marker_range in &block.marker_ranges {
                    if reveal_state.is_marker_revealed(marker_range) {
                        push_byte_range(&snapshot, marker_range.clone(), &mut ranges.revealed_marker);
                    } else {
                        push_byte_range(&snapshot, marker_range.clone(), &mut ranges.marker);
                    }
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
            MarkdownBlockKind::FencedCodeBlock => {
                for marker_range in &block.marker_ranges {
                    if reveal_state.is_marker_revealed(marker_range) {
                        push_byte_range(&snapshot, marker_range.clone(), &mut ranges.revealed_marker);
                    } else {
                        push_byte_range(&snapshot, marker_range.clone(), &mut ranges.marker);
                    }
                }
                push_byte_range(&snapshot, block.content_range.clone(), &mut ranges.fenced_code_content);
            }
            MarkdownBlockKind::PipeTable => {
                for marker_range in &block.marker_ranges {
                    if reveal_state.is_marker_revealed(marker_range) {
                        push_byte_range(&snapshot, marker_range.clone(), &mut ranges.revealed_marker);
                    } else {
                        push_byte_range(&snapshot, marker_range.clone(), &mut ranges.marker);
                    }
                }
                push_byte_range(&snapshot, block.content_range.clone(), &mut ranges.pipe_table_content);
            }
            _ => {}
        }
    }

    for span in tree.inline_spans() {
        let target = match span.kind {
            MarkdownInlineKind::Strong => &mut ranges.strong,
            MarkdownInlineKind::Emphasis => &mut ranges.emphasis,
            MarkdownInlineKind::InlineCode => &mut ranges.inline_code,
            MarkdownInlineKind::Link => &mut ranges.link,
            MarkdownInlineKind::Strikethrough => &mut ranges.strikethrough,
            MarkdownInlineKind::Image => &mut ranges.image,
            MarkdownInlineKind::InlineMath => &mut ranges.inline_math,
        };
        for content_range in &span.content_ranges {
            push_byte_range(&snapshot, content_range.clone(), target);
        }
        for marker_range in &span.marker_ranges {
            if reveal_state.is_marker_revealed(marker_range) {
                push_byte_range(&snapshot, marker_range.clone(), &mut ranges.revealed_marker);
            } else {
                push_byte_range(&snapshot, marker_range.clone(), &mut ranges.marker);
            }
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
        markdown_revealed_marker_highlight_key(),
        ranges.revealed_marker,
        markdown_revealed_marker_highlight_style(cx),
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
    set_markdown_highlight(
        editor,
        markdown_image_highlight_key(),
        ranges.image,
        markdown_image_highlight_style(cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_inline_math_highlight_key(),
        ranges.inline_math,
        markdown_inline_math_highlight_style(cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_fenced_code_content_highlight_key(),
        ranges.fenced_code_content,
        markdown_fenced_code_content_highlight_style(mode, cx),
        cx,
    );
    set_markdown_highlight(
        editor,
        markdown_pipe_table_content_highlight_key(),
        ranges.pipe_table_content,
        markdown_pipe_table_content_highlight_style(mode, cx),
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
    HighlightKey::SyntaxTreeView(usize::MAX - 11)
}

fn markdown_revealed_marker_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 10)
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

fn markdown_image_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 12)
}

fn markdown_inline_math_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 13)
}

fn markdown_fenced_code_content_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 14)
}

fn markdown_pipe_table_content_highlight_key() -> HighlightKey {
    HighlightKey::SyntaxTreeView(usize::MAX - 15)
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

fn markdown_revealed_marker_highlight_style(cx: &App) -> HighlightStyle {
    HighlightStyle {
        color: Some(cx.theme().colors().text_muted.opacity(0.5)),
        fade_out: Some(0.3),
        ..Default::default()
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

fn markdown_image_highlight_style(cx: &App) -> HighlightStyle {
    let colors = cx.theme().colors();
    HighlightStyle {
        color: Some(colors.link_text_hover),
        ..Default::default()
    }
}

fn markdown_inline_math_highlight_style(cx: &App) -> HighlightStyle {
    let colors = cx.theme().colors();
    HighlightStyle {
        color: Some(colors.text_accent),
        font_style: Some(FontStyle::Italic),
        ..Default::default()
    }
}

fn markdown_fenced_code_content_highlight_style(mode: MarkdownEditMode, cx: &App) -> HighlightStyle {
    let colors = cx.theme().colors();
    HighlightStyle {
        color: Some(colors.text),
        background_color: match mode {
            MarkdownEditMode::Source => None,
            MarkdownEditMode::Rendered => Some(colors.editor_foreground.opacity(0.06)),
        },
        font_size: match mode {
            MarkdownEditMode::Source => None,
            MarkdownEditMode::Rendered => Some(px(14.).into()),
        },
        ..Default::default()
    }
}

fn markdown_pipe_table_content_highlight_style(mode: MarkdownEditMode, cx: &App) -> HighlightStyle {
    let colors = cx.theme().colors();
    HighlightStyle {
        color: Some(colors.text_muted),
        background_color: match mode {
            MarkdownEditMode::Source => None,
            MarkdownEditMode::Rendered => Some(colors.editor_foreground.opacity(0.04)),
        },
        font_size: match mode {
            MarkdownEditMode::Source => None,
            MarkdownEditMode::Rendered => Some(px(13.).into()),
        },
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

impl gpui::Render for MarkdownEditorShell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
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

pub fn run() {
    let path = env::args_os().nth(1).map(PathBuf::from);

    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            cx.set_http_client(Arc::new(UreqHttpClient));
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

fn command_row(label: &'static str, action: impl gpui::Action) -> impl gpui::IntoElement {
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

fn singleton_buffer(editor: &Entity<Editor>, cx: &App) -> Option<Entity<language::Buffer>> {
    editor.read(cx).buffer().read(cx).as_singleton()
}

struct UreqHttpClient;

impl HttpClient for UreqHttpClient {
    fn user_agent(&self) -> Option<&http::HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        req: http::Request<AsyncBody>,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<http::Response<AsyncBody>>> {
        let uri = req.uri().clone();
        let method = req.method().clone();
        Box::pin(async move {
            smol::unblock(move || {
                let url = uri.to_string();
                let agent = ureq::Agent::new_with_defaults();
                let request = http::Request::builder()
                    .method(method)
                    .uri(&url)
                    .header("User-Agent", "markdown-editor/1.0")
                    .body(())
                    .map_err(|e| anyhow::anyhow!("failed to build request: {e}"))?;
                let response = agent.run(request)?;
                let (parts, mut body) = response.into_parts();
                let body_bytes = body.read_to_vec()?;
                let async_body = AsyncBody::from(body_bytes);
                Ok(http::Response::from_parts(parts, async_body))
            })
            .await
        })
    }
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
