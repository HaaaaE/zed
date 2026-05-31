use std::{ops::Range, sync::Arc};

use markdown_wysiwyg::MarkdownBlockKind;
use md_buffer::BufferSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct DisplayItemId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum RenderedDisplayItemKind {
    Paragraph,
    Heading,
    EmptyParagraph,
    StructuredBlock,
    TableRow,
    SourceFallback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RenderedDisplayItem {
    pub(crate) id: DisplayItemId,
    pub(crate) index: usize,
    pub(crate) source_range: Range<usize>,
    pub(crate) row_range: Range<usize>,
    pub(crate) kind: RenderedDisplayItemKind,
}

#[derive(Clone, Debug)]
pub(crate) struct RenderedDisplayIndex {
    version: md_text::Global,
    items: Vec<RenderedDisplayItem>,
    row_to_item: Vec<Option<usize>>,
    blank_row_roles: Vec<Option<BlankRowRole>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BlankRowRole {
    Separator,
    EmptyParagraph,
    IgnoredExtra,
}

impl RenderedDisplayIndex {
    pub(crate) fn build(snapshot: &BufferSnapshot) -> Arc<Self> {
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
                    let source_range = super::row_source_range(snapshot, row as u32);
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
                    let source_range = super::row_source_range(snapshot, row as u32);
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
            let source_range = super::row_source_range(snapshot, row as u32);
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

    pub(crate) fn version(&self) -> &md_text::Global {
        &self.version
    }

    pub(crate) fn item_count(&self) -> usize {
        self.items.len()
    }

    pub(crate) fn item(&self, index: usize) -> Option<&RenderedDisplayItem> {
        self.items.get(index)
    }

    pub(crate) fn item_index_for_source_row(&self, row: usize) -> Option<usize> {
        self.row_to_item.get(row).copied().flatten()
    }

    pub(crate) fn blank_row_role_for_source_row(&self, row: usize) -> Option<BlankRowRole> {
        self.blank_row_roles.get(row).copied().flatten()
    }

    pub(crate) fn item_index_for_source_offset(
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
            || !source_range_is_blank(snapshot, super::row_source_range(snapshot, row as u32))
        {
            row += 1;
            continue;
        }

        let run_start = row;
        while row < blank_row_roles.len()
            && !covered_rows[row]
            && source_range_is_blank(snapshot, super::row_source_range(snapshot, row as u32))
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

    let start = super::row_source_range(snapshot, row_range.start as u32).start;
    let end = super::row_source_range(snapshot, row_range.end.saturating_sub(1) as u32).end;
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

pub(crate) fn source_display_item_id(source_range: &Range<usize>, row: usize) -> DisplayItemId {
    DisplayItemId(display_item_id(
        source_range,
        &(row..row.saturating_add(1)),
        RenderedDisplayItemKind::SourceFallback,
    ))
}

pub(crate) fn display_item_id(
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
