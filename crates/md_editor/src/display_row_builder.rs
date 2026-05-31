#[cfg(test)]
use std::sync::Arc;
use std::{ops::Range, path::Path};

use markdown_wysiwyg::{
    MarkdownBlock, MarkdownBlockKind, MarkdownInlineKind, MarkdownInlineSpan,
    MarkdownProjectionMap, MarkdownRangeSemantics,
};
use md_buffer::BufferSnapshot;
#[cfg(test)]
use md_text::Selection;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point};

#[cfg(test)]
use crate::rendered_index::RenderedDisplayIndex;
use crate::{
    MarkdownEditorMode,
    display_model::{DisplayInsertion, DisplayRow},
    inline_atom::INLINE_IMAGE_PLACEHOLDER,
    layout::DisplayRowProjectionState,
    range_contains, ranges_overlap,
    rendered_element::{
        RenderedElementDescriptor, RenderedElementPlacement,
        rendered_element_descriptor_for_inline_span_in_row,
    },
    rendered_index::{self, DisplayItemId, RenderedDisplayItem},
};

pub fn display_rows(snapshot: &BufferSnapshot, range: Range<usize>) -> Vec<DisplayRow> {
    display_rows_in_text_snapshot(snapshot.as_text_snapshot(), range)
}

pub(crate) fn display_rows_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    range: Range<usize>,
) -> Vec<DisplayRow> {
    let row_count = snapshot.row_count() as usize;
    let start = range.start.min(row_count);
    let end = range.end.min(row_count);
    (start..end)
        .map(|row| source_display_row_in_text_snapshot(snapshot, row as u32))
        .collect()
}

