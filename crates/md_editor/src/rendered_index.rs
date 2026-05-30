use std::{ops::Range, sync::Arc};

use markdown_wysiwyg::MarkdownBlockKind;
use md_buffer::BufferSnapshot;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) struct DisplayItemId(pub(crate) u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub(crate) enum RenderedDisplayItemKind {
    SourceRow,
    Paragraph,
    PipeTableRow,
    FencedCodeBlock,
    IndentedCodeBlock,
    ThematicBreak,
    LinkReferenceDefinition,
    HtmlBlock,
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
    row_to_item: Vec<usize>,
}

impl RenderedDisplayIndex {
    pub(crate) fn build(snapshot: &BufferSnapshot) -> Arc<Self> {
        let version = snapshot.version().clone();
        let row_count = snapshot.row_count() as usize;
        let mut items = Vec::new();
        let mut covered_rows = vec![false; row_count];

        for block in snapshot.syntax_tree().blocks() {
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

            if kind == RenderedDisplayItemKind::PipeTableRow {
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
            } else if matches!(
                kind,
                RenderedDisplayItemKind::FencedCodeBlock
                    | RenderedDisplayItemKind::IndentedCodeBlock
            ) || kind == RenderedDisplayItemKind::Paragraph
                && paragraph_can_merge(
                    snapshot,
                    block.source_range.clone(),
                    block.row_range.clone(),
                )
            {
                push_item(
                    &mut items,
                    &mut covered_rows,
                    block.source_range.clone(),
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

        for row in 0..row_count {
            if covered_rows[row] {
                continue;
            }
            let source_range = super::row_source_range(snapshot, row as u32);
            push_item(
                &mut items,
                &mut covered_rows,
                source_range,
                row..row + 1,
                RenderedDisplayItemKind::SourceRow,
            );
        }

        items.sort_by_key(|item| (item.row_range.start, item.row_range.end, item.index));
        for (index, item) in items.iter_mut().enumerate() {
            item.index = index;
        }

        let mut row_to_item = vec![0; row_count];
        for item in &items {
            for row in item.row_range.clone() {
                if let Some(slot) = row_to_item.get_mut(row) {
                    *slot = item.index;
                }
            }
        }

        Arc::new(Self {
            version,
            items,
            row_to_item,
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
        self.row_to_item.get(row).copied()
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

fn item_kind_for_block(kind: MarkdownBlockKind) -> Option<RenderedDisplayItemKind> {
    Some(match kind {
        MarkdownBlockKind::Paragraph => RenderedDisplayItemKind::Paragraph,
        MarkdownBlockKind::PipeTable => RenderedDisplayItemKind::PipeTableRow,
        MarkdownBlockKind::FencedCodeBlock => RenderedDisplayItemKind::FencedCodeBlock,
        MarkdownBlockKind::IndentedCodeBlock => RenderedDisplayItemKind::IndentedCodeBlock,
        MarkdownBlockKind::ThematicBreak => RenderedDisplayItemKind::ThematicBreak,
        MarkdownBlockKind::LinkReferenceDefinition => {
            RenderedDisplayItemKind::LinkReferenceDefinition
        }
        MarkdownBlockKind::HtmlBlock => RenderedDisplayItemKind::HtmlBlock,
        MarkdownBlockKind::Blank
        | MarkdownBlockKind::AtxHeading { .. }
        | MarkdownBlockKind::SetextHeading { .. }
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

fn display_item_id(
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
