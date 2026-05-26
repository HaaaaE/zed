use std::ops::Range;

use gpui::{
    App, ImgResourceLoader, IntoElement, LineFragment, Resource, SharedString, Window, div, img,
    prelude::*, px,
};
use md_assets::EDITOR_FONT_FAMILY;
use md_theme::editor_palette;

use super::{
    DisplayRow, DisplayTextStyle, RowDisplayStyle, StyledDisplaySegment, inline_style,
    rendered_element::{RenderedElementDescriptor, RenderedElementKind, RenderedElementPlacement},
};

pub(super) const INLINE_MATH_ATOM_EXTRA_HEIGHT: gpui::Pixels = px(4.);
pub(super) const INLINE_MATH_ATOM_HORIZONTAL_PADDING: gpui::Pixels = px(4.);
pub(super) const INLINE_IMAGE_ATOM_SIZE: gpui::Pixels = px(24.);
pub(super) const INLINE_IMAGE_ATOM_MAX_WIDTH: gpui::Pixels = px(96.);
pub(super) const INLINE_IMAGE_PLACEHOLDER: &str = "\u{fffc}";

#[derive(Clone, Debug, PartialEq)]
pub(super) enum DisplayInlineFragment {
    Text(StyledDisplaySegment),
    Atom(DisplayInlineAtom),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct DisplayInlineRowInputs {
    pub(super) style_ranges: Vec<(Range<usize>, DisplayTextStyle)>,
    pub(super) atom_ranges: Vec<DisplayInlineAtom>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct DisplayInlineAtom {
    pub(super) descriptor: RenderedElementDescriptor,
    pub(super) source_range: Range<usize>,
    pub(super) display_range: Range<usize>,
    pub(super) fallback_text: String,
    pub(super) image_url: Option<String>,
    pub(super) style: DisplayTextStyle,
    pub(super) height: gpui::Pixels,
    pub(super) width: gpui::Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct DisplayInlineAtomMeasurement {
    pub(super) size: gpui::Size<gpui::Pixels>,
    pub(super) cacheable: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DisplayInlineAtomKind {
    InlineMath,
    InlineImage,
}

impl DisplayInlineAtomKind {
    pub(super) fn for_descriptor(descriptor: &RenderedElementDescriptor) -> Option<Self> {
        if descriptor.placement != RenderedElementPlacement::Inline {
            return None;
        }

        match descriptor.kind {
            RenderedElementKind::Math { .. } => Some(Self::InlineMath),
            RenderedElementKind::Image { .. } => Some(Self::InlineImage),
            RenderedElementKind::Custom { .. } => None,
        }
    }

    pub(super) fn height(self, row_style: RowDisplayStyle) -> gpui::Pixels {
        match self {
            Self::InlineMath => row_style.line_height + INLINE_MATH_ATOM_EXTRA_HEIGHT,
            Self::InlineImage => INLINE_IMAGE_ATOM_SIZE,
        }
    }

    pub(super) fn width_for_content(self, content_width: gpui::Pixels) -> gpui::Pixels {
        match self {
            Self::InlineMath => content_width + self.horizontal_padding() * 2.,
            Self::InlineImage => INLINE_IMAGE_ATOM_SIZE,
        }
    }

    pub(super) fn horizontal_padding(self) -> gpui::Pixels {
        match self {
            Self::InlineMath => INLINE_MATH_ATOM_HORIZONTAL_PADDING,
            Self::InlineImage => px(0.),
        }
    }
}

impl DisplayInlineAtom {
    pub(super) fn kind(&self) -> DisplayInlineAtomKind {
        DisplayInlineAtomKind::for_descriptor(&self.descriptor)
            .expect("inline atom descriptor must map to an atom kind")
    }

    pub(super) fn fallback_size(
        &self,
        shaped_line: &gpui::ShapedLine,
        row_style: RowDisplayStyle,
    ) -> gpui::Size<gpui::Pixels> {
        let content_width = self.fallback_content_width(shaped_line);
        gpui::size(
            self.kind().width_for_content(content_width),
            self.kind().height(row_style),
        )
    }

    fn fallback_content_width(&self, shaped_line: &gpui::ShapedLine) -> gpui::Pixels {
        let start_x = shaped_line.x_for_index(self.display_range.start);
        let end_x = shaped_line.x_for_index(self.display_range.end);
        (end_x - start_x).max(px(1.))
    }

    pub(super) fn measure_size(
        &self,
        fallback_size: gpui::Size<gpui::Pixels>,
        row_style: RowDisplayStyle,
        window: &mut Window,
        cx: &mut App,
    ) -> DisplayInlineAtomMeasurement {
        if self.kind() == DisplayInlineAtomKind::InlineImage {
            return self.measure_inline_image_size(fallback_size, window, cx);
        }

        let mut element = self.render_measurement_piece(self.fallback_text.clone(), row_style);
        let size = element.layout_as_root(
            gpui::size(
                gpui::AvailableSpace::MaxContent,
                gpui::AvailableSpace::MaxContent,
            ),
            window,
            cx,
        );

        DisplayInlineAtomMeasurement {
            size: gpui::size(
                size.width.max(fallback_size.width).max(px(1.)),
                size.height.max(fallback_size.height).max(px(1.)),
            ),
            cacheable: true,
        }
    }

    fn measure_inline_image_size(
        &self,
        fallback_size: gpui::Size<gpui::Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> DisplayInlineAtomMeasurement {
        let Some(image_url) = self.image_url.as_ref() else {
            return DisplayInlineAtomMeasurement {
                size: fallback_size,
                cacheable: true,
            };
        };

        let resource = Resource::Uri(image_url.clone().into());
        let Some(image) = window.use_asset::<ImgResourceLoader>(&resource, cx) else {
            return DisplayInlineAtomMeasurement {
                size: fallback_size,
                cacheable: false,
            };
        };

        let size = image
            .ok()
            .and_then(|image| {
                let size = image.size(0);
                inline_image_atom_size_for_size(size.width.0, size.height.0)
            })
            .unwrap_or(fallback_size);

        DisplayInlineAtomMeasurement {
            size,
            cacheable: true,
        }
    }

    pub(super) fn render_piece(
        &self,
        text: String,
        row_style: RowDisplayStyle,
        selected: bool,
    ) -> gpui::AnyElement {
        self.render_element(
            text,
            row_style,
            Some(gpui::size(self.width, self.height)),
            selected,
        )
    }

    fn render_measurement_piece(
        &self,
        text: String,
        row_style: RowDisplayStyle,
    ) -> gpui::AnyElement {
        self.render_element(text, row_style, None, false)
    }

    fn render_element(
        &self,
        text: String,
        row_style: RowDisplayStyle,
        size: Option<gpui::Size<gpui::Pixels>>,
        selected: bool,
    ) -> gpui::AnyElement {
        match self.kind() {
            DisplayInlineAtomKind::InlineMath => {
                let palette = editor_palette();
                let mut style = self.style.clone();
                if selected {
                    style.color = Some(palette.selection_text);
                }
                let mut element = div()
                    .min_h(self.height)
                    .px(self.kind().horizontal_padding())
                    .flex()
                    .items_center()
                    .font_family(EDITOR_FONT_FAMILY)
                    .text_size(row_style.text_size)
                    .line_height(row_style.line_height)
                    .whitespace_nowrap()
                    .rounded_sm()
                    .bg(if selected {
                        palette.selection_background
                    } else {
                        palette.inline_math_text.opacity(0.08)
                    })
                    .child(render_text_piece(text, &style));
                if let Some(size) = size {
                    element = element.w(size.width).h(size.height);
                }
                element.into_any_element()
            }
            DisplayInlineAtomKind::InlineImage => {
                let palette = editor_palette();
                let image_url = self.image_url.clone().unwrap_or(text);
                let size = size.unwrap_or_else(|| gpui::size(self.width, self.height));
                let mut element = div()
                    .w(size.width)
                    .h(size.height)
                    .flex_none()
                    .overflow_hidden()
                    .rounded_sm()
                    .border_1()
                    .border_color(if selected {
                        palette.selection_background
                    } else {
                        palette.gutter_text.opacity(0.45)
                    })
                    .child(img(image_url).size_full().with_fallback(|| {
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(editor_palette().muted_text)
                            .child("img")
                            .into_any_element()
                    }));
                if selected {
                    element = element.bg(palette.selection_background.opacity(0.32));
                }
                element.into_any_element()
            }
        }
    }

    pub(super) fn is_selected(&self, selected_range: Option<&Range<usize>>) -> bool {
        selected_range.is_some_and(|selected_range| {
            selected_range.start <= self.display_range.start
                && selected_range.end >= self.display_range.end
        })
    }

    pub(super) fn push_line_fragment<'a>(
        &self,
        display_text: &'a str,
        line_fragments: &mut Vec<LineFragment<'a>>,
    ) -> Option<()> {
        let text = display_text.get(self.display_range.clone())?;
        if !text.is_empty() {
            line_fragments.push(LineFragment::element(self.width.max(px(1.)), text.len()));
        }
        Some(())
    }

    pub(super) fn contains_display_index(&self, display_index: usize) -> bool {
        self.display_range.start < display_index && display_index < self.display_range.end
    }

    pub(super) fn boundary_for_x(
        &self,
        atom_start_x: gpui::Pixels,
        atom_end_x: gpui::Pixels,
        display_x: gpui::Pixels,
    ) -> usize {
        let midpoint = atom_start_x + (atom_end_x - atom_start_x) / 2.;
        if display_x < midpoint {
            self.display_range.start
        } else {
            self.display_range.end
        }
    }

    pub(super) fn from_descriptor(
        display_row: &DisplayRow,
        descriptor: RenderedElementDescriptor,
        row_style: RowDisplayStyle,
    ) -> Option<Self> {
        let kind = DisplayInlineAtomKind::for_descriptor(&descriptor)?;
        let display_range = display_row.source_to_display(descriptor.source_range.start)
            ..display_row.source_to_display(descriptor.source_range.end);
        let fallback_text = display_row.text.get(display_range.clone())?.to_string();
        if fallback_text.is_empty() {
            return None;
        }
        let image_url = match &descriptor.kind {
            RenderedElementKind::Image { url, .. } => Some(url.clone()),
            RenderedElementKind::Math { .. } | RenderedElementKind::Custom { .. } => None,
        };
        let style = inline_style(match kind {
            DisplayInlineAtomKind::InlineMath => markdown_wysiwyg::MarkdownInlineKind::InlineMath,
            DisplayInlineAtomKind::InlineImage => markdown_wysiwyg::MarkdownInlineKind::Image,
        });

        Some(Self {
            source_range: descriptor.source_range.clone(),
            descriptor,
            display_range,
            fallback_text,
            image_url,
            style,
            height: kind.height(row_style),
            width: px(0.),
        })
    }
}

pub(super) fn inline_image_atom_size_for_size(
    image_width: i32,
    image_height: i32,
) -> Option<gpui::Size<gpui::Pixels>> {
    if image_width <= 0 || image_height <= 0 {
        return None;
    }

    let aspect_ratio = image_width as f32 / image_height as f32;
    let width_at_default_height = INLINE_IMAGE_ATOM_SIZE * aspect_ratio;
    if width_at_default_height <= INLINE_IMAGE_ATOM_MAX_WIDTH {
        return Some(gpui::size(
            width_at_default_height.max(px(1.)),
            INLINE_IMAGE_ATOM_SIZE,
        ));
    }

    Some(gpui::size(
        INLINE_IMAGE_ATOM_MAX_WIDTH,
        (INLINE_IMAGE_ATOM_MAX_WIDTH / aspect_ratio).max(px(1.)),
    ))
}

pub(super) fn render_text_piece(text: String, style: &DisplayTextStyle) -> gpui::AnyElement {
    let mut element = div().child(SharedString::from(text));

    if let Some(color) = style.color {
        element = element.text_color(color);
    }
    if let Some(weight) = style.font_weight {
        element = element.font_weight(weight);
    }
    if let Some(text_background) = style.text_background {
        element = element.text_bg(text_background);
    }
    if style.italic {
        element = element.italic();
    }
    if style.underline {
        element = element.underline();
        if let Some(color) = style.color {
            element = element.text_decoration_color(color);
        }
    }
    if style.line_through {
        element = element.line_through();
    }

    element.into_any_element()
}
