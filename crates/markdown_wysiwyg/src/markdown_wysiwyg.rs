use std::{collections::HashMap, fmt, ops::Range};

use tree_sitter::{InputEdit, Node, Point, Tree};

mod blocks;
mod inline;
mod parser;
mod projection;
mod tables;

use blocks::collect_blocks;
use inline::{
    collect_inline_spans, collect_projection_replacements, inline_span_prefix_maximum_ends,
    projection_marker_dependencies, projection_marker_prefix_maximum_ends,
    projection_replacement_prefix_maximum_ends,
};
use parser::parse_markdown;
use tables::collect_tables;

#[derive(Clone)]
pub struct MarkdownSyntaxTree {
    tree: MarkdownParseTree,
    source_len: usize,
    line_starts: Vec<usize>,
    blocks: Vec<MarkdownBlock>,
    tables: Vec<MarkdownTable>,
    inline_spans: Vec<MarkdownInlineSpan>,
    inline_span_prefix_maximum_ends: Vec<usize>,
    projection_replacements: Vec<MarkdownProjectionReplacement>,
    projection_replacement_prefix_maximum_ends: Vec<usize>,
    projection_marker_dependencies: Vec<ProjectionMarkerDependency>,
    projection_marker_prefix_maximum_ends: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct MarkdownParseTree {
    block_tree: Tree,
    inline_trees: Vec<MarkdownInlineTree>,
    inline_tree_by_parent_id: HashMap<usize, usize>,
}

#[derive(Clone, Debug)]
pub struct MarkdownInlineTree {
    pub parent_id: usize,
    pub parent_range: Range<usize>,
    tree: Tree,
}

impl fmt::Debug for MarkdownSyntaxTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MarkdownSyntaxTree")
            .field("source_len", &self.source_len)
            .field("line_starts", &self.line_starts)
            .field("blocks", &self.blocks)
            .field("tables", &self.tables)
            .field("inline_spans", &self.inline_spans)
            .field("projection_replacements", &self.projection_replacements)
            .finish_non_exhaustive()
    }
}

impl MarkdownParseTree {
    pub fn block_tree(&self) -> &Tree {
        &self.block_tree
    }

    pub fn inline_trees(&self) -> &[MarkdownInlineTree] {
        &self.inline_trees
    }

    pub fn inline_tree_for_parent(&self, parent: Node<'_>) -> Option<&Tree> {
        self.inline_tree_by_parent_id
            .get(&parent.id())
            .map(|index| &self.inline_trees[*index].tree)
    }

    fn edit(&mut self, edit: &InputEdit) {
        self.block_tree.edit(edit);
        for inline_tree in &mut self.inline_trees {
            inline_tree.tree.edit(edit);
        }
    }
}

