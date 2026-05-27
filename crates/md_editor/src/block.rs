use std::{ops::Range, path::Path};

use gpui::{
    App, Context, ImgResourceLoader, IntoElement, MouseButton, SharedString, Window, div, img,
    prelude::*, px,
};
use md_assets::EDITOR_FONT_FAMILY;
use md_buffer::BufferSnapshot;
use md_text::{Point, Selection, SelectionGoal};
use md_theme::{editor_palette, gutter_width};

use super::rendered_element::{
    RenderedElementDescriptor, RenderedElementKind, RenderedElementPlacement,
};
use super::{
    MarkdownEditor, MarkdownEditorMode, RowDisplayStyle, VisualLineBoundary, clip_cursor,
    display_model::DisplayRow,
    formula_render::{FormulaRenderMode, FormulaRenderState, formula_render_key, render_formula},
    markdown_image::MarkdownImageSource,
    range_contains, rendered_element_descriptor_for_inline_span_in_row,
    rendered_element_source_range_is_active, selection_byte_range, visual_horizontal_goal,
};

pub(super) const RENDERED_IMAGE_BLOCK_MAX_WIDTH: gpui::Pixels = px(600.);
pub(super) const RENDERED_IMAGE_BLOCK_PLACEHOLDER_HEIGHT: gpui::Pixels = px(120.);
pub(super) const RENDERED_IMAGE_BLOCK_VERTICAL_PADDING: gpui::Pixels = px(4.);
pub(super) const RENDERED_FORMULA_BLOCK_HORIZONTAL_PADDING: gpui::Pixels = px(12.);
pub(super) const RENDERED_FORMULA_BLOCK_VERTICAL_PADDING: gpui::Pixels = px(8.);

#[derive(Clone, Debug, PartialEq, Eq)]
enum DisplayBlockKind {
    RemoteImage(RenderedImageBlock),
    Formula(RenderedFormulaBlock),
}

impl DisplayBlockKind {
    fn for_display_row(
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        selection: &Selection<Point>,
        mode: MarkdownEditorMode,
        document_path: Option<&Path>,
    ) -> Option<Self> {
        rendered_block_for_row(snapshot, display_row, selection, mode, document_path)
    }

