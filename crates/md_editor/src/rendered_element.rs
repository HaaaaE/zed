use std::ops::Range;

use markdown_wysiwyg::{MarkdownInlineKind, MarkdownInlineSpan};
use md_buffer::BufferSnapshot;
use md_text::{Point, Selection};

use super::{
    HorizontalDirection, clip_cursor, clip_selection, is_remote_image_url, range_contains,
    ranges_overlap, row_source_range, row_text, selection_byte_range,
};

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
            let source_range = rendered_element_source_range_for_span(snapshot, span)?;
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
        let selection_range = selection_byte_range(snapshot, &selection);
        if selection_range_is_whole_rendered_element(snapshot, &selection_range) {
            return None;
        }
        return Some(selection_range);
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

fn selection_range_is_whole_rendered_element(
    snapshot: &BufferSnapshot,
    selection_range: &Range<usize>,
) -> bool {
    snapshot
        .syntax_tree()
        .inline_spans_in_source_range(selection_range.clone())
        .any(|span| {
            rendered_element_source_range_for_span(snapshot, span)
                .is_some_and(|source_range| &source_range == selection_range)
        })
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
        .filter_map(|span| rendered_element_source_range_for_span(snapshot, span))
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
                    rendered_element_source_range_for_span(snapshot, span).is_some_and(
                        |source_range| {
                            source_range.start == source_offset || source_range.end == source_offset
                        },
                    )
                })
        })
}

fn rendered_element_source_range_for_span(
    snapshot: &BufferSnapshot,
    span: &MarkdownInlineSpan,
) -> Option<Range<usize>> {
    if span.marker_ranges.is_empty() {
        return None;
    }

    match span.kind {
        MarkdownInlineKind::InlineMath => Some(span.source_range.clone()),
        MarkdownInlineKind::Image if rendered_remote_image_span_is_block(snapshot, span) => {
            Some(span.source_range.clone())
        }
        MarkdownInlineKind::Image
        | MarkdownInlineKind::Emphasis
        | MarkdownInlineKind::Strong
        | MarkdownInlineKind::InlineCode
        | MarkdownInlineKind::Link
        | MarkdownInlineKind::Strikethrough => None,
    }
}

fn rendered_remote_image_span_is_block(
    snapshot: &BufferSnapshot,
    span: &MarkdownInlineSpan,
) -> bool {
    let row = snapshot
        .as_text_snapshot()
        .offset_to_point(span.source_range.start)
        .row;
    let row_source_range = row_source_range(snapshot, row);
    let source_text = row_text(snapshot, row);

    rendered_remote_image_span_is_block_in_row(span, &source_text, &row_source_range)
}

pub(super) fn rendered_remote_image_span_is_block_in_row(
    span: &MarkdownInlineSpan,
    source_text: &str,
    row_source_range: &Range<usize>,
) -> bool {
    if !span
        .url
        .as_ref()
        .is_some_and(|url| is_remote_image_url(url))
        || !range_contains(row_source_range, &span.source_range)
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
