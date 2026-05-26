use gpui::{Pixels, px};
use md_buffer::BufferSnapshot;
use md_text::{Point, SelectionGoal};

use super::{
    DisplayInlineFragment, DisplayRow, DisplayRowTextLayout, TextBufferSnapshot, VisualDisplayRow,
    ranges_overlap,
};

pub(super) fn visual_row_index_for_caret(
    visual_rows: &[VisualDisplayRow],
    display_offset: usize,
    text_len: usize,
    goal: SelectionGoal,
) -> Option<usize> {
    if let SelectionGoal::WrappedHorizontalPosition((visual_row_index, _)) = goal
        && let Ok(visual_row_index) = usize::try_from(visual_row_index)
        && let Some(visual_row) = visual_rows.get(visual_row_index)
        && visual_row_contains_display_offset(visual_row, display_offset)
    {
        return Some(visual_row_index);
    }

    visual_row_index_containing_caret(visual_rows, display_offset, text_len)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VisualLineBoundary {
    Start,
    End,
}

pub(super) fn visual_line_boundary_for_caret(
    visual_rows: &[VisualDisplayRow],
    display_offset: usize,
    text_len: usize,
    goal: SelectionGoal,
    boundary: VisualLineBoundary,
) -> Option<(usize, usize)> {
    let visual_row_index = visual_row_index_for_caret(visual_rows, display_offset, text_len, goal)?;
    let visual_row = visual_rows.get(visual_row_index)?;
    let target = match boundary {
        VisualLineBoundary::Start => visual_row.display_range.start,
        VisualLineBoundary::End => visual_row.display_range.end,
    };

    Some((visual_row_index, target))
}

pub(super) fn desired_visual_x(goal: SelectionGoal, cursor_x: Pixels) -> Pixels {
    match goal {
        SelectionGoal::HorizontalPosition(x) if x.is_finite() => px(x as f32),
        SelectionGoal::WrappedHorizontalPosition((_, x)) if x.is_finite() => px(x),
        _ => cursor_x,
    }
}

pub(super) fn visual_horizontal_goal(visual_row_index: usize, x: Pixels) -> SelectionGoal {
    SelectionGoal::WrappedHorizontalPosition((
        u32::try_from(visual_row_index).unwrap_or(u32::MAX),
        f32::from(x),
    ))
}

pub(super) fn point_for_visual_row_x(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    text_layout: &DisplayRowTextLayout,
    visual_row: &VisualDisplayRow,
    x: Pixels,
) -> Option<Point> {
    point_for_visual_row_x_in_text_snapshot(
        snapshot.as_text_snapshot(),
        display_row,
        text_layout,
        visual_row,
        x,
    )
}

pub(super) fn point_for_visual_row_x_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    display_row: &DisplayRow,
    text_layout: &DisplayRowTextLayout,
    visual_row: &VisualDisplayRow,
    x: Pixels,
) -> Option<Point> {
    let display_offset = display_offset_for_visual_row_x(text_layout, visual_row, x);
    Some(point_for_display_offset_in_text_snapshot(
        snapshot,
        display_row,
        text_layout,
        display_offset,
    ))
}

pub(super) fn point_for_display_offset(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    text_layout: &DisplayRowTextLayout,
    display_offset: usize,
) -> Point {
    point_for_display_offset_in_text_snapshot(
        snapshot.as_text_snapshot(),
        display_row,
        text_layout,
        display_offset,
    )
}

pub(super) fn point_for_display_offset_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    display_row: &DisplayRow,
    text_layout: &DisplayRowTextLayout,
    display_offset: usize,
) -> Point {
    let source_offset =
        source_offset_for_display_offset(display_row, &text_layout.fragments, display_offset);
    let source_offset = snapshot.as_rope().floor_char_boundary(source_offset);
    snapshot.offset_to_point(source_offset)
}

pub(super) fn source_offset_for_display_offset(
    display_row: &DisplayRow,
    fragments: &[DisplayInlineFragment],
    display_offset: usize,
) -> usize {
    for fragment in fragments {
        if let DisplayInlineFragment::Atom(atom) = fragment
            && atom.display_range.end == display_offset
        {
            return atom.source_range.end;
        }
    }

    for fragment in fragments {
        if let DisplayInlineFragment::Atom(atom) = fragment
            && atom.display_range.start == display_offset
        {
            return atom.source_range.start;
        }
    }

    display_row.display_to_source(display_offset)
}

pub(super) fn display_offset_for_visual_row_x(
    text_layout: &DisplayRowTextLayout,
    visual_row: &VisualDisplayRow,
    x: Pixels,
) -> usize {
    let display_x = x.max(px(0.)) + visual_row.line_start_x;
    let display_offset =
        closest_display_offset_for_x(&text_layout.fragments, &text_layout.shaped_line, display_x)
            .clamp(visual_row.display_range.start, visual_row.display_range.end);

    if let Some(atom_offset) = snap_display_offset_to_inline_atom_boundary(
        text_layout,
        visual_row,
        display_offset,
        display_x,
    ) {
        return atom_offset;
    }

    display_offset
}

