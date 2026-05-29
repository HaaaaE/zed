use std::ops::Range;

use gpui::{Pixels, px};
use markdown_wysiwyg::MarkdownProjectionOperation;
use md_text::{Point, SelectionGoal};

use super::{
    TextBufferSnapshot, clip_cursor_in_text_snapshot,
    display_model::DisplayRow,
    gutter_width,
    layout::{DisplayRowTextLayout, VisualDisplayRow},
    visual_row::{
        display_offset_for_visual_row_x, display_x_for_offset, source_offset_for_display_offset,
        visual_horizontal_goal,
    },
};

pub(super) fn mouse_target_for_text_layout(
    snapshot: &TextBufferSnapshot,
    display_row: &DisplayRow,
    visual_row_index: usize,
    visual_row: &VisualDisplayRow,
    x: Pixels,
    text_layout: &DisplayRowTextLayout,
) -> (Point, SelectionGoal) {
    let text_x = (x - gutter_width()).max(px(0.));
    let display_offset = display_offset_for_visual_row_x(text_layout, visual_row, text_x);
    let source_offset =
        source_offset_for_display_offset(display_row, &text_layout.fragments, display_offset);
    let source_offset = snapshot.as_rope().floor_char_boundary(source_offset);
    let point = clip_cursor_in_text_snapshot(snapshot, snapshot.offset_to_point(source_offset));
    let target_x = display_x_for_offset(
        &text_layout.fragments,
        &text_layout.shaped_line,
        display_offset,
    ) - visual_row.line_start_x;

    (
        point,
        visual_horizontal_goal(visual_row_index, target_x.max(px(0.))),
    )
}

pub(super) fn task_checkbox_source_range_for_text_layout_click(
    display_row: &DisplayRow,
    visual_row: &VisualDisplayRow,
    x: Pixels,
    text_layout: &DisplayRowTextLayout,
) -> Option<Range<usize>> {
    let text_x = (x - gutter_width()).max(px(0.));

    for operation in display_row.projection.operations() {
        let MarkdownProjectionOperation::Replace {
            source_range,
            display_text,
        } = operation
        else {
            continue;
        };
        if display_text.as_str() != "\u{2610}" && display_text.as_str() != "\u{2611}" {
            continue;
        }

        let display_start = display_row.source_to_display(source_range.start);
        let display_end = display_start + operation.display_len();
        if display_start >= visual_row.display_range.end
            || display_end <= visual_row.display_range.start
        {
            continue;
        }

        let start_x = display_x_for_offset(
            &text_layout.fragments,
            &text_layout.shaped_line,
            display_start,
        ) - visual_row.line_start_x;
        let end_x = display_x_for_offset(
            &text_layout.fragments,
            &text_layout.shaped_line,
            display_end,
        ) - visual_row.line_start_x;

        if start_x <= text_x && text_x <= end_x {
            return Some(source_range.clone());
        }
    }

    None
}
