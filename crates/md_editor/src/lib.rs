use std::ops::Range;

use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Render, SharedString, StatefulInteractiveElement, TextAlign, TextRun, Window,
    div, font, prelude::*, px, uniform_list,
};
use md_buffer::{Buffer, BufferSnapshot};
use md_text::{Bias, Point, Selection, SelectionGoal};

gpui::actions!(
    md_editor,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveToBeginningOfLine,
        MoveToEndOfLine,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectToBeginningOfLine,
        SelectToEndOfLine,
        SelectAll,
    ]
);

pub struct MarkdownEditor {
    buffer: Buffer,
    focus_handle: FocusHandle,
    selection: Selection<Point>,
    is_selecting_with_mouse: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisplayRow {
    pub row: u32,
    pub text: String,
}

impl MarkdownEditor {
    pub fn new(buffer: Buffer, cx: &mut Context<Self>) -> Self {
        Self {
            buffer,
            focus_handle: cx.focus_handle(),
            selection: collapsed_selection(Point::zero()),
            is_selecting_with_mouse: false,
        }
    }

    pub fn for_text(text: impl Into<String>, cx: &mut Context<Self>) -> Self {
        Self::new(Buffer::local(text), cx)
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffer
    }

    pub fn row_count(&mut self) -> u32 {
        self.buffer.snapshot().row_count()
    }

    pub fn row_text(&mut self, row: u32) -> String {
        row_text(&self.buffer.snapshot(), row)
    }

    pub fn display_rows(&mut self, range: Range<usize>) -> Vec<DisplayRow> {
        display_rows(&self.buffer.snapshot(), range)
    }

    pub fn cursor(&self) -> Point {
        self.selection.head()
    }

    pub fn selection(&self) -> &Selection<Point> {
        &self.selection
    }

    pub fn set_cursor(&mut self, cursor: Point) {
        self.selection = collapsed_selection(clip_cursor(&self.buffer.snapshot(), cursor));
    }

    pub fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = move_selection_left(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = move_selection_right(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = move_selection_vertical(&self.buffer.snapshot(), &self.selection, -1);
        cx.notify();
    }

    pub fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = move_selection_vertical(&self.buffer.snapshot(), &self.selection, 1);
        cx.notify();
    }

