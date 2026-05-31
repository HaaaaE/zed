use std::{ops::Range, path::Path};

use markdown_wysiwyg::{MarkdownInlineKind, MarkdownInlineSpan, MarkdownProjectionOperation};
use md_buffer::BufferSnapshot;
use md_text::{Point, Selection, SelectionGoal};

use super::{
    clip_cursor, clip_selection, display_row_builder::row_source_range,
    markdown_image::MarkdownImageSource, range_contains, ranges_overlap, row_text,
    selection::HorizontalDirection, selection_byte_range,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum RenderedElementKind {
    Image {
        image_source: MarkdownImageSource,
    },
    Math {
        tex: String,
    },
    #[allow(dead_code)]
    Custom {
        key: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum RenderedElementPlacement {
    Inline,
    Block,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct RenderedElementDescriptor {
    pub(super) kind: RenderedElementKind,
    pub(super) placement: RenderedElementPlacement,
    pub(super) source_range: Range<usize>,
}

pub(super) fn rendered_element_descriptor_for_inline_span_in_row(
    span: &MarkdownInlineSpan,
    source_text: &str,
    row_source_range: &Range<usize>,
    document_path: Option<&Path>,
) -> Option<RenderedElementDescriptor> {
    if span.marker_ranges.is_empty() || !range_contains(row_source_range, &span.source_range) {
        return None;
    }

    match span.kind {
        MarkdownInlineKind::InlineMath => Some(RenderedElementDescriptor {
            kind: RenderedElementKind::Math {
                tex: inline_span_content_text(span, source_text, row_source_range),
            },
            placement: if rendered_math_span_is_block_in_row(span, source_text, row_source_range) {
                RenderedElementPlacement::Block
            } else {
                RenderedElementPlacement::Inline
            },
            source_range: span.source_range.clone(),
        }),
        MarkdownInlineKind::Image => {
            let image_source = MarkdownImageSource::resolve(
                span.url.clone()?,
                inline_span_content_text(span, source_text, row_source_range),
                document_path,
            );
            let placement = if rendered_image_span_is_block_in_row(
                span,
                source_text,
                row_source_range,
                &image_source,
            ) {
                RenderedElementPlacement::Block
            } else {
                RenderedElementPlacement::Inline
            };
            Some(RenderedElementDescriptor {
                kind: RenderedElementKind::Image { image_source },
                placement,
                source_range: span.source_range.clone(),
            })
        }
        MarkdownInlineKind::Emphasis
        | MarkdownInlineKind::Strong
        | MarkdownInlineKind::InlineCode
        | MarkdownInlineKind::Link
        | MarkdownInlineKind::Strikethrough
        | MarkdownInlineKind::Escape
        | MarkdownInlineKind::Entity
        | MarkdownInlineKind::HardBreak
        | MarkdownInlineKind::SoftBreak
        | MarkdownInlineKind::InlineHtml => None,
    }
}

fn inline_span_content_text(
    span: &MarkdownInlineSpan,
    source_text: &str,
    row_source_range: &Range<usize>,
) -> String {
    let mut text = String::new();
    for content_range in &span.content_ranges {
        let start = content_range.start.max(row_source_range.start);
        let end = content_range.end.min(row_source_range.end);
        if start >= end {
            continue;
        }

        let local_start = start - row_source_range.start;
        let local_end = end - row_source_range.start;
        if let Some(content) = source_text.get(local_start..local_end) {
            text.push_str(content);
        }
    }
    text
}

pub(super) fn rendered_element_range_at_cursor(
    snapshot: &BufferSnapshot,
    cursor: Point,
    direction: HorizontalDirection,
) -> Option<Range<usize>> {
    let cursor = clip_cursor(snapshot, cursor);
    let source_offset = snapshot.as_text_snapshot().point_to_offset(cursor);
    snapshot
        .syntax_tree()
        .inline_spans_in_source_range(rendered_element_boundary_query_range(
            snapshot,
            source_offset,
            direction,
        )?)
        .find_map(|span| {
            let source_range = rendered_element_descriptor_for_span(snapshot, span)?.source_range;
            match direction {
                HorizontalDirection::Left if source_offset == source_range.end => {
                    Some(source_range)
                }
                HorizontalDirection::Right if source_offset == source_range.start => {
                    Some(source_range)
                }
                _ => None,
            }
        })
}

pub(super) fn projection_replacement_range_at_cursor(
    snapshot: &BufferSnapshot,
    cursor: Point,
    direction: HorizontalDirection,
) -> Option<Range<usize>> {
    let cursor = clip_cursor(snapshot, cursor);
    let source_offset = snapshot.as_text_snapshot().point_to_offset(cursor);
    let row_source_range = row_source_range(snapshot, cursor.row);
    if source_offset < row_source_range.start || source_offset > row_source_range.end {
        return None;
    }

    let active_source_range = active_source_range_for_selection(
        snapshot,
        &Selection {
            id: 0,
            start: cursor,
            end: cursor,
            reversed: false,
            goal: SelectionGoal::None,
        },
    );
    let projection = snapshot
        .syntax_tree()
        .projection_for_source_range(row_source_range, active_source_range);

    projection.operations().iter().find_map(|operation| {
        let MarkdownProjectionOperation::Replace { source_range, .. } = operation else {
            return None;
        };
        match direction {
            HorizontalDirection::Left if source_offset == source_range.end => {
                Some(source_range.clone())
            }
            HorizontalDirection::Right if source_offset == source_range.start => {
                Some(source_range.clone())
            }
            _ => None,
        }
    })
}

fn rendered_element_boundary_query_range(
    snapshot: &BufferSnapshot,
    source_offset: usize,
    direction: HorizontalDirection,
) -> Option<Range<usize>> {
    let text_snapshot = snapshot.as_text_snapshot();
    match direction {
        HorizontalDirection::Left => {
            if source_offset == 0 {
                return None;
            }
            let start = text_snapshot
                .as_rope()
                .floor_char_boundary(source_offset.saturating_sub(1));
            Some(start..source_offset)
        }
        HorizontalDirection::Right => {
            if source_offset >= text_snapshot.len() {
                return None;
            }
            let end = text_snapshot
                .as_rope()
                .ceil_char_boundary(source_offset.saturating_add(1));
            Some(source_offset..end)
        }
    }
}

pub(super) fn active_source_range_for_selection(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Option<Range<usize>> {
    let selection = clip_selection(snapshot, selection);
    if !selection.is_empty() {
        return None;
    }

    let text_snapshot = snapshot.as_text_snapshot();
    if text_snapshot.len() == 0 {
        return None;
    }

    let offset = text_snapshot.point_to_offset(selection.head());
    if source_offset_is_rendered_element_boundary(snapshot, offset) {
        return None;
    }
    if offset < text_snapshot.len() {
        let end = text_snapshot
            .as_rope()
            .ceil_char_boundary(offset.saturating_add(1));
        Some(offset..end)
    } else {
        let start = text_snapshot
            .as_rope()
            .floor_char_boundary(offset.saturating_sub(1));
        Some(start..offset)
    }
}

pub(super) fn inactive_rendered_element_source_ranges_for_selection(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Vec<Range<usize>> {
    let selection = clip_selection(snapshot, selection);
    if selection.is_empty() {
        return Vec::new();
    }

    let selection_range = selection_byte_range(snapshot, &selection);
    snapshot
        .syntax_tree()
        .inline_spans_in_source_range(selection_range.clone())
        .filter_map(|span| {
            rendered_element_descriptor_for_span(snapshot, span)
                .map(|descriptor| descriptor.source_range)
        })
        .filter(|source_range| range_contains(&selection_range, source_range))
        .collect()
}

pub(super) fn rendered_element_source_range_is_active(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    source_range: &Range<usize>,
) -> bool {
    let Some(active_source_range) = active_source_range_for_selection(snapshot, selection) else {
        return false;
    };
    if !ranges_overlap(source_range, &active_source_range) {
        return false;
    }

    !inactive_rendered_element_source_ranges_for_selection(snapshot, selection)
        .iter()
        .any(|inactive_source_range| range_contains(inactive_source_range, source_range))
}

pub(super) fn source_offset_is_rendered_element_boundary(
    snapshot: &BufferSnapshot,
    source_offset: usize,
) -> bool {
    [HorizontalDirection::Left, HorizontalDirection::Right]
        .into_iter()
        .filter_map(|direction| {
            rendered_element_boundary_query_range(snapshot, source_offset, direction)
        })
        .any(|source_range| {
            snapshot
                .syntax_tree()
                .inline_spans_in_source_range(source_range)
                .any(|span| {
                    rendered_element_descriptor_for_span(snapshot, span).is_some_and(|descriptor| {
                        descriptor.source_range.start == source_offset
                            || descriptor.source_range.end == source_offset
                    })
                })
        })
}

fn rendered_element_descriptor_for_span(
    snapshot: &BufferSnapshot,
    span: &MarkdownInlineSpan,
) -> Option<RenderedElementDescriptor> {
    let row = snapshot
        .as_text_snapshot()
        .offset_to_point(span.source_range.start)
        .row;
    let row_source_range = row_source_range(snapshot, row);
    let source_text = row_text(snapshot, row);

    rendered_element_descriptor_for_inline_span_in_row(span, &source_text, &row_source_range, None)
}

pub(super) fn rendered_image_span_is_block_in_row(
    span: &MarkdownInlineSpan,
    source_text: &str,
    row_source_range: &Range<usize>,
    image_source: &MarkdownImageSource,
) -> bool {
    if !image_source.is_renderable() || !range_contains(row_source_range, &span.source_range) {
        return false;
    }

    let local_start = span.source_range.start - row_source_range.start;
    let local_end = span.source_range.end - row_source_range.start;
    let Some(before) = source_text.get(..local_start) else {
        return false;
    };
    let Some(after) = source_text.get(local_end..) else {
        return false;
    };

    before.trim().is_empty() && after.trim().is_empty()
}

fn rendered_math_span_is_block_in_row(
    span: &MarkdownInlineSpan,
    source_text: &str,
    row_source_range: &Range<usize>,
) -> bool {
    if !range_contains(row_source_range, &span.source_range) {
        return false;
    }

    if span.marker_ranges.len() != 2 {
        return false;
    }

    let Some(start_marker) = span.marker_ranges.first() else {
        return false;
    };
    let Some(end_marker) = span.marker_ranges.last() else {
        return false;
    };
    if start_marker.end.saturating_sub(start_marker.start) != 2
        || end_marker.end.saturating_sub(end_marker.start) != 2
    {
        return false;
    }

    let local_start = span.source_range.start - row_source_range.start;
    let local_end = span.source_range.end - row_source_range.start;
    let Some(before) = source_text.get(..local_start) else {
        return false;
    };
    let Some(after) = source_text.get(local_end..) else {
        return false;
    };

    before.trim().is_empty() && after.trim().is_empty()
}