impl MarkdownInlineTree {
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownBlock {
    pub id: MarkdownNodeId,
    pub kind: MarkdownBlockKind,
    pub source_range: Range<usize>,
    pub content_range: Range<usize>,
    pub marker_ranges: Vec<Range<usize>>,
    pub row_range: Range<usize>,
    pub tagfilter_disallowed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownNodeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownBlockKind {
    Blank,
    Paragraph,
    AtxHeading { level: u8 },
    SetextHeading { level: u8 },
    ThematicBreak,
    BlockQuote,
    OrderedList,
    UnorderedList,
    ListItem,
    TaskListItem { checked: bool },
    IndentedCodeBlock,
    FencedCodeBlock,
    HtmlBlock,
    LinkReferenceDefinition,
    PipeTable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownTable {
    pub id: MarkdownNodeId,
    pub source_range: Range<usize>,
    pub row_range: Range<usize>,
    pub header: MarkdownTableRow,
    pub delimiter: MarkdownTableRow,
    pub body: Vec<MarkdownTableRow>,
    pub alignments: Vec<MarkdownTableAlignment>,
    pub pipe_marker_ranges: Vec<Range<usize>>,
    pub delimiter_marker_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownTableRow {
    pub source_range: Range<usize>,
    pub row: usize,
    pub cells: Vec<MarkdownTableCell>,
    pub pipe_marker_ranges: Vec<Range<usize>>,
    pub delimiter_marker_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownTableCell {
    pub source_range: Range<usize>,
    pub content_range: Range<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MarkdownTableAlignment {
    #[default]
    Left,
    Center,
    Right,
}

impl MarkdownTable {
    pub fn rows(&self) -> impl Iterator<Item = &MarkdownTableRow> {
        std::iter::once(&self.header)
            .chain(std::iter::once(&self.delimiter))
            .chain(self.body.iter())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownInlineSpan {
    pub kind: MarkdownInlineKind,
    pub source_range: Range<usize>,
    pub content_ranges: Vec<Range<usize>>,
    pub marker_ranges: Vec<Range<usize>>,
    pub url: Option<String>,
    pub tagfilter_disallowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectionMarkerDependency {
    marker_range: Range<usize>,
    owner_source_range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MarkdownProjectionReplacement {
    source_range: Range<usize>,
    owner_source_range: Range<usize>,
    display_text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownInlineKind {
    Emphasis,
    Strong,
    InlineCode,
    Link,
    Strikethrough,
    Escape,
    Entity,
    HardBreak,
    SoftBreak,
    InlineHtml,
    Image,
    InlineMath,
}

impl MarkdownInlineKind {
    pub fn is_rendered_element_candidate(self) -> bool {
        matches!(self, Self::Image | Self::InlineMath)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkdownProjectionMap {
    source_len: usize,
    visible_source_range: Range<usize>,
    operations: Vec<MarkdownProjectionOperation>,
    hidden_ranges: Vec<Range<usize>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownProjectionOperation {
    Hide {
        source_range: Range<usize>,
    },
    Replace {
        source_range: Range<usize>,
        display_text: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkdownRangeSemantics {
    pub blocks: Vec<MarkdownBlock>,
    pub inline_spans: Vec<MarkdownInlineSpan>,
    pub projection: MarkdownProjectionMap,
    pub active_projection_source_ranges: Vec<Range<usize>>,
    pub rendered_element_candidates: Vec<MarkdownInlineSpan>,
}

impl MarkdownSyntaxTree {
    pub fn parse(source: &str) -> Self {
        Self::parse_with_previous_tree(source, None)
    }

    pub fn reparse_after_edit(
        &self,
        old_source: &str,
        old_range: Range<usize>,
        new_source: &str,
    ) -> Self {
        self.reparse_after_edit_range(old_source, old_range, new_source)
    }

    pub fn reparse_after_edit_range(
        &self,
        old_source: &str,
        old_range: Range<usize>,
        new_source: &str,
    ) -> Self {
        let old_line_starts = line_starts(old_source);
        let new_line_starts = line_starts(new_source);
        let inserted_len = new_source
            .len()
            .checked_sub(old_source.len() - (old_range.end - old_range.start))
            .expect("new source must match the supplied edit range");
        let new_end_byte = old_range.start + inserted_len;

        let mut edited_tree = self.tree.clone();
        edited_tree.edit(&InputEdit {
            start_byte: old_range.start,
            old_end_byte: old_range.end,
            new_end_byte,
            start_position: point_for_offset(&old_line_starts, old_range.start),
            old_end_position: point_for_offset(&old_line_starts, old_range.end),
            new_end_position: point_for_offset(&new_line_starts, new_end_byte),
        });

        Self::parse_with_previous_tree(new_source, Some(&edited_tree))
    }

    pub fn parse_tree(&self) -> &MarkdownParseTree {
        &self.tree
    }

    pub fn block_tree(&self) -> &Tree {
        self.tree.block_tree()
    }

    pub fn inline_trees(&self) -> &[MarkdownInlineTree] {
        self.tree.inline_trees()
    }

    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub fn blocks(&self) -> &[MarkdownBlock] {
        &self.blocks
    }

    pub fn tables(&self) -> &[MarkdownTable] {
        &self.tables
    }

    pub fn table_for_source_row(&self, row: usize) -> Option<&MarkdownTable> {
        self.tables
            .iter()
            .find(|table| table.row_range.contains(&row))
    }

    pub fn table_for_source_range(&self, range: Range<usize>) -> Option<&MarkdownTable> {
        self.tables
            .iter()
            .find(|table| ranges_overlap(&table.source_range, &range))
    }

    pub fn table_row_for_source_row(
        &self,
        row: usize,
    ) -> Option<(&MarkdownTable, &MarkdownTableRow)> {
        let table = self.table_for_source_row(row)?;
        table
            .rows()
            .find(|table_row| table_row.row == row)
            .map(|table_row| (table, table_row))
    }

    pub fn inline_spans(&self) -> &[MarkdownInlineSpan] {
        &self.inline_spans
    }

    pub fn inline_spans_in_source_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = &MarkdownInlineSpan> {
        let start = range.start;
        let end = range.end;
        let start_index = self.partition_inline_spans_by_prefix_end(start);
        self.inline_spans[start_index..]
            .iter()
            .take_while(move |span| span.source_range.start < end)
            .filter(move |span| span.source_range.start < end && span.source_range.end > start)
    }

    fn projection_replacements_in_source_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = &MarkdownProjectionReplacement> {
        let start = range.start;
        let end = range.end;
        let start_index = self.partition_projection_replacements_by_prefix_end(start);
        self.projection_replacements[start_index..]
            .iter()
            .take_while(move |replacement| replacement.source_range.start < end)
            .filter(move |replacement| {
                replacement.source_range.start < end && replacement.source_range.end > start
            })
    }

    pub fn blocks_in_source_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = &MarkdownBlock> {
        let start = self.partition_blocks_by_end(range.start);
        self.blocks[start..]
            .iter()
            .take_while(move |block| block.source_range.start < range.end)
    }

    pub fn source_range_for_rows(&self, rows: Range<usize>) -> Range<usize> {
        let start = self
            .line_starts
            .get(rows.start)
            .copied()
            .unwrap_or(self.source_len);
        let end = self
            .line_starts
            .get(rows.end)
            .copied()
            .unwrap_or(self.source_len);
        start..end
    }

    pub fn range_semantics_for_visible_rows(
        &self,
        rows: Range<usize>,
        active_source_range: Option<Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
    ) -> MarkdownRangeSemantics {
        self.range_semantics_for_source_range(
            self.source_range_for_rows(rows),
            active_source_range,
            inactive_source_ranges,
        )
    }

    pub fn range_semantics_for_source_range(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
    ) -> MarkdownRangeSemantics {
        let blocks = self
            .blocks_in_source_range(visible_source_range.clone())
            .cloned()
            .collect::<Vec<_>>();
        let inline_spans = self
            .inline_spans_in_source_range(visible_source_range.clone())
            .cloned()
            .collect::<Vec<_>>();
        let projection = self.projection_for_source_range_with_semantics(
            visible_source_range.clone(),
            active_source_range.as_ref(),
            inactive_source_ranges,
            blocks.iter(),
            inline_spans.iter(),
        );
        let active_projection_source_ranges = self
            .active_projection_source_ranges_for_source_range(
                visible_source_range,
                active_source_range,
                inactive_source_ranges,
            );
        let rendered_element_candidates = inline_spans
            .iter()
            .filter(|span| span.kind.is_rendered_element_candidate())
            .cloned()
            .collect();

        MarkdownRangeSemantics {
            blocks,
            inline_spans,
            projection,
            active_projection_source_ranges,
            rendered_element_candidates,
        }
    }

    pub fn projection_for_visible_rows(
        &self,
        rows: Range<usize>,
        active_source_range: Option<Range<usize>>,
    ) -> MarkdownProjectionMap {
        self.projection_for_source_range(self.source_range_for_rows(rows), active_source_range)
    }

    pub fn projection_for_source_range(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<Range<usize>>,
    ) -> MarkdownProjectionMap {
        self.projection_for_source_range_with_inactive_ranges(
            visible_source_range,
            active_source_range,
            &[],
        )
    }

    pub fn projection_for_source_range_with_inactive_ranges(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
    ) -> MarkdownProjectionMap {
        let blocks = self.blocks_in_source_range(visible_source_range.clone());
        let inline_spans = self.inline_spans_in_source_range(visible_source_range.clone());
        self.projection_for_source_range_with_semantics(
            visible_source_range,
            active_source_range.as_ref(),
            inactive_source_ranges,
            blocks,
            inline_spans,
        )
    }

    fn projection_for_source_range_with_semantics<'a>(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<&Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
        blocks: impl IntoIterator<Item = &'a MarkdownBlock>,
        inline_spans: impl IntoIterator<Item = &'a MarkdownInlineSpan>,
    ) -> MarkdownProjectionMap {
        let mut operations = Vec::new();
        for block in blocks {
            let block_is_active = if block.kind == MarkdownBlockKind::PipeTable {
                table_row_source_range_is_active(
                    &block.source_range,
                    &visible_source_range,
                    active_source_range,
                    inactive_source_ranges,
                )
            } else {
                source_range_is_active(
                    &block.source_range,
                    active_source_range,
                    inactive_source_ranges,
                )
            };
            if block_is_active {
                continue;
            }

            for marker_range in &block.marker_ranges {
                let start = marker_range.start.max(visible_source_range.start);
                let end = marker_range.end.min(visible_source_range.end);
                if start < end {
                    operations.push(MarkdownProjectionOperation::Hide {
                        source_range: start..end,
                    });
                }
            }
        }

        let active_table_row = self.tables.iter().any(|table| {
            table_row_source_range_is_active(
                &table.source_range,
                &visible_source_range,
                active_source_range,
                inactive_source_ranges,
            )
        });
        for span in inline_spans {
            let span_is_active = if active_table_row {
                ranges_overlap(&span.source_range, &visible_source_range)
            } else {
                source_range_is_active(
                    &span.source_range,
                    active_source_range,
                    inactive_source_ranges,
                )
            };
            if span_is_active {
                continue;
            }

            if span.kind == MarkdownInlineKind::SoftBreak {
                let start = span.source_range.start.max(visible_source_range.start);
                let end = span.source_range.end.min(visible_source_range.end);
                if start < end {
                    operations.push(MarkdownProjectionOperation::Replace {
                        source_range: start..end,
                        display_text: " ".to_string(),
                    });
                }
                continue;
            }

            for marker_range in &span.marker_ranges {
                let start = marker_range.start.max(visible_source_range.start);
                let end = marker_range.end.min(visible_source_range.end);
                if start < end {
                    operations.push(MarkdownProjectionOperation::Hide {
                        source_range: start..end,
                    });
                }
            }
        }

        for replacement in
            self.projection_replacements_in_source_range(visible_source_range.clone())
        {
            let replacement_is_active = if active_table_row {
                ranges_overlap(&replacement.source_range, &visible_source_range)
            } else {
                source_range_is_active(
                    &replacement.owner_source_range,
                    active_source_range,
                    inactive_source_ranges,
                )
            };
            if replacement_is_active {
                continue;
            }

            let start = replacement
                .source_range
                .start
                .max(visible_source_range.start);
            let end = replacement.source_range.end.min(visible_source_range.end);
            if start < end {
                operations.push(MarkdownProjectionOperation::Replace {
                    source_range: start..end,
                    display_text: replacement.display_text.clone(),
                });
            }
        }

        MarkdownProjectionMap::with_operations(self.source_len, visible_source_range, operations)
    }

    pub fn active_projection_source_ranges_for_source_range(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
    ) -> Vec<Range<usize>> {
        let Some(active_source_range) = active_source_range.as_ref() else {
            return Vec::new();
        };

        let start_index =
            self.partition_projection_marker_dependencies_by_prefix_end(visible_source_range.start);
        let mut source_ranges = self.projection_marker_dependencies[start_index..]
            .iter()
            .take_while(|dependency| dependency.marker_range.start < visible_source_range.end)
            .filter(|dependency| ranges_overlap(&dependency.marker_range, &visible_source_range))
            .filter(|dependency| {
                source_range_is_active(
                    &dependency.owner_source_range,
                    Some(active_source_range),
                    inactive_source_ranges,
                )
            })
            .map(|dependency| dependency.owner_source_range.clone())
            .collect::<Vec<_>>();

        source_ranges.sort_by_key(|source_range| (source_range.start, source_range.end));
        source_ranges.dedup();
        source_ranges
    }

    fn parse_with_previous_tree(source: &str, old_tree: Option<&MarkdownParseTree>) -> Self {
        let tree = parse_markdown(source, old_tree);
        let line_starts = line_starts(source);
        let blocks = collect_blocks(source, &line_starts, tree.block_tree());
        let tables = collect_tables(source, &line_starts, &blocks);
        let inline_spans = collect_inline_spans(source, &tree);
        let inline_span_prefix_maximum_ends = inline_span_prefix_maximum_ends(&inline_spans);
        let projection_replacements = collect_projection_replacements(source, &tree);
        let projection_replacement_prefix_maximum_ends =
            projection_replacement_prefix_maximum_ends(&projection_replacements);
        let projection_marker_dependencies =
            projection_marker_dependencies(&blocks, &inline_spans, &projection_replacements);
        let projection_marker_prefix_maximum_ends =
            projection_marker_prefix_maximum_ends(&projection_marker_dependencies);

        Self {
            tree,
            source_len: source.len(),
            line_starts,
            blocks,
            tables,
            inline_spans,
            inline_span_prefix_maximum_ends,
            projection_replacements,
            projection_replacement_prefix_maximum_ends,
            projection_marker_dependencies,
            projection_marker_prefix_maximum_ends,
        }
    }

    fn partition_blocks_by_end(&self, offset: usize) -> usize {
        self.blocks
            .partition_point(|block| block.source_range.end <= offset)
    }

    fn partition_inline_spans_by_prefix_end(&self, offset: usize) -> usize {
        self.inline_span_prefix_maximum_ends
            .partition_point(|end| *end <= offset)
    }

    fn partition_projection_marker_dependencies_by_prefix_end(&self, offset: usize) -> usize {
        self.projection_marker_prefix_maximum_ends
            .partition_point(|end| *end <= offset)
    }

    fn partition_projection_replacements_by_prefix_end(&self, offset: usize) -> usize {
        self.projection_replacement_prefix_maximum_ends
            .partition_point(|end| *end <= offset)
    }
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn line_range(source: &str, line_starts: &[usize], row: usize) -> Range<usize> {
    let start = line_starts[row];
    let end = line_starts.get(row + 1).copied().unwrap_or(source.len());
    start..end
}

fn line_range_checked(source: &str, line_starts: &[usize], row: usize) -> Option<Range<usize>> {
    let start = line_starts.get(row).copied()?;
    let end = line_starts.get(row + 1).copied().unwrap_or(source.len());
    Some(start..end)
}

fn trim_line_end(source: &str, mut range: Range<usize>) -> Range<usize> {
    while range.end > range.start && matches!(source.as_bytes()[range.end - 1], b'\r' | b'\n') {
        range.end -= 1;
    }
    range
}

fn last_line_range(source: &str, range: Range<usize>) -> Option<Range<usize>> {
    let trimmed = trim_line_end(source, range.clone());
    if trimmed.is_empty() {
        return None;
    }

    let start = source[range.start..trimmed.end]
        .rfind('\n')
        .map(|index| range.start + index + 1)
        .unwrap_or(range.start);
    Some(start..trimmed.end)
}

fn trim_ascii_whitespace(source: &str, mut range: Range<usize>) -> Range<usize> {
    while range.start < range.end && matches!(source.as_bytes()[range.start], b' ' | b'\t') {
        range.start += 1;
    }
    while range.end > range.start && matches!(source.as_bytes()[range.end - 1], b' ' | b'\t') {
        range.end -= 1;
    }
    range
}

fn point_for_offset(line_starts: &[usize], offset: usize) -> Point {
    let row = line_starts.partition_point(|line_start| *line_start <= offset) - 1;
    Point {
        row,
        column: offset - line_starts[row],
    }
}

fn node_id(node: Node<'_>) -> MarkdownNodeId {
    MarkdownNodeId(node.id() as u64)
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn range_contains(container: &Range<usize>, candidate: &Range<usize>) -> bool {
    container.start <= candidate.start && container.end >= candidate.end
}

fn source_range_is_active(
    source_range: &Range<usize>,
    active_source_range: Option<&Range<usize>>,
    inactive_source_ranges: &[Range<usize>],
) -> bool {
    let Some(active_source_range) = active_source_range else {
        return false;
    };
    if !ranges_overlap(source_range, active_source_range) {
        return false;
    }

    !inactive_source_ranges
        .iter()
        .any(|inactive_source_range| range_contains(inactive_source_range, source_range))
}

fn table_row_source_range_is_active(
    table_source_range: &Range<usize>,
    row_source_range: &Range<usize>,
    active_source_range: Option<&Range<usize>>,
    inactive_source_ranges: &[Range<usize>],
) -> bool {
    let Some(active_source_range) = active_source_range else {
        return false;
    };
    if !ranges_overlap(table_source_range, active_source_range)
        || !ranges_overlap(row_source_range, active_source_range)
    {
        return false;
    }

    !inactive_source_ranges
        .iter()
        .any(|inactive_source_range| range_contains(inactive_source_range, row_source_range))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_atx_headings_with_tree_sitter() {
        let tree = MarkdownSyntaxTree::parse("# Title\n\nText\n");
        assert_eq!(tree.block_tree().root_node().kind(), "document");
        assert_eq!(tree.source_len(), 14);
        assert_eq!(tree.blocks().len(), 3);
        assert_eq!(
            tree.blocks()[0].kind,
            MarkdownBlockKind::AtxHeading { level: 1 }
        );
        assert_eq!(tree.blocks()[0].source_range, 0..8);
        assert_eq!(tree.blocks()[0].content_range, 2..7);
        assert_eq!(tree.blocks()[0].marker_ranges, vec![0..2]);
        assert_eq!(tree.blocks()[1].kind, MarkdownBlockKind::Blank);
        assert_eq!(tree.blocks()[2].kind, MarkdownBlockKind::Paragraph);
    }

    #[test]
    fn parses_setext_headings_with_marker_ranges() {
        let source = "Title\n=====\n\nSubtitle\n--------\n";
        let tree = MarkdownSyntaxTree::parse(source);

        assert_eq!(tree.blocks().len(), 3);
        assert_eq!(
            tree.blocks()[0].kind,
            MarkdownBlockKind::SetextHeading { level: 1 }
        );
        assert_eq!(&source[tree.blocks()[0].content_range.clone()], "Title");
        assert_eq!(&source[tree.blocks()[0].marker_ranges[0].clone()], "=====");
        assert_eq!(tree.blocks()[0].row_range, 0..2);
        assert_eq!(tree.blocks()[1].kind, MarkdownBlockKind::Blank);
        assert_eq!(
            tree.blocks()[2].kind,
            MarkdownBlockKind::SetextHeading { level: 2 }
        );
        assert_eq!(&source[tree.blocks()[2].content_range.clone()], "Subtitle");
        assert_eq!(
            &source[tree.blocks()[2].marker_ranges[0].clone()],
            "--------"
        );
        assert_eq!(tree.blocks()[2].row_range, 3..5);
    }

    #[test]
    fn parses_additional_gfm_leaf_blocks() {
        let source = "---\n\n    code\n\n<div>\nhi\n</div>\n\n[ref]: https://example.com\n";
        let tree = MarkdownSyntaxTree::parse(source);

        let thematic_break = tree
            .blocks()
            .iter()
            .find(|block| block.kind == MarkdownBlockKind::ThematicBreak)
            .expect("expected thematic break");
        assert_eq!(
            &source[trim_line_end(source, thematic_break.source_range.clone())],
            "---"
        );
        assert!(thematic_break.content_range.is_empty());

        let indented_code = tree
            .blocks()
            .iter()
            .find(|block| block.kind == MarkdownBlockKind::IndentedCodeBlock)
            .expect("expected indented code block");
        assert_eq!(
            &source[trim_line_end(source, indented_code.content_range.clone())],
            "    code"
        );

        let html = tree
            .blocks()
            .iter()
            .find(|block| block.kind == MarkdownBlockKind::HtmlBlock)
            .expect("expected HTML block");
        let html_content = &source[trim_line_end(source, html.content_range.clone())];
        assert!(html_content.starts_with("<div>"));
        assert!(html_content.ends_with("</div>"));
        assert!(!html.tagfilter_disallowed);

        let link_reference = tree
            .blocks()
            .iter()
            .find(|block| block.kind == MarkdownBlockKind::LinkReferenceDefinition)
            .expect("expected link reference definition");
        assert_eq!(
            &source[trim_line_end(source, link_reference.content_range.clone())],
            "[ref]: https://example.com"
        );
    }

    #[test]
    fn parses_blockquotes_and_list_containers_without_losing_nested_blocks() {
        let source = "> quote\n> - [ ] todo\n>   1. ordered\n\n- loose\n  - nested\n1. one\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let block_source =
            |block: &MarkdownBlock| &source[trim_line_end(source, block.source_range.clone())];

        let blockquote = tree
            .blocks()
            .iter()
            .find(|block| block.kind == MarkdownBlockKind::BlockQuote)
            .expect("expected block quote");
        let marker_sources = |block: &MarkdownBlock| {
            block
                .marker_ranges
                .iter()
                .map(|range| &source[range.clone()])
                .collect::<Vec<_>>()
        };
        assert_eq!(blockquote.row_range, 0..3);
        assert_eq!(
            block_source(blockquote),
            "> quote\n> - [ ] todo\n>   1. ordered"
        );
        assert_eq!(marker_sources(blockquote), vec!["> ", "> ", "> "]);

        let unordered_lists = tree
            .blocks()
            .iter()
            .filter(|block| block.kind == MarkdownBlockKind::UnorderedList)
            .collect::<Vec<_>>();
        assert!(
            unordered_lists
                .iter()
                .any(|block| block_source(block).contains("- [ ] todo"))
        );
        assert!(
            unordered_lists
                .iter()
                .any(|block| block_source(block).contains("- nested"))
        );

        let ordered_lists = tree
            .blocks()
            .iter()
            .filter(|block| block.kind == MarkdownBlockKind::OrderedList)
            .collect::<Vec<_>>();
        assert!(
            ordered_lists
                .iter()
                .any(|block| block_source(block).contains("1. ordered"))
        );
        assert!(
            ordered_lists
                .iter()
                .any(|block| block_source(block).contains("1. one"))
        );

        let list_item_sources = tree
            .blocks()
            .iter()
            .filter(|block| {
                matches!(
                    block.kind,
                    MarkdownBlockKind::ListItem | MarkdownBlockKind::TaskListItem { .. }
                )
            })
            .map(block_source)
            .collect::<Vec<_>>();
        let list_item_marker_sources = tree
            .blocks()
            .iter()
            .filter(|block| {
                matches!(
                    block.kind,
                    MarkdownBlockKind::ListItem | MarkdownBlockKind::TaskListItem { .. }
                )
            })
            .map(marker_sources)
            .collect::<Vec<_>>();
        assert!(
            list_item_marker_sources
                .iter()
                .any(|markers| markers == &vec!["- "])
        );
        assert!(
            list_item_marker_sources
                .iter()
                .any(|markers| markers == &vec!["  - "])
        );
        assert!(
            list_item_marker_sources
                .iter()
                .any(|markers| markers == &vec!["1. "])
        );
        assert!(
            list_item_sources
                .iter()
                .any(|source| source.contains("[ ] todo"))
        );
        assert!(
            list_item_sources
                .iter()
                .any(|source| source.contains("ordered"))
        );
        assert!(
            list_item_sources
                .iter()
                .any(|source| source.contains("loose"))
        );
        assert!(
            list_item_sources
                .iter()
                .any(|source| source.contains("nested"))
        );
        assert!(
            list_item_sources
                .iter()
                .any(|source| source.contains("one"))
        );
        assert!(tree.blocks().iter().any(|block| {
            block.kind == MarkdownBlockKind::Paragraph
                && block.source_range.start >= blockquote.source_range.start
                && block.source_range.end <= blockquote.source_range.end
        }));
    }

    #[test]
    fn parses_task_list_item_semantics() {
        let source = "- [ ] todo\n- [x] done\n- [X] cap\n- [ ]todo\n> - [x] quoted\n\n- outer\n  - [ ] nested\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let item_text =
            |block: &MarkdownBlock| &source[trim_line_end(source, block.source_range.clone())];
        let content_text =
            |block: &MarkdownBlock| &source[trim_line_end(source, block.content_range.clone())];

        let task_items = tree
            .blocks()
            .iter()
            .filter_map(|block| match block.kind {
                MarkdownBlockKind::TaskListItem { checked } => Some((block, checked)),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(task_items.len(), 5);
        assert!(task_items.iter().any(|(block, checked)| {
            !checked && item_text(block) == "- [ ] todo" && content_text(block) == "todo"
        }));
        assert!(task_items.iter().any(|(block, checked)| {
            *checked && item_text(block) == "- [x] done" && content_text(block) == "done"
        }));
        assert!(task_items.iter().any(|(block, checked)| {
            *checked && item_text(block) == "- [X] cap" && content_text(block) == "cap"
        }));
        assert!(task_items.iter().any(|(block, checked)| {
            *checked && item_text(block) == "- [x] quoted" && content_text(block) == "quoted"
        }));
        assert!(task_items.iter().any(|(block, checked)| {
            !checked && item_text(block) == "  - [ ] nested" && content_text(block) == "nested"
        }));

        let invalid = tree
            .blocks()
            .iter()
            .find(|block| {
                block.kind == MarkdownBlockKind::ListItem && item_text(block) == "- [ ]todo"
            })
            .expect("expected invalid task marker to remain a plain list item");
        assert_eq!(content_text(invalid), "[ ]todo");
    }

    #[test]
    fn reparses_after_edit_with_tree_sitter() {
        let old_source = "# Title\nBody\n";
        let tree = MarkdownSyntaxTree::parse(old_source);
        let new_source = "# Title!\nBody\n";
        let tree = tree.reparse_after_edit(old_source, 7..7, new_source);

        assert_eq!(tree.source_len(), new_source.len());
        assert_eq!(
            tree.blocks()[0].kind,
            MarkdownBlockKind::AtxHeading { level: 1 }
        );
        assert_eq!(tree.blocks()[0].source_range, 0..9);
        assert_eq!(tree.blocks()[0].content_range, 2..8);
    }

    #[test]
    fn reparses_after_edit_range_with_tree_sitter() {
        let old_source = "# Title\nBody\n";
        let tree = MarkdownSyntaxTree::parse(old_source);
        let new_source = "# Title\n## Body\n";
        let tree = tree.reparse_after_edit_range(old_source, 8..12, new_source);

        assert_eq!(tree.source_len(), new_source.len());
        assert_eq!(
            tree.blocks()[1].kind,
            MarkdownBlockKind::AtxHeading { level: 2 }
        );
    }

    #[test]
    fn parses_gfm_tagfilter_disallowed_raw_html() {
        let source = "<script>alert(1)</script>\n\n<div>safe</div>\n\nInline <IFRAME src=\"x\"></IFRAME> <scripted>ok</scripted>\n";
        let tree = MarkdownSyntaxTree::parse(source);

        let html_blocks = tree
            .blocks()
            .iter()
            .filter(|block| block.kind == MarkdownBlockKind::HtmlBlock)
            .map(|block| {
                (
                    &source[trim_line_end(source, block.source_range.clone())],
                    block.tagfilter_disallowed,
                )
            })
            .collect::<Vec<_>>();
        assert!(
            html_blocks
                .iter()
                .any(|(text, disallowed)| text.starts_with("<script>") && *disallowed)
        );
        assert!(
            html_blocks
                .iter()
                .any(|(text, disallowed)| text.starts_with("<div>") && !*disallowed)
        );

        let inline_html = tree
            .inline_spans()
            .iter()
            .filter(|span| span.kind == MarkdownInlineKind::InlineHtml)
            .map(|span| {
                (
                    &source[span.source_range.clone()],
                    span.tagfilter_disallowed,
                )
            })
            .collect::<Vec<_>>();
        assert!(inline_html.contains(&("<IFRAME src=\"x\">", true)));
        assert!(inline_html.contains(&("</IFRAME>", true)));
        assert!(inline_html.contains(&("<scripted>", false)));
    }

    #[test]
    fn parses_inline_trees_with_tree_sitter() {
        let tree = MarkdownSyntaxTree::parse("Text with **bold** and [link](https://zed.dev).\n");

        assert_eq!(tree.inline_trees().len(), 1);
        let inline_root = tree.inline_trees()[0].tree().root_node();
        let mut cursor = inline_root.walk();
        let inline_kinds = inline_root
            .named_children(&mut cursor)
            .map(|node| node.kind())
            .collect::<Vec<_>>();

        assert!(inline_kinds.contains(&"strong_emphasis"));
        assert!(inline_kinds.contains(&"inline_link"));
        assert_eq!(tree.inline_spans().len(), 2);
        assert_eq!(tree.inline_spans()[0].kind, MarkdownInlineKind::Strong);
        assert_eq!(tree.inline_spans()[1].kind, MarkdownInlineKind::Link);
    }

    #[test]
    fn parses_gfm_inline_leaf_spans() {
        let source = "one \\* &amp;  \ntwo <span>html</span>\nthree\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let spans = tree.inline_spans();

        assert!(spans.iter().any(|span| {
            span.kind == MarkdownInlineKind::Escape && &source[span.source_range.clone()] == "\\*"
        }));
        assert!(spans.iter().any(|span| {
            span.kind == MarkdownInlineKind::Entity && &source[span.source_range.clone()] == "&amp;"
        }));
        assert!(
            spans
                .iter()
                .any(|span| span.kind == MarkdownInlineKind::HardBreak)
        );
        assert!(
            spans
                .iter()
                .any(|span| span.kind == MarkdownInlineKind::SoftBreak)
        );
        assert_eq!(
            spans
                .iter()
                .filter(|span| span.kind == MarkdownInlineKind::InlineHtml)
                .map(|span| &source[span.source_range.clone()])
                .collect::<Vec<_>>(),
            vec!["<span>", "</span>"]
        );
        assert!(
            spans
                .iter()
                .filter(|span| span.kind == MarkdownInlineKind::InlineHtml)
                .all(|span| span.marker_ranges.is_empty() && !span.tagfilter_disallowed)
        );
    }

    #[test]
    fn parses_reference_and_autolink_inline_spans() {
        let source = "See <https://example.com> <me@example.com> [full][ref] [ref][] [shortcut]\n\n[ref]: https://example.com\n[shortcut]: https://example.com\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let links = tree
            .inline_spans()
            .iter()
            .filter(|span| span.kind == MarkdownInlineKind::Link)
            .map(|span| &source[span.source_range.clone()])
            .collect::<Vec<_>>();

        assert_eq!(
            links,
            vec![
                "<https://example.com>",
                "<me@example.com>",
                "[full][ref]",
                "[ref][]",
                "[shortcut]"
            ]
        );
    }

    #[test]
    fn inline_spans_in_source_range_returns_overlapping_spans() {
        let source = "before **bold**\nafter [link](url)\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let second_row_start = source.find("after").expect("expected second row");

        let first_row_spans = tree
            .inline_spans_in_source_range(0..second_row_start)
            .map(|span| span.kind)
            .collect::<Vec<_>>();
        let second_row_spans = tree
            .inline_spans_in_source_range(second_row_start..source.len())
            .map(|span| span.kind)
            .collect::<Vec<_>>();

        assert_eq!(
            first_row_spans,
            vec![MarkdownInlineKind::Strong, MarkdownInlineKind::SoftBreak]
        );
        assert_eq!(second_row_spans, vec![MarkdownInlineKind::Link]);
    }

    #[test]
    fn inline_spans_in_source_range_includes_spans_starting_before_range() {
        let source = "before **bold\nstill bold** after\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let second_row_start = source.find("still").expect("expected second row");

        let second_row_spans = tree
            .inline_spans_in_source_range(second_row_start..source.len())
            .map(|span| span.kind)
            .collect::<Vec<_>>();

        assert_eq!(second_row_spans, vec![MarkdownInlineKind::Strong]);
    }

    #[test]
    fn hides_inactive_heading_markers_in_projection() {
        let tree = MarkdownSyntaxTree::parse("# Title\nBody\n");
        let projection = tree.projection_for_visible_rows(0..2, None);
        assert_eq!(projection.hidden_ranges(), &[0..2]);
        assert_eq!(projection.source_to_display(0), 0);
        assert_eq!(projection.source_to_display(2), 0);
        assert_eq!(projection.source_to_display(7), 5);
        assert_eq!(projection.display_to_source(0), 2);
        assert_eq!(projection.display_to_source(5), 7);
    }

    #[test]
    fn reveals_active_block_markers() {
        let tree = MarkdownSyntaxTree::parse("# Title\n## Other\n");
        let projection = tree.projection_for_visible_rows(0..2, Some(0..1));
        assert_eq!(projection.hidden_ranges(), &[8..11]);
        assert_eq!(projection.source_to_display(2), 2);
        assert_eq!(projection.display_to_source(0), 0);
    }

    #[test]
    fn hides_inactive_blockquote_and_list_item_markers_in_projection() {
        let source = "> quote\n- [ ] todo\n  - nested\n1) ordered\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let projection = tree.projection_for_visible_rows(0..4, None);

        assert_eq!(
            projection.project_source_text(source),
            "quote\n☐ todo\nnested\nordered\n"
        );

        let active_quote = source.find("quote").expect("expected quote text");
        let projection =
            tree.projection_for_visible_rows(0..4, Some(active_quote..active_quote + 1));

        assert_eq!(
            projection.project_source_text(source),
            "> quote\n☐ todo\nnested\nordered\n"
        );
    }

    #[test]
    fn hides_inactive_inline_markers_in_projection() {
        let tree = MarkdownSyntaxTree::parse("Before **bold** after\n");
        let projection = tree.projection_for_visible_rows(0..1, None);
        assert_eq!(projection.hidden_ranges(), &[7..9, 13..15]);
        assert_eq!(projection.display_len(), "Before bold after\n".len());
    }

    #[test]
    fn projection_operations_support_hide_and_replace_mappings() {
        let projection = MarkdownProjectionMap::with_operations(
            10,
            0..10,
            [
                MarkdownProjectionOperation::Hide { source_range: 0..2 },
                MarkdownProjectionOperation::Replace {
                    source_range: 5..8,
                    display_text: "X".to_string(),
                },
            ],
        );

        assert_eq!(projection.hidden_ranges(), &[0..2, 5..8]);
        assert_eq!(
            projection.operations(),
            &[
                MarkdownProjectionOperation::Hide { source_range: 0..2 },
                MarkdownProjectionOperation::Replace {
                    source_range: 5..8,
                    display_text: "X".to_string()
                }
            ]
        );
        assert_eq!(projection.project_source_text("0123456789"), "234X89");
        assert_eq!(projection.display_len(), 6);
        assert_eq!(projection.source_to_display(0), 0);
        assert_eq!(projection.source_to_display(2), 0);
        assert_eq!(projection.source_to_display(5), 3);
        assert_eq!(projection.source_to_display(6), 3);
        assert_eq!(projection.source_to_display(8), 4);
        assert_eq!(projection.source_to_display(10), 6);
        assert_eq!(projection.display_to_source(0), 2);
        assert_eq!(projection.display_to_source(3), 5);
        assert_eq!(projection.display_to_source(4), 8);
        assert_eq!(projection.display_to_source(6), 10);
    }

    #[test]
    fn projection_replaces_inactive_escapes_and_entities() {
        let source = "Escape \\* &amp; &#42; &#x2A;\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let escaped = source.find("\\*").expect("expected escape");
        let amp = source.find("&amp;").expect("expected entity");
        let decimal = source.find("&#42;").expect("expected decimal entity");
        let hex = source.find("&#x2A;").expect("expected hex entity");

        let projection = tree.projection_for_visible_rows(0..1, None);

        assert_eq!(
            projection.hidden_ranges(),
            &[
                escaped..escaped + 2,
                amp..amp + 5,
                decimal..decimal + 5,
                hex..hex + 6
            ]
        );
        assert_eq!(projection.project_source_text(source), "Escape * & * *\n");
        assert_eq!(projection.display_len(), "Escape * & * *\n".len());
    }

    #[test]
    fn projection_replaces_inactive_soft_breaks_with_spaces() {
        let source = "first\nsecond\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let projection = tree.projection_for_visible_rows(0..2, None);

        assert_eq!(projection.project_source_text(source), "first second\n");
    }

    #[test]
    fn projection_replaces_full_html5_named_entities() {
        let source = "Entities &CounterClockwiseContourIntegral; &Aopf; &NotEqualTilde;\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let contour = source
            .find("&CounterClockwiseContourIntegral;")
            .expect("expected contour entity");
        let aopf = source.find("&Aopf;").expect("expected Aopf entity");
        let not_equal_tilde = source
            .find("&NotEqualTilde;")
            .expect("expected NotEqualTilde entity");

        let projection = tree.projection_for_visible_rows(0..1, None);

        assert_eq!(
            projection.hidden_ranges(),
            &[
                contour..contour + "&CounterClockwiseContourIntegral;".len(),
                aopf..aopf + "&Aopf;".len(),
                not_equal_tilde..not_equal_tilde + "&NotEqualTilde;".len()
            ]
        );
        assert_eq!(
            projection.project_source_text(source),
            "Entities \u{2233} \u{1D538} \u{2242}\u{0338}\n"
        );
    }

    #[test]
    fn active_escape_reveals_source_projection() {
        let source = "Escape \\* &amp;\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let escaped = source.find("\\*").expect("expected escape");
        let projection = tree.projection_for_visible_rows(0..1, Some(escaped + 1..escaped + 2));

        assert_eq!(projection.project_source_text(source), "Escape \\* &\n");
        assert_eq!(
            tree.active_projection_source_ranges_for_source_range(
                0..source.len(),
                Some(escaped + 1..escaped + 2),
                &[],
            ),
            vec![escaped..escaped + 2]
        );
    }

    #[test]
    fn projection_replaces_inactive_task_list_markers() {
        let source = "- [ ] todo\n- [x] done\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let unchecked = source.find("[ ]").expect("expected unchecked task marker");
        let checked = source.find("[x]").expect("expected checked task marker");

        let projection = tree.projection_for_visible_rows(0..2, None);

        assert_eq!(
            projection.hidden_ranges(),
            &[0..unchecked + 3, 11..checked + 3]
        );
        assert_eq!(
            projection.project_source_text(source),
            "\u{2610} todo\n\u{2611} done\n"
        );
    }

    #[test]
    fn active_task_list_marker_reveals_source_projection() {
        let source = "- [ ] todo\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let marker = source.find("[ ]").expect("expected task marker");
        let projection = tree.projection_for_visible_rows(0..1, Some(marker..marker + 1));

        assert_eq!(projection.project_source_text(source), source);
        assert_eq!(
            tree.active_projection_source_ranges_for_source_range(
                0..source.len(),
                Some(marker..marker + 1),
                &[],
            ),
            vec![0..source.len(), marker..marker + 3]
        );
    }

    #[test]
    fn reveals_active_inline_markers() {
        let tree = MarkdownSyntaxTree::parse("Before **bold** after\n");
        let projection = tree.projection_for_visible_rows(0..1, Some(10..11));
        assert!(projection.hidden_ranges().is_empty());
        assert_eq!(projection.display_to_source(7), 7);
    }

    #[test]
    fn inactive_ranges_keep_markers_hidden_inside_active_range() {
        let tree = MarkdownSyntaxTree::parse("Before **bold** and $x$ after\n");
        let projection =
            tree.projection_for_source_range_with_inactive_ranges(0..30, Some(7..23), &[20..23]);

        assert_eq!(projection.hidden_ranges(), &[20..21, 22..23]);
        assert_eq!(projection.display_len(), 28);
    }

    #[test]
    fn active_projection_source_ranges_uses_marker_dependencies() {
        let tree = MarkdownSyntaxTree::parse("```rust\nlet x = 1;\n```\n");
        let content_start = "```rust\n".len();
        let content_end = content_start + "let x = 1;".len();
        let closing_marker_start = content_end + "\n".len();

        assert_eq!(
            tree.active_projection_source_ranges_for_source_range(
                closing_marker_start..closing_marker_start + "```".len(),
                Some(content_start..content_end),
                &[],
            ),
            vec![tree.blocks()[0].source_range.clone()]
        );
    }

    #[test]
    fn range_semantics_collects_visible_markdown_state_once() {
        let source = "# Title\nBefore **bold** and ![alt](img.png)\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let row_start = source.find("Before").expect("expected second row");
        let visible_source_range = row_start..source.len();
        let active_start = source.find("bold").expect("expected bold text");
        let active_source_range = active_start..active_start + 1;

        let semantics = tree.range_semantics_for_source_range(
            visible_source_range.clone(),
            Some(active_source_range.clone()),
            &[],
        );

        assert_eq!(
            semantics.blocks,
            tree.blocks_in_source_range(visible_source_range.clone())
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            semantics.inline_spans,
            tree.inline_spans_in_source_range(visible_source_range.clone())
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            semantics.projection,
            tree.projection_for_source_range_with_inactive_ranges(
                visible_source_range.clone(),
                Some(active_source_range.clone()),
                &[],
            )
        );
        assert_eq!(
            semantics.active_projection_source_ranges,
            tree.active_projection_source_ranges_for_source_range(
                visible_source_range,
                Some(active_source_range.clone()),
                &[],
            )
        );
        assert_eq!(
            semantics
                .rendered_element_candidates
                .iter()
                .map(|span| span.kind)
                .collect::<Vec<_>>(),
            vec![MarkdownInlineKind::Image]
        );
        assert_eq!(
            semantics,
            tree.range_semantics_for_visible_rows(1..2, Some(active_source_range), &[])
        );
    }

    #[test]
    fn clips_projection_to_visible_rows() {
        let tree = MarkdownSyntaxTree::parse("# One\n# Two\n# Three\n");
        let projection = tree.projection_for_visible_rows(1..2, None);
        assert_eq!(projection.visible_source_range(), 6..12);
        assert_eq!(projection.hidden_ranges(), &[6..8]);
        assert_eq!(projection.display_len(), 4);
    }

    #[test]
    fn keeps_fenced_code_block_as_one_block() {
        let source = "```rust\n# not heading\n```\n# Heading\n";
        let tree = MarkdownSyntaxTree::parse(source);
        assert_eq!(tree.blocks().len(), 2);
        assert_eq!(tree.blocks()[0].kind, MarkdownBlockKind::FencedCodeBlock);
        assert_eq!(tree.blocks()[0].row_range, 0..3);
        assert_eq!(
            tree.blocks()[1].kind,
            MarkdownBlockKind::AtxHeading { level: 1 }
        );
    }

    #[test]
    fn parses_pipe_table() {
        let source = "# Title\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let table_block = tree
            .blocks()
            .iter()
            .find(|b| b.kind == MarkdownBlockKind::PipeTable);
        assert!(table_block.is_some(), "expected a PipeTable block");
        let table = table_block.unwrap();
        assert!(
            !table.marker_ranges.is_empty(),
            "table should have marker ranges"
        );
        let has_pipe_marker = table
            .marker_ranges
            .iter()
            .any(|r| &source[r.clone()] == "|");
        assert!(
            has_pipe_marker,
            "table markers should include pipe characters"
        );
    }

    #[test]
    fn parses_pipe_table_structure_and_ranges() {
        let source = "| left | center | right |\n| :--- | :---: | ---: |\n| **a** |  | c |\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let table = tree.tables().first().expect("expected a table");

        assert_eq!(table.row_range, 0..3);
        assert_eq!(
            table.alignments,
            vec![
                MarkdownTableAlignment::Left,
                MarkdownTableAlignment::Center,
                MarkdownTableAlignment::Right
            ]
        );
        assert_eq!(
            table
                .header
                .cells
                .iter()
                .map(|cell| &source[cell.content_range.clone()])
                .collect::<Vec<_>>(),
            vec!["left", "center", "right"]
        );
        assert_eq!(
            table
                .body
                .first()
                .expect("expected a body row")
                .cells
                .iter()
                .map(|cell| &source[cell.content_range.clone()])
                .collect::<Vec<_>>(),
            vec!["**a**", "", "c"]
        );
        assert_eq!(table.delimiter_marker_ranges, vec![28..32, 35..40, 43..47]);
        assert_eq!(tree.table_for_source_row(1), Some(table));
        assert_eq!(
            tree.table_row_for_source_row(2).map(|(_, row)| row.row),
            Some(2)
        );
    }

    #[test]
    fn parses_pipe_table_without_leading_or_trailing_pipes() {
        let source = "left | center | right\n--- | :---: | ---:\n1 | 2 | 3\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let table = tree.tables().first().expect("expected a table");

        assert_eq!(
            table
                .header
                .cells
                .iter()
                .map(|cell| &source[cell.content_range.clone()])
                .collect::<Vec<_>>(),
            vec!["left", "center", "right"]
        );
        assert_eq!(
            table.alignments,
            vec![
                MarkdownTableAlignment::Left,
                MarkdownTableAlignment::Center,
                MarkdownTableAlignment::Right
            ]
        );
        assert_eq!(
            table
                .body
                .first()
                .expect("expected a body row")
                .cells
                .iter()
                .map(|cell| &source[cell.content_range.clone()])
                .collect::<Vec<_>>(),
            vec!["1", "2", "3"]
        );
    }

    #[test]
    fn parses_pipe_table_empty_cells() {
        let source = "| a |  | c |\n| - | - | - |\n|  | b |  |\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let table = tree.tables().first().expect("expected a table");

        assert_eq!(
            table
                .header
                .cells
                .iter()
                .map(|cell| &source[cell.content_range.clone()])
                .collect::<Vec<_>>(),
            vec!["a", "", "c"]
        );
        assert_eq!(
            table
                .body
                .first()
                .expect("expected a body row")
                .cells
                .iter()
                .map(|cell| &source[cell.content_range.clone()])
                .collect::<Vec<_>>(),
            vec!["", "b", ""]
        );
    }

    #[test]
    fn pipe_table_projection_reveals_only_active_source_row() {
        let source = "| a | b |\n| - | - |\n| **1** | 2 |\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let body_start = source.find("| **1").expect("expected body row");
        let header_projection =
            tree.projection_for_visible_rows(0..1, Some(body_start..body_start + 1));
        let body_projection =
            tree.projection_for_visible_rows(2..3, Some(body_start..body_start + 1));

        assert_eq!(header_projection.hidden_ranges(), &[0..1, 4..5, 8..9]);
        assert!(body_projection.hidden_ranges().is_empty());
    }

    #[test]
    fn malformed_pipe_table_does_not_build_structured_table() {
        let source = "| a | b |\n| not a delimiter |\n";
        let tree = MarkdownSyntaxTree::parse(source);

        assert!(tree.tables().is_empty());
    }

    #[test]
    fn parses_image_and_math_inline() {
        let source = "text ![alt](url) $math$\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let spans = tree.inline_spans();
        assert!(spans.iter().any(|s| s.kind == MarkdownInlineKind::Image));
        assert!(
            spans
                .iter()
                .any(|s| s.kind == MarkdownInlineKind::InlineMath)
        );
    }

    #[test]
    fn parses_block_math_markers_and_content_ranges() {
        let source = "$$x + y$$\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let span = tree
            .inline_spans()
            .iter()
            .find(|span| span.kind == MarkdownInlineKind::InlineMath)
            .expect("expected block math span");

        assert_eq!(span.source_range, 0.."$$x + y$$".len());
        assert_eq!(span.marker_ranges, vec![0..2, 7..9]);
        assert_eq!(span.content_ranges, vec![2..7]);
    }

    #[test]
    fn inline_math_projection_keeps_operator_content() {
        let tree = MarkdownSyntaxTree::parse("Before $x + y$ after\n");
        let projection = tree.projection_for_visible_rows(0..1, None);

        assert_eq!(projection.hidden_ranges(), &[7..8, 13..14]);
        assert_eq!(projection.display_len(), "Before x + y after\n".len());
    }

    #[test]
    fn block_math_projection_hides_double_dollar_markers() {
        let tree = MarkdownSyntaxTree::parse("$$x + y$$\n");
        let projection = tree.projection_for_visible_rows(0..1, None);

        assert_eq!(projection.hidden_ranges(), &[0..2, 7..9]);
        assert_eq!(projection.display_len(), "x + y\n".len());
    }
}
