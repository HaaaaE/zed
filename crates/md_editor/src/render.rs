use std::ops::Range;

use gpui::{Context, IntoElement, MouseButton, SharedString, div, prelude::*, px};
use md_buffer::BufferSnapshot;
use md_text::{Point, Selection};
use md_theme::editor_palette;

use super::{
    DisplayInlineFragment, MarkdownEditor, RowDisplayStyle, TextBufferSnapshot,
    clip_cursor_in_text_snapshot, display_model::DisplayRow, render_text_piece,
    selected_range_for_row_in_text_snapshot,
};
use super::{
    layout::{DisplayRowLayout, DisplayRowTextLayout, VisualDisplayRow},
    visual_row::{display_x_for_offset, visual_row_index_for_caret},
};

pub(super) fn render_row_text(
    snapshot: &TextBufferSnapshot,
    display_row: &DisplayRow,
    text_layout: DisplayRowTextLayout,
    selection: &Selection<Point>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> Vec<gpui::AnyElement> {
    let selected_range = if selection.is_empty() {
        None
    } else {
        selected_range_for_row_in_text_snapshot(snapshot, display_row, selection)
            .filter(|selected_range| !selected_range.is_empty())
    };

    let mut elements = Vec::new();
    for (visual_row_index, visual_row) in text_layout.visual_rows.clone().into_iter().enumerate() {
        elements.push(render_visual_text_row(
            snapshot,
            display_row,
            &text_layout,
            visual_row_index,
            visual_row,
            selection,
            selected_range.as_ref(),
            row_style,
            cx,
        ));
    }
    elements
}

pub(super) fn render_display_row_layout(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    row_layout: DisplayRowLayout,
    selection: &Selection<Point>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> Vec<gpui::AnyElement> {
    match row_layout {
        DisplayRowLayout::Text(text_layout) => render_row_text(
            snapshot.as_text_snapshot(),
            display_row,
            text_layout,
            selection,
            row_style,
            cx,
        ),
        DisplayRowLayout::Block(block_layout) => {
            block_layout.render(snapshot, selection, row_style, cx)
        }
    }
}

pub(super) fn selection_bounds_for_visual_row(
    text_layout: &DisplayRowTextLayout,
    selected_range: Option<&Range<usize>>,
    visual_row: &VisualDisplayRow,
) -> Option<(gpui::Pixels, gpui::Pixels)> {
    let selected_range = selected_range?;
    let start = selected_range.start.max(visual_row.display_range.start);
    let end = selected_range.end.min(visual_row.display_range.end);
    if start > end {
        return None;
    }
    if start == end {
        return visual_row
            .display_range
            .is_empty()
            .then_some((px(0.), px(1.)));
    }

    let start_x = display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, start)
        - visual_row.line_start_x;
    let end_x = display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, end)
        - visual_row.line_start_x;
    Some((start_x, (end_x - start_x).max(px(1.))))
}

pub(super) fn fragment_text_for_visual_row(
    display_range: &Range<usize>,
    text: &str,
    visual_row: &VisualDisplayRow,
) -> Option<String> {
    if display_range.end <= visual_row.display_range.start
        || display_range.start >= visual_row.display_range.end
    {
        return None;
    }

    let start = display_range.start.max(visual_row.display_range.start);
    let end = display_range.end.min(visual_row.display_range.end);
    if start >= end {
        return None;
    }

    let local_start = start - display_range.start;
    let local_end = end - display_range.start;
    let text = text.get(local_start..local_end)?;
    if text.is_empty() {
        return None;
    }

    Some(text.to_string())
}