    pub fn move_to_beginning_of_line(
        &mut self,
        _: &MoveToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection =
            move_selection_to_beginning_of_line(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn move_to_end_of_line(
        &mut self,
        _: &MoveToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = move_selection_to_end_of_line(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = select_left(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = select_right(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = select_vertical(&self.buffer.snapshot(), &self.selection, -1);
        cx.notify();
    }

    pub fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = select_vertical(&self.buffer.snapshot(), &self.selection, 1);
        cx.notify();
    }

    pub fn select_to_beginning_of_line(
        &mut self,
        _: &SelectToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = select_to_beginning_of_line(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_to_end_of_line(
        &mut self,
        _: &SelectToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = select_to_end_of_line(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let max_point = self.buffer.snapshot().as_text_snapshot().max_point();
        self.selection = Selection {
            id: 0,
            start: Point::zero(),
            end: max_point,
            reversed: false,
            goal: SelectionGoal::None,
        };
        cx.notify();
    }

    fn mouse_left_down_on_row(
        &mut self,
        display_row: &DisplayRow,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle.clone(), cx);
        self.is_selecting_with_mouse = true;

        let snapshot = self.buffer.snapshot();
        let point = point_for_mouse_x(&snapshot, display_row, event.position.x, window);
        self.selection = if event.modifiers.shift {
            select_to_point(&snapshot, &self.selection, point)
        } else {
            collapsed_selection(point)
        };
        cx.notify();
    }

    fn mouse_move_on_row(
        &mut self,
        display_row: &DisplayRow,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting_with_mouse || event.pressed_button != Some(MouseButton::Left) {
            return;
        }

        let snapshot = self.buffer.snapshot();
        let point = point_for_mouse_x(&snapshot, display_row, event.position.x, window);
        self.selection = select_to_point(&snapshot, &self.selection, point);
        cx.notify();
    }

    fn mouse_left_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting_with_mouse = false;
    }
}

impl Focusable for MarkdownEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row_count = self.row_count() as usize;
        let selection = self.selection.clone();

        div()
            .id("md-editor")
            .size_full()
            .key_context("MarkdownEditor")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_to_beginning_of_line))
            .on_action(cx.listener(Self::move_to_end_of_line))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_to_beginning_of_line))
            .on_action(cx.listener(Self::select_to_end_of_line))
            .on_action(cx.listener(Self::select_all))
            .bg(gpui::rgb(0x181818))
            .text_color(gpui::rgb(0xd6d6d6))
            .font_family("Zed Mono")
            .text_size(px(14.))
            .line_height(px(22.))
            .overflow_y_scroll()
            .child(
                uniform_list(
                    "md-editor-display-rows",
                    row_count,
                    cx.processor(move |this, range, _window, _cx| {
                        let snapshot = this.buffer.snapshot();
                        let selection = clip_selection(&snapshot, &selection);
                        let cursor = selection.head();
                        display_rows(&snapshot, range)
                            .into_iter()
                            .map(|display_row| {
                                let is_cursor_row = display_row.row == cursor.row;
                                let mouse_row = display_row.clone();
                                let mouse_move_row = display_row.clone();
                                div()
                                    .id(display_row.row as usize)
                                    .min_h(px(22.))
                                    .flex()
                                    .items_baseline()
                                    .when(is_cursor_row, |this| this.bg(gpui::rgba(0xffffff10)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        _cx.listener(move |this, event, window, cx| {
                                            this.mouse_left_down_on_row(
                                                &mouse_row, event, window, cx,
                                            )
                                        }),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        _cx.listener(Self::mouse_left_up),
                                    )
                                    .on_mouse_up_out(
                                        MouseButton::Left,
                                        _cx.listener(Self::mouse_left_up),
                                    )
                                    .on_mouse_move(_cx.listener(move |this, event, window, cx| {
                                        this.mouse_move_on_row(&mouse_move_row, event, window, cx)
                                    }))
                                    .child(
                                        div()
                                            .w(px(48.))
                                            .pr_2()
                                            .text_align(TextAlign::Right)
                                            .text_color(if is_cursor_row {
                                                gpui::rgba(0xffffffcc)
                                            } else {
                                                gpui::rgba(0xffffff66)
                                            })
                                            .child(SharedString::from(
                                                (display_row.row + 1).to_string(),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .items_baseline()
                                            .whitespace_nowrap()
                                            .children(render_row_text(display_row, &selection)),
                                    )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .h_full(),
            )
    }
}

pub fn display_rows(snapshot: &BufferSnapshot, range: Range<usize>) -> Vec<DisplayRow> {
    let row_count = snapshot.row_count() as usize;
    let start = range.start.min(row_count);
    let end = range.end.min(row_count);

    (start..end)
        .map(|row| DisplayRow {
            row: row as u32,
            text: row_text(snapshot, row as u32),
        })
        .collect()
}

pub fn row_text(snapshot: &BufferSnapshot, row: u32) -> String {
    let text_snapshot = snapshot.as_text_snapshot();
    if row >= text_snapshot.row_count() {
        return String::new();
    }

    let start = text_snapshot.point_to_offset(Point::new(row, 0));
    let end = start + text_snapshot.line_len(row) as usize;
    text_snapshot.text_for_range(start..end).collect()
}

pub fn clip_cursor(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    snapshot.as_text_snapshot().clip_point(cursor, Bias::Left)
}

pub fn clip_selection(snapshot: &BufferSnapshot, selection: &Selection<Point>) -> Selection<Point> {
    let head = clip_cursor(snapshot, selection.head());
    let tail = clip_cursor(snapshot, selection.tail());
    let mut clipped = selection.clone();
    clipped.set_head_tail(head, tail, selection.goal);
    clipped.id = selection.id;
    clipped
}

pub fn collapsed_selection(point: Point) -> Selection<Point> {
    Selection {
        id: 0,
        start: point,
        end: point,
        reversed: false,
        goal: SelectionGoal::None,
    }
}

pub fn move_left(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    let text_snapshot = snapshot.as_text_snapshot();
    let offset = text_snapshot.point_to_offset(clip_cursor(snapshot, cursor));
    if offset == 0 {
        return Point::zero();
    }

    text_snapshot.offset_to_point(
        text_snapshot
            .as_rope()
            .floor_char_boundary(offset.saturating_sub(1)),
    )
}

pub fn move_right(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    let text_snapshot = snapshot.as_text_snapshot();
    let offset = text_snapshot.point_to_offset(clip_cursor(snapshot, cursor));
    if offset >= text_snapshot.len() {
        return text_snapshot.max_point();
    }

    text_snapshot.offset_to_point(
        text_snapshot
            .as_rope()
            .ceil_char_boundary(offset.saturating_add(1)),
    )
}

pub fn move_vertical(snapshot: &BufferSnapshot, cursor: Point, delta_rows: i32) -> Point {
    let text_snapshot = snapshot.as_text_snapshot();
    let current = clip_cursor(snapshot, cursor);
    let max_row = text_snapshot.row_count().saturating_sub(1);
    let target_row = if delta_rows.is_negative() {
        current.row.saturating_sub(delta_rows.unsigned_abs())
    } else {
        current.row.saturating_add(delta_rows as u32).min(max_row)
    };

    point_for_row_and_column(snapshot, target_row, current.column)
}

pub fn move_to_beginning_of_line(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    Point::new(clip_cursor(snapshot, cursor).row, 0)
}

pub fn move_to_end_of_line(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    let row = clip_cursor(snapshot, cursor).row;
    Point::new(row, snapshot.as_text_snapshot().line_len(row))
}

pub fn move_selection_left(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    if selection.is_empty() {
        collapsed_selection(move_left(snapshot, selection.head()))
    } else {
        collapsed_selection(selection.start)
    }
}

pub fn move_selection_right(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    if selection.is_empty() {
        collapsed_selection(move_right(snapshot, selection.head()))
    } else {
        collapsed_selection(selection.end)
    }
}

pub fn move_selection_vertical(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    delta_rows: i32,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    collapsed_selection(move_vertical(snapshot, selection.head(), delta_rows))
}

pub fn move_selection_to_beginning_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    collapsed_selection(move_to_beginning_of_line(snapshot, selection.head()))
}

pub fn move_selection_to_end_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    collapsed_selection(move_to_end_of_line(snapshot, selection.head()))
}

pub fn select_left(snapshot: &BufferSnapshot, selection: &Selection<Point>) -> Selection<Point> {
    select_to_point(snapshot, selection, move_left(snapshot, selection.head()))
}

pub fn select_right(snapshot: &BufferSnapshot, selection: &Selection<Point>) -> Selection<Point> {
    select_to_point(snapshot, selection, move_right(snapshot, selection.head()))
}

pub fn select_vertical(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    delta_rows: i32,
) -> Selection<Point> {
    select_to_point(
        snapshot,
        selection,
        move_vertical(snapshot, selection.head(), delta_rows),
    )
}

pub fn select_to_beginning_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_point(
        snapshot,
        selection,
        move_to_beginning_of_line(snapshot, selection.head()),
    )
}

pub fn select_to_end_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_point(
        snapshot,
        selection,
        move_to_end_of_line(snapshot, selection.head()),
    )
}

pub fn select_to_point(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    head: Point,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    let mut updated = selection.clone();
    updated.set_head(head, SelectionGoal::None);
    updated
}

fn point_for_row_and_column(snapshot: &BufferSnapshot, row: u32, column: u32) -> Point {
    let text_snapshot = snapshot.as_text_snapshot();
    let row = row.min(text_snapshot.row_count().saturating_sub(1));
    let row_start = text_snapshot.point_to_offset(Point::new(row, 0));
    let row_end = row_start + text_snapshot.line_len(row) as usize;
    let target = row_start.saturating_add(column as usize).min(row_end);
    let target = text_snapshot
        .as_rope()
        .floor_char_boundary(target)
        .max(row_start);

    text_snapshot.offset_to_point(target)
}

fn point_for_mouse_x(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    x: gpui::Pixels,
    window: &mut Window,
) -> Point {
    let text_x = (x - px(48.)).max(px(0.));
    let shaped_line = window.text_system().shape_line(
        SharedString::from(display_row.text.clone()),
        px(14.),
        &[TextRun {
            len: display_row.text.len(),
            font: font("Zed Mono"),
            color: gpui::rgb(0xd6d6d6).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let column = shaped_line.closest_index_for_x(text_x) as u32;

    Point::new(
        display_row.row,
        clip_cursor(snapshot, Point::new(display_row.row, column)).column,
    )
}

fn render_row_text(display_row: DisplayRow, selection: &Selection<Point>) -> Vec<gpui::AnyElement> {
    let selection = selection.clone();
    if selection.is_empty() {
        return render_caret_row(display_row, selection.head());
    }

    let Some(selected_range) = selected_range_for_row(&display_row, &selection) else {
        return vec![SharedString::from(display_row.text).into_any_element()];
    };
    if selected_range.is_empty() {
        return vec![SharedString::from(display_row.text).into_any_element()];
    }

    let before = SharedString::from(display_row.text[..selected_range.start].to_string());
    let selected = SharedString::from(display_row.text[selected_range.clone()].to_string());
    let after = SharedString::from(display_row.text[selected_range.end..].to_string());

    vec![
        before.into_any_element(),
        div()
            .bg(gpui::rgb(0x264f78))
            .text_color(gpui::rgb(0xf5fbff))
            .child(selected)
            .into_any_element(),
        after.into_any_element(),
    ]
}

fn render_caret_row(display_row: DisplayRow, cursor: Point) -> Vec<gpui::AnyElement> {
    if display_row.row != cursor.row {
        return vec![SharedString::from(display_row.text).into_any_element()];
    }

    let column = (cursor.column as usize).min(display_row.text.len());
    let column = display_row.text.floor_char_boundary(column);
    let left = SharedString::from(display_row.text[..column].to_string());
    let right = SharedString::from(display_row.text[column..].to_string());

    vec![
        left.into_any_element(),
        div()
            .w(px(1.))
            .h(px(17.))
            .bg(gpui::rgb(0xf0f0f0))
            .into_any_element(),
        right.into_any_element(),
    ]
}

fn selected_range_for_row(
    display_row: &DisplayRow,
    selection: &Selection<Point>,
) -> Option<Range<usize>> {
    if selection.is_empty() {
        return None;
    }

    let selection_range = selection.range();
    let row = display_row.row;
    if row < selection_range.start.row || row > selection_range.end.row {
        return None;
    }

    let mut start = if row == selection_range.start.row {
        selection_range.start.column as usize
    } else {
        0
    };
    let mut end = if row == selection_range.end.row {
        selection_range.end.column as usize
    } else {
        display_row.text.len()
    };

    start = display_row
        .text
        .floor_char_boundary(start.min(display_row.text.len()));
    end = display_row
        .text
        .floor_char_boundary(end.min(display_row.text.len()));

    Some(start.min(end)..end.max(start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_rows_preserve_empty_lines_and_final_empty_row() {
        let mut buffer = Buffer::local("alpha\n\nbeta\n");
        let snapshot = buffer.snapshot();

        assert_eq!(
            display_rows(&snapshot, 0..snapshot.row_count() as usize),
            vec![
                DisplayRow {
                    row: 0,
                    text: "alpha".to_string(),
                },
                DisplayRow {
                    row: 1,
                    text: String::new(),
                },
                DisplayRow {
                    row: 2,
                    text: "beta".to_string(),
                },
                DisplayRow {
                    row: 3,
                    text: String::new(),
                },
            ]
        );
    }

    #[test]
    fn display_rows_clips_requested_range_to_buffer_rows() {
        let mut buffer = Buffer::local("one\ntwo");
        let snapshot = buffer.snapshot();

        assert_eq!(
            display_rows(&snapshot, 1..10),
            vec![DisplayRow {
                row: 1,
                text: "two".to_string(),
            }]
        );
    }

    #[test]
    fn row_text_returns_empty_string_for_out_of_bounds_rows() {
        let mut buffer = Buffer::local("one");
        let snapshot = buffer.snapshot();

        assert_eq!(row_text(&snapshot, 1), "");
    }

    #[test]
    fn horizontal_movement_crosses_lines_and_respects_utf8_boundaries() {
        let mut buffer = Buffer::local("a\nβ");
        let snapshot = buffer.snapshot();

        let cursor = move_right(&snapshot, Point::zero());
        assert_eq!(cursor, Point::new(0, 1));
        let cursor = move_right(&snapshot, cursor);
        assert_eq!(cursor, Point::new(1, 0));
        let cursor = move_right(&snapshot, cursor);
        assert_eq!(cursor, Point::new(1, "β".len() as u32));
        assert_eq!(move_right(&snapshot, cursor), cursor);

        let cursor = move_left(&snapshot, cursor);
        assert_eq!(cursor, Point::new(1, 0));
        let cursor = move_left(&snapshot, cursor);
        assert_eq!(cursor, Point::new(0, 1));
        let cursor = move_left(&snapshot, cursor);
        assert_eq!(cursor, Point::zero());
        assert_eq!(move_left(&snapshot, cursor), Point::zero());
    }

    #[test]
    fn vertical_movement_clips_to_target_line_end() {
        let mut buffer = Buffer::local("abcd\nx\nβγ");
        let snapshot = buffer.snapshot();

        assert_eq!(
            move_vertical(&snapshot, Point::new(0, 3), 1),
            Point::new(1, 1)
        );
        assert_eq!(
            move_vertical(&snapshot, Point::new(0, 3), 2),
            Point::new(2, "β".len() as u32)
        );
        assert_eq!(
            move_vertical(&snapshot, Point::new(2, 2), -1),
            Point::new(1, 1)
        );
    }

    #[test]
    fn line_boundary_movement_uses_current_row() {
        let mut buffer = Buffer::local("abc\nβ");
        let snapshot = buffer.snapshot();

        assert_eq!(
            move_to_beginning_of_line(&snapshot, Point::new(1, 1)),
            Point::new(1, 0)
        );
        assert_eq!(
            move_to_end_of_line(&snapshot, Point::new(1, 0)),
            Point::new(1, "β".len() as u32)
        );
    }

    #[test]
    fn moving_left_collapses_non_empty_selection_to_start() {
        let mut buffer = Buffer::local("abcd");
        let snapshot = buffer.snapshot();
        let selection = Selection {
            id: 1,
            start: Point::new(0, 1),
            end: Point::new(0, 3),
            reversed: false,
            goal: SelectionGoal::None,
        };

        assert_eq!(
            move_selection_left(&snapshot, &selection),
            collapsed_selection(Point::new(0, 1))
        );
    }

    #[test]
    fn select_left_moves_head_and_preserves_tail() {
        let mut buffer = Buffer::local("abcd");
        let snapshot = buffer.snapshot();
        let selection = collapsed_selection(Point::new(0, 3));

        assert_eq!(
            select_left(&snapshot, &selection),
            Selection {
                id: 0,
                start: Point::new(0, 2),
                end: Point::new(0, 3),
                reversed: true,
                goal: SelectionGoal::None,
            }
        );
    }

    #[test]
    fn selected_range_for_row_handles_multiline_selection() {
        let selection = Selection {
            id: 1,
            start: Point::new(0, 2),
            end: Point::new(2, 1),
            reversed: false,
            goal: SelectionGoal::None,
        };

        assert_eq!(
            selected_range_for_row(
                &DisplayRow {
                    row: 0,
                    text: "abcd".to_string(),
                },
                &selection
            ),
            Some(2..4)
        );
        assert_eq!(
            selected_range_for_row(
                &DisplayRow {
                    row: 1,
                    text: "xy".to_string(),
                },
                &selection
            ),
            Some(0..2)
        );
        assert_eq!(
            selected_range_for_row(
                &DisplayRow {
                    row: 2,
                    text: "pq".to_string(),
                },
                &selection
            ),
            Some(0..1)
        );
    }
}
