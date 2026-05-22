use std::ops::Range;

use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, Render, SharedString,
    StatefulInteractiveElement, TextAlign, Window, div, prelude::*, px, uniform_list,
};
use md_buffer::{Buffer, BufferSnapshot};
use md_text::{Bias, Point};

gpui::actions!(
    md_editor,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveToBeginningOfLine,
        MoveToEndOfLine,
    ]
);

pub struct MarkdownEditor {
    buffer: Buffer,
    focus_handle: FocusHandle,
    cursor: Point,
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
            cursor: Point::zero(),
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
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: Point) {
        self.cursor = clip_cursor(&self.buffer.snapshot(), cursor);
    }

    pub fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.cursor = move_left(&self.buffer.snapshot(), self.cursor);
        cx.notify();
    }

    pub fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.cursor = move_right(&self.buffer.snapshot(), self.cursor);
        cx.notify();
    }

    pub fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.cursor = move_vertical(&self.buffer.snapshot(), self.cursor, -1);
        cx.notify();
    }

    pub fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.cursor = move_vertical(&self.buffer.snapshot(), self.cursor, 1);
        cx.notify();
    }

    pub fn move_to_beginning_of_line(
        &mut self,
        _: &MoveToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cursor = move_to_beginning_of_line(&self.buffer.snapshot(), self.cursor);
        cx.notify();
    }

    pub fn move_to_end_of_line(
        &mut self,
        _: &MoveToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.cursor = move_to_end_of_line(&self.buffer.snapshot(), self.cursor);
        cx.notify();
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
        let cursor = self.cursor;

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
                        let cursor = clip_cursor(&this.buffer.snapshot(), cursor);
                        this.display_rows(range)
                            .into_iter()
                            .map(|display_row| {
                                let is_cursor_row = display_row.row == cursor.row;
                                div()
                                    .id(display_row.row as usize)
                                    .min_h(px(22.))
                                    .flex()
                                    .items_baseline()
                                    .when(is_cursor_row, |this| this.bg(gpui::rgba(0xffffff10)))
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
                                            .children(render_row_text(display_row, cursor)),
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

fn render_row_text(display_row: DisplayRow, cursor: Point) -> Vec<gpui::AnyElement> {
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
}
