use std::ops::Range;

use gpui::{
    App, Context, ImgResourceLoader, IntoElement, MouseButton, Resource, SharedString, Window, div,
    img, prelude::*, px,
};
use markdown_wysiwyg::{MarkdownBlockKind, MarkdownInlineKind};
use md_buffer::BufferSnapshot;
use md_text::{Point, Selection, SelectionGoal};
use md_theme::{editor_palette, gutter_width};

use super::{
    DisplayRow, MarkdownEditor, MarkdownEditorMode, RowDisplayStyle, VisualLineBoundary,
    clip_cursor, range_contains, ranges_overlap, rendered_element_source_range_is_active,
    rendered_remote_image_span_is_block_in_row, selection_byte_range, visual_horizontal_goal,
};

pub(super) const RENDERED_IMAGE_BLOCK_MAX_WIDTH: gpui::Pixels = px(600.);
pub(super) const RENDERED_IMAGE_BLOCK_PLACEHOLDER_HEIGHT: gpui::Pixels = px(120.);
pub(super) const RENDERED_IMAGE_BLOCK_VERTICAL_PADDING: gpui::Pixels = px(4.);
pub(super) const RENDERED_GENERIC_BLOCK_VERTICAL_PADDING: gpui::Pixels = px(4.);
pub(super) const RENDERED_GENERIC_BLOCK_HORIZONTAL_PADDING: gpui::Pixels = px(8.);

#[derive(Clone, Debug, PartialEq, Eq)]
enum DisplayBlockKind {
    RemoteImage(RenderedImageBlock),
    Generic(RenderedGenericBlock),
}

impl DisplayBlockKind {
    fn for_display_row(
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        selection: &Selection<Point>,
        mode: MarkdownEditorMode,
    ) -> Option<Self> {
        rendered_image_block_for_row(snapshot, display_row, selection, mode)
            .map(Self::RemoteImage)
            .or_else(|| {
                rendered_generic_block_for_row(snapshot, display_row, selection, mode)
                    .map(Self::Generic)
            })
    }

