use std::{ops::Range, sync::Arc};

use md_buffer::BufferSnapshot;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point};

use crate::{
    layout::DisplayRowProjectionState,
    rendered_index::{
        BlankRowRole, RenderedDisplayIndex, RenderedDisplayItem, RenderedDisplayItemKind,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RenderedCaretAffinity {
    Before,
    After,
}

pub(crate) struct RenderedTopology<'a> {
    snapshot: &'a BufferSnapshot,
    index: Arc<RenderedDisplayIndex>,
}

impl<'a> RenderedTopology<'a> {
    pub(crate) fn new(snapshot: &'a BufferSnapshot, index: Arc<RenderedDisplayIndex>) -> Self {
        Self { snapshot, index }
    }

    pub(crate) fn normalize_caret(&self, point: Point, affinity: RenderedCaretAffinity) -> Point {
        let point = self
            .snapshot
            .as_text_snapshot()
            .clip_point(point, md_text::Bias::Left);
        let row = point.row as usize;
        let Some(role) = self.index.blank_row_role_for_source_row(row) else {
            return point;
        };

        match role {
            BlankRowRole::EmptyParagraph => Point::new(point.row, 0),
            BlankRowRole::Separator | BlankRowRole::IgnoredExtra
                if !has_content_row_after(self.snapshot, &self.index, row) =>
            {
                Point::new(point.row, 0)
            }
            BlankRowRole::Separator | BlankRowRole::IgnoredExtra => {
                nearest_rendered_caret_stop(self.snapshot, &self.index, row, affinity)
            }
        }
    }

    pub(crate) fn item_display_source_range(
        &self,
        item: &RenderedDisplayItem,
        display_row_state: &DisplayRowProjectionState,
        active_cursor_maps_to_item: bool,
    ) -> (u32, Range<usize>, Range<usize>) {
        let mut source_range = item.source_range.clone();
        let mut source_row_range = item.row_range.clone();
        if active_cursor_maps_to_item
            && matches!(
                item.kind,
                RenderedDisplayItemKind::Paragraph | RenderedDisplayItemKind::Heading
            )
            && let Some(cursor) = display_row_state.active_cursor
            && cursor.row as usize >= item.row_range.end
        {
            let text_snapshot = self.snapshot.as_text_snapshot();
            let cursor_offset = text_snapshot.point_to_offset(cursor);
            let cursor_at_trailing_blank_tail = text_snapshot
                .text_for_range(cursor_offset..text_snapshot.len())
                .all(|chunk| chunk.chars().all(is_line_break_char));
            let active_trailing_break =
                display_row_state
                    .active_source_range
                    .as_ref()
                    .is_some_and(|range| {
                        range.start == item.source_range.end && range.end == cursor_offset
                    })
                    || cursor.row as usize == item.row_range.end
                        && cursor_starts_multi_blank_run(text_snapshot, cursor.row as usize)
                    || cursor_at_trailing_blank_tail;
            if cursor_offset > item.source_range.end
                && active_trailing_break
                && text_snapshot
                    .text_for_range(item.source_range.end..cursor_offset)
                    .all(|chunk| chunk.chars().all(is_line_break_char))
            {
                source_range.end = cursor_offset;
                source_row_range.end = cursor.row as usize + 1;
            }
        }

        (item.row_range.start as u32, source_range, source_row_range)
    }
}

fn cursor_starts_multi_blank_run(snapshot: &TextBufferSnapshot, row: usize) -> bool {
    let row_count = snapshot.row_count() as usize;
    row.saturating_add(1) < row_count
        && source_row_is_blank_in_text_snapshot(snapshot, row)
        && source_row_is_blank_in_text_snapshot(snapshot, row.saturating_add(1))
}

fn source_row_is_blank_in_text_snapshot(snapshot: &TextBufferSnapshot, row: usize) -> bool {
    snapshot
        .text_for_range(
            crate::display_row_builder::row_source_range_in_text_snapshot(snapshot, row as u32),
        )
        .all(|chunk| chunk.trim().is_empty())
}

fn is_line_break_char(ch: char) -> bool {
    matches!(ch, '\n' | '\r')
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
