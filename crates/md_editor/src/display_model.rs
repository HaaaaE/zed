use std::ops::Range;

use gpui::FontWeight;
use markdown_wysiwyg::{MarkdownInlineSpan, MarkdownProjectionMap};

#[derive(Clone, Debug)]
pub struct DisplayRow {
    pub row: u32,
    pub text: String,
    pub(crate) source_text: String,
    pub(crate) source_range: Range<usize>,
    pub(crate) active_projection_source_ranges: Vec<Range<usize>>,
    pub(crate) heading_level: Option<u8>,
    pub(crate) inline_spans: Vec<MarkdownInlineSpan>,
    pub(crate) projection: MarkdownProjectionMap,
    pub(crate) insertions: Vec<DisplayInsertion>,
}

impl PartialEq for DisplayRow {
    fn eq(&self, other: &Self) -> bool {
        self.row == other.row && self.text == other.text
    }
}

impl Eq for DisplayRow {}

impl DisplayRow {
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
