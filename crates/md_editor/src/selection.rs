use std::ops::Range;

use gpui::Pixels;
use md_buffer::BufferSnapshot;
use md_text::{Bias, BufferSnapshot as TextBufferSnapshot, Point, Selection, SelectionGoal};

use super::{
    MarkdownEditorMode, MdListState, merge_overlapping_row_ranges,
    rendered_element::{projection_replacement_range_at_cursor, rendered_element_range_at_cursor},
    source_range_to_row_range,
};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransactionSelectionState {
    pub(crate) before: Selection<Point>,
    pub(crate) after: Selection<Point>,
}

pub(crate) fn collapsed_selection(point: Point) -> Selection<Point> {
    collapsed_selection_with_goal(point, SelectionGoal::None)
}

pub(crate) fn collapsed_selection_with_goal(point: Point, goal: SelectionGoal) -> Selection<Point> {
    Selection {
        id: 0,
        start: point,
        end: point,
        reversed: false,
        goal,
    }
}

pub(crate) fn clip_cursor_in_text_snapshot(snapshot: &TextBufferSnapshot, cursor: Point) -> Point {
    snapshot.clip_point(cursor, Bias::Left)
}

pub fn clip_cursor(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    clip_cursor_in_text_snapshot(snapshot.as_text_snapshot(), cursor)
}

pub fn clip_selection(snapshot: &BufferSnapshot, selection: &Selection<Point>) -> Selection<Point> {
    clip_selection_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub(crate) fn clip_selection_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let head = clip_cursor_in_text_snapshot(snapshot, selection.head());
    let tail = clip_cursor_in_text_snapshot(snapshot, selection.tail());
    let mut clipped = selection.clone();
    clipped.set_head_tail(head, tail, selection.goal);
    clipped.id = selection.id;
    clipped
}

pub(crate) fn selection_without_goal(selection: &Selection<Point>) -> Selection<Point> {
    let mut selection = selection.clone();
    selection.goal = SelectionGoal::None;
    selection
}

pub(crate) fn selection_without_wrapped_visual_row_goal(
    selection: &Selection<Point>,
) -> Selection<Point> {
    let mut selection = selection.clone();
    if let SelectionGoal::WrappedHorizontalPosition((_, x)) = selection.goal {
        selection.goal = SelectionGoal::HorizontalPosition(f64::from(x));
    }
    selection
}

pub(crate) fn transaction_selection_state_without_goals(
    before: Selection<Point>,
    after: Selection<Point>,
) -> TransactionSelectionState {
    TransactionSelectionState {
        before: selection_without_goal(&before),
        after: selection_without_goal(&after),
    }
}

pub(crate) fn reveal_selection_head_row_in_text_snapshot(
    display_list_state: &MdListState,
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) {
    let item_count = display_list_state.item_count();
    if item_count == 0 {
        return;
    }

    let cursor = clip_cursor_in_text_snapshot(snapshot, selection.head());
    let row = (cursor.row as usize).min(item_count.saturating_sub(1));
    display_list_state.scroll_to_reveal_item(row);
}

pub(crate) fn reveal_selection_item(display_list_state: &MdListState, item_index: Option<usize>) {
    let item_count = display_list_state.item_count();
    if item_count == 0 {
        return;
    }

    let item_index = item_index.unwrap_or(0).min(item_count.saturating_sub(1));
    display_list_state.scroll_to_reveal_item(item_index);
}

pub(crate) fn apply_text_wrap_width_change(
    last_text_wrap_width: &mut Option<Pixels>,
    selection: &mut Selection<Point>,
    wrap_width: Pixels,
) -> bool {
    if *last_text_wrap_width == Some(wrap_width) {
        return false;
    }

    *last_text_wrap_width = Some(wrap_width);
    *selection = selection_without_goal(selection);
    true
}

pub(crate) fn apply_rendered_active_source_range_change(
    selection: &mut Selection<Point>,
    previous_active: Option<&Range<usize>>,
    current_active: Option<&Range<usize>>,
) -> bool {
    if previous_active == current_active {
        return false;
    }

    *selection = selection_without_wrapped_visual_row_goal(selection);
    true
}

