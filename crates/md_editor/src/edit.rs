use md_buffer::{Buffer, BufferSnapshot};
use md_text::{Point, Selection, SelectionGoal};

use super::rendered_element::{
    projection_replacement_range_at_cursor, rendered_element_range_at_cursor,
};
use super::{
    MarkdownEditorMode,
    rendered_index::RenderedDisplayIndex,
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

pub(crate) fn insert_newline_in_mode(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    let insert_text = match mode {
        MarkdownEditorMode::Source => {
            let current_line_indent =
                current_line_indent_in_text_snapshot(buffer.as_text_snapshot(), selection.head());
            format!("\n{current_line_indent}")
        }
        MarkdownEditorMode::Rendered => "\n\n".to_string(),
    };

    replace_selection(buffer, &selection, &insert_text)
}

pub(crate) fn insert_soft_break_in_mode(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    if mode == MarkdownEditorMode::Source {
        return insert_newline_in_mode(buffer, selection, mode);
    }

    replace_selection(buffer, selection, "\n")
}

pub(crate) fn backspace_selection_in_mode(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if mode == MarkdownEditorMode::Rendered && selection.is_empty() {
        let snapshot = buffer.snapshot();
        if let Some(deletion) = rendered_blank_paragraph_deletion_at_cursor(
            &snapshot,
            selection.head(),
            HorizontalDirection::Left,
        ) {
            let (mut selection, transaction_id) = replace_selection(
                buffer,
                &selection_for_source_range(&snapshot, selection.id, deletion.range),
                "",
            );
            if let Some(cursor) = deletion.cursor_after_delete {
                selection = collapsed_selection(cursor);
            }
            return (selection, transaction_id);
        }
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
        if let Some(deletion) = rendered_blank_paragraph_deletion_at_cursor(
            &snapshot,
            selection.head(),
            HorizontalDirection::Right,
        ) {
            let (mut selection, transaction_id) = replace_selection(
                buffer,
                &selection_for_source_range(&snapshot, selection.id, deletion.range),
                "",
            );
            if let Some(cursor) = deletion.cursor_after_delete {
                selection = collapsed_selection(cursor);
            }
            return (selection, transaction_id);
        }
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

struct RenderedBlankParagraphDeletion {
    range: std::ops::Range<usize>,
    cursor_after_delete: Option<Point>,
}

fn rendered_blank_paragraph_deletion_at_cursor(
    snapshot: &BufferSnapshot,
    cursor: Point,
    direction: HorizontalDirection,
) -> Option<RenderedBlankParagraphDeletion> {
    let index = RenderedDisplayIndex::build(snapshot);
    let row = empty_paragraph_row_for_cursor(&index, snapshot, cursor.row as usize, direction)?;
    let item_index = index.item_index_for_source_row(row)?;
    let item = index.item(item_index)?;
    if !matches!(
        item.kind,
        super::rendered_index::RenderedDisplayItemKind::EmptyParagraph
    ) {
        return None;
    }

    let text_snapshot = snapshot.as_text_snapshot();
    let start = text_snapshot.point_to_offset(Point::new(item.row_range.start as u32, 0));
    let end_row = empty_paragraph_delete_end_row(snapshot, item.row_range.end);
    let end = text_snapshot.point_to_offset(Point::new(end_row, 0));
    Some(RenderedBlankParagraphDeletion {
        range: start..end,
        cursor_after_delete: match direction {
            HorizontalDirection::Left => Some(previous_editable_point_before_row(
                snapshot,
                item.row_range.start,
            )),
            HorizontalDirection::Right => None,
        },
    })
}

fn empty_paragraph_delete_end_row(snapshot: &BufferSnapshot, row_after_empty: usize) -> u32 {
    let row_count = snapshot.as_text_snapshot().row_count() as usize;
    let mut end_row = row_after_empty.min(row_count);
    if end_row < row_count && source_row_is_blank(snapshot, end_row) {
        end_row += 1;
    }
    end_row as u32
}

fn previous_editable_point_before_row(snapshot: &BufferSnapshot, row: usize) -> Point {
    for previous_row in (0..row).rev() {
        if source_row_is_blank(snapshot, previous_row) {
            continue;
        }
        let previous_row = previous_row as u32;
        return Point::new(
            previous_row,
            snapshot.as_text_snapshot().line_len(previous_row),
        );
    }
    Point::zero()
}

fn source_row_is_blank(snapshot: &BufferSnapshot, row: usize) -> bool {
    snapshot
        .as_text_snapshot()
        .text_for_range(super::row_source_range(snapshot, row as u32))
        .all(|chunk| chunk.trim().is_empty())
}

fn empty_paragraph_row_for_cursor(
    index: &RenderedDisplayIndex,
    snapshot: &BufferSnapshot,
    row: usize,
    direction: HorizontalDirection,
) -> Option<usize> {
    if index
        .item_index_for_source_row(row)
        .and_then(|item_index| index.item(item_index))
        .is_some_and(|item| {
            matches!(
                item.kind,
                super::rendered_index::RenderedDisplayItemKind::EmptyParagraph
            )
        })
    {
        return Some(row);
    }

    let neighbor = match direction {
        HorizontalDirection::Left => row.checked_sub(1)?,
        HorizontalDirection::Right => {
            let next = row.saturating_add(1);
            (next < snapshot.as_text_snapshot().row_count() as usize).then_some(next)?
        }
    };
    index
        .item_index_for_source_row(neighbor)
        .and_then(|item_index| index.item(item_index))
        .is_some_and(|item| {
            matches!(
                item.kind,
                super::rendered_index::RenderedDisplayItemKind::EmptyParagraph
            )
        })
        .then_some(neighbor)
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
