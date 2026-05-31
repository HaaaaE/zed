use std::{ops::Range, sync::Arc};

use markdown_wysiwyg::MarkdownBlockKind;
use md_buffer::BufferSnapshot;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct DisplayItemId(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum RenderedDisplayItemKind {
    Paragraph,
    Heading,
    EmptyParagraph,
    StructuredBlock,
    TableRow,
    SourceFallback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDisplayItem {
    pub id: DisplayItemId,
    pub index: usize,
    pub source_range: Range<usize>,
    pub row_range: Range<usize>,
    pub kind: RenderedDisplayItemKind,
}

#[derive(Clone, Debug)]
pub struct RenderedDisplayIndex {
    version: md_text::Global,
    items: Vec<RenderedDisplayItem>,
    row_to_item: Vec<Option<usize>>,
    blank_row_roles: Vec<Option<BlankRowRole>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlankRowRole {
    Separator,
    EmptyParagraph,
    IgnoredExtra,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedNewlineRun {
    pub source_range: Range<usize>,
    pub left_point: Point,
    pub right_point: Point,
    pub newline_count: usize,
    pub left_item: Option<usize>,
    pub right_item: Option<usize>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderedNewlineRunKind {
    SoftBreak,
    ParagraphBoundary,
    BoundaryWithSoftBreakSlot,
    EmptyParagraphs {
        count: usize,
        has_soft_break_slot: bool,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RenderedCaretAffinity {
    Before,
    After,
}

#[derive(Clone, Debug, Default)]
pub struct RenderedProjectionState {
    pub active_source_range: Option<Range<usize>>,
    pub inactive_source_ranges: Vec<Range<usize>>,
    pub active_cursor: Option<Point>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDisplaySourceRange {
    pub row: u32,
    pub source_range: Range<usize>,
    pub source_row_range: Range<usize>,
}

pub struct RenderedTopology<'a> {
    snapshot: &'a BufferSnapshot,
    index: Arc<RenderedDisplayIndex>,
}

impl<'a> RenderedTopology<'a> {
    pub fn new(snapshot: &'a BufferSnapshot, index: Arc<RenderedDisplayIndex>) -> Self {
        Self { snapshot, index }
    }

    pub fn normalize_caret(&self, point: Point, affinity: RenderedCaretAffinity) -> Point {
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

    pub fn newline_run_after_line_end(&self, point: Point) -> Option<RenderedNewlineRun> {
        let text_snapshot = self.snapshot.as_text_snapshot();
        let point = text_snapshot.clip_point(point, md_text::Bias::Left);
        if point.column != text_snapshot.line_len(point.row) {
            return None;
        }

        let row = point.row as usize;
        let row_count = text_snapshot.row_count() as usize;
        if row.saturating_add(1) >= row_count {
            return None;
        }

        let right_row = if source_row_is_blank_in_text_snapshot(text_snapshot, row + 1) {
            next_nonblank_row_after_blank_run(text_snapshot, row + 1)
        } else {
            row + 1
        };
        self.newline_run_between_rows(row, right_row)
    }

    pub fn newline_run_before_line_start(&self, point: Point) -> Option<RenderedNewlineRun> {
        let text_snapshot = self.snapshot.as_text_snapshot();
        let point = text_snapshot.clip_point(point, md_text::Bias::Left);
        if point.column != 0 || point.row == 0 {
            return None;
        }

        let row = point.row as usize;
        let left_row = if source_row_is_blank_in_text_snapshot(text_snapshot, row - 1) {
            previous_nonblank_row_before_blank_run(text_snapshot, row - 1)?
        } else {
            row - 1
        };
        self.newline_run_between_rows(left_row, row)
    }

    pub fn newline_run_containing_row(&self, row: usize) -> Option<RenderedNewlineRun> {
        let text_snapshot = self.snapshot.as_text_snapshot();
        let row_count = text_snapshot.row_count() as usize;
        if row >= row_count {
            return None;
        }

        if !source_row_is_blank_in_text_snapshot(text_snapshot, row) {
            let line_end = Point::new(row as u32, text_snapshot.line_len(row as u32));
            return self
                .newline_run_after_line_end(line_end)
                .or_else(|| self.newline_run_before_line_start(Point::new(row as u32, 0)));
        }

        let left_row = previous_nonblank_row_before_blank_run(text_snapshot, row)?;
        let right_row = next_nonblank_row_after_blank_run(text_snapshot, row);
        self.newline_run_between_rows(left_row, right_row)
    }

    pub fn classify_newline_run(&self, run: &RenderedNewlineRun) -> RenderedNewlineRunKind {
        match run.newline_count {
            0 => RenderedNewlineRunKind::SoftBreak,
            1 => RenderedNewlineRunKind::SoftBreak,
            2 => RenderedNewlineRunKind::ParagraphBoundary,
            3 => RenderedNewlineRunKind::BoundaryWithSoftBreakSlot,
            newline_count => RenderedNewlineRunKind::EmptyParagraphs {
                count: (newline_count - 2) / 2,
                has_soft_break_slot: newline_count % 2 == 1,
            },
        }
    }

    pub fn item_display_source_range(
        &self,
        item: &RenderedDisplayItem,
        projection_state: &RenderedProjectionState,
    ) -> RenderedDisplaySourceRange {
        let mut source_range = item.source_range.clone();
        let mut source_row_range = item.row_range.clone();
        let active_cursor_maps_to_item = projection_state.active_cursor.is_some_and(|cursor| {
            self.index.item_index_for_source_row(cursor.row as usize) == Some(item.index)
        });
        if active_cursor_maps_to_item
            && matches!(
                item.kind,
                RenderedDisplayItemKind::Paragraph | RenderedDisplayItemKind::Heading
            )
            && let Some(cursor) = projection_state.active_cursor
            && cursor.row as usize >= item.row_range.end
        {
            let text_snapshot = self.snapshot.as_text_snapshot();
            let cursor_offset = text_snapshot.point_to_offset(cursor);
            let cursor_at_trailing_blank_tail = text_snapshot
                .text_for_range(cursor_offset..text_snapshot.len())
                .all(|chunk| chunk.chars().all(is_line_break_char));
            let active_trailing_break =
                projection_state
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

        RenderedDisplaySourceRange {
            row: item.row_range.start as u32,
            source_range,
            source_row_range,
        }
    }

    fn newline_run_between_rows(
        &self,
        left_row: usize,
        right_row: usize,
    ) -> Option<RenderedNewlineRun> {
        if left_row >= right_row {
            return None;
        }

        let text_snapshot = self.snapshot.as_text_snapshot();
        let row_count = text_snapshot.row_count() as usize;
        if left_row >= row_count || right_row >= row_count {
            return None;
        }

        let left_point = Point::new(left_row as u32, text_snapshot.line_len(left_row as u32));
        let right_point = Point::new(right_row as u32, 0);
        let source_range =
            text_snapshot.point_to_offset(left_point)..text_snapshot.point_to_offset(right_point);
        Some(RenderedNewlineRun {
            source_range,
            left_point,
            right_point,
            newline_count: right_row - left_row,
            left_item: self.index.item_index_for_source_row(left_row),
            right_item: self.index.item_index_for_source_row(right_row),
        })
    }
}

impl RenderedDisplayIndex {
    pub fn build(snapshot: &BufferSnapshot) -> Arc<Self> {
        let version = snapshot.version().clone();
        let row_count = snapshot.row_count() as usize;
        let mut items = Vec::new();
        let mut covered_rows = vec![false; row_count];
        let mut blank_row_roles = vec![None; row_count];

        for block in snapshot.syntax_tree().blocks() {
            if block.kind == MarkdownBlockKind::Blank {
                continue;
            }

            let Some(kind) = item_kind_for_block(block.kind) else {
                continue;
            };
            if block.row_range.is_empty()
                || block.row_range.start >= row_count
                || block
                    .row_range
                    .clone()
                    .any(|row| covered_rows.get(row).copied().unwrap_or(true))
            {
                continue;
            }

            if kind == RenderedDisplayItemKind::TableRow {
                for row in block.row_range.clone() {
                    let source_range = row_source_range(snapshot, row as u32);
                    push_item(
                        &mut items,
                        &mut covered_rows,
                        source_range,
                        row..row + 1,
                        kind,
                    );
                }
            } else if kind == RenderedDisplayItemKind::Paragraph
                && paragraph_can_merge(
                    snapshot,
                    block.source_range.clone(),
                    block.row_range.clone(),
                )
            {
                let source_range = source_range_for_row_range(snapshot, block.row_range.clone());
                push_item(
                    &mut items,
                    &mut covered_rows,
                    source_range,
                    block.row_range.clone(),
                    kind,
                );
            } else {
                for row in block.row_range.clone() {
                    let source_range = row_source_range(snapshot, row as u32);
                    push_item(
                        &mut items,
                        &mut covered_rows,
                        source_range,
                        row..row + 1,
                        kind,
                    );
                }
            }
        }

        assign_blank_row_roles(snapshot, &covered_rows, &mut blank_row_roles);

        for row in 0..row_count {
            if covered_rows[row] {
                continue;
            }
            let source_range = row_source_range(snapshot, row as u32);
            if source_range_is_blank(snapshot, source_range.clone()) {
                if blank_row_roles[row] == Some(BlankRowRole::EmptyParagraph) {
                    push_item(
                        &mut items,
                        &mut covered_rows,
                        source_range,
                        row..row + 1,
                        RenderedDisplayItemKind::EmptyParagraph,
                    );
                }
                continue;
            }
            push_item(
                &mut items,
                &mut covered_rows,
                source_range,
                row..row + 1,
                RenderedDisplayItemKind::SourceFallback,
            );
        }

        items.sort_by_key(|item| (item.row_range.start, item.row_range.end, item.index));
        for (index, item) in items.iter_mut().enumerate() {
            item.index = index;
        }

        let mut row_to_item = vec![None; row_count];
        for item in &items {
            for row in item.row_range.clone() {
                if let Some(slot) = row_to_item.get_mut(row) {
                    *slot = Some(item.index);
                }
            }
        }
        fill_blank_row_mappings(&mut row_to_item, &blank_row_roles);

        Arc::new(Self {
            version,
            items,
            row_to_item,
            blank_row_roles,
        })
    }

    pub fn version(&self) -> &md_text::Global {
        &self.version
    }

    pub fn item_count(&self) -> usize {
        self.items.len()
    }

    pub fn item(&self, index: usize) -> Option<&RenderedDisplayItem> {
        self.items.get(index)
    }

    pub fn item_index_for_source_row(&self, row: usize) -> Option<usize> {
        self.row_to_item.get(row).copied().flatten()
    }

    pub fn blank_row_role_for_source_row(&self, row: usize) -> Option<BlankRowRole> {
        self.blank_row_roles.get(row).copied().flatten()
    }

    pub fn item_index_for_source_offset(
        &self,
        snapshot: &BufferSnapshot,
        source_offset: usize,
    ) -> Option<usize> {
        let row = snapshot
            .as_text_snapshot()
            .offset_to_point(source_offset)
            .row as usize;
        self.item_index_for_source_row(row)
    }
}

fn row_source_range(snapshot: &BufferSnapshot, row: u32) -> Range<usize> {
    row_source_range_in_text_snapshot(snapshot.as_text_snapshot(), row)
}

fn row_source_range_in_text_snapshot(snapshot: &md_text::BufferSnapshot, row: u32) -> Range<usize> {
    if row >= snapshot.row_count() {
        let end = snapshot.len();
        return end..end;
    }

    let start = snapshot.point_to_offset(md_text::Point::new(row, 0));
    let end = start + snapshot.line_len(row) as usize;
    start..end
}

fn cursor_starts_multi_blank_run(snapshot: &TextBufferSnapshot, row: usize) -> bool {
    let row_count = snapshot.row_count() as usize;
    row.saturating_add(1) < row_count
        && source_row_is_blank_in_text_snapshot(snapshot, row)
        && source_row_is_blank_in_text_snapshot(snapshot, row.saturating_add(1))
}

fn source_row_is_blank_in_text_snapshot(snapshot: &TextBufferSnapshot, row: usize) -> bool {
    snapshot
        .text_for_range(row_source_range_in_text_snapshot(snapshot, row as u32))
        .all(|chunk| chunk.trim().is_empty())
}

fn previous_nonblank_row_before_blank_run(
    snapshot: &TextBufferSnapshot,
    blank_row: usize,
) -> Option<usize> {
    let mut row = blank_row;
    loop {
        if !source_row_is_blank_in_text_snapshot(snapshot, row) {
            return Some(row);
        }
        row = row.checked_sub(1)?;
    }
}

fn next_nonblank_row_after_blank_run(snapshot: &TextBufferSnapshot, blank_row: usize) -> usize {
    let row_count = snapshot.row_count() as usize;
    let mut row = blank_row;
    while row + 1 < row_count && source_row_is_blank_in_text_snapshot(snapshot, row) {
        row += 1;
    }
    row
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
fn source_range_is_blank(snapshot: &BufferSnapshot, source_range: Range<usize>) -> bool {
    snapshot
        .as_text_snapshot()
        .text_for_range(source_range)
        .all(|chunk| chunk.trim().is_empty())
}

fn assign_blank_row_roles(
    snapshot: &BufferSnapshot,
    covered_rows: &[bool],
    blank_row_roles: &mut [Option<BlankRowRole>],
) {
    let mut row = 0;
    while row < blank_row_roles.len() {
        if covered_rows[row]
            || !source_range_is_blank(snapshot, row_source_range(snapshot, row as u32))
        {
            row += 1;
            continue;
        }

        let run_start = row;
        while row < blank_row_roles.len()
            && !covered_rows[row]
            && source_range_is_blank(snapshot, row_source_range(snapshot, row as u32))
        {
            row += 1;
        }

        let run_len = row - run_start;
        let effective_len = if run_len % 2 == 0 {
            run_len.saturating_sub(1)
        } else {
            run_len
        };
        for offset in 0..run_len {
            let role = if offset >= effective_len {
                BlankRowRole::IgnoredExtra
            } else if offset % 2 == 0 {
                BlankRowRole::Separator
            } else {
                BlankRowRole::EmptyParagraph
            };
            blank_row_roles[run_start + offset] = Some(role);
        }
    }
}

fn fill_blank_row_mappings(
    row_to_item: &mut [Option<usize>],
    blank_row_roles: &[Option<BlankRowRole>],
) {
    let original = row_to_item.to_vec();
    for row in 0..row_to_item.len() {
        if row_to_item[row].is_some() || blank_row_roles.get(row).copied().flatten().is_none() {
            continue;
        }
        row_to_item[row] = nearest_item_index(&original, row);
    }
}

fn nearest_item_index(row_to_item: &[Option<usize>], row: usize) -> Option<usize> {
    let previous = row_to_item[..row].iter().rev().find_map(|item| *item);
    let next = row_to_item[row.saturating_add(1)..]
        .iter()
        .find_map(|item| *item);
    previous.or(next)
}

fn push_item(
    items: &mut Vec<RenderedDisplayItem>,
    covered_rows: &mut [bool],
    source_range: Range<usize>,
    row_range: Range<usize>,
    kind: RenderedDisplayItemKind,
) {
    if row_range.is_empty() {
        return;
    }
    let index = items.len();
    for row in row_range.clone() {
        if let Some(covered) = covered_rows.get_mut(row) {
            *covered = true;
        }
    }
    items.push(RenderedDisplayItem {
        id: DisplayItemId(display_item_id(&source_range, &row_range, kind)),
        index,
        source_range,
        row_range,
        kind,
    });
}

fn source_range_for_row_range(snapshot: &BufferSnapshot, row_range: Range<usize>) -> Range<usize> {
    if row_range.is_empty() {
        let end = snapshot.as_text_snapshot().len();
        return end..end;
    }

    let start = row_source_range(snapshot, row_range.start as u32).start;
    let end = row_source_range(snapshot, row_range.end.saturating_sub(1) as u32).end;
    start..end
}

fn item_kind_for_block(kind: MarkdownBlockKind) -> Option<RenderedDisplayItemKind> {
    Some(match kind {
        MarkdownBlockKind::Paragraph => RenderedDisplayItemKind::Paragraph,
        MarkdownBlockKind::AtxHeading { .. } | MarkdownBlockKind::SetextHeading { .. } => {
            RenderedDisplayItemKind::Heading
        }
        MarkdownBlockKind::PipeTable => RenderedDisplayItemKind::TableRow,
        MarkdownBlockKind::FencedCodeBlock
        | MarkdownBlockKind::IndentedCodeBlock
        | MarkdownBlockKind::ThematicBreak
        | MarkdownBlockKind::LinkReferenceDefinition
        | MarkdownBlockKind::HtmlBlock => RenderedDisplayItemKind::StructuredBlock,
        MarkdownBlockKind::Blank
        | MarkdownBlockKind::BlockQuote
        | MarkdownBlockKind::OrderedList
        | MarkdownBlockKind::UnorderedList
        | MarkdownBlockKind::ListItem
        | MarkdownBlockKind::TaskListItem { .. } => return None,
    })
}

fn paragraph_can_merge(
    snapshot: &BufferSnapshot,
    source_range: Range<usize>,
    row_range: Range<usize>,
) -> bool {
    if snapshot.syntax_tree().blocks().iter().any(|block| {
        matches!(
            block.kind,
            MarkdownBlockKind::BlockQuote
                | MarkdownBlockKind::ListItem
                | MarkdownBlockKind::TaskListItem { .. }
        ) && ranges_overlap(&block.row_range, &row_range)
    }) {
        return false;
    }

    let text = snapshot
        .as_text_snapshot()
        .text_for_range(source_range.clone())
        .collect::<String>();
    if text.contains("![") || text.contains("[ ]") || text.contains("[x]") || text.contains("[X]") {
        return false;
    }

    snapshot
        .syntax_tree()
        .inline_spans_in_source_range(source_range)
        .all(|span| !span.kind.is_rendered_element_candidate())
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

pub fn source_display_item_id(source_range: &Range<usize>, row: usize) -> DisplayItemId {
    DisplayItemId(display_item_id(
        source_range,
        &(row..row.saturating_add(1)),
        RenderedDisplayItemKind::SourceFallback,
    ))
}

pub fn display_item_id(
    source_range: &Range<usize>,
    row_range: &Range<usize>,
    kind: RenderedDisplayItemKind,
) -> u64 {
    let kind = kind as u64;
    ((row_range.start as u64) << 40)
        ^ ((row_range.end as u64) << 28)
        ^ ((source_range.start as u64) << 8)
        ^ (source_range.end as u64)
        ^ kind
}

#[cfg(test)]
mod tests {
    use super::*;
    use md_buffer::Buffer;

    #[test]
    fn groups_paragraphs_and_keeps_structured_rows_addressable() {
        let mut buffer = Buffer::local(
            "first paragraph\ncontinued\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n```rust\nlet x = 1;\n```\n[label]: https://example.com\n<div>raw</div>\n",
        );
        let snapshot = buffer.snapshot();
        let index = RenderedDisplayIndex::build(&snapshot);

        let items = (0..index.item_count())
            .map(|ix| index.item(ix).expect("item should exist"))
            .map(|item| (item.index, item.row_range.clone(), item.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            items,
            vec![
                (0, 0..2, RenderedDisplayItemKind::Paragraph),
                (1, 3..4, RenderedDisplayItemKind::TableRow),
                (2, 4..5, RenderedDisplayItemKind::TableRow),
                (3, 5..6, RenderedDisplayItemKind::TableRow),
                (4, 7..8, RenderedDisplayItemKind::StructuredBlock),
                (5, 8..9, RenderedDisplayItemKind::StructuredBlock),
                (6, 9..10, RenderedDisplayItemKind::StructuredBlock),
                (7, 10..11, RenderedDisplayItemKind::StructuredBlock),
                (8, 11..12, RenderedDisplayItemKind::StructuredBlock),
            ]
        );

        assert_eq!(index.item_index_for_source_row(0), Some(0));
        assert_eq!(index.item_index_for_source_row(1), Some(0));
        assert_eq!(index.item_index_for_source_row(2), Some(0));
        assert_eq!(index.item_index_for_source_row(4), Some(2));
        assert_eq!(index.item_index_for_source_row(6), Some(3));
        assert_eq!(index.item_index_for_source_row(8), Some(5));
        assert_eq!(index.item_index_for_source_row(12), Some(8));
    }

    #[test]
    fn maps_single_blank_row_as_separator() {
        let mut buffer = Buffer::local(
            "# GFM Feature Test\n\nThis file is a manual fixture for checking GFM support.\n",
        );
        let snapshot = buffer.snapshot();
        let index = RenderedDisplayIndex::build(&snapshot);

        let items = (0..index.item_count())
            .map(|ix| index.item(ix).expect("item should exist"))
            .map(|item| (item.row_range.clone(), item.kind))
            .collect::<Vec<_>>();

        assert_eq!(
            items,
            vec![
                (0..1, RenderedDisplayItemKind::Heading),
                (2..3, RenderedDisplayItemKind::Paragraph),
            ]
        );
        assert_eq!(index.item_index_for_source_row(0), Some(0));
        assert_eq!(index.item_index_for_source_row(1), Some(0));
        assert_eq!(index.item_index_for_source_row(2), Some(1));
        assert_eq!(
            index.blank_row_role_for_source_row(1),
            Some(BlankRowRole::Separator)
        );
    }

    #[test]
    fn assigns_empty_paragraphs_from_blank_runs() {
        let cases = [
            ("A\n\nB", 0, vec![BlankRowRole::Separator]),
            (
                "A\n\n\nB",
                0,
                vec![BlankRowRole::Separator, BlankRowRole::IgnoredExtra],
            ),
            (
                "A\n\n\n\nB",
                1,
                vec![
                    BlankRowRole::Separator,
                    BlankRowRole::EmptyParagraph,
                    BlankRowRole::Separator,
                ],
            ),
            (
                "A\n\n\n\n\nB",
                1,
                vec![
                    BlankRowRole::Separator,
                    BlankRowRole::EmptyParagraph,
                    BlankRowRole::Separator,
                    BlankRowRole::IgnoredExtra,
                ],
            ),
            (
                "A\n\n\n\n\n\nB",
                2,
                vec![
                    BlankRowRole::Separator,
                    BlankRowRole::EmptyParagraph,
                    BlankRowRole::Separator,
                    BlankRowRole::EmptyParagraph,
                    BlankRowRole::Separator,
                ],
            ),
        ];

        for (source, expected_empty_count, expected_roles) in cases {
            let mut buffer = Buffer::local(source);
            let snapshot = buffer.snapshot();
            let index = RenderedDisplayIndex::build(&snapshot);
            let empty_items = (0..index.item_count())
                .filter_map(|ix| index.item(ix))
                .filter(|item| item.kind == RenderedDisplayItemKind::EmptyParagraph)
                .collect::<Vec<_>>();
            let roles = (1..=expected_roles.len())
                .map(|row| index.blank_row_role_for_source_row(row))
                .collect::<Vec<_>>();

            assert_eq!(empty_items.len(), expected_empty_count, "{source:?}");
            assert_eq!(
                roles,
                expected_roles.into_iter().map(Some).collect::<Vec<_>>(),
                "{source:?}"
            );
            for item in empty_items {
                assert_eq!(
                    index.item_index_for_source_row(item.row_range.start),
                    Some(item.index)
                );
            }
        }
    }

    #[test]
    fn classifies_source_backed_newline_runs() {
        let cases = [
            ("1\n2", 1, RenderedNewlineRunKind::SoftBreak),
            ("1\n\n2", 2, RenderedNewlineRunKind::ParagraphBoundary),
            (
                "1\n\n\n2",
                3,
                RenderedNewlineRunKind::BoundaryWithSoftBreakSlot,
            ),
            (
                "1\n\n\n\n2",
                4,
                RenderedNewlineRunKind::EmptyParagraphs {
                    count: 1,
                    has_soft_break_slot: false,
                },
            ),
            (
                "1\n\n\n\n\n2",
                5,
                RenderedNewlineRunKind::EmptyParagraphs {
                    count: 1,
                    has_soft_break_slot: true,
                },
            ),
            (
                "1\n\n\n\n\n\n2",
                6,
                RenderedNewlineRunKind::EmptyParagraphs {
                    count: 2,
                    has_soft_break_slot: false,
                },
            ),
        ];

        for (source, expected_newline_count, expected_kind) in cases {
            let mut buffer = Buffer::local(source);
            let snapshot = buffer.snapshot();
            let index = RenderedDisplayIndex::build(&snapshot);
            let topology = RenderedTopology::new(&snapshot, index);
            let run = topology
                .newline_run_after_line_end(Point::new(0, 1))
                .expect("expected newline run");

            assert_eq!(run.newline_count, expected_newline_count, "{source:?}");
            assert_eq!(
                topology.classify_newline_run(&run),
                expected_kind,
                "{source:?}"
            );
            assert_eq!(
                topology.newline_run_before_line_start(run.right_point),
                Some(run.clone()),
                "{source:?}"
            );
            assert_eq!(
                topology.newline_run_containing_row(1),
                Some(run),
                "{source:?}"
            );
        }
    }

    #[test]
    fn newline_run_ranges_are_byte_safe() {
        let mut buffer = Buffer::local("甲🙂\n\n乙🚀");
        let snapshot = buffer.snapshot();
        let index = RenderedDisplayIndex::build(&snapshot);
        let topology = RenderedTopology::new(&snapshot, index);
        let left_column = "甲🙂".len() as u32;
        let run = topology
            .newline_run_after_line_end(Point::new(0, left_column))
            .expect("expected newline run");

        assert_eq!(run.left_point, Point::new(0, left_column));
        assert_eq!(run.right_point, Point::new(2, 0));
        assert_eq!(run.newline_count, 2);
        assert_eq!(
            snapshot
                .as_text_snapshot()
                .text_for_range(run.source_range)
                .collect::<String>(),
            "\n\n"
        );
    }

    #[test]
    fn normalizes_blank_row_carets_to_rendered_stops() {
        let mut buffer = Buffer::local("A\n\n\n\nB");
        let snapshot = buffer.snapshot();
        let index = RenderedDisplayIndex::build(&snapshot);
        let topology = RenderedTopology::new(&snapshot, index);

        assert_eq!(
            topology.normalize_caret(Point::new(1, 5), RenderedCaretAffinity::Before),
            Point::new(0, 1)
        );
        assert_eq!(
            topology.normalize_caret(Point::new(1, 5), RenderedCaretAffinity::After),
            Point::new(2, 0)
        );
        assert_eq!(
            topology.normalize_caret(Point::new(2, 5), RenderedCaretAffinity::After),
            Point::new(2, 0)
        );
        assert_eq!(
            topology.normalize_caret(Point::new(3, 5), RenderedCaretAffinity::After),
            Point::new(4, 0)
        );
    }

    #[test]
    fn normalizes_trailing_blank_rows_without_content_after_to_row_start() {
        let mut buffer = Buffer::local("A\n\n\n");
        let snapshot = buffer.snapshot();
        let index = RenderedDisplayIndex::build(&snapshot);
        let topology = RenderedTopology::new(&snapshot, index);

        assert_eq!(
            topology.normalize_caret(Point::new(1, 5), RenderedCaretAffinity::After),
            Point::new(1, 0)
        );
    }

    #[test]
    fn extends_merged_paragraph_source_range_to_active_trailing_blank_cursor() {
        let mut buffer = Buffer::local("hello\ncontinued\n\n");
        let snapshot = buffer.snapshot();
        let index = RenderedDisplayIndex::build(&snapshot);
        let item = index.item(0).expect("paragraph item should exist").clone();
        let cursor = Point::new(2, 0);
        let cursor_offset = snapshot.as_text_snapshot().point_to_offset(cursor);
        let topology = RenderedTopology::new(&snapshot, index);

        let display_range = topology.item_display_source_range(
            &item,
            &RenderedProjectionState {
                active_source_range: Some(item.source_range.end..cursor_offset),
                inactive_source_ranges: Vec::new(),
                active_cursor: Some(cursor),
            },
        );

        assert_eq!(display_range.row, 0);
        assert_eq!(display_range.source_range.end, cursor_offset);
        assert_eq!(display_range.source_row_range, 0..3);
    }
}