pub(super) fn closest_display_offset_for_x(
    fragments: &[DisplayInlineFragment],
    shaped_line: &gpui::ShapedLine,
    display_x: Pixels,
) -> usize {
    for fragment in fragments {
        match fragment {
            DisplayInlineFragment::Text(segment) => {
                let start_x =
                    display_x_for_offset(fragments, shaped_line, segment.display_range.start);
                let end_x = display_x_for_offset(fragments, shaped_line, segment.display_range.end);
                if display_x < start_x {
                    return segment.display_range.start;
                }
                if display_x <= end_x {
                    let shaped_start_x = shaped_line.x_for_index(segment.display_range.start);
                    let adjusted_x = display_x - (start_x - shaped_start_x);
                    return shaped_line
                        .closest_index_for_x(adjusted_x)
                        .clamp(segment.display_range.start, segment.display_range.end);
                }
            }
            DisplayInlineFragment::Atom(atom) => {
                let start_x =
                    display_x_for_offset(fragments, shaped_line, atom.display_range.start);
                let end_x = display_x_for_offset(fragments, shaped_line, atom.display_range.end);
                if display_x < start_x {
                    return atom.display_range.start;
                }
                if display_x <= end_x {
                    return atom.boundary_for_x(start_x, end_x, display_x);
                }
            }
        }
    }

    shaped_line.len()
}

pub(super) fn snap_display_offset_to_inline_atom_boundary(
    text_layout: &DisplayRowTextLayout,
    visual_row: &VisualDisplayRow,
    display_offset: usize,
    display_x: Pixels,
) -> Option<usize> {
    text_layout.fragments.iter().find_map(|fragment| {
        let DisplayInlineFragment::Atom(atom) = fragment else {
            return None;
        };
        if !ranges_overlap(&atom.display_range, &visual_row.display_range) {
            return None;
        }

        let atom_start_x = display_x_for_offset(
            &text_layout.fragments,
            &text_layout.shaped_line,
            atom.display_range.start,
        );
        let atom_end_x = display_x_for_offset(
            &text_layout.fragments,
            &text_layout.shaped_line,
            atom.display_range.end,
        );
        let offset_inside_atom =
            atom.display_range.start < display_offset && display_offset < atom.display_range.end;
        let x_inside_atom = atom_start_x <= display_x && display_x <= atom_end_x;
        if !offset_inside_atom && !x_inside_atom {
            return None;
        }

        Some(atom.boundary_for_x(atom_start_x, atom_end_x, display_x))
    })
}

pub(super) fn display_x_for_offset(
    fragments: &[DisplayInlineFragment],
    shaped_line: &gpui::ShapedLine,
    display_offset: usize,
) -> Pixels {
    let mut delta = px(0.);
    for fragment in fragments {
        let DisplayInlineFragment::Atom(atom) = fragment else {
            continue;
        };
        let fallback_start_x = shaped_line.x_for_index(atom.display_range.start);
        let fallback_end_x = shaped_line.x_for_index(atom.display_range.end);
        let fallback_width = (fallback_end_x - fallback_start_x).max(px(0.));

        if display_offset >= atom.display_range.end {
            delta += atom.width - fallback_width;
        } else if display_offset > atom.display_range.start {
            let local_fallback_x = shaped_line.x_for_index(display_offset) - fallback_start_x;
            let atom_ratio = if fallback_width > px(0.) {
                local_fallback_x / fallback_width
            } else {
                0.
            };
            return fallback_start_x + delta + atom.width * atom_ratio;
        }
    }

    shaped_line.x_for_index(display_offset) + delta
}

pub(super) fn visual_row_contains_caret(
    visual_row: &VisualDisplayRow,
    display_offset: usize,
    text_len: usize,
) -> bool {
    if display_offset < visual_row.display_range.start
        || display_offset > visual_row.display_range.end
    {
        return false;
    }
    display_offset < visual_row.display_range.end || visual_row.display_range.end == text_len
}

pub(super) fn visual_row_index_containing_caret(
    visual_rows: &[VisualDisplayRow],
    display_offset: usize,
    text_len: usize,
) -> Option<usize> {
    visual_rows
        .iter()
        .position(|visual_row| visual_row_contains_caret(visual_row, display_offset, text_len))
}

fn visual_row_contains_display_offset(
    visual_row: &VisualDisplayRow,
    display_offset: usize,
) -> bool {
    visual_row.display_range.start <= display_offset
        && display_offset <= visual_row.display_range.end
}