pub(crate) fn source_rows_for_active_range_change(
    snapshot: &BufferSnapshot,
    previous_source_range: Option<&Range<usize>>,
    current_source_range: Option<&Range<usize>>,
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    if let Some(previous_source_range) = previous_source_range
        && let Some(rows) = source_range_to_row_range(snapshot, previous_source_range)
    {
        ranges.push(rows);
    }
    if let Some(current_source_range) = current_source_range
        && let Some(rows) = source_range_to_row_range(snapshot, current_source_range)
    {
        ranges.push(rows);
    }

    merge_overlapping_row_ranges(ranges)
}

pub fn move_left(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    move_left_in_text_snapshot(snapshot.as_text_snapshot(), cursor)
}

pub(crate) fn move_left_in_text_snapshot(snapshot: &TextBufferSnapshot, cursor: Point) -> Point {
    let offset = snapshot.point_to_offset(clip_cursor_in_text_snapshot(snapshot, cursor));
    if offset == 0 {
        return Point::zero();
    }

    snapshot.offset_to_point(
        snapshot
            .as_rope()
            .floor_char_boundary(offset.saturating_sub(1)),
    )
}

pub fn move_right(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    move_right_in_text_snapshot(snapshot.as_text_snapshot(), cursor)
}

pub(crate) fn move_right_in_text_snapshot(snapshot: &TextBufferSnapshot, cursor: Point) -> Point {
    let offset = snapshot.point_to_offset(clip_cursor_in_text_snapshot(snapshot, cursor));
    if offset >= snapshot.len() {
        return snapshot.max_point();
    }

    snapshot.offset_to_point(
        snapshot
            .as_rope()
            .ceil_char_boundary(offset.saturating_add(1)),
    )
}

#[cfg(test)]
pub fn move_vertical(snapshot: &BufferSnapshot, cursor: Point, delta_rows: i32) -> Point {
    move_vertical_in_text_snapshot(snapshot.as_text_snapshot(), cursor, delta_rows)
}

pub(crate) fn move_vertical_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    cursor: Point,
    delta_rows: i32,
) -> Point {
    let current = clip_cursor_in_text_snapshot(snapshot, cursor);
    let max_row = snapshot.row_count().saturating_sub(1);
    let target_row = if delta_rows.is_negative() {
        current.row.saturating_sub(delta_rows.unsigned_abs())
    } else {
        current.row.saturating_add(delta_rows as u32).min(max_row)
    };

    point_for_row_and_column_in_text_snapshot(snapshot, target_row, current.column)
}

pub fn move_to_beginning_of_line(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    move_to_beginning_of_line_in_text_snapshot(snapshot.as_text_snapshot(), cursor)
}

pub(crate) fn move_to_beginning_of_line_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    cursor: Point,
) -> Point {
    Point::new(clip_cursor_in_text_snapshot(snapshot, cursor).row, 0)
}

pub fn move_to_end_of_line(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    move_to_end_of_line_in_text_snapshot(snapshot.as_text_snapshot(), cursor)
}

pub(crate) fn move_to_end_of_line_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    cursor: Point,
) -> Point {
    let row = clip_cursor_in_text_snapshot(snapshot, cursor).row;
    Point::new(row, snapshot.line_len(row))
}

pub(crate) fn move_selection_left(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    move_selection_left_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub(crate) fn move_selection_left_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection_in_text_snapshot(snapshot, selection);
    if selection.is_empty() {
        collapsed_selection(move_left_in_text_snapshot(snapshot, selection.head()))
    } else {
        collapsed_selection(selection.start)
    }
}

pub(crate) fn move_selection_right(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    move_selection_right_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub(crate) fn move_selection_right_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection_in_text_snapshot(snapshot, selection);
    if selection.is_empty() {
        collapsed_selection(move_right_in_text_snapshot(snapshot, selection.head()))
    } else {
        collapsed_selection(selection.end)
    }
}

