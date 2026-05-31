use md_buffer::{Buffer, BufferSnapshot};
use md_text::{BufferSnapshot as TextBufferSnapshot, Point, Selection, SelectionGoal};

use super::rendered_element::{
    projection_replacement_range_at_cursor, rendered_element_range_at_cursor,
};
use super::{
    MarkdownEditorMode,
    display_row_builder::row_source_range,
    selection::{
        HorizontalDirection, clip_selection_in_text_snapshot, collapsed_selection,
        selection_byte_range_in_text_snapshot, selection_for_source_range,
    },
};
use md_projection::{RenderedDisplayIndex, RenderedDisplayItemKind};

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
    if selection.is_empty() {
        let snapshot = buffer.snapshot();
        if let Some(exit) = rendered_line_exit_at_cursor(&snapshot, selection.head()) {
            let (_selection, transaction_id) = replace_selection(
                buffer,
                &selection_for_source_range(&snapshot, selection.id, exit.range),
                &exit.replacement,
            );
            let selection = collapsed_selection(
                buffer
                    .as_text_snapshot()
                    .offset_to_point(exit.cursor_offset_after_edit),
            );
            return (selection, transaction_id);
        }
    }

    let insertion = rendered_newline_insertion(buffer, selection);
    let range_start =
        selection_byte_range_in_text_snapshot(buffer.as_text_snapshot(), selection).start;
    let (mut selection, transaction_id) = replace_selection(buffer, selection, &insertion.text);
    if insertion.cursor_delta != insertion.text.len() {
        let cursor = buffer
            .as_text_snapshot()
            .offset_to_point(range_start + insertion.cursor_delta);
        selection = collapsed_selection(cursor);
    }
    (selection, transaction_id)
}

struct RenderedLineExit {
    range: std::ops::Range<usize>,
    replacement: String,
    cursor_offset_after_edit: usize,
}

fn rendered_line_exit_at_cursor(
    snapshot: &BufferSnapshot,
    cursor: Point,
) -> Option<RenderedLineExit> {
    let text_snapshot = snapshot.as_text_snapshot();
    if cursor.column != text_snapshot.line_len(cursor.row) {
        return None;
    }

    let line_start = text_snapshot.point_to_offset(Point::new(cursor.row, 0));
    let line_end = line_start + text_snapshot.line_len(cursor.row) as usize;
    let line = text_snapshot
        .text_for_range(line_start..line_end)
        .collect::<String>();
    let exit = rendered_line_exit_replacement(&line)?;
    let range = line_start..line_start + exit.delete_len;
    Some(RenderedLineExit {
        range,
        replacement: exit.replacement,
        cursor_offset_after_edit: line_start + exit.cursor_column,
    })
}

struct RenderedLineExitReplacement {
    delete_len: usize,
    replacement: String,
    cursor_column: usize,
}

fn rendered_line_exit_replacement(line: &str) -> Option<RenderedLineExitReplacement> {
    let (quote_prefix, after_quote) = split_blockquote_prefix(line);
    let (indent, rest) = split_ascii_indent(after_quote);
    if let Some((marker, content)) = unordered_list_marker(rest) {
        let task_marker = task_marker_for_content(content);
        if !content_after_optional_task_marker(content)
            .trim()
            .is_empty()
        {
            return None;
        }
        return Some(RenderedLineExitReplacement {
            delete_len: quote_prefix.len() + indent.len() + marker.len() + task_marker.len(),
            replacement: quote_prefix.to_string(),
            cursor_column: quote_prefix.len(),
        });
    }
    if let Some((_number, _delimiter, content)) = ordered_list_marker(rest) {
        if !content.trim().is_empty() {
            return None;
        }
        return Some(RenderedLineExitReplacement {
            delete_len: quote_prefix.len() + indent.len() + (rest.len() - content.len()),
            replacement: quote_prefix.to_string(),
            cursor_column: quote_prefix.len(),
        });
    }
    if !quote_prefix.is_empty() && after_quote.trim().is_empty() {
        return Some(RenderedLineExitReplacement {
            delete_len: quote_prefix.len(),
            replacement: String::new(),
            cursor_column: 0,
        });
    }
    None
}

struct RenderedNewlineInsertion {
    text: String,
    cursor_delta: usize,
}

