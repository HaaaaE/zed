use std::ops::Range;

use gpui::{FontWeight, Pixels, px};
use markdown_wysiwyg::{MarkdownBlock, MarkdownInlineSpan, MarkdownProjectionMap};
use md_text::Point;

use crate::{rendered_element::RenderedElementDescriptor, rendered_index::DisplayItemId};

#[derive(Clone, Debug)]
pub struct DisplayRow {
    pub(crate) item_id: DisplayItemId,
    pub(crate) item_index: u32,
    pub row: u32,
    pub(crate) source_row_range: Range<usize>,
    pub text: String,
    pub(crate) source_text: String,
    pub(crate) source_range: Range<usize>,
    pub(crate) active_projection_source_ranges: Vec<Range<usize>>,
    pub(crate) markdown_blocks: Vec<MarkdownBlock>,
    pub(crate) heading_level: Option<u8>,
    pub(crate) rendered_indent_level: u16,
    pub(crate) inline_spans: Vec<MarkdownInlineSpan>,
    pub(crate) rendered_element_descriptors: Vec<RenderedElementDescriptor>,
    pub(crate) rendered_element_descriptors_have_document_path: bool,
    pub(crate) projection: MarkdownProjectionMap,
    pub(crate) insertions: Vec<DisplayInsertion>,
}

impl PartialEq for DisplayRow {
    fn eq(&self, other: &Self) -> bool {
        self.row == other.row
            && self.item_id == other.item_id
            && self.item_index == other.item_index
            && self.source_row_range == other.source_row_range
            && self.text == other.text
            && self.rendered_indent_level == other.rendered_indent_level
    }
}

impl Eq for DisplayRow {}

impl DisplayRow {
    pub(crate) fn contains_source_point(&self, point: Point) -> bool {
        self.source_row_range.contains(&(point.row as usize))
    }

    pub(crate) fn rendered_indent_width(&self) -> Pixels {
        px(f32::from(self.rendered_indent_level) * 24.)
    }

    pub(crate) fn source_to_display(&self, source_offset: usize) -> usize {
        let mut display_offset = self.projection.source_to_display(source_offset);
        for insertion in &self.insertions {
            if source_offset > insertion.source_range.start {
                display_offset += insertion.display_range.len();
            }
        }
        display_offset
    }

    pub(crate) fn display_to_source(&self, display_offset: usize) -> usize {
        let mut projected_offset = display_offset;
        for insertion in &self.insertions {
            if display_offset < insertion.display_range.start {
                break;
            }
            if display_offset == insertion.display_range.start {
                return insertion.source_range.start;
            }
            if display_offset <= insertion.display_range.end {
                return insertion.source_range.end;
            }
            projected_offset = projected_offset.saturating_sub(insertion.display_range.len());
        }
        self.projection.display_to_source(projected_offset)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisplayInsertion {
    pub(crate) source_range: Range<usize>,
    pub(crate) display_range: Range<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct DisplayTextStyle {
    pub(crate) color: Option<gpui::Hsla>,
    pub(crate) font_weight: Option<FontWeight>,
    pub(crate) text_background: Option<gpui::Hsla>,
    pub(crate) italic: bool,
    pub(crate) underline: bool,
    pub(crate) line_through: bool,
}

impl DisplayTextStyle {
    pub(crate) fn merge(mut self, overlay: &DisplayTextStyle) -> Self {
        if let Some(color) = overlay.color {
            self.color = Some(color);
        }
        if let Some(text_background) = overlay.text_background {
            self.text_background = Some(text_background);
        }
        self.font_weight = match (self.font_weight, overlay.font_weight) {
            (Some(current), Some(overlay)) => {
                Some(if current >= overlay { current } else { overlay })
            }
            (None, Some(overlay)) => Some(overlay),
            (current, None) => current,
        };
        self.italic |= overlay.italic;
        self.underline |= overlay.underline;
        self.line_through |= overlay.line_through;
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StyledDisplaySegment {
    pub(crate) display_range: Range<usize>,
    pub(crate) text: String,
    pub(crate) style: DisplayTextStyle,
}

impl StyledDisplaySegment {
    pub(crate) fn text_boundary_for_display_offset(&self, display_offset: usize) -> Option<usize> {
        if display_offset < self.display_range.start || display_offset > self.display_range.end {
            return None;
        }

        let local_offset = display_offset - self.display_range.start;
        Some(self.text.floor_char_boundary(local_offset))
    }
}