pub(crate) fn move_selection_left_in_mode(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> Selection<Point> {
    if mode == MarkdownEditorMode::Source {
        return move_selection_left(snapshot, selection);
    }

    let selection = clip_selection(snapshot, selection);
    if selection.is_empty() {
        collapsed_selection(move_horizontal_in_mode(
            snapshot,
            selection.head(),
            mode,
            HorizontalDirection::Left,
        ))
    } else {
        collapsed_selection(selection.start)
    }
}

pub(crate) fn move_selection_right_in_mode(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> Selection<Point> {
    if mode == MarkdownEditorMode::Source {
        return move_selection_right(snapshot, selection);
    }

    let selection = clip_selection(snapshot, selection);
    if selection.is_empty() {
        collapsed_selection(move_horizontal_in_mode(
            snapshot,
            selection.head(),
            mode,
            HorizontalDirection::Right,
        ))
    } else {
        collapsed_selection(selection.end)
    }
}

pub(crate) fn move_selection_vertical(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    delta_rows: i32,
) -> Selection<Point> {
    move_selection_vertical_in_text_snapshot(snapshot.as_text_snapshot(), selection, delta_rows)
}

pub(crate) fn move_selection_vertical_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
    delta_rows: i32,
) -> Selection<Point> {
    let selection = clip_selection_in_text_snapshot(snapshot, selection);
    collapsed_selection(move_vertical_in_text_snapshot(
        snapshot,
        selection.head(),
        delta_rows,
    ))
}

pub(crate) fn move_selection_to_beginning_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    move_selection_to_beginning_of_line_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub(crate) fn move_selection_to_beginning_of_line_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection_in_text_snapshot(snapshot, selection);
    collapsed_selection(move_to_beginning_of_line_in_text_snapshot(
        snapshot,
        selection.head(),
    ))
}

pub(crate) fn move_selection_to_end_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    move_selection_to_end_of_line_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub(crate) fn move_selection_to_end_of_line_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection_in_text_snapshot(snapshot, selection);
    collapsed_selection(move_to_end_of_line_in_text_snapshot(
        snapshot,
        selection.head(),
    ))
}

pub fn select_left(snapshot: &BufferSnapshot, selection: &Selection<Point>) -> Selection<Point> {
    select_left_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub fn select_right(snapshot: &BufferSnapshot, selection: &Selection<Point>) -> Selection<Point> {
    select_right_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub(crate) fn select_left_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_point_in_text_snapshot(
        snapshot,
        selection,
        move_left_in_text_snapshot(snapshot, selection.head()),
    )
}

pub(crate) fn select_right_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_point_in_text_snapshot(
        snapshot,
        selection,
        move_right_in_text_snapshot(snapshot, selection.head()),
    )
}

pub(crate) fn select_left_in_mode(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> Selection<Point> {
    if mode == MarkdownEditorMode::Source {
        return select_left_in_text_snapshot(snapshot.as_text_snapshot(), selection);
    }

    select_to_point(
        snapshot,
        selection,
        move_horizontal_in_mode(snapshot, selection.head(), mode, HorizontalDirection::Left),
    )
}

pub(crate) fn select_right_in_mode(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> Selection<Point> {
    if mode == MarkdownEditorMode::Source {
        return select_right_in_text_snapshot(snapshot.as_text_snapshot(), selection);
    }

    select_to_point(
        snapshot,
        selection,
        move_horizontal_in_mode(snapshot, selection.head(), mode, HorizontalDirection::Right),
    )
}

pub fn select_vertical(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    delta_rows: i32,
) -> Selection<Point> {
    select_vertical_in_text_snapshot(snapshot.as_text_snapshot(), selection, delta_rows)
}

pub(crate) fn select_vertical_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
    delta_rows: i32,
) -> Selection<Point> {
    select_to_point_in_text_snapshot(
        snapshot,
        selection,
        move_vertical_in_text_snapshot(snapshot, selection.head(), delta_rows),
    )
}

pub fn select_to_beginning_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_beginning_of_line_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub(crate) fn select_to_beginning_of_line_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_point_in_text_snapshot(
        snapshot,
        selection,
        move_to_beginning_of_line_in_text_snapshot(snapshot, selection.head()),
    )
}

pub fn select_to_end_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_end_of_line_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub(crate) fn select_to_end_of_line_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_point_in_text_snapshot(
        snapshot,
        selection,
        move_to_end_of_line_in_text_snapshot(snapshot, selection.head()),
    )
}