fn rendered_newline_insertion(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
) -> RenderedNewlineInsertion {
    let default = RenderedNewlineInsertion {
        text: "\n\n".to_string(),
        cursor_delta: "\n\n".len(),
    };
    if !selection.is_empty() {
        return default;
    }

    let snapshot = buffer.snapshot();
    let text_snapshot = snapshot.as_text_snapshot();
    let cursor = selection.head();
    if let Some(insertion) = rendered_line_continuation_insertion(text_snapshot, cursor) {
        return insertion;
    }

    let source_offset = text_snapshot.point_to_offset(cursor);
    let index = RenderedDisplayIndex::build(&snapshot);
    let Some(item_index) = index.item_index_for_source_offset(&snapshot, source_offset) else {
        return default;
    };
    let Some(item) = index.item(item_index) else {
        return default;
    };
    if !matches!(
        item.kind,
        RenderedDisplayItemKind::Paragraph | RenderedDisplayItemKind::Heading
    ) || source_offset != item.source_range.end
    {
        return default;
    }

    let next_row = item.row_range.end;
    if next_row < text_snapshot.row_count() as usize && source_row_is_blank(&snapshot, next_row) {
        return default;
    }

    RenderedNewlineInsertion {
        text: "\n\n\n".to_string(),
        cursor_delta: "\n\n".len(),
    }
}

fn rendered_line_continuation_insertion(
    snapshot: &TextBufferSnapshot,
    cursor: Point,
) -> Option<RenderedNewlineInsertion> {
    if cursor.column != snapshot.line_len(cursor.row) {
        return None;
    }

    let line_start = snapshot.point_to_offset(Point::new(cursor.row, 0));
    let line_end = line_start + snapshot.line_len(cursor.row) as usize;
    let line = snapshot
        .text_for_range(line_start..line_end)
        .collect::<String>();
    let marker = rendered_line_continuation_marker(&line)?;
    Some(RenderedNewlineInsertion {
        text: format!("\n{marker}"),
        cursor_delta: "\n".len() + marker.len(),
    })
}

fn rendered_line_continuation_marker(line: &str) -> Option<String> {
    let (quote_prefix, after_quote) = split_blockquote_prefix(line);
    let (indent, rest) = split_ascii_indent(after_quote);
    let prefix = format!("{quote_prefix}{indent}");
    if let Some((marker, content)) = unordered_list_marker(rest) {
        let task_marker = task_marker_for_content(content);
        if content_after_optional_task_marker(content)
            .trim()
            .is_empty()
        {
            return None;
        }
        return Some(format!("{prefix}{marker}{task_marker}"));
    }
    if let Some((number, delimiter, content)) = ordered_list_marker(rest) {
        if content.trim().is_empty() {
            return None;
        }
        return Some(format!("{prefix}{}{delimiter} ", number.saturating_add(1)));
    }
    if !quote_prefix.is_empty() && !after_quote.trim().is_empty() {
        return Some(quote_prefix.to_string());
    }
    None
}

fn split_blockquote_prefix(line: &str) -> (&str, &str) {
    let mut cursor = 0;
    let bytes = line.as_bytes();
    loop {
        while cursor < bytes.len() && matches!(bytes[cursor], b' ' | b'\t') {
            cursor += 1;
        }
        if bytes.get(cursor) != Some(&b'>') {
            break;
        }
        cursor += 1;
        if bytes.get(cursor) == Some(&b' ') {
            cursor += 1;
        }
    }
    line.split_at(cursor)
}

fn split_ascii_indent(line: &str) -> (&str, &str) {
    let indent_len = line
        .as_bytes()
        .iter()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    line.split_at(indent_len)
}

fn unordered_list_marker(line: &str) -> Option<(&str, &str)> {
    let bytes = line.as_bytes();
    if bytes.len() >= 2 && matches!(bytes[0], b'-' | b'*' | b'+') && bytes[1] == b' ' {
        return Some((&line[..2], &line[2..]));
    }
    None
}

fn ordered_list_marker(line: &str) -> Option<(u32, char, &str)> {
    let bytes = line.as_bytes();
    let digit_len = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digit_len == 0 || bytes.len() < digit_len + 2 {
        return None;
    }
    let delimiter = bytes[digit_len] as char;
    if !matches!(delimiter, '.' | ')') || bytes[digit_len + 1] != b' ' {
        return None;
    }
    let number = line[..digit_len].parse::<u32>().ok()?;
    Some((number, delimiter, &line[digit_len + 2..]))
}

fn task_marker_for_content(content: &str) -> &'static str {
    if matches!(
        content.as_bytes().get(..4),
        Some([b'[', b' ', b']', b' '])
            | Some([b'[', b'x', b']', b' '])
            | Some([b'[', b'X', b']', b' '])
    ) {
        "[ ] "
    } else {
        ""
    }
}

