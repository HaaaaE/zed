use std::sync::Arc;

use md_buffer::BufferSnapshot;
use md_text::Point;

use crate::rendered_index::{BlankRowRole, RenderedDisplayIndex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderedCaretAffinity {
    Before,
    After,
}

pub(crate) fn normalize_rendered_caret(
    snapshot: &BufferSnapshot,
    index: &Arc<RenderedDisplayIndex>,
    point: Point,
    affinity: RenderedCaretAffinity,
) -> Point {
    let point = snapshot
        .as_text_snapshot()
        .clip_point(point, md_text::Bias::Left);
    let row = point.row as usize;
    let Some(role) = index.blank_row_role_for_source_row(row) else {
        return point;
    };

    match role {
        BlankRowRole::EmptyParagraph => Point::new(point.row, 0),
        BlankRowRole::Separator | BlankRowRole::IgnoredExtra
            if !has_content_row_after(snapshot, index, row) =>
        {
            Point::new(point.row, 0)
        }
        BlankRowRole::Separator | BlankRowRole::IgnoredExtra => {
            nearest_rendered_caret_stop(snapshot, index, row, affinity)
        }
    }
}

fn has_content_row_after(
    snapshot: &BufferSnapshot,
    index: &RenderedDisplayIndex,
    row: usize,
) -> bool {
    let row_count = snapshot.as_text_snapshot().row_count() as usize;
    (row.saturating_add(1)..row_count)
        .any(|next_row| index.blank_row_role_for_source_row(next_row).is_none())
}

fn nearest_rendered_caret_stop(
    snapshot: &BufferSnapshot,
    index: &RenderedDisplayIndex,
    row: usize,
    affinity: RenderedCaretAffinity,
) -> Point {
    match affinity {
        RenderedCaretAffinity::Before => previous_rendered_caret_stop(snapshot, index, row)
            .or_else(|| next_rendered_caret_stop(snapshot, index, row))
            .unwrap_or_else(Point::zero),
        RenderedCaretAffinity::After => next_rendered_caret_stop(snapshot, index, row)
            .or_else(|| previous_rendered_caret_stop(snapshot, index, row))
            .unwrap_or_else(Point::zero),
    }
}

fn previous_rendered_caret_stop(
    snapshot: &BufferSnapshot,
    index: &RenderedDisplayIndex,
    row: usize,
) -> Option<Point> {
    for previous_row in (0..row).rev() {
        match index.blank_row_role_for_source_row(previous_row) {
            Some(BlankRowRole::Separator | BlankRowRole::IgnoredExtra) => continue,
            Some(BlankRowRole::EmptyParagraph) => {
                return Some(Point::new(previous_row as u32, 0));
            }
            None => {
                let previous_row = previous_row as u32;
                return Some(Point::new(
                    previous_row,
                    snapshot.as_text_snapshot().line_len(previous_row),
                ));
            }
        }
    }
    None
}

fn next_rendered_caret_stop(
    snapshot: &BufferSnapshot,
    index: &RenderedDisplayIndex,
    row: usize,
) -> Option<Point> {
    let row_count = snapshot.as_text_snapshot().row_count() as usize;
    for next_row in row.saturating_add(1)..row_count {
        match index.blank_row_role_for_source_row(next_row) {
            Some(BlankRowRole::Separator | BlankRowRole::IgnoredExtra) => continue,
            Some(BlankRowRole::EmptyParagraph) => return Some(Point::new(next_row as u32, 0)),
            None => return Some(Point::new(next_row as u32, 0)),
        }
    }
    None
}
