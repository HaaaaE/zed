use super::rendered_edit::{
    RenderedEditPlan, plan_rendered_delete_backward, plan_rendered_delete_forward,
    plan_rendered_insert_paragraph_break, plan_rendered_insert_soft_break,
    rendered_blank_run_normalization_after_delete,
};
use super::{
    MarkdownEditorMode,
    selection::{
        clip_selection_in_text_snapshot, collapsed_selection, selection_byte_range_in_text_snapshot,
    },
};
use md_buffer::{Buffer, BufferEditSummary, BufferSnapshot};
use md_text::{Point, Selection, SelectionGoal};

pub fn replace_selection(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    text: &str,
) -> (Selection<Point>, Option<BufferEditSummary>) {
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
    let mut summary = buffer.edit([(range, text)]);
    let transaction_id = buffer.end_transaction();
    if let Some(summary) = summary.as_mut() {
        summary.transaction_id = transaction_id;
    }

    let cursor = buffer.as_text_snapshot().offset_to_point(cursor_offset);
    (collapsed_selection(cursor), summary)
}

pub(crate) fn insert_newline_in_mode(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> (Selection<Point>, Option<BufferEditSummary>) {
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
) -> (Selection<Point>, Option<BufferEditSummary>) {
    let snapshot = buffer.snapshot();
    if let Some(plan) = plan_rendered_insert_paragraph_break(&snapshot, selection) {
        return apply_rendered_edit_plan(buffer, plan);
    }

    replace_selection(buffer, selection, "\n\n")
}

fn apply_rendered_edit_plan(
    buffer: &mut Buffer,
    plan: RenderedEditPlan,
) -> (Selection<Point>, Option<BufferEditSummary>) {
    if plan.edits.is_empty() {
        return (plan.selection_after, None);
    }

    let mut selection_after = plan.selection_after;
    buffer.start_transaction();
    let mut summary = buffer.edit(plan.edits);
    if plan.normalize_blank_run_after_delete {
        let mut cursor_offset = buffer
            .as_text_snapshot()
            .point_to_offset(selection_after.head());
        if let Some((range, replacement)) =
            rendered_blank_run_normalization_after_delete(&buffer.snapshot(), cursor_offset)
        {
            if range.start <= cursor_offset {
                cursor_offset = range.start + replacement.len();
            }
            summary = buffer.edit([(range, replacement)]);
            let cursor = buffer
                .as_text_snapshot()
                .offset_to_point(cursor_offset.min(buffer.as_text_snapshot().len()));
            selection_after = collapsed_selection(cursor);
        }
    }
    let transaction_id = buffer.end_transaction();
    if let Some(summary) = summary.as_mut() {
        summary.transaction_id = transaction_id;
    }
    (selection_after, summary)
}

pub(crate) fn insert_soft_break_in_mode(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> (Selection<Point>, Option<BufferEditSummary>) {
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
) -> (Selection<Point>, Option<BufferEditSummary>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if mode == MarkdownEditorMode::Rendered && !selection.is_empty() {
        let snapshot = buffer.snapshot();
        if let Some(plan) = plan_rendered_delete_backward(&snapshot, &selection) {
            return apply_rendered_edit_plan(buffer, plan);
        }
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
) -> (Selection<Point>, Option<BufferEditSummary>) {
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
) -> (Selection<Point>, Option<BufferEditSummary>) {
    let selection = clip_selection_in_text_snapshot(buffer.as_text_snapshot(), selection);
    if mode == MarkdownEditorMode::Rendered && !selection.is_empty() {
        let snapshot = buffer.snapshot();
        if let Some(plan) = plan_rendered_delete_forward(&snapshot, &selection) {
            return apply_rendered_edit_plan(buffer, plan);
        }
    }

    if mode == MarkdownEditorMode::Rendered && selection.is_empty() {
        let snapshot = buffer.snapshot();
        if let Some(plan) = plan_rendered_delete_forward(&snapshot, &selection) {
            return apply_rendered_edit_plan(buffer, plan);
        }
    }

    delete_selection(buffer, &selection)
}

pub fn delete_selection(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
) -> (Selection<Point>, Option<BufferEditSummary>) {
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