fn content_after_optional_task_marker(content: &str) -> &str {
    if matches!(
        content.as_bytes().get(..4),
        Some([b'[', b' ', b']', b' '])
            | Some([b'[', b'x', b']', b' '])
            | Some([b'[', b'X', b']', b' '])
    ) {
        &content[4..]
    } else {
        content
    }
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
    if mode == MarkdownEditorMode::Rendered && !selection.is_empty() {
        return delete_rendered_selection(buffer, &selection);
    }

    if mode == MarkdownEditorMode::Rendered && selection.is_empty() {
        let snapshot = buffer.snapshot();
        if let Some(removal) = rendered_line_prefix_removal_at_cursor(&snapshot, selection.head()) {
            let (_selection, transaction_id) = replace_selection(
                buffer,
                &selection_for_source_range(&snapshot, selection.id, removal.range),
                &removal.replacement,
            );
            let selection = collapsed_selection(
                buffer
                    .as_text_snapshot()
                    .offset_to_point(removal.cursor_offset_after_edit),
            );
            return (selection, transaction_id);
        }
        if let Some(deletion) = rendered_paragraph_boundary_deletion_at_cursor(
            &snapshot,
            selection.head(),
            HorizontalDirection::Left,
        ) {
            let (_selection, transaction_id) = replace_selection(
                buffer,
                &selection_for_source_range(&snapshot, selection.id, deletion.range),
                &deletion.replacement,
            );
            let selection = collapsed_selection(
                buffer
                    .as_text_snapshot()
                    .offset_to_point(deletion.cursor_offset_after_edit),
            );
            return (selection, transaction_id);
        }
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

struct RenderedLinePrefixRemoval {
    range: std::ops::Range<usize>,
    replacement: String,
    cursor_offset_after_edit: usize,
}

fn rendered_line_prefix_removal_at_cursor(
    snapshot: &BufferSnapshot,
    cursor: Point,
) -> Option<RenderedLinePrefixRemoval> {
    let text_snapshot = snapshot.as_text_snapshot();
    let line_start = text_snapshot.point_to_offset(Point::new(cursor.row, 0));
    let line_end = line_start + text_snapshot.line_len(cursor.row) as usize;
    let line = text_snapshot
        .text_for_range(line_start..line_end)
        .collect::<String>();
    let removal = rendered_line_prefix_removal(&line, cursor.column as usize)?;
    Some(RenderedLinePrefixRemoval {
        range: line_start..line_start + removal.delete_len,
        replacement: removal.replacement,
        cursor_offset_after_edit: line_start + removal.cursor_column,
    })
}

struct RenderedLinePrefixRemovalSpec {
    delete_len: usize,
    replacement: String,
    cursor_column: usize,
}

fn rendered_line_prefix_removal(
    line: &str,
    cursor_column: usize,
) -> Option<RenderedLinePrefixRemovalSpec> {
    let (quote_prefix, after_quote) = split_blockquote_prefix(line);
    let (indent, rest) = split_ascii_indent(after_quote);
    if let Some((marker, content)) = unordered_list_marker(rest) {
        let task_marker = task_marker_for_content(content);
        let marker_len = quote_prefix.len() + indent.len() + marker.len() + task_marker.len();
        if cursor_column != marker_len
            || content_after_optional_task_marker(content)
                .trim()
                .is_empty()
        {
            return None;
        }
        return Some(RenderedLinePrefixRemovalSpec {
            delete_len: marker_len,
            replacement: quote_prefix.to_string(),
            cursor_column: quote_prefix.len(),
        });
    }
    if let Some((_number, _delimiter, content)) = ordered_list_marker(rest) {
        let marker_len = quote_prefix.len() + indent.len() + (rest.len() - content.len());
        if cursor_column != marker_len || content.trim().is_empty() {
            return None;
        }
        return Some(RenderedLinePrefixRemovalSpec {
            delete_len: marker_len,
            replacement: quote_prefix.to_string(),
            cursor_column: quote_prefix.len(),
        });
    }
    if !quote_prefix.is_empty()
        && cursor_column == quote_prefix.len()
        && !after_quote.trim().is_empty()
    {
        return Some(RenderedLinePrefixRemovalSpec {
            delete_len: quote_prefix.len(),
            replacement: String::new(),
            cursor_column: 0,
        });
    }
    None
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
        if let Some(deletion) = rendered_paragraph_boundary_deletion_at_cursor(
            &snapshot,
            selection.head(),
            HorizontalDirection::Right,
        ) {
            let (_selection, transaction_id) = replace_selection(
                buffer,
                &selection_for_source_range(&snapshot, selection.id, deletion.range),
                &deletion.replacement,
            );
            let selection = collapsed_selection(
                buffer
                    .as_text_snapshot()
                    .offset_to_point(deletion.cursor_offset_after_edit),
            );
            return (selection, transaction_id);
        }
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

struct RenderedBlankParagraphDeletion {
    range: std::ops::Range<usize>,
    cursor_after_delete: Option<Point>,
}

struct RenderedParagraphBoundaryDeletion {
    range: std::ops::Range<usize>,
    replacement: String,
    cursor_offset_after_edit: usize,
}

fn rendered_paragraph_boundary_deletion_at_cursor(
    snapshot: &BufferSnapshot,
    cursor: Point,
    direction: HorizontalDirection,
) -> Option<RenderedParagraphBoundaryDeletion> {
    let text_snapshot = snapshot.as_text_snapshot();
    let index = RenderedDisplayIndex::build(snapshot);
    let cursor = text_snapshot.clip_point(cursor, md_text::Bias::Left);
    let cursor_row = cursor.row as usize;
    let row_count = text_snapshot.row_count() as usize;
    if cursor_row >= row_count || source_row_is_blank(snapshot, cursor_row) {
        return None;
    }

    let item = index
        .item_index_for_source_row(cursor_row)
        .and_then(|item_index| index.item(item_index))?;
    if !matches!(
        item.kind,
        RenderedDisplayItemKind::Paragraph | RenderedDisplayItemKind::Heading
    ) {
        return None;
    }

    match direction {
        HorizontalDirection::Left => {
            if cursor.column != 0 || cursor_row == 0 {
                return None;
            }

            let blank_run_end = cursor_row;
            let mut blank_run_start = cursor_row;
            while blank_run_start > 0 && source_row_is_blank(snapshot, blank_run_start - 1) {
                blank_run_start -= 1;
            }
            if blank_run_start == blank_run_end || blank_run_start == 0 {
                return None;
            }

            let previous_row = blank_run_start - 1;
            if source_row_is_blank(snapshot, previous_row) {
                return None;
            }
            let previous_item = index
                .item_index_for_source_row(previous_row)
                .and_then(|item_index| index.item(item_index))?;
            if !matches!(
                previous_item.kind,
                RenderedDisplayItemKind::Paragraph | RenderedDisplayItemKind::Heading
            ) {
                return None;
            }

            let previous_end = text_snapshot.point_to_offset(Point::new(
                previous_row as u32,
                text_snapshot.line_len(previous_row as u32),
            ));
            let cursor_offset = text_snapshot.point_to_offset(cursor);
            let blank_row_count = blank_run_end - blank_run_start;
            let (range, replacement) = if blank_row_count == 1 {
                (previous_end..cursor_offset, String::new())
            } else {
                (
                    text_snapshot.point_to_offset(Point::new(blank_run_start as u32, 0))
                        ..cursor_offset,
                    "\n".to_string(),
                )
            };
            Some(RenderedParagraphBoundaryDeletion {
                range,
                replacement,
                cursor_offset_after_edit: previous_end,
            })
        }
        HorizontalDirection::Right => {
            if cursor.column != text_snapshot.line_len(cursor.row) {
                return None;
            }

            let blank_run_start = cursor_row.saturating_add(1);
            if blank_run_start >= row_count || !source_row_is_blank(snapshot, blank_run_start) {
                return None;
            }

            let mut blank_run_end = blank_run_start;
            while blank_run_end < row_count && source_row_is_blank(snapshot, blank_run_end) {
                blank_run_end += 1;
            }
            if blank_run_end >= row_count || source_row_is_blank(snapshot, blank_run_end) {
                return None;
            }

            let next_item = index
                .item_index_for_source_row(blank_run_end)
                .and_then(|item_index| index.item(item_index))?;
            if !matches!(
                next_item.kind,
                RenderedDisplayItemKind::Paragraph | RenderedDisplayItemKind::Heading
            ) {
                return None;
            }

            let cursor_offset = text_snapshot.point_to_offset(cursor);
            let next_start = text_snapshot.point_to_offset(Point::new(blank_run_end as u32, 0));
            let blank_row_count = blank_run_end - blank_run_start;
            let (range, replacement, cursor_offset_after_edit) = if blank_row_count == 1 {
                (cursor_offset..next_start, String::new(), cursor_offset)
            } else {
                let blank_start =
                    text_snapshot.point_to_offset(Point::new(blank_run_start as u32, 0));
                (blank_start..next_start, "\n".to_string(), blank_start + 1)
            };
            Some(RenderedParagraphBoundaryDeletion {
                range,
                replacement,
                cursor_offset_after_edit,
            })
        }
    }
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
        md_projection::RenderedDisplayItemKind::EmptyParagraph
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
        .text_for_range(row_source_range(snapshot, row as u32))
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
                md_projection::RenderedDisplayItemKind::EmptyParagraph
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
                md_projection::RenderedDisplayItemKind::EmptyParagraph
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