    fn into_layout(
        self,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
        measure_layout: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> DisplayBlockLayout {
        match self {
            Self::RemoteImage(image_block) => DisplayBlockLayout::RemoteImage(
                RenderedImageBlockLayout::new(image_block, wrap_width, measure_layout, window, cx),
            ),
            Self::Formula(formula_block) => {
                DisplayBlockLayout::Formula(RenderedFormulaBlockLayout::new(
                    formula_block,
                    wrap_width,
                    row_style,
                    measure_layout,
                    window,
                    cx,
                ))
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum DisplayBlockLayout {
    RemoteImage(RenderedImageBlockLayout),
    Formula(RenderedFormulaBlockLayout),
}

impl DisplayBlockLayout {
    pub(super) fn for_display_row(
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        selection: &Selection<Point>,
        mode: MarkdownEditorMode,
        document_path: Option<&Path>,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
        measure_layout: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Self> {
        DisplayBlockKind::for_display_row(snapshot, display_row, selection, mode, document_path)
            .map(|kind| kind.into_layout(wrap_width, row_style, measure_layout, window, cx))
    }

    pub(super) fn height(&self) -> gpui::Pixels {
        match self {
            Self::RemoteImage(image_layout) => image_layout.height(),
            Self::Formula(formula_layout) => formula_layout.height(),
        }
    }

    pub(super) fn cacheable(&self) -> bool {
        match self {
            Self::RemoteImage(image_layout) => image_layout.cacheable(),
            Self::Formula(formula_layout) => formula_layout.cacheable(),
        }
    }

    pub(super) fn source_range(&self) -> &Range<usize> {
        match self {
            Self::RemoteImage(image_layout) => &image_layout.image_block.source_range,
            Self::Formula(formula_layout) => &formula_layout.formula_block.source_range,
        }
    }

    pub(super) fn visible_x_for_source_offset(&self, source_offset: usize) -> gpui::Pixels {
        match self {
            Self::RemoteImage(image_layout) => image_block_visible_x_for_source_offset(
                &image_layout.image_block.source_range,
                image_layout.width,
                source_offset,
            ),
            Self::Formula(formula_layout) => block_visible_x_for_source_offset(
                &formula_layout.formula_block.source_range,
                formula_layout.width,
                source_offset,
            ),
        }
    }

    pub(super) fn source_offset_for_x(&self, x: gpui::Pixels) -> usize {
        match self {
            Self::RemoteImage(image_layout) => {
                image_block_source_offset_for_x(&image_layout.image_block, image_layout.width, x)
            }
            Self::Formula(formula_layout) => block_source_offset_for_x(
                &formula_layout.formula_block.source_range,
                formula_layout.width,
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
        &self,
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
            Self::Formula(formula_layout) => {
                vec![render_formula_block(
                    formula_layout,
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
    pub(super) descriptor: RenderedElementDescriptor,
    pub(super) image_source: MarkdownImageSource,
    pub(super) source_range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RenderedImageBlockLayout {
    pub(super) image_block: RenderedImageBlock,
    pub(super) width: gpui::Pixels,
    pub(super) image_height: gpui::Pixels,
    pub(super) cacheable: bool,
}

impl RenderedImageBlockLayout {
    pub(super) fn new(
        image_block: RenderedImageBlock,
        wrap_width: gpui::Pixels,
        measure_layout: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let width = wrap_width.max(px(1.)).min(RENDERED_IMAGE_BLOCK_MAX_WIDTH);
        if !measure_layout {
            return Self {
                image_block,
                width,
                image_height: RENDERED_IMAGE_BLOCK_PLACEHOLDER_HEIGHT,
                cacheable: false,
            };
        }

        let loaded_height = image_block
            .image_source
            .resource()
            .and_then(|resource| window.use_asset::<ImgResourceLoader>(&resource, cx))
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RenderedFormulaBlock {
    pub(super) descriptor: RenderedElementDescriptor,
    pub(super) tex: String,
    pub(super) source_range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RenderedFormulaBlockLayout {
    pub(super) formula_block: RenderedFormulaBlock,
    pub(super) width: gpui::Pixels,
    pub(super) height: gpui::Pixels,
    pub(super) rendered_formula: Option<super::formula_render::FormulaRenderAsset>,
    pub(super) cacheable: bool,
}

impl RenderedFormulaBlockLayout {
    pub(super) fn new(
        formula_block: RenderedFormulaBlock,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
        measure_layout: bool,
        window: &mut Window,
        _cx: &mut App,
    ) -> Self {
        let width = wrap_width.max(px(1.));
        if !measure_layout {
            return Self {
                formula_block,
                width,
                height: row_style.line_height + RENDERED_FORMULA_BLOCK_VERTICAL_PADDING * 2.,
                rendered_formula: None,
                cacheable: false,
            };
        }

        let fallback_height = row_style.line_height + RENDERED_FORMULA_BLOCK_VERTICAL_PADDING * 2.;
        let key = formula_render_key(
            formula_block.tex.clone(),
            FormulaRenderMode::Block,
            row_style.text_size,
            row_style.line_height,
            editor_palette().inline_math_text,
            RENDERED_FORMULA_BLOCK_VERTICAL_PADDING,
            window.scale_factor(),
        );
        let rendered_formula = match render_formula(&key, gpui::size(width, fallback_height)) {
            FormulaRenderState::Ready(asset) => Some(asset),
            FormulaRenderState::Invalid(_) => None,
        };
        let height = rendered_formula
            .as_ref()
            .map(|asset| asset.logical_size.height + RENDERED_FORMULA_BLOCK_VERTICAL_PADDING * 2.)
            .unwrap_or(fallback_height);

        Self {
            formula_block,
            width,
            height: height.max(row_style.line_height),
            rendered_formula,
            cacheable: true,
        }
    }

    pub(super) fn height(&self) -> gpui::Pixels {
        self.height
    }

    pub(super) fn cacheable(&self) -> bool {
        self.cacheable
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
    image_layout: &RenderedImageBlockLayout,
    selected: bool,
    caret_x: Option<gpui::Pixels>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> gpui::AnyElement {
    let palette = editor_palette();
    let image_height = image_layout.image_height();
    let mouse_down_block_layout = DisplayBlockLayout::RemoteImage(image_layout.clone());
    let mouse_move_block_layout = mouse_down_block_layout.clone();
    let image_block = &image_layout.image_block;
    let fallback_label = image_block.image_source.fallback_label();
    let invalid_fallback_label = fallback_label.clone();
    let image_source = image_block.image_source.image_source();

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
                .when_some(image_source, |this, image_source| {
                    this.child(
                        img(image_source)
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
                    )
                })
                .when(!image_block.image_source.is_renderable(), |this| {
                    this.child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .px_3()
                            .text_color(palette.muted_text)
                            .child(SharedString::from(invalid_fallback_label.clone())),
                    )
                }),
        )
        .when_some(caret_x, |this, caret_x| {
            this.child(caret_element(caret_x, row_style))
        })
        .into_any_element()
}

fn render_formula_block(
    formula_layout: &RenderedFormulaBlockLayout,
    selected: bool,
    caret_x: Option<gpui::Pixels>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> gpui::AnyElement {
    let mouse_down_block_layout = DisplayBlockLayout::Formula(formula_layout.clone());
    let mouse_move_block_layout = mouse_down_block_layout.clone();

    div()
        .w_full()
        .relative()
        .when(selected, |this| {
            this.bg(editor_palette().selection_background.opacity(0.20))
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
        .child(render_formula_block_inner(
            formula_layout,
            row_style,
            selected,
        ))
        .when_some(caret_x, |this, caret_x| {
            this.child(caret_element(caret_x, row_style))
        })
        .into_any_element()
}

fn render_formula_block_inner(
    formula_layout: &RenderedFormulaBlockLayout,
    row_style: RowDisplayStyle,
    selected: bool,
) -> gpui::AnyElement {
    let palette = editor_palette();
    let mut element = formula_block_measurement_element(
        &formula_layout.formula_block.tex,
        formula_layout.width,
        row_style,
    )
    .border_1()
    .border_color(if selected {
        palette.selection_background
    } else {
        palette.inline_math_text.opacity(0.35)
    });

    if selected {
        element = element.bg(palette.selection_background.opacity(0.12));
    }

    if let Some(asset) = &formula_layout.rendered_formula {
        element = element.child(img(asset.image.clone()).h(asset.logical_size.height));
    }

    element.into_any_element()
}

fn formula_block_measurement_element(
    tex: &str,
    width: gpui::Pixels,
    row_style: RowDisplayStyle,
) -> gpui::Div {
    let palette = editor_palette();
    div()
        .w(width)
        .px(RENDERED_FORMULA_BLOCK_HORIZONTAL_PADDING)
        .py(RENDERED_FORMULA_BLOCK_VERTICAL_PADDING)
        .rounded_md()
        .bg(palette.inline_math_text.opacity(0.08))
        .font_family(EDITOR_FONT_FAMILY)
        .text_size(row_style.text_size)
        .line_height(row_style.line_height)
        .text_color(palette.inline_math_text)
        .text_center()
        .child(SharedString::from(tex.to_string()))
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

#[cfg(test)]
pub(super) fn rendered_image_block_for_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
    document_path: Option<&Path>,
) -> Option<RenderedImageBlock> {
    let descriptor =
        rendered_block_descriptor_for_row(snapshot, display_row, selection, mode, document_path)?;

    let RenderedElementKind::Image { image_source } = descriptor.kind.clone() else {
        return None;
    };
    Some(RenderedImageBlock {
        source_range: descriptor.source_range.clone(),
        descriptor,
        image_source,
    })
}

#[cfg(test)]
pub(super) fn rendered_formula_block_for_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> Option<RenderedFormulaBlock> {
    let descriptor =
        rendered_block_descriptor_for_row(snapshot, display_row, selection, mode, None)?;

    let RenderedElementKind::Math { tex } = descriptor.kind.clone() else {
        return None;
    };
    Some(RenderedFormulaBlock {
        source_range: descriptor.source_range.clone(),
        descriptor,
        tex,
    })
}

fn rendered_block_for_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
    document_path: Option<&Path>,
) -> Option<DisplayBlockKind> {
    let descriptor =
        rendered_block_descriptor_for_row(snapshot, display_row, selection, mode, document_path)?;

    match descriptor.kind.clone() {
        RenderedElementKind::Image { image_source } => {
            Some(DisplayBlockKind::RemoteImage(RenderedImageBlock {
                source_range: descriptor.source_range.clone(),
                descriptor,
                image_source,
            }))
        }
        RenderedElementKind::Math { tex } => {
            Some(DisplayBlockKind::Formula(RenderedFormulaBlock {
                source_range: descriptor.source_range.clone(),
                descriptor,
                tex,
            }))
        }
        RenderedElementKind::Custom { .. } => None,
    }
}

fn rendered_block_descriptor_for_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
    document_path: Option<&Path>,
) -> Option<RenderedElementDescriptor> {
    if mode != MarkdownEditorMode::Rendered {
        return None;
    }

    let matching_descriptors = display_row
        .rendered_element_descriptors
        .iter()
        .filter(|descriptor| descriptor.placement == RenderedElementPlacement::Block)
        .cloned()
        .collect::<Vec<_>>();
    let should_retry_with_document_path = matching_descriptors.is_empty()
        && document_path.is_some()
        && !display_row.rendered_element_descriptors_have_document_path;
    let matching_descriptors = if should_retry_with_document_path {
        let row_source_range = &display_row.source_range;
        display_row
            .inline_spans
            .iter()
            .filter_map(|span| {
                rendered_element_descriptor_for_inline_span_in_row(
                    span,
                    &display_row.source_text,
                    row_source_range,
                    document_path,
                )
                .filter(|descriptor| descriptor.placement == RenderedElementPlacement::Block)
            })
            .collect::<Vec<_>>()
    } else {
        matching_descriptors
    };
    let mut matching_descriptors = matching_descriptors.into_iter();

    let descriptor = matching_descriptors.next()?;
    if matching_descriptors.next().is_some() {
        return None;
    }
    if rendered_element_source_range_is_active(snapshot, selection, &descriptor.source_range) {
        return None;
    }

    Some(descriptor)
}