#[cfg(test)]
pub(crate) fn display_rows_in_mode(
    snapshot: &BufferSnapshot,
    range: Range<usize>,
    selection: Option<&Selection<Point>>,
    mode: MarkdownEditorMode,
) -> Vec<DisplayRow> {
    if mode == MarkdownEditorMode::Source {
        return display_rows_in_text_snapshot(snapshot.as_text_snapshot(), range);
    }

    let row_count = snapshot.row_count() as usize;
    let start = range.start.min(row_count);
    let end = range.end.min(row_count);
    let display_row_state = DisplayRowProjectionState::new(snapshot, selection, mode);
    (start..end)
        .map(|row| {
            let row = row as u32;
            let source_range = row_source_range(snapshot, row);
            let range_semantics = snapshot.syntax_tree().range_semantics_for_source_range(
                source_range.clone(),
                display_row_state.active_source_range.clone(),
                &display_row_state.inactive_source_ranges,
            );
            rendered_display_row(
                snapshot,
                rendered_index::source_display_item_id(&source_range, row as usize),
                row,
                row,
                source_range,
                row as usize..row as usize + 1,
                range_semantics,
                None,
            )
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn rendered_display_index_for_tests(
    snapshot: &BufferSnapshot,
) -> Arc<RenderedDisplayIndex> {
    RenderedDisplayIndex::build(snapshot)
}

pub(crate) fn rendered_item_display_source_range(
    snapshot: &BufferSnapshot,
    item: &RenderedDisplayItem,
    display_row_state: &DisplayRowProjectionState,
    active_cursor_maps_to_item: bool,
) -> (u32, Range<usize>, Range<usize>) {
    let mut source_range = item.source_range.clone();
    let mut source_row_range = item.row_range.clone();
    if active_cursor_maps_to_item
        && matches!(
            item.kind,
            rendered_index::RenderedDisplayItemKind::Paragraph
                | rendered_index::RenderedDisplayItemKind::Heading
        )
        && let Some(cursor) = display_row_state.active_cursor
        && cursor.row as usize >= item.row_range.end
    {
        let text_snapshot = snapshot.as_text_snapshot();
        let cursor_offset = text_snapshot.point_to_offset(cursor);
        let cursor_at_trailing_blank_tail = text_snapshot
            .text_for_range(cursor_offset..text_snapshot.len())
            .all(|chunk| chunk.chars().all(is_line_break_char));
        let active_trailing_break =
            display_row_state
                .active_source_range
                .as_ref()
                .is_some_and(|range| {
                    range.start == item.source_range.end && range.end == cursor_offset
                })
                || cursor.row as usize == item.row_range.end
                    && cursor_starts_multi_blank_run(text_snapshot, cursor.row as usize)
                || cursor_at_trailing_blank_tail;
        if cursor_offset > item.source_range.end
            && active_trailing_break
            && text_snapshot
                .text_for_range(item.source_range.end..cursor_offset)
                .all(|chunk| chunk.chars().all(is_line_break_char))
        {
            source_range.end = cursor_offset;
            source_row_range.end = cursor.row as usize + 1;
        }
    }

    (item.row_range.start as u32, source_range, source_row_range)
}

#[cfg(test)]
pub(crate) fn rendered_display_row_for_item_for_tests(
    snapshot: &BufferSnapshot,
    item_index: usize,
    selection: Option<&Selection<Point>>,
) -> DisplayRow {
    let index = RenderedDisplayIndex::build(snapshot);
    let item = index.item(item_index).expect("item should exist");
    let display_row_state =
        DisplayRowProjectionState::new(snapshot, selection, MarkdownEditorMode::Rendered);
    let active_cursor_maps_to_item = display_row_state.active_cursor.is_some_and(|cursor| {
        index.item_index_for_source_row(cursor.row as usize) == Some(item_index)
    });
    let (row, source_range, source_row_range) = rendered_item_display_source_range(
        snapshot,
        item,
        &display_row_state,
        active_cursor_maps_to_item,
    );
    let range_semantics = snapshot.syntax_tree().range_semantics_for_source_range(
        source_range.clone(),
        display_row_state.active_source_range.clone(),
        &display_row_state.inactive_source_ranges,
    );

    rendered_display_row(
        snapshot,
        item.id,
        item.index as u32,
        row,
        source_range,
        source_row_range,
        range_semantics,
        None,
    )
}

pub(crate) fn rendered_display_row(
    snapshot: &BufferSnapshot,
    item_id: DisplayItemId,
    item_index: u32,
    row: u32,
    source_range: Range<usize>,
    source_row_range: Range<usize>,
    range_semantics: MarkdownRangeSemantics,
    document_path: Option<&Path>,
) -> DisplayRow {
    let source_text: String = snapshot
        .as_text_snapshot()
        .text_for_range(source_range.clone())
        .collect();
    let MarkdownRangeSemantics {
        blocks: markdown_blocks,
        inline_spans,
        projection,
        active_projection_source_ranges,
        rendered_element_candidates,
    } = range_semantics;

    let rendered_element_descriptors = rendered_element_descriptors_for_display_row(
        &rendered_element_candidates,
        &source_text,
        &source_range,
        document_path,
    );
    let (text, insertions) = project_display_row_text(
        &source_text,
        &source_range,
        &projection,
        &inline_spans,
        &rendered_element_descriptors,
        MarkdownEditorMode::Rendered,
    );
    let heading_level = heading_level_for_display_row(&markdown_blocks, row);
    let rendered_indent_level = rendered_indent_level_for_display_row(&markdown_blocks, row);
    DisplayRow {
        item_id,
        item_index,
        row,
        source_row_range,
        text,
        source_text,
        source_range,
        active_projection_source_ranges,
        markdown_blocks,
        heading_level,
        rendered_indent_level,
        inline_spans,
        rendered_element_descriptors,
        rendered_element_descriptors_have_document_path: document_path.is_some(),
        projection,
        insertions,
    }
}

pub fn row_text(snapshot: &BufferSnapshot, row: u32) -> String {
    row_text_in_text_snapshot(snapshot.as_text_snapshot(), row)
}

pub(crate) fn row_text_in_text_snapshot(snapshot: &TextBufferSnapshot, row: u32) -> String {
    if row >= snapshot.row_count() {
        return String::new();
    }

    let start = snapshot.point_to_offset(Point::new(row, 0));
    let end = start + snapshot.line_len(row) as usize;
    snapshot.text_for_range(start..end).collect()
}

pub(crate) fn source_display_row_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    row: u32,
) -> DisplayRow {
    let source_range = row_source_range_in_text_snapshot(snapshot, row);
    let source_text: String = snapshot.text_for_range(source_range.clone()).collect();
    let projection = MarkdownProjectionMap::new(snapshot.len(), source_range.clone(), Vec::new());
    DisplayRow {
        item_id: rendered_index::source_display_item_id(&source_range, row as usize),
        item_index: row,
        row,
        source_row_range: row as usize..row as usize + 1,
        text: source_text.clone(),
        source_text,
        source_range,
        active_projection_source_ranges: Vec::new(),
        markdown_blocks: Vec::new(),
        heading_level: None,
        rendered_indent_level: 0,
        inline_spans: Vec::new(),
        rendered_element_descriptors: Vec::new(),
        rendered_element_descriptors_have_document_path: false,
        projection,
        insertions: Vec::new(),
    }
}

fn rendered_indent_level_for_display_row(markdown_blocks: &[MarkdownBlock], row: u32) -> u16 {
    markdown_blocks
        .iter()
        .filter(|block| {
            matches!(
                block.kind,
                MarkdownBlockKind::BlockQuote
                    | MarkdownBlockKind::ListItem
                    | MarkdownBlockKind::TaskListItem { .. }
            ) && block.row_range.contains(&(row as usize))
        })
        .count()
        .try_into()
        .unwrap_or(u16::MAX)
}

fn heading_level_for_display_row(markdown_blocks: &[MarkdownBlock], row: u32) -> Option<u8> {
    markdown_blocks.iter().find_map(|block| match block.kind {
        MarkdownBlockKind::AtxHeading { level } | MarkdownBlockKind::SetextHeading { level }
            if block.row_range.start == row as usize =>
        {
            Some(level)
        }
        _ => None,
    })
}

fn rendered_element_descriptors_for_display_row(
    rendered_element_candidates: &[MarkdownInlineSpan],
    source_text: &str,
    row_source_range: &Range<usize>,
    document_path: Option<&Path>,
) -> Vec<RenderedElementDescriptor> {
    rendered_element_candidates
        .iter()
        .filter_map(|span| {
            rendered_element_descriptor_for_inline_span_in_row(
                span,
                source_text,
                row_source_range,
                document_path,
            )
        })
        .collect()
}

pub(crate) fn row_source_range(snapshot: &BufferSnapshot, row: u32) -> Range<usize> {
    row_source_range_in_text_snapshot(snapshot.as_text_snapshot(), row)
}

pub(crate) fn row_source_range_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    row: u32,
) -> Range<usize> {
    if row >= snapshot.row_count() {
        let end = snapshot.len();
        return end..end;
    }

    let start = snapshot.point_to_offset(Point::new(row, 0));
    let end = start + snapshot.line_len(row) as usize;
    start..end
}

fn cursor_starts_multi_blank_run(snapshot: &TextBufferSnapshot, row: usize) -> bool {
    let row_count = snapshot.row_count() as usize;
    row.saturating_add(1) < row_count
        && source_row_is_blank_in_text_snapshot(snapshot, row)
        && source_row_is_blank_in_text_snapshot(snapshot, row.saturating_add(1))
}

fn source_row_is_blank_in_text_snapshot(snapshot: &TextBufferSnapshot, row: usize) -> bool {
    snapshot
        .text_for_range(row_source_range_in_text_snapshot(snapshot, row as u32))
        .all(|chunk| chunk.trim().is_empty())
}

fn project_display_row_text(
    source_text: &str,
    row_source_range: &Range<usize>,
    projection: &MarkdownProjectionMap,
    inline_spans: &[MarkdownInlineSpan],
    rendered_element_descriptors: &[RenderedElementDescriptor],
    mode: MarkdownEditorMode,
) -> (String, Vec<DisplayInsertion>) {
    if mode != MarkdownEditorMode::Rendered {
        return (project_row_text(source_text, projection), Vec::new());
    }

    let mut display_text = project_rendered_row_text(source_text, row_source_range, projection);
    restore_rendered_soft_breaks(
        &mut display_text,
        row_source_range,
        inline_spans,
        projection,
    );
    restore_rendered_trailing_soft_break(&mut display_text, source_text);
    let mut insertions = Vec::new();
    for span in inline_spans {
        let descriptor = rendered_element_descriptors
            .iter()
            .find(|descriptor| descriptor.source_range == span.source_range);
        if span.kind != MarkdownInlineKind::Image
            || descriptor
                .is_some_and(|descriptor| descriptor.placement == RenderedElementPlacement::Block)
            || !range_contains(&row_source_range, &span.source_range)
            || !span.marker_ranges.iter().any(|marker_range| {
                projection
                    .hidden_ranges()
                    .iter()
                    .any(|hidden_range| ranges_overlap(marker_range, hidden_range))
            })
        {
            continue;
        }

        let display_start = projection.source_to_display(span.source_range.start);
        let display_end = projection.source_to_display(span.source_range.end);
        if display_start != display_end {
            continue;
        }

        let inserted_len = insertions
            .iter()
            .filter(|insertion: &&DisplayInsertion| {
                span.source_range.start > insertion.source_range.start
            })
            .map(|insertion| insertion.display_range.len())
            .sum::<usize>();
        let display_start = display_start + inserted_len;
        display_text.insert_str(display_start, INLINE_IMAGE_PLACEHOLDER);
        insertions.push(DisplayInsertion {
            source_range: span.source_range.clone(),
            display_range: display_start..display_start + INLINE_IMAGE_PLACEHOLDER.len(),
        });
    }

    (display_text, insertions)
}

fn restore_rendered_trailing_soft_break(display_text: &mut String, source_text: &str) {
    let trailing_line_break_count = trailing_line_break_count(source_text);
    if trailing_line_break_count == 0 {
        return;
    }

    let trailing_spaces = " ".repeat(trailing_line_break_count);
    if !display_text.ends_with(&trailing_spaces) {
        return;
    }

    let trailing_space_start = display_text.len().saturating_sub(trailing_line_break_count);
    display_text.replace_range(
        trailing_space_start..,
        &"\n".repeat(trailing_line_break_count),
    );
}

fn trailing_line_break_count(text: &str) -> usize {
    let mut count = 0;
    let mut end = text.len();
    while end > 0 {
        if text[..end].ends_with("\r\n") {
            count += 1;
            end = end.saturating_sub("\r\n".len());
        } else if text[..end].ends_with(['\n', '\r']) {
            count += 1;
            end = end.saturating_sub('\n'.len_utf8());
        } else {
            break;
        }
    }
    count
}

fn is_line_break_char(ch: char) -> bool {
    matches!(ch, '\n' | '\r')
}

fn restore_rendered_soft_breaks(
    display_text: &mut String,
    row_source_range: &Range<usize>,
    inline_spans: &[MarkdownInlineSpan],
    projection: &MarkdownProjectionMap,
) {
    let mut replacements = inline_spans
        .iter()
        .filter(|span| {
            span.kind == MarkdownInlineKind::SoftBreak
                && range_contains(row_source_range, &span.source_range)
        })
        .filter_map(|span| {
            let display_start = projection.source_to_display(span.source_range.start);
            let display_end = projection.source_to_display(span.source_range.end);
            (display_start < display_end && display_end <= display_text.len())
                .then_some(display_start..display_end)
        })
        .collect::<Vec<_>>();

    replacements.sort_by_key(|range| range.start);
    for range in replacements.into_iter().rev() {
        display_text.replace_range(range, "\n");
    }
}

fn project_row_text(source_text: &str, projection: &MarkdownProjectionMap) -> String {
    projection.project_source_text(source_text)
}

fn project_rendered_row_text(
    source_text: &str,
    row_source_range: &Range<usize>,
    projection: &MarkdownProjectionMap,
) -> String {
    let mut rendered_text = String::new();
    let mut cursor = row_source_range.start;
    for operation in projection.operations() {
        let operation_range = operation.source_range();
        let start = operation_range.start.max(row_source_range.start);
        let end = operation_range.end.min(row_source_range.end);
        if start >= end {
            continue;
        }

        if cursor < start {
            push_rendered_source_text_chunk(
                &mut rendered_text,
                source_text,
                row_source_range,
                cursor..start,
            );
        }
        if let markdown_wysiwyg::MarkdownProjectionOperation::Replace { display_text, .. } =
            operation
        {
            rendered_text.push_str(display_text);
        }
        cursor = cursor.max(end);
    }

    if cursor < row_source_range.end {
        push_rendered_source_text_chunk(
            &mut rendered_text,
            source_text,
            row_source_range,
            cursor..row_source_range.end,
        );
    }

    rendered_text
}

fn push_rendered_source_text_chunk(
    rendered_text: &mut String,
    source_text: &str,
    row_source_range: &Range<usize>,
    source_range: Range<usize>,
) {
    let local_start = source_range.start - row_source_range.start;
    let local_end = source_range.end - row_source_range.start;
    if let Some(text) = source_text.get(local_start..local_end) {
        rendered_text.push_str(
            text.replace("\r\n", " ")
                .replace(['\n', '\r'], " ")
                .as_str(),
        );
    }
}
