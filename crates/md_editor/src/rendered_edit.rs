use std::{ops::Range, sync::Arc};

use md_buffer::BufferSnapshot;
use md_projection::{
    RenderedDisplayIndex, RenderedDisplayItemKind, RenderedNewlineRun, RenderedNewlineRunKind,
    RenderedTopology,
};
use md_text::{BufferSnapshot as TextBufferSnapshot, Point, Selection, SelectionGoal};

use super::{
    display_row_builder::row_source_range,
    rendered_element::{projection_replacement_range_at_cursor, rendered_element_range_at_cursor},
    selection::{HorizontalDirection, collapsed_selection, selection_byte_range_in_text_snapshot},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RenderedEditIntent {
    InsertParagraphBreak,
    InsertSoftBreak,
    DeleteBackward,
    DeleteForward,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum RenderedSemanticPosition {
    PlainText,
    SoftBreakBoundary {
        run: RenderedNewlineRun,
        slot: RenderedNewlineRunSlot,
    },
    ParagraphBoundary {
        run: RenderedNewlineRun,
        slot: RenderedNewlineRunSlot,
    },
    BoundaryWithSoftBreakSlot {
        run: RenderedNewlineRun,
        slot: RenderedNewlineRunSlot,
    },
    EmptyParagraph {
        run: RenderedNewlineRun,
        slot: RenderedNewlineRunSlot,
    },
    LinePrefix {
        range: Range<usize>,
        replacement: String,
        cursor_offset: usize,
    },
    ProjectionReplacement {
        range: Range<usize>,
    },
    RenderedElement {
        range: Range<usize>,
    },
    StructuredBoundary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RenderedNewlineRunSlot {
    LeftBoundary,
    RightBoundary,
    Separator,
    EmptyParagraph(usize),
    SoftBreakSlot,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RenderedEditPlan {
    pub(crate) edits: Vec<(Range<usize>, String)>,
    pub(crate) selection_after: Selection<Point>,
    pub(crate) normalize_blank_run_after_delete: bool,
}

struct RenderedEditContext<'a> {
    snapshot: &'a BufferSnapshot,
    index: Arc<RenderedDisplayIndex>,
    topology: RenderedTopology<'a>,
}

impl<'a> RenderedEditContext<'a> {
    fn new(snapshot: &'a BufferSnapshot) -> Self {
        let index = RenderedDisplayIndex::build(snapshot);
        let topology = RenderedTopology::new(snapshot, index.clone());
        Self {
            snapshot,
            index,
            topology,
        }
    }
}

pub(crate) fn plan_rendered_insert_paragraph_break(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Option<RenderedEditPlan> {
    let context = RenderedEditContext::new(snapshot);
    if !selection.is_empty() {
        return Some(replace_selection_plan(
            snapshot,
            selection,
            "\n\n",
            "\n\n".len(),
        ));
    }

    let cursor = selection.head();
    if let Some(RenderedSemanticPosition::LinePrefix {
        range,
        replacement,
        cursor_offset,
    }) = rendered_semantic_position(&context, cursor, RenderedEditIntent::InsertParagraphBreak)
    {
        return Some(single_edit_plan_from_offset(
            snapshot,
            range,
            replacement,
            cursor_offset,
        ));
    }

    if let Some(insertion) =
        rendered_line_continuation_insertion(snapshot.as_text_snapshot(), cursor)
    {
        return Some(replace_selection_plan(
            snapshot,
            selection,
            &insertion.text,
            insertion.cursor_delta,
        ));
    }

    if let Some(position) = newline_run_position_at_cursor(&context, cursor) {
        match position {
            RenderedSemanticPosition::SoftBreakBoundary { run, .. } => {
                return Some(canonical_newline_run_plan(
                    &run,
                    2,
                    Point::new(run.left_point.row + 2, 0),
                ));
            }
            RenderedSemanticPosition::ParagraphBoundary { run, .. } => {
                return Some(canonical_newline_run_plan(
                    &run,
                    4,
                    Point::new(run.left_point.row + 2, 0),
                ));
            }
            RenderedSemanticPosition::BoundaryWithSoftBreakSlot { run, .. } => {
                let next_newline_count = if run_right_is_document_tail(snapshot, &run) {
                    5
                } else {
                    4
                };
                return Some(canonical_newline_run_plan(
                    &run,
                    next_newline_count,
                    if run_right_is_document_tail(snapshot, &run) {
                        Point::new(run.left_point.row + next_newline_count as u32 - 1, 0)
                    } else {
                        Point::new(run.left_point.row + 2, 0)
                    },
                ));
            }
            RenderedSemanticPosition::EmptyParagraph { run, .. } => {
                let next_count = empty_paragraph_count(run.newline_count).saturating_add(1);
                return Some(canonical_newline_run_plan(
                    &run,
                    canonical_newline_count_for_empty_paragraphs(next_count),
                    Point::new(run.left_point.row + (2 * next_count) as u32, 0),
                ));
            }
            _ => {}
        }
    }

    let insertion = rendered_plain_paragraph_break_insertion(&context, cursor);
    Some(replace_selection_plan(
        snapshot,
        selection,
        &insertion.text,
        insertion.cursor_delta,
    ))
}

pub(crate) fn plan_rendered_insert_soft_break(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Option<RenderedEditPlan> {
    let context = RenderedEditContext::new(snapshot);
    if !selection.is_empty() {
        return Some(replace_selection_plan(
            snapshot,
            selection,
            "\n",
            "\n".len(),
        ));
    }

    let cursor = selection.head();
    if let Some(position) =
        rendered_semantic_position(&context, cursor, RenderedEditIntent::InsertSoftBreak)
    {
        match position {
            RenderedSemanticPosition::SoftBreakBoundary { run, .. } => {
                if run_right_is_document_tail(snapshot, &run) {
                    return Some(canonical_newline_run_plan(
                        &run,
                        2,
                        Point::new(run.left_point.row + 2, 0),
                    ));
                }
                return Some(move_only_plan(run.right_point));
            }
            RenderedSemanticPosition::ParagraphBoundary { run, .. } => {
                return Some(canonical_newline_run_plan(
                    &run,
                    3,
                    Point::new(run.left_point.row + 1, 0),
                ));
            }
            RenderedSemanticPosition::BoundaryWithSoftBreakSlot { run, .. } => {
                return Some(move_only_plan(Point::new(run.left_point.row + 1, 0)));
            }
            RenderedSemanticPosition::EmptyParagraph { run, .. } => {
                let kind = context.topology.classify_newline_run(&run);
                let has_slot = matches!(
                    kind,
                    RenderedNewlineRunKind::EmptyParagraphs {
                        has_soft_break_slot: true,
                        ..
                    }
                );
                if has_slot {
                    return Some(move_only_plan(Point::new(run.left_point.row + 1, 0)));
                }
                return Some(canonical_newline_run_plan(
                    &run,
                    run.newline_count + 1,
                    Point::new(run.left_point.row + 1, 0),
                ));
            }
            _ => {}
        }
    }

    Some(replace_selection_plan(
        snapshot,
        selection,
        "\n",
        "\n".len(),
    ))
}

pub(crate) fn plan_rendered_delete_backward(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Option<RenderedEditPlan> {
    let context = RenderedEditContext::new(snapshot);
    if !selection.is_empty() {
        return Some(delete_rendered_selection_plan(snapshot, selection));
    }

    let cursor = selection.head();
    let position =
        rendered_semantic_position(&context, cursor, RenderedEditIntent::DeleteBackward)?;
    match position {
        RenderedSemanticPosition::LinePrefix {
            range,
            replacement,
            cursor_offset,
        } => Some(single_edit_plan_from_offset(
            snapshot,
            range,
            replacement,
            cursor_offset,
        )),
        RenderedSemanticPosition::SoftBreakBoundary { run, .. }
        | RenderedSemanticPosition::ParagraphBoundary { run, .. } => {
            Some(canonical_newline_run_plan(&run, 0, run.left_point))
        }
        RenderedSemanticPosition::BoundaryWithSoftBreakSlot { run, slot } => {
            let cursor = match slot {
                RenderedNewlineRunSlot::Separator => run.left_point,
                RenderedNewlineRunSlot::SoftBreakSlot => Point::new(run.left_point.row + 2, 0),
                _ => run.left_point,
            };
            Some(canonical_newline_run_plan(&run, 2, cursor))
        }
        RenderedSemanticPosition::EmptyParagraph { run, slot } => Some(
            delete_empty_paragraph_plan(&run, slot, HorizontalDirection::Left),
        ),
        RenderedSemanticPosition::RenderedElement { range }
        | RenderedSemanticPosition::ProjectionReplacement { range } => {
            Some(single_edit_plan(snapshot, range.clone(), String::new(), 0))
        }
        _ => None,
    }
}

pub(crate) fn plan_rendered_delete_forward(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Option<RenderedEditPlan> {
    let context = RenderedEditContext::new(snapshot);
    if !selection.is_empty() {
        return Some(delete_rendered_selection_plan(snapshot, selection));
    }

    let cursor = selection.head();
    let position = rendered_semantic_position(&context, cursor, RenderedEditIntent::DeleteForward)?;
    match position {
        RenderedSemanticPosition::SoftBreakBoundary { run, .. }
        | RenderedSemanticPosition::ParagraphBoundary { run, .. } => {
            Some(canonical_newline_run_plan(&run, 0, run.left_point))
        }
        RenderedSemanticPosition::BoundaryWithSoftBreakSlot { run, slot } => {
            let cursor = match slot {
                RenderedNewlineRunSlot::Separator => Point::new(run.left_point.row + 2, 0),
                RenderedNewlineRunSlot::SoftBreakSlot => Point::new(run.left_point.row + 2, 0),
                _ => Point::new(run.left_point.row + 2, 0),
            };
            Some(canonical_newline_run_plan(&run, 2, cursor))
        }
        RenderedSemanticPosition::EmptyParagraph { run, slot } => Some(
            delete_empty_paragraph_plan(&run, slot, HorizontalDirection::Right),
        ),
        RenderedSemanticPosition::RenderedElement { range }
        | RenderedSemanticPosition::ProjectionReplacement { range } => {
            Some(single_edit_plan(snapshot, range.clone(), String::new(), 0))
        }
        _ => None,
    }
}

fn rendered_semantic_position(
    context: &RenderedEditContext<'_>,
    cursor: Point,
    intent: RenderedEditIntent,
) -> Option<RenderedSemanticPosition> {
    let snapshot = context.snapshot;
    match intent {
        RenderedEditIntent::InsertParagraphBreak => {
            if let Some(exit) = rendered_line_exit_at_cursor(snapshot, cursor) {
                return Some(RenderedSemanticPosition::LinePrefix {
                    range: exit.range,
                    replacement: exit.replacement,
                    cursor_offset: exit.cursor_offset_after_edit,
                });
            }
            newline_run_position_at_cursor(context, cursor)
        }
        RenderedEditIntent::InsertSoftBreak => newline_run_position_at_cursor(context, cursor),
        RenderedEditIntent::DeleteBackward => {
            if let Some(removal) = rendered_line_prefix_removal_at_cursor(snapshot, cursor) {
                return Some(RenderedSemanticPosition::LinePrefix {
                    range: removal.range,
                    replacement: removal.replacement,
                    cursor_offset: removal.cursor_offset_after_edit,
                });
            }
            newline_run_position_for_delete(context, cursor, HorizontalDirection::Left)
                .or_else(|| {
                    rendered_element_range_at_cursor(snapshot, cursor, HorizontalDirection::Left)
                        .map(|range| RenderedSemanticPosition::RenderedElement { range })
                })
                .or_else(|| {
                    projection_replacement_range_at_cursor(
                        snapshot,
                        cursor,
                        HorizontalDirection::Left,
                    )
                    .map(|range| RenderedSemanticPosition::ProjectionReplacement { range })
                })
        }
        RenderedEditIntent::DeleteForward => {
            newline_run_position_for_delete(context, cursor, HorizontalDirection::Right)
                .or_else(|| {
                    rendered_element_range_at_cursor(snapshot, cursor, HorizontalDirection::Right)
                        .map(|range| RenderedSemanticPosition::RenderedElement { range })
                })
                .or_else(|| {
                    projection_replacement_range_at_cursor(
                        snapshot,
                        cursor,
                        HorizontalDirection::Right,
                    )
                    .map(|range| RenderedSemanticPosition::ProjectionReplacement { range })
                })
        }
    }
}

fn newline_run_position_at_cursor(
    context: &RenderedEditContext<'_>,
    cursor: Point,
) -> Option<RenderedSemanticPosition> {
    let snapshot = context.snapshot;
    let text_snapshot = snapshot.as_text_snapshot();
    let cursor = text_snapshot.clip_point(cursor, md_text::Bias::Left);
    let run = if source_row_is_blank(snapshot, cursor.row as usize) {
        context.topology.newline_run_containing_row(cursor.row as usize)
    } else {
        context.topology.newline_run_after_line_end(cursor)
    }?;
    if !newline_run_is_between_editable_items(&context.index, &run)
        && !run_ends_at_document_tail(&run)
    {
        return None;
    }
    Some(position_for_newline_run(&context.topology, run, cursor))
}

fn newline_run_position_for_delete(
    context: &RenderedEditContext<'_>,
    cursor: Point,
    direction: HorizontalDirection,
) -> Option<RenderedSemanticPosition> {
    let snapshot = context.snapshot;
    let text_snapshot = snapshot.as_text_snapshot();
    let cursor = text_snapshot.clip_point(cursor, md_text::Bias::Left);
    let run = if source_row_is_blank(snapshot, cursor.row as usize) {
        context.topology.newline_run_containing_row(cursor.row as usize)
    } else {
        match direction {
            HorizontalDirection::Left => context.topology.newline_run_before_line_start(cursor),
            HorizontalDirection::Right => context.topology.newline_run_after_line_end(cursor),
        }
    }?;
    if !newline_run_is_between_editable_items(&context.index, &run)
        && !run_ends_at_document_tail(&run)
    {
        return None;
    }
    Some(position_for_newline_run(&context.topology, run, cursor))
}

fn position_for_newline_run(
    topology: &RenderedTopology<'_>,
    run: RenderedNewlineRun,
    cursor: Point,
) -> RenderedSemanticPosition {
    let kind = topology.classify_newline_run(&run);
    let slot = slot_for_newline_run(&run, cursor, kind);
    match topology.classify_newline_run(&run) {
        RenderedNewlineRunKind::SoftBreak => {
            RenderedSemanticPosition::SoftBreakBoundary { run, slot }
        }
        RenderedNewlineRunKind::ParagraphBoundary => {
            RenderedSemanticPosition::ParagraphBoundary { run, slot }
        }
        RenderedNewlineRunKind::BoundaryWithSoftBreakSlot => {
            RenderedSemanticPosition::BoundaryWithSoftBreakSlot { run, slot }
        }
        RenderedNewlineRunKind::EmptyParagraphs { .. } => {
            RenderedSemanticPosition::EmptyParagraph { run, slot }
        }
    }
}

fn slot_for_newline_run(
    run: &RenderedNewlineRun,
    cursor: Point,
    kind: RenderedNewlineRunKind,
) -> RenderedNewlineRunSlot {
    if cursor == run.left_point {
        return RenderedNewlineRunSlot::LeftBoundary;
    }
    if cursor == run.right_point {
        return RenderedNewlineRunSlot::RightBoundary;
    }

    let Some(row_delta) = cursor.row.checked_sub(run.left_point.row) else {
        return RenderedNewlineRunSlot::LeftBoundary;
    };
    let row_delta = row_delta as usize;

    match kind {
        RenderedNewlineRunKind::SoftBreak => RenderedNewlineRunSlot::RightBoundary,
        RenderedNewlineRunKind::ParagraphBoundary => RenderedNewlineRunSlot::Separator,
        RenderedNewlineRunKind::BoundaryWithSoftBreakSlot => {
            if row_delta <= 1 {
                RenderedNewlineRunSlot::Separator
            } else {
                RenderedNewlineRunSlot::SoftBreakSlot
            }
        }
        RenderedNewlineRunKind::EmptyParagraphs {
            count,
            has_soft_break_slot,
        } => {
            if row_delta % 2 == 0 {
                let index = row_delta / 2;
                if index <= count {
                    RenderedNewlineRunSlot::EmptyParagraph(index)
                } else if has_soft_break_slot {
                    RenderedNewlineRunSlot::SoftBreakSlot
                } else {
                    RenderedNewlineRunSlot::RightBoundary
                }
            } else {
                RenderedNewlineRunSlot::Separator
            }
        }
    }
}

fn newline_run_is_between_editable_items(
    index: &RenderedDisplayIndex,
    run: &RenderedNewlineRun,
) -> bool {
    let editable = |item_index| {
        index.item(item_index).is_some_and(|item| {
            matches!(
                item.kind,
                RenderedDisplayItemKind::Paragraph
                    | RenderedDisplayItemKind::Heading
                    | RenderedDisplayItemKind::EmptyParagraph
            )
        })
    };
    run.left_item.is_some_and(editable) && run.right_item.is_some_and(editable)
}

fn run_ends_at_document_tail(run: &RenderedNewlineRun) -> bool {
    run.right_item == run.left_item && run.right_point.column == 0
}

fn run_right_is_document_tail(snapshot: &BufferSnapshot, run: &RenderedNewlineRun) -> bool {
    let text_snapshot = snapshot.as_text_snapshot();
    run.right_point.row as usize == text_snapshot.row_count().saturating_sub(1) as usize
        && source_row_is_blank(snapshot, run.right_point.row as usize)
}

struct RenderedNewlineInsertion {
    text: String,
    cursor_delta: usize,
}

fn rendered_plain_paragraph_break_insertion(
    context: &RenderedEditContext<'_>,
    cursor: Point,
) -> RenderedNewlineInsertion {
    let snapshot = context.snapshot;
    if let Some(insertion) =
        rendered_line_continuation_insertion(snapshot.as_text_snapshot(), cursor)
    {
        return insertion;
    }

    let text_snapshot = snapshot.as_text_snapshot();
    let source_offset = text_snapshot.point_to_offset(cursor);
    let Some(item_index) = context
        .index
        .item_index_for_source_offset(snapshot, source_offset)
    else {
        return RenderedNewlineInsertion {
            text: "\n\n".to_string(),
            cursor_delta: 2,
        };
    };
    let Some(item) = context.index.item(item_index) else {
        return RenderedNewlineInsertion {
            text: "\n\n".to_string(),
            cursor_delta: 2,
        };
    };
    if matches!(
        item.kind,
        RenderedDisplayItemKind::Paragraph | RenderedDisplayItemKind::Heading
    ) && source_offset == item.source_range.end
        && item.row_range.end >= text_snapshot.row_count() as usize
    {
        return RenderedNewlineInsertion {
            text: "\n\n\n".to_string(),
            cursor_delta: 2,
        };
    }

    RenderedNewlineInsertion {
        text: "\n\n".to_string(),
        cursor_delta: 2,
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

struct RenderedLineExit {
    range: Range<usize>,
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

struct RenderedLinePrefixRemoval {
    range: Range<usize>,
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

fn delete_empty_paragraph_plan(
    run: &RenderedNewlineRun,
    slot: RenderedNewlineRunSlot,
    direction: HorizontalDirection,
) -> RenderedEditPlan {
    let current_count = empty_paragraph_count(run.newline_count);
    let next_count = current_count.saturating_sub(1);
    let next_newline_count = if next_count == 0 {
        2
    } else {
        canonical_newline_count_for_empty_paragraphs(next_count)
    };
    let cursor = match direction {
        HorizontalDirection::Left => {
            if let RenderedNewlineRunSlot::EmptyParagraph(index) = slot
                && index > 1
            {
                empty_paragraph_point(run, index - 1)
            } else {
                run.left_point
            }
        }
        HorizontalDirection::Right => Point::new(run.left_point.row + next_newline_count as u32, 0),
    };
    canonical_newline_run_plan(run, next_newline_count, cursor)
}

fn empty_paragraph_point(run: &RenderedNewlineRun, empty_paragraph_index: usize) -> Point {
    Point::new(run.left_point.row + (2 * empty_paragraph_index) as u32, 0)
}

fn canonical_newline_run_plan(
    run: &RenderedNewlineRun,
    newline_count: usize,
    cursor: Point,
) -> RenderedEditPlan {
    let replacement = "\n".repeat(newline_count);
    RenderedEditPlan {
        edits: vec![(run.source_range.clone(), replacement)],
        selection_after: collapsed_selection(cursor),
        normalize_blank_run_after_delete: false,
    }
}

fn empty_paragraph_count(newline_count: usize) -> usize {
    newline_count.saturating_sub(2) / 2
}

fn canonical_newline_count_for_empty_paragraphs(count: usize) -> usize {
    2 + 2 * count
}

fn delete_rendered_selection_plan(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> RenderedEditPlan {
    let range = selection_byte_range_in_text_snapshot(snapshot.as_text_snapshot(), selection);
    let cursor = snapshot.as_text_snapshot().offset_to_point(range.start);
    RenderedEditPlan {
        edits: vec![(range, String::new())],
        selection_after: collapsed_selection(cursor),
        normalize_blank_run_after_delete: true,
    }
}

fn replace_selection_plan(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    replacement: &str,
    cursor_delta: usize,
) -> RenderedEditPlan {
    let range = selection_byte_range_in_text_snapshot(snapshot.as_text_snapshot(), selection);
    single_edit_plan(snapshot, range, replacement.to_string(), cursor_delta)
}

fn single_edit_plan(
    snapshot: &BufferSnapshot,
    range: Range<usize>,
    replacement: String,
    cursor_delta: usize,
) -> RenderedEditPlan {
    let cursor = point_after_inserted_text(snapshot, range.start, &replacement, cursor_delta);
    RenderedEditPlan {
        edits: vec![(range, replacement)],
        selection_after: collapsed_selection(cursor),
        normalize_blank_run_after_delete: false,
    }
}

fn single_edit_plan_from_offset(
    snapshot: &BufferSnapshot,
    range: Range<usize>,
    replacement: String,
    cursor_offset_after_edit: usize,
) -> RenderedEditPlan {
    let cursor_offset_in_replacement = cursor_offset_after_edit
        .saturating_sub(range.start)
        .min(replacement.len());
    single_edit_plan(snapshot, range, replacement, cursor_offset_in_replacement)
}

fn move_only_plan(cursor: Point) -> RenderedEditPlan {
    RenderedEditPlan {
        edits: Vec::new(),
        selection_after: Selection {
            id: 0,
            start: cursor,
            end: cursor,
            reversed: false,
            goal: SelectionGoal::None,
        },
        normalize_blank_run_after_delete: false,
    }
}

pub(crate) fn rendered_blank_run_normalization_after_delete(
    snapshot: &BufferSnapshot,
    cursor_offset: usize,
) -> Option<(Range<usize>, String)> {
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

fn blank_run_containing_row(snapshot: &BufferSnapshot, row: usize) -> Range<usize> {
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

fn point_after_inserted_text(
    snapshot: &BufferSnapshot,
    range_start: usize,
    replacement: &str,
    cursor_delta: usize,
) -> Point {
    let mut point = snapshot.as_text_snapshot().offset_to_point(range_start);
    for ch in replacement
        .get(..cursor_delta.min(replacement.len()))
        .unwrap_or(replacement)
        .chars()
    {
        if ch == '\n' {
            point.row += 1;
            point.column = 0;
        } else {
            point.column += ch.len_utf8() as u32;
        }
    }
    point
}

fn source_row_is_blank(snapshot: &BufferSnapshot, row: usize) -> bool {
    snapshot
        .as_text_snapshot()
        .text_for_range(row_source_range(snapshot, row as u32))
        .all(|chunk| chunk.trim().is_empty())
}