    fn into_layout(
        self,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
        window: &mut Window,
        cx: &mut App,
    ) -> DisplayBlockLayout {
        match self {
            Self::RemoteImage(image_block) => DisplayBlockLayout::RemoteImage(
                RenderedImageBlockLayout::new(image_block, wrap_width, window, cx),
            ),
            Self::Generic(generic_block) => DisplayBlockLayout::Generic(
                RenderedGenericBlockLayout::new(generic_block, wrap_width, row_style),
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum DisplayBlockLayout {
    RemoteImage(RenderedImageBlockLayout),
    Generic(RenderedGenericBlockLayout),
}

impl DisplayBlockLayout {
    pub(super) fn for_display_row(
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        selection: &Selection<Point>,
        mode: MarkdownEditorMode,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Self> {
        DisplayBlockKind::for_display_row(snapshot, display_row, selection, mode)
            .map(|kind| kind.into_layout(wrap_width, row_style, window, cx))
    }

    pub(super) fn height(&self) -> gpui::Pixels {
        match self {
            Self::RemoteImage(image_layout) => image_layout.height(),
            Self::Generic(generic_layout) => generic_layout.height(),
        }
    }

    pub(super) fn cacheable(&self) -> bool {
        match self {
            Self::RemoteImage(image_layout) => image_layout.cacheable(),
            Self::Generic(generic_layout) => generic_layout.cacheable(),
        }
    }

    pub(super) fn source_range(&self) -> &Range<usize> {
        match self {
            Self::RemoteImage(image_layout) => &image_layout.image_block.source_range,
            Self::Generic(generic_layout) => &generic_layout.generic_block.source_range,
        }
    }

    pub(super) fn visible_x_for_source_offset(&self, source_offset: usize) -> gpui::Pixels {
        match self {
            Self::RemoteImage(image_layout) => image_block_visible_x_for_source_offset(
                &image_layout.image_block.source_range,
                image_layout.width,
                source_offset,
            ),
            Self::Generic(generic_layout) => image_block_visible_x_for_source_offset(
                &generic_layout.generic_block.source_range,
                generic_layout.width,
                source_offset,
            ),
        }
    }

    pub(super) fn source_offset_for_x(&self, x: gpui::Pixels) -> usize {
        match self {
            Self::RemoteImage(image_layout) => {
                image_block_source_offset_for_x(&image_layout.image_block, image_layout.width, x)
            }
            Self::Generic(generic_layout) => block_source_offset_for_x(
                &generic_layout.generic_block.source_range,
                generic_layout.width,
                x,
            ),
        }
    }

    pub(super) fn point_for_x(&self, snapshot: &BufferSnapshot, x: gpui::Pixels) -> Point {
        let offset = self.source_offset_for_x(x);
        let offset = snapshot
            .as_text_snapshot()
            .as_rope()
            .floor_char_boundary(offset);

        clip_cursor(
            snapshot,
            snapshot.as_text_snapshot().offset_to_point(offset),
        )
    }

    pub(super) fn point_for_mouse_x(&self, snapshot: &BufferSnapshot, x: gpui::Pixels) -> Point {
        self.point_for_x(snapshot, x - gutter_width())
    }

    pub(super) fn mouse_target_for_x(
        &self,
        snapshot: &BufferSnapshot,
        x: gpui::Pixels,
    ) -> (Point, SelectionGoal) {
        let point = self.point_for_mouse_x(snapshot, x);
        let source_offset = snapshot.as_text_snapshot().point_to_offset(point);
        (
            point,
            visual_horizontal_goal(0, self.visible_x_for_source_offset(source_offset)),
        )
    }

    pub(super) fn line_boundary_target(
        &self,
        snapshot: &BufferSnapshot,
        boundary: VisualLineBoundary,
    ) -> (Point, SelectionGoal) {
        let source_range = self.source_range();
        let source_offset = match boundary {
            VisualLineBoundary::Start => source_range.start,
            VisualLineBoundary::End => source_range.end,
        };
        let x = self.visible_x_for_source_offset(source_offset);
        (
            snapshot.as_text_snapshot().offset_to_point(source_offset),
            visual_horizontal_goal(0, x),
        )
    }

    pub(super) fn caret_x(
        &self,
        snapshot: &BufferSnapshot,
        selection: &Selection<Point>,
    ) -> Option<gpui::Pixels> {
        if !selection.is_empty() {
            return None;
        }

        let source_offset = snapshot
            .as_text_snapshot()
            .point_to_offset(clip_cursor(snapshot, selection.head()));
        let source_range = self.source_range();
        if source_offset == source_range.start || source_offset == source_range.end {
            Some(self.visible_x_for_source_offset(source_offset))
        } else {
            None
        }
    }

    pub(super) fn is_whole_selected(
        &self,
        snapshot: &BufferSnapshot,
        selection: &Selection<Point>,
    ) -> bool {
        !selection.is_empty()
            && range_contains(
                &selection_byte_range(snapshot, selection),
                self.source_range(),
            )
    }

    pub(super) fn render(
        self,
        snapshot: &BufferSnapshot,
        selection: &Selection<Point>,
        row_style: RowDisplayStyle,
        cx: &mut Context<MarkdownEditor>,
    ) -> Vec<gpui::AnyElement> {
        let selected = self.is_whole_selected(snapshot, selection);
        let caret_x = self.caret_x(snapshot, selection);
        match self {
            Self::RemoteImage(image_layout) => {
                vec![render_image_block(
                    image_layout,
                    selected,
                    caret_x,
                    row_style,
                    cx,
                )]
            }
            Self::Generic(generic_layout) => {
                vec![render_generic_block(
                    generic_layout,
                    selected,
                    caret_x,
                    row_style,
                    cx,
                )]
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RenderedImageBlock {
    pub(super) url: String,
    pub(super) alt_text: String,
    pub(super) source_range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RenderedGenericBlock {
    pub(super) source_range: Range<usize>,
    pub(super) text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RenderedImageBlockLayout {
    pub(super) image_block: RenderedImageBlock,
    pub(super) width: gpui::Pixels,
    pub(super) image_height: gpui::Pixels,
    pub(super) cacheable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RenderedGenericBlockLayout {
    pub(super) generic_block: RenderedGenericBlock,
    pub(super) width: gpui::Pixels,
    pub(super) content_height: gpui::Pixels,
}

impl RenderedImageBlockLayout {
    pub(super) fn new(
        image_block: RenderedImageBlock,
        wrap_width: gpui::Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let width = wrap_width.max(px(1.)).min(RENDERED_IMAGE_BLOCK_MAX_WIDTH);
        let resource = Resource::Uri(image_block.url.clone().into());
        let loaded_height = window
            .use_asset::<ImgResourceLoader>(&resource, cx)
            .and_then(|image| {
                let image = image.ok()?;
                let size = image.size(0);
                image_block_height_for_size(width, size.width.0, size.height.0)
            });
        let (image_height, cacheable) = loaded_height
            .map(|height| (height, true))
            .unwrap_or((RENDERED_IMAGE_BLOCK_PLACEHOLDER_HEIGHT, false));

        Self {
            image_block,
            width,
            image_height,
            cacheable,
        }
    }

    pub(super) fn image_height(&self) -> gpui::Pixels {
        self.image_height
    }

    pub(super) fn height(&self) -> gpui::Pixels {
        self.image_height() + RENDERED_IMAGE_BLOCK_VERTICAL_PADDING * 2.
    }

    pub(super) fn cacheable(&self) -> bool {
        self.cacheable
    }
}

impl RenderedGenericBlockLayout {
    pub(super) fn new(
        generic_block: RenderedGenericBlock,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
    ) -> Self {
        Self {
            generic_block,
            width: wrap_width.max(px(1.)),
            content_height: row_style.line_height,
        }
    }

    pub(super) fn height(&self) -> gpui::Pixels {
        self.content_height + RENDERED_GENERIC_BLOCK_VERTICAL_PADDING * 2.
    }

    pub(super) fn cacheable(&self) -> bool {
        true
    }
}

pub(super) fn image_block_height_for_size(
    width: gpui::Pixels,
    image_width: i32,
    image_height: i32,
) -> Option<gpui::Pixels> {
    if image_width <= 0 || image_height <= 0 {
        return None;
    }

    Some(width * (image_height as f32 / image_width as f32))
}

pub(super) fn image_block_source_offset_for_x(
    image_block: &RenderedImageBlock,
    image_width: gpui::Pixels,
    x: gpui::Pixels,
) -> usize {
    block_source_offset_for_x(&image_block.source_range, image_width, x)
}

fn image_block_visible_x_for_source_offset(
    source_range: &Range<usize>,
    image_width: gpui::Pixels,
    source_offset: usize,
) -> gpui::Pixels {
    block_visible_x_for_source_offset(source_range, image_width, source_offset)
}

fn block_source_offset_for_x(
    source_range: &Range<usize>,
    block_width: gpui::Pixels,
    x: gpui::Pixels,
) -> usize {
    let block_x = x.max(px(0.));
    if block_x < block_width * 0.5 {
        source_range.start
    } else {
        source_range.end
    }
}

fn block_visible_x_for_source_offset(
    source_range: &Range<usize>,
    block_width: gpui::Pixels,
    source_offset: usize,
) -> gpui::Pixels {
    if source_offset <= source_range.start {
        px(0.)
    } else if source_offset >= source_range.end {
        block_width
    } else {
        block_width * 0.5
    }
}

fn render_image_block(
    image_layout: RenderedImageBlockLayout,
    selected: bool,
    caret_x: Option<gpui::Pixels>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> gpui::AnyElement {
    let palette = editor_palette();
    let image_height = image_layout.image_height();
    let mouse_down_block_layout = DisplayBlockLayout::RemoteImage(image_layout.clone());
    let mouse_move_block_layout = mouse_down_block_layout.clone();
    let image_block = image_layout.image_block;
    let fallback_label = if image_block.alt_text.trim().is_empty() {
        image_block
            .url
            .split('?')
            .next()
            .unwrap_or(&image_block.url)
            .to_string()
    } else {
        image_block.alt_text
    };

    div()
        .w_full()
        .py(RENDERED_IMAGE_BLOCK_VERTICAL_PADDING)
        .relative()
        .when(selected, |this| {
            this.bg(palette.selection_background.opacity(0.20))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event, window, cx| {
                this.mouse_left_down_on_block(&mouse_down_block_layout, event, window, cx)
            }),
        )
        .on_mouse_move(cx.listener(move |this, event, window, cx| {
            this.mouse_move_on_block(&mouse_move_block_layout, event, window, cx)
        }))
        .child(
            div()
                .w(image_layout.width)
                .h(image_height)
                .rounded_md()
                .border_1()
                .border_color(if selected {
                    palette.selection_background
                } else {
                    palette.gutter_text
                })
                .bg(palette.fenced_code_background)
                .overflow_hidden()
                .child(
                    img(image_block.url)
                        .size_full()
                        .object_fit(gpui::ObjectFit::Contain)
                        .with_fallback(move || {
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .px_3()
                                .text_color(palette.muted_text)
                                .child(SharedString::from(fallback_label.clone()))
                                .into_any_element()
                        }),
                ),
        )
        .when_some(caret_x, |this, caret_x| {
            this.child(caret_element(caret_x, row_style))
        })
        .into_any_element()
}

fn render_generic_block(
    generic_layout: RenderedGenericBlockLayout,
    selected: bool,
    caret_x: Option<gpui::Pixels>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> gpui::AnyElement {
    let palette = editor_palette();
    let mouse_down_block_layout = DisplayBlockLayout::Generic(generic_layout.clone());
    let mouse_move_block_layout = mouse_down_block_layout.clone();
    let generic_block = generic_layout.generic_block;
    let text = if generic_block.text.is_empty() {
        " ".to_string()
    } else {
        generic_block.text
    };

    div()
        .w_full()
        .py(RENDERED_GENERIC_BLOCK_VERTICAL_PADDING)
        .relative()
        .when(selected, |this| {
            this.bg(palette.selection_background.opacity(0.20))
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event, window, cx| {
                this.mouse_left_down_on_block(&mouse_down_block_layout, event, window, cx)
            }),
        )
        .on_mouse_move(cx.listener(move |this, event, window, cx| {
            this.mouse_move_on_block(&mouse_move_block_layout, event, window, cx)
        }))
        .child(
            div()
                .w(generic_layout.width)
                .h(generic_layout.content_height)
                .px(RENDERED_GENERIC_BLOCK_HORIZONTAL_PADDING)
                .rounded_md()
                .border_1()
                .border_color(if selected {
                    palette.selection_background
                } else {
                    palette.gutter_text
                })
                .bg(palette.fenced_code_background)
                .overflow_hidden()
                .whitespace_nowrap()
                .child(SharedString::from(text)),
        )
        .when_some(caret_x, |this, caret_x| {
            this.child(caret_element(caret_x, row_style))
        })
        .into_any_element()
}

fn caret_element(caret_x: gpui::Pixels, row_style: RowDisplayStyle) -> gpui::AnyElement {
    let palette = editor_palette();
    div()
        .absolute()
        .left(caret_x)
        .top_0()
        .bottom_0()
        .w(px(1.))
        .flex()
        .items_center()
        .child(div().w(px(1.)).h(row_style.caret_height).bg(palette.caret))
        .into_any_element()
}

pub(super) fn rendered_image_block_for_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> Option<RenderedImageBlock> {
    if mode != MarkdownEditorMode::Rendered {
        return None;
    }

    let row_source_range = &display_row.source_range;
    let source_text = &display_row.source_text;
    let mut matching_spans = snapshot
        .syntax_tree()
        .inline_spans_in_source_range(row_source_range.clone())
        .filter(|span| {
            span.kind == MarkdownInlineKind::Image
                && rendered_remote_image_span_is_block_in_row(span, source_text, row_source_range)
        });

    let span = matching_spans.next()?;
    if matching_spans.next().is_some() {
        return None;
    }
    if rendered_element_source_range_is_active(snapshot, selection, &span.source_range) {
        return None;
    }

    Some(RenderedImageBlock {
        url: span.url.clone()?,
        alt_text: display_row.text.trim().to_string(),
        source_range: span.source_range.clone(),
    })
}

pub(super) fn rendered_generic_block_for_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> Option<RenderedGenericBlock> {
    if mode != MarkdownEditorMode::Rendered {
        return None;
    }

    let row = display_row.row as usize;
    let row_source_range = &display_row.source_range;
    let code_block = snapshot
        .syntax_tree()
        .blocks_in_source_range(row_source_range.clone())
        .find(|block| {
            block.kind == MarkdownBlockKind::FencedCodeBlock
                && block.row_range.contains(&row)
                && ranges_overlap(&block.content_range, row_source_range)
        })?;

    if rendered_element_source_range_is_active(snapshot, selection, &code_block.source_range) {
        return None;
    }

    Some(RenderedGenericBlock {
        source_range: row_source_range.clone(),
        text: display_row.text.trim_end_matches(['\r', '\n']).to_string(),
    })
}