pub fn select_to_point(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    head: Point,
) -> Selection<Point> {
    select_to_point_with_goal(snapshot, selection, head, SelectionGoal::None)
}

pub(crate) fn select_to_point_with_goal(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    head: Point,
    goal: SelectionGoal,
) -> Selection<Point> {
    select_to_point_in_text_snapshot_with_goal(snapshot.as_text_snapshot(), selection, head, goal)
}

pub(crate) fn select_to_point_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
    head: Point,
) -> Selection<Point> {
    select_to_point_in_text_snapshot_with_goal(snapshot, selection, head, SelectionGoal::None)
}

pub(crate) fn select_to_point_in_text_snapshot_with_goal(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
    head: Point,
    goal: SelectionGoal,
) -> Selection<Point> {
    let selection = clip_selection_in_text_snapshot(snapshot, selection);
    let mut updated = selection.clone();
    updated.set_head(head, goal);
    updated
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HorizontalDirection {
    Left,
    Right,
}

pub(crate) fn move_horizontal_in_mode(
    snapshot: &BufferSnapshot,
    cursor: Point,
    mode: MarkdownEditorMode,
    direction: HorizontalDirection,
) -> Point {
    if mode == MarkdownEditorMode::Rendered
        && let Some(point) = move_across_rendered_element(snapshot, cursor, direction)
    {
        return point;
    }
    if mode == MarkdownEditorMode::Rendered
        && let Some(point) = move_across_projection_replacement(snapshot, cursor, direction)
    {
        return point;
    }

    match direction {
        HorizontalDirection::Left => move_left(snapshot, cursor),
        HorizontalDirection::Right => move_right(snapshot, cursor),
    }
}

pub(crate) fn move_across_rendered_element(
    snapshot: &BufferSnapshot,
    cursor: Point,
    direction: HorizontalDirection,
) -> Option<Point> {
    let text_snapshot = snapshot.as_text_snapshot();
    let source_range = rendered_element_range_at_cursor(snapshot, cursor, direction)?;
    let target_offset = match direction {
        HorizontalDirection::Left => source_range.start,
        HorizontalDirection::Right => source_range.end,
    };
    Some(text_snapshot.offset_to_point(target_offset))
}

fn move_across_projection_replacement(
    snapshot: &BufferSnapshot,
    cursor: Point,
    direction: HorizontalDirection,
) -> Option<Point> {
    let text_snapshot = snapshot.as_text_snapshot();
    let source_range = projection_replacement_range_at_cursor(snapshot, cursor, direction)?;
    let target_offset = match direction {
        HorizontalDirection::Left => source_range.start,
        HorizontalDirection::Right => source_range.end,
    };
    Some(text_snapshot.offset_to_point(target_offset))
}

pub fn selection_byte_range(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Range<usize> {
    selection_byte_range_in_text_snapshot(snapshot.as_text_snapshot(), selection)
}

pub(crate) fn selection_byte_range_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    selection: &Selection<Point>,
) -> Range<usize> {
    let selection = clip_selection_in_text_snapshot(snapshot, selection);
    snapshot.point_to_offset(selection.start)..snapshot.point_to_offset(selection.end)
}

pub(crate) fn selection_for_source_range(
    snapshot: &BufferSnapshot,
    id: usize,
    source_range: Range<usize>,
) -> Selection<Point> {
    let text_snapshot = snapshot.as_text_snapshot();
    Selection {
        id,
        start: text_snapshot.offset_to_point(source_range.start),
        end: text_snapshot.offset_to_point(source_range.end),
        reversed: false,
        goal: SelectionGoal::None,
    }
}

pub(crate) fn point_for_row_and_column_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    row: u32,
    column: u32,
) -> Point {
    let row = row.min(snapshot.row_count().saturating_sub(1));
    let row_start = snapshot.point_to_offset(Point::new(row, 0));
    let row_end = row_start + snapshot.line_len(row) as usize;
    let target = row_start.saturating_add(column as usize).min(row_end);
    let target = snapshot
        .as_rope()
        .floor_char_boundary(target)
        .max(row_start);

    snapshot.offset_to_point(target)
}
