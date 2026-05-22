use std::ops::Range;

use gpui::{
    App, Context, FocusHandle, Focusable, IntoElement, Render, SharedString,
    StatefulInteractiveElement, TextAlign, Window, div, prelude::*, px, uniform_list,
};
use md_buffer::Buffer;
use md_text::Point;

pub struct MarkdownEditor {
    buffer: Buffer,
    focus_handle: FocusHandle,
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
}

impl Focusable for MarkdownEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row_count = self.row_count() as usize;

        div()
            .id("md-editor")
            .size_full()
            .key_context("MarkdownEditor")
            .track_focus(&self.focus_handle(cx))
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
                    cx.processor(|this, range, _window, _cx| {
                        this.display_rows(range)
                            .into_iter()
                            .map(|display_row| {
                                div()
                                    .id(display_row.row as usize)
                                    .min_h(px(22.))
                                    .flex()
                                    .items_baseline()
                                    .child(
                                        div()
                                            .w(px(48.))
                                            .pr_2()
                                            .text_align(TextAlign::Right)
                                            .text_color(gpui::rgba(0xffffff66))
                                            .child(SharedString::from(
                                                (display_row.row + 1).to_string(),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .whitespace_nowrap()
                                            .child(SharedString::from(display_row.text)),
                                    )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .h_full(),
            )
    }
}

pub fn display_rows(snapshot: &md_buffer::BufferSnapshot, range: Range<usize>) -> Vec<DisplayRow> {
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

pub fn row_text(snapshot: &md_buffer::BufferSnapshot, row: u32) -> String {
    let text_snapshot = snapshot.as_text_snapshot();
    if row >= text_snapshot.row_count() {
        return String::new();
    }

    let start = text_snapshot.point_to_offset(Point::new(row, 0));
    let end = start + text_snapshot.line_len(row) as usize;
    text_snapshot.text_for_range(start..end).collect()
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
}
