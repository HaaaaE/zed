use md_buffer::{Buffer, BufferSnapshot};
use md_text::{Point, Selection, SelectionGoal};

use super::rendered_element::{
    projection_replacement_range_at_cursor, rendered_element_range_at_cursor,
};
use super::{
    MarkdownEditorMode,
    selection::{
        HorizontalDirection, clip_selection_in_text_snapshot, collapsed_selection,
        selection_byte_range_in_text_snapshot, selection_for_source_range,
    },
};

pub fn replace_selection(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    text: &str,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let (selection, range) = {
        let snapshot = buffer.as_text_snapshot();
        let selection = clip_selection_in_text_snapshot(snapshot, selection);
        let range = selection_byte_range_in_text_snapshot(snapshot, &selection);
        (selection, range)
    };

    if range.is_empty() && text.is_empty() {
        return (selection, None);
    }

    let cursor_offset = range.start.saturating_add(text.len());
    buffer.start_transaction();
    buffer.edit([(range, text)]);
    let transaction_id = buffer.end_transaction();

    let cursor = buffer.as_text_snapshot().offset_to_point(cursor_offset);
    (collapsed_selection(cursor), transaction_id)
}

pub(crate) fn backspace_selection_in_mode(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if mode == MarkdownEditorMode::Rendered && selection.is_empty() {
        let snapshot = buffer.snapshot();
        let range = rendered_element_range_at_cursor(
            &snapshot,
            selection.head(),
            HorizontalDirection::Left,
        )
        .or_else(|| {
            projection_replacement_range_at_cursor(
                &snapshot,
                selection.head(),
                HorizontalDirection::Left,
            )
        });
        if let Some(range) = range {
            return replace_selection(
                buffer,
                &selection_for_source_range(&snapshot, selection.id, range),
                "",
            );
        }
    }

    backspace_selection(buffer, &selection)
}

pub fn backspace_selection(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if !selection.is_empty() {
        return replace_selection(buffer, &selection, "");
    }

    let text_snapshot = buffer.as_text_snapshot();
    let offset = text_snapshot.point_to_offset(selection.head());
    if offset == 0 {
        return (selection, None);
    }

    let previous_offset = text_snapshot
        .as_rope()
        .floor_char_boundary(offset.saturating_sub(1));
    replace_selection(
        buffer,
        &Selection {
            id: selection.id,
            start: text_snapshot.offset_to_point(previous_offset),
            end: selection.head(),
            reversed: false,
            goal: SelectionGoal::None,
        },
        "",
    )
}

pub(crate) fn delete_selection_in_mode(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if mode == MarkdownEditorMode::Rendered && selection.is_empty() {
        let snapshot = buffer.snapshot();
        let range = rendered_element_range_at_cursor(
            &snapshot,
            selection.head(),
            HorizontalDirection::Right,
        )
        .or_else(|| {
            projection_replacement_range_at_cursor(
                &snapshot,
                selection.head(),
                HorizontalDirection::Right,
            )
        });
        if let Some(range) = range {
            return replace_selection(
                buffer,
                &selection_for_source_range(&snapshot, selection.id, range),
                "",
            );
        }
    }

    delete_selection(buffer, &selection)
}

pub fn delete_selection(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if !selection.is_empty() {
        return replace_selection(buffer, &selection, "");
    }

    let text_snapshot = buffer.as_text_snapshot();
    let offset = text_snapshot.point_to_offset(selection.head());
    if offset >= text_snapshot.len() {
        return (selection, None);
    }

    let next_offset = text_snapshot
        .as_rope()
        .ceil_char_boundary(offset.saturating_add(1));
    replace_selection(
        buffer,
        &Selection {
            id: selection.id,
            start: selection.head(),
            end: text_snapshot.offset_to_point(next_offset),
            reversed: false,
            goal: SelectionGoal::None,
        },
        "",
    )
}

pub fn current_line_indent(snapshot: &BufferSnapshot, cursor: Point) -> String {
    current_line_indent_in_text_snapshot(snapshot.as_text_snapshot(), cursor)
}

pub(crate) fn current_line_indent_in_text_snapshot(
    snapshot: &md_text::BufferSnapshot,
    cursor: Point,
) -> String {
    if cursor.row >= snapshot.row_count() {
        return String::new();
    }

    let line_start = Point::new(cursor.row, 0);
    let line_end = Point::new(cursor.row, snapshot.line_len(cursor.row));
    snapshot
        .text_for_range(line_start..line_end)
        .flat_map(str::chars)
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}
