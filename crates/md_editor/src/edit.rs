use super::rendered_edit::{
    RenderedEditPlan, plan_rendered_delete_backward, plan_rendered_delete_forward,
    plan_rendered_insert_paragraph_break, plan_rendered_insert_soft_break,
};
use super::{
    MarkdownEditorMode,
    display_row_builder::row_source_range,
    selection::{
        clip_selection_in_text_snapshot, collapsed_selection, selection_byte_range_in_text_snapshot,
    },
};
use md_buffer::{Buffer, BufferSnapshot};
use md_text::{Point, Selection, SelectionGoal};

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
    if mode == MarkdownEditorMode::Rendered {
        return insert_rendered_newline(buffer, &selection);
    }

    let current_line_indent =
        current_line_indent_in_text_snapshot(buffer.as_text_snapshot(), selection.head());
    let insert_text = format!("\n{current_line_indent}");
    replace_selection(buffer, &selection, &insert_text)
}

fn insert_rendered_newline(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let snapshot = buffer.snapshot();
    if let Some(plan) = plan_rendered_insert_paragraph_break(&snapshot, selection) {
        return apply_rendered_edit_plan(buffer, plan);
    }

    replace_selection(buffer, selection, "\n\n")
}

fn apply_rendered_edit_plan(
    buffer: &mut Buffer,
    plan: RenderedEditPlan,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    if plan.edits.is_empty() {
        return (plan.selection_after, None);
    }

    buffer.start_transaction();
    buffer.edit(plan.edits);
    let transaction_id = buffer.end_transaction();
    (plan.selection_after, transaction_id)
}

pub(crate) fn insert_soft_break_in_mode(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if mode == MarkdownEditorMode::Source {
        return insert_newline_in_mode(buffer, &selection, mode);
    }

    let snapshot = buffer.snapshot();
    if let Some(plan) = plan_rendered_insert_soft_break(&snapshot, &selection) {
        return apply_rendered_edit_plan(buffer, plan);
    }

    replace_selection(buffer, &selection, "\n")
}

pub(crate) fn backspace_selection_in_mode(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if mode == MarkdownEditorMode::Rendered && !selection.is_empty() {
        return delete_rendered_selection(buffer, &selection);
    }

    if mode == MarkdownEditorMode::Rendered && selection.is_empty() {
        let snapshot = buffer.snapshot();
        if let Some(plan) = plan_rendered_delete_backward(&snapshot, &selection) {
            return apply_rendered_edit_plan(buffer, plan);
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
    if mode == MarkdownEditorMode::Rendered && !selection.is_empty() {
        return delete_rendered_selection(buffer, &selection);
    }

    if mode == MarkdownEditorMode::Rendered && selection.is_empty() {
        let snapshot = buffer.snapshot();
        if let Some(plan) = plan_rendered_delete_forward(&snapshot, &selection) {
            return apply_rendered_edit_plan(buffer, plan);
        }
    }

    delete_selection(buffer, &selection)
}

fn delete_rendered_selection(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let range = selection_byte_range_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if range.is_empty() {
        return (selection.clone(), None);
    }

    let mut cursor_offset = range.start;
    buffer.start_transaction();
    buffer.edit([(range, "")]);

    if let Some((range, replacement)) =
        rendered_blank_run_normalization_after_delete(&buffer.snapshot(), cursor_offset)
    {
        if range.start <= cursor_offset {
            cursor_offset = range.start + replacement.len();
        }
        buffer.edit([(range, replacement)]);
    }

    let transaction_id = buffer.end_transaction();
    let cursor = buffer
        .as_text_snapshot()
        .offset_to_point(cursor_offset.min(buffer.as_text_snapshot().len()));
    (collapsed_selection(cursor), transaction_id)
}

fn rendered_blank_run_normalization_after_delete(
    snapshot: &BufferSnapshot,
    cursor_offset: usize,
) -> Option<(std::ops::Range<usize>, String)> {
    let text_snapshot = snapshot.as_text_snapshot();
    let row_count = text_snapshot.row_count() as usize;
    if row_count == 0 {
        return None;
    }

    let cursor = text_snapshot.offset_to_point(cursor_offset.min(text_snapshot.len()));
    let cursor_row = cursor.row as usize;
    let candidate_rows = [
        cursor_row.min(row_count.saturating_sub(1)),
        cursor_row.saturating_sub(1),
    ];
    let blank_row = candidate_rows
        .into_iter()
        .find(|row| *row < row_count && source_row_is_blank(snapshot, *row))?;
    let blank_run = blank_run_containing_row(snapshot, blank_row);

    let has_previous_paragraph =
        blank_run.start > 0 && !source_row_is_blank(snapshot, blank_run.start - 1);
    let has_next_paragraph =
        blank_run.end < row_count && !source_row_is_blank(snapshot, blank_run.end);
    let target_blank_rows = usize::from(has_previous_paragraph && has_next_paragraph);

    let start = text_snapshot.point_to_offset(Point::new(blank_run.start as u32, 0));
    let end = text_snapshot.point_to_offset(Point::new(blank_run.end as u32, 0));
    let replacement = "\n".repeat(target_blank_rows);
    let current_text = text_snapshot.text_for_range(start..end).collect::<String>();
    (current_text != replacement).then_some((start..end, replacement))
}

fn blank_run_containing_row(snapshot: &BufferSnapshot, row: usize) -> std::ops::Range<usize> {
    let row_count = snapshot.as_text_snapshot().row_count() as usize;
    let mut start = row;
    while start > 0 && source_row_is_blank(snapshot, start - 1) {
        start -= 1;
    }

    let mut end = row + 1;
    while end < row_count && source_row_is_blank(snapshot, end) {
        end += 1;
    }

    start..end
}

fn source_row_is_blank(snapshot: &BufferSnapshot, row: usize) -> bool {
    snapshot
        .as_text_snapshot()
        .text_for_range(row_source_range(snapshot, row as u32))
        .all(|chunk| chunk.trim().is_empty())
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