fn render_visual_text_row(
    snapshot: &TextBufferSnapshot,
    display_row: &DisplayRow,
    text_layout: &DisplayRowTextLayout,
    visual_row_index: usize,
    visual_row: VisualDisplayRow,
    selection: &Selection<Point>,
    selected_range: Option<&Range<usize>>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> gpui::AnyElement {
    let mouse_down_row = display_row.clone();
    let mouse_down_visual_row_index = visual_row_index;
    let mouse_down_visual_row = visual_row.clone();
    let mouse_move_row = display_row.clone();
    let mouse_move_visual_row_index = visual_row_index;
    let mouse_move_visual_row = visual_row.clone();

    div()
        .h(visual_row.height)
        .flex()
        .items_center()
        .relative()
        .overflow_hidden()
        .whitespace_nowrap()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event, window, cx| {
                this.mouse_left_down_on_row(
                    &mouse_down_row,
                    mouse_down_visual_row_index,
                    &mouse_down_visual_row,
                    event,
                    window,
                    cx,
                )
            }),
        )
        .on_mouse_move(cx.listener(move |this, event, window, cx| {
            this.mouse_move_on_row(
                &mouse_move_row,
                mouse_move_visual_row_index,
                &mouse_move_visual_row,
                event,
                window,
                cx,
            )
        }))
        .children(selection_elements_for_visual_row(
            text_layout,
            selected_range,
            &visual_row,
        ))
        .children(render_fragments_for_visual_row(
            &text_layout.fragments,
            &visual_row,
            selected_range,
            row_style,
        ))
        .when_some(
            caret_position_for_visual_row(
                snapshot,
                display_row,
                selection,
                text_layout,
                visual_row_index,
                &visual_row,
            ),
            |this, caret_x| this.child(caret_element(caret_x, row_style)),
        )
        .into_any_element()
}

fn selection_elements_for_visual_row(
    text_layout: &DisplayRowTextLayout,
    selected_range: Option<&Range<usize>>,
    visual_row: &VisualDisplayRow,
) -> Vec<gpui::AnyElement> {
    let Some((start_x, width)) =
        selection_bounds_for_visual_row(text_layout, selected_range, visual_row)
    else {
        return Vec::new();
    };

    let palette = editor_palette();

    vec![
        div()
            .absolute()
            .left(start_x)
            .top_0()
            .h(visual_row.height)
            .w(width)
            .bg(palette.selection_background)
            .into_any_element(),
    ]
}

fn render_fragments_for_visual_row(
    fragments: &[DisplayInlineFragment],
    visual_row: &VisualDisplayRow,
    selected_range: Option<&Range<usize>>,
    row_style: RowDisplayStyle,
) -> Vec<gpui::AnyElement> {
    let mut elements = Vec::new();

    for fragment in fragments {
        match fragment {
            DisplayInlineFragment::Text(segment) => {
                let Some(text) = fragment_text_for_visual_row(
                    &segment.display_range,
                    segment.text.as_str(),
                    visual_row,
                ) else {
                    continue;
                };
                elements.push(render_text_piece(text, &segment.style));
            }
            DisplayInlineFragment::Atom(atom) => {
                let Some(text) = fragment_text_for_visual_row(
                    &atom.display_range,
                    atom.fallback_text.as_str(),
                    visual_row,
                ) else {
                    continue;
                };
                elements.push(atom.render_piece(text, row_style, atom.is_selected(selected_range)));
            }
        }
    }

    if elements.is_empty() {
        elements.push(SharedString::from(String::new()).into_any_element());
    }

    elements
}

fn caret_position_for_visual_row(
    snapshot: &TextBufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    text_layout: &DisplayRowTextLayout,
    visual_row_index: usize,
    visual_row: &VisualDisplayRow,
) -> Option<gpui::Pixels> {
    if !selection.is_empty() {
        return None;
    }

    let cursor = selection.head();
    if display_row.row != cursor.row {
        return None;
    }

    let cursor_offset = snapshot.point_to_offset(clip_cursor_in_text_snapshot(snapshot, cursor));
    let display_offset = display_row
        .source_to_display(cursor_offset)
        .min(text_layout.text_len);
    if visual_row_index_for_caret(
        &text_layout.visual_rows,
        display_offset,
        text_layout.text_len,
        selection.goal,
    )? != visual_row_index
    {
        return None;
    }

    Some(
        display_x_for_offset(
            &text_layout.fragments,
            &text_layout.shaped_line,
            display_offset,
        ) - visual_row.line_start_x,
    )
}

fn caret_element(caret_x: gpui::Pixels, row_style: RowDisplayStyle) -> gpui::AnyElement {
    let palette = editor_palette();
    div()
        .absolute()
        .left(caret_x)
        .top_0()
        .bottom_0()
        .w(px(1.))
        .flex()
        .items_center()
        .child(div().w(px(1.)).h(row_style.caret_height).bg(palette.caret))
        .into_any_element()
}
