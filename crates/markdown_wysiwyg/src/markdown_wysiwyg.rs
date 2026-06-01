#[cfg(any(test, perf_enabled))]
use std::{cell::Cell, time::Instant};
use std::{collections::HashMap, fmt, ops::Range};

use tree_sitter::{InputEdit, Node, Tree};

mod assembler;
mod backend;
mod blocks;
mod inline;
mod parser;
mod projection;
mod source;
mod structure;
mod tables;

use backend::{MarkdownBackendOutput, TreeSitterMarkdownBackend};
use source::{edit_byte_range, line_starts_after_edit_range, point_for_offset};
use structure::MarkdownStructureBlock;

#[cfg(any(test, perf_enabled))]
thread_local! {
    static MARKDOWN_SYNTAX_STATS: Cell<MarkdownSyntaxStats> =
        const { Cell::new(MarkdownSyntaxStats {
            parse_calls: 0,
            parse_ns: 0,
            block_parse_ns: 0,
            inline_parent_scan_ns: 0,
            inline_reuse_index_ns: 0,
            inline_range_build_ns: 0,
            inline_parse_ns: 0,
            line_start_collect_ns: 0,
            block_collect_ns: 0,
            table_collect_ns: 0,
            inline_collect_ns: 0,
            projection_collect_ns: 0,
        }) };
}

#[cfg(any(test, perf_enabled))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct MarkdownSyntaxStats {
    pub parse_calls: usize,
    pub parse_ns: u128,
    pub block_parse_ns: u128,
    pub inline_parent_scan_ns: u128,
    pub inline_reuse_index_ns: u128,
    pub inline_range_build_ns: u128,
    pub inline_parse_ns: u128,
    pub line_start_collect_ns: u128,
    pub block_collect_ns: u128,
    pub table_collect_ns: u128,
    pub inline_collect_ns: u128,
    pub projection_collect_ns: u128,
}

#[derive(Clone)]
pub struct MarkdownSyntaxTree {
    parser_state: MarkdownParseTree,
    data: MarkdownSyntaxData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownSyntaxData {
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
            .field("source_len", &self.data.source_len)
            .field("line_starts", &self.data.line_starts)
            .field("blocks", &self.data.blocks)
            .field("tables", &self.data.tables)
            .field("inline_spans", &self.data.inline_spans)
            .field(
                "projection_replacements",
                &self.data.projection_replacements,
            )
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
            inline_tree.parent_range = edit_byte_range(inline_tree.parent_range.clone(), edit);
        }
    }
}

impl MarkdownInlineTree {
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

impl MarkdownSyntaxData {
    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub fn line_starts(&self) -> &[usize] {
        &self.line_starts
    }

    pub fn blocks(&self) -> &[MarkdownBlock] {
        &self.blocks
    }

    pub fn tables(&self) -> &[MarkdownTable] {
        &self.tables
    }

    pub fn inline_spans(&self) -> &[MarkdownInlineSpan] {
        &self.inline_spans
    }

    pub fn projection_replacements(&self) -> &[MarkdownProjectionReplacement] {
        &self.projection_replacements
    }

    pub fn projection_marker_dependencies(&self) -> &[ProjectionMarkerDependency] {
        &self.projection_marker_dependencies
    }

    pub fn checksum_for_benchmarks(&self) -> usize {
        let mut checksum = self.source_len.wrapping_add(self.line_starts.len());
        for line_start in &self.line_starts {
            checksum = checksum.wrapping_mul(31).wrapping_add(*line_start);
        }
        for block in &self.blocks {
            checksum = checksum
                .wrapping_mul(31)
                .wrapping_add(block.source_range.start)
                .wrapping_add(block.source_range.end)
                .wrapping_add(block.content_range.start)
                .wrapping_add(block.content_range.end)
                .wrapping_add(block.marker_ranges.len())
                .wrapping_add(block.row_range.start)
                .wrapping_add(block.row_range.end)
                .wrapping_add(usize::from(block.tagfilter_disallowed));
        }
        for table in &self.tables {
            checksum = checksum
                .wrapping_mul(31)
                .wrapping_add(table.source_range.start)
                .wrapping_add(table.source_range.end)
                .wrapping_add(table.rows().count())
                .wrapping_add(table.pipe_marker_ranges.len())
                .wrapping_add(table.delimiter_marker_ranges.len());
        }
        for span in &self.inline_spans {
            checksum = checksum
                .wrapping_mul(31)
                .wrapping_add(span.source_range.start)
                .wrapping_add(span.source_range.end)
                .wrapping_add(span.content_ranges.len())
                .wrapping_add(span.marker_ranges.len())
                .wrapping_add(span.url.as_ref().map_or(0, String::len))
                .wrapping_add(usize::from(span.tagfilter_disallowed));
        }
        for replacement in &self.projection_replacements {
            checksum = checksum
                .wrapping_mul(31)
                .wrapping_add(replacement.source_range.start)
                .wrapping_add(replacement.source_range.end)
                .wrapping_add(replacement.owner_source_range.start)
                .wrapping_add(replacement.owner_source_range.end)
                .wrapping_add(replacement.display_text.len());
        }
        for dependency in &self.projection_marker_dependencies {
            checksum = checksum
                .wrapping_mul(31)
                .wrapping_add(dependency.marker_range.start)
                .wrapping_add(dependency.marker_range.end)
                .wrapping_add(dependency.owner_source_range.start)
                .wrapping_add(dependency.owner_source_range.end);
        }
        checksum
            .wrapping_add(self.inline_span_prefix_maximum_ends.len())
            .wrapping_add(self.projection_replacement_prefix_maximum_ends.len())
            .wrapping_add(self.projection_marker_prefix_maximum_ends.len())
    }
}

impl MarkdownBlock {
    fn from_structure(block: &MarkdownStructureBlock) -> Self {
        Self {
            id: block.id,
            kind: block.kind,
            source_range: block.source_range.clone(),
            content_range: block.content_range.clone(),
            marker_ranges: block.marker_ranges.clone(),
            row_range: block.row_range.clone(),
            tagfilter_disallowed: block.tagfilter_disallowed,
        }
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
pub struct ProjectionMarkerDependency {
    pub marker_range: Range<usize>,
    pub owner_source_range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownProjectionReplacement {
    pub source_range: Range<usize>,
    pub owner_source_range: Range<usize>,
    pub display_text: String,
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
        Self::parse_with_previous_tree(source, None, None)
    }

    #[cfg(any(test, perf_enabled))]
    pub fn reset_stats_for_tests() {
        MARKDOWN_SYNTAX_STATS.with(|stats| stats.set(MarkdownSyntaxStats::default()));
    }

    #[cfg(any(test, perf_enabled))]
    pub fn stats_for_tests() -> MarkdownSyntaxStats {
        MARKDOWN_SYNTAX_STATS.with(Cell::get)
    }

    pub fn reparse_after_edit_range(
        &self,
        old_range: Range<usize>,
        new_range: Range<usize>,
        new_source: &str,
    ) -> Self {
        let new_line_starts = line_starts_after_edit_range(
            &self.data.line_starts,
            old_range.clone(),
            new_range.clone(),
            new_source,
        );
        let old_len = self.data.source_len - old_range.len();
        let new_len = new_source.len() - new_range.len();
        assert_eq!(
            old_len, new_len,
            "new source must match the supplied edit ranges"
        );

        let mut edited_tree = self.parser_state.clone();
        edited_tree.edit(&InputEdit {
            start_byte: old_range.start,
            old_end_byte: old_range.end,
            new_end_byte: new_range.end,
            start_position: point_for_offset(&self.data.line_starts, old_range.start),
            old_end_position: point_for_offset(&self.data.line_starts, old_range.end),
            new_end_position: point_for_offset(&new_line_starts, new_range.end),
        });

        Self::parse_with_previous_syntax(new_source, self, &edited_tree, old_range, new_range)
    }

    pub fn parse_tree(&self) -> &MarkdownParseTree {
        &self.parser_state
    }

    pub fn syntax_data(&self) -> &MarkdownSyntaxData {
        &self.data
    }

    pub fn block_tree(&self) -> &Tree {
        self.parser_state.block_tree()
    }

    pub fn inline_trees(&self) -> &[MarkdownInlineTree] {
        self.parser_state.inline_trees()
    }

    pub fn source_len(&self) -> usize {
        self.data.source_len
    }

    fn parse_with_previous_tree(
        source: &str,
        old_tree: Option<&MarkdownParseTree>,
        changed_range: Option<&Range<usize>>,
    ) -> Self {
        let MarkdownBackendOutput {
            structure,
            parser_state,
            data,
        } = TreeSitterMarkdownBackend::parse(source, old_tree, changed_range);
        let _ = structure;

        Self { parser_state, data }
    }

    fn parse_with_previous_syntax(
        source: &str,
        previous: &MarkdownSyntaxTree,
        edited_tree: &MarkdownParseTree,
        old_range: Range<usize>,
        new_range: Range<usize>,
    ) -> Self {
        let MarkdownBackendOutput {
            structure,
            parser_state,
            data,
        } = TreeSitterMarkdownBackend::parse_after_edit(
            source,
            previous,
            edited_tree,
            old_range,
            new_range,
        );
        let _ = structure;

        Self { parser_state, data }
    }
}

#[cfg(any(test, perf_enabled))]
fn update_markdown_syntax_stats(update: impl FnOnce(&mut MarkdownSyntaxStats)) {
    MARKDOWN_SYNTAX_STATS.with(|stats| {
        let mut value = stats.get();
        update(&mut value);
        stats.set(value);
    });
}

#[cfg(any(test, perf_enabled))]
fn record_timed<T>(
    run: impl FnOnce() -> T,
    update: impl FnOnce(&mut MarkdownSyntaxStats, u128),
) -> T {
    let start = Instant::now();
    let value = run();
    let elapsed = start.elapsed().as_nanos();
    update_markdown_syntax_stats(|stats| update(stats, elapsed));
    value
}

#[cfg(any(test, perf_enabled))]
fn record_timed_parse<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.parse_calls += 1;
        stats.parse_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
fn record_timed_parse<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
pub(crate) fn record_timed_block_parse<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.block_parse_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
pub(crate) fn record_timed_block_parse<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
pub(crate) fn record_timed_inline_parent_scan<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.inline_parent_scan_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
pub(crate) fn record_timed_inline_parent_scan<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
pub(crate) fn record_timed_inline_reuse_index<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.inline_reuse_index_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
pub(crate) fn record_timed_inline_reuse_index<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
pub(crate) fn record_timed_inline_range_build<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.inline_range_build_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
pub(crate) fn record_timed_inline_range_build<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
pub(crate) fn record_timed_inline_parse<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.inline_parse_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
pub(crate) fn record_timed_inline_parse<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
fn record_timed_line_start_collect<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.line_start_collect_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
fn record_timed_line_start_collect<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
fn record_timed_block_collect<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.block_collect_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
fn record_timed_block_collect<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
fn record_timed_table_collect<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.table_collect_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
fn record_timed_table_collect<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
fn record_timed_inline_collect<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.inline_collect_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
fn record_timed_inline_collect<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(any(test, perf_enabled))]
fn record_timed_projection_collect<T>(run: impl FnOnce() -> T) -> T {
    record_timed(run, |stats, elapsed| {
        stats.projection_collect_ns += elapsed;
    })
}

#[cfg(not(any(test, perf_enabled)))]
fn record_timed_projection_collect<T>(run: impl FnOnce() -> T) -> T {
    run()
}

#[cfg(test)]
mod tests {
    use super::backend::PulldownMarkdownBackend;
    use super::source::{line_starts, trim_line_end};
    use super::*;

    fn block_semantics_without_id(
        block: &MarkdownBlock,
    ) -> (
        MarkdownBlockKind,
        Range<usize>,
        Range<usize>,
        Vec<Range<usize>>,
        Range<usize>,
        bool,
    ) {
        (
            block.kind,
            block.source_range.clone(),
            block.content_range.clone(),
            block.marker_ranges.clone(),
            block.row_range.clone(),
            block.tagfilter_disallowed,
        )
    }

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
    fn pulldown_backend_enters_structure_assembler_boundary() {
        let source = "# Title\n\nParagraph\n\n- item\n\n---\n";
        let data = PulldownMarkdownBackend::parse_syntax_data(source);

        assert_eq!(data.source_len(), source.len());
        assert_eq!(data.line_starts(), line_starts(source));
        assert!(
            data.blocks()
                .iter()
                .any(|block| block.kind == MarkdownBlockKind::AtxHeading { level: 1 })
        );
        assert!(
            data.blocks()
                .iter()
                .any(|block| block.kind == MarkdownBlockKind::Paragraph)
        );
        assert!(
            data.blocks()
                .iter()
                .any(|block| block.kind == MarkdownBlockKind::UnorderedList)
        );
        assert!(
            data.blocks()
                .iter()
                .any(|block| block.kind == MarkdownBlockKind::ThematicBreak)
        );
    }

    #[test]
    fn pulldown_backend_matches_tree_sitter_block_semantics() {
        let source = concat!(
            "# Title\n",
            "\n",
            "Setext\n",
            "------\n",
            "\n",
            "> quoted\n",
            "> - [x] task\n",
            "\n",
            "- [ ] todo\n",
            "  - nested\n",
            "\n",
            "```rust\n",
            "let x = 1;\n",
            "```\n",
            "\n",
            "<script>alert(1)</script>\n",
            "\n",
            "| a | b |\n",
            "| - | - |\n",
            "| 1 | 2 |\n",
            "\n",
            "---\n",
        );
        let tree_sitter = MarkdownSyntaxTree::parse(source);
        let pulldown = PulldownMarkdownBackend::parse_syntax_data(source);

        assert_eq!(
            pulldown
                .blocks()
                .iter()
                .map(block_semantics_without_id)
                .collect::<Vec<_>>(),
            tree_sitter
                .blocks()
                .iter()
                .map(block_semantics_without_id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn pulldown_backend_matches_tree_sitter_inline_and_projection_semantics() {
        let source = concat!(
            "# **Title** &amp;\n",
            "\n",
            "Paragraph \\* and [link](https://example.com) with *em* and `code`\n",
            "continued\n",
        );
        let tree_sitter = MarkdownSyntaxTree::parse(source);
        let pulldown = PulldownMarkdownBackend::parse_syntax_data(source);

        assert_eq!(pulldown.inline_spans(), tree_sitter.inline_spans());
        assert_eq!(
            pulldown.projection_replacements(),
            tree_sitter.syntax_data().projection_replacements()
        );
        assert_eq!(
            pulldown.projection_marker_dependencies(),
            tree_sitter.syntax_data().projection_marker_dependencies()
        );
    }

    #[test]
    fn pulldown_backend_matches_tree_sitter_broader_inline_semantics() {
        let source = concat!(
            "~~strike~~ <span>html</span> <IFRAME src=\"x\"></IFRAME>\n",
            "hard break  \n",
            "soft break with \\* and &copy;\n",
            "autolink <https://example.com> mail <me@example.com>\n",
            "CJK 中文 **粗体** and $数学 + x$ with ![图](image.png)\n",
        );
        let tree_sitter = MarkdownSyntaxTree::parse(source);
        let pulldown = PulldownMarkdownBackend::parse_syntax_data(source);
        let tree_sitter_rendered_candidates = tree_sitter
            .inline_spans()
            .iter()
            .filter(|span| span.kind.is_rendered_element_candidate())
            .cloned()
            .collect::<Vec<_>>();
        let pulldown_rendered_candidates = pulldown
            .inline_spans()
            .iter()
            .filter(|span| span.kind.is_rendered_element_candidate())
            .cloned()
            .collect::<Vec<_>>();

        assert!(tree_sitter.inline_spans().iter().any(|span| {
            span.kind == MarkdownInlineKind::InlineHtml && span.tagfilter_disallowed
        }));
        assert_eq!(pulldown.inline_spans(), tree_sitter.inline_spans());
        assert_eq!(
            pulldown.projection_replacements(),
            tree_sitter.syntax_data().projection_replacements()
        );
        assert_eq!(
            pulldown.projection_marker_dependencies(),
            tree_sitter.syntax_data().projection_marker_dependencies()
        );
        assert_eq!(
            pulldown_rendered_candidates,
            tree_sitter_rendered_candidates
        );
    }

    #[test]
    fn pulldown_backend_matches_tree_sitter_table_cell_inline_semantics() {
        let source = concat!(
            "| **Head** | ![alt](img.png) |\n",
            "| --- | --- |\n",
            "| [link](https://example.com) &amp; | $x + y$ and \\* |\n",
        );
        let tree_sitter = MarkdownSyntaxTree::parse(source);
        let pulldown = PulldownMarkdownBackend::parse_syntax_data(source);
        let table_source_range = tree_sitter.tables()[0].source_range.clone();
        let tree_sitter_table_semantics =
            tree_sitter.range_semantics_for_source_range(table_source_range, None, &[]);
        let pulldown_rendered_candidates = pulldown
            .inline_spans()
            .iter()
            .filter(|span| span.kind.is_rendered_element_candidate())
            .cloned()
            .collect::<Vec<_>>();

        assert_eq!(pulldown.inline_spans(), tree_sitter.inline_spans());
        assert_eq!(
            pulldown.projection_replacements(),
            tree_sitter.syntax_data().projection_replacements()
        );
        assert_eq!(
            pulldown.projection_marker_dependencies(),
            tree_sitter.syntax_data().projection_marker_dependencies()
        );
        assert_eq!(
            pulldown_rendered_candidates,
            tree_sitter_table_semantics.rendered_element_candidates
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
        let tree = tree.reparse_after_edit_range(7..7, 7..8, new_source);

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
        let tree = tree.reparse_after_edit_range(8..12, 8..15, new_source);
        let full_tree = MarkdownSyntaxTree::parse(new_source);

        assert_eq!(tree.source_len(), new_source.len());
        assert_eq!(
            tree.blocks()[1].kind,
            MarkdownBlockKind::AtxHeading { level: 2 }
        );
        assert_eq!(
            tree.blocks()
                .iter()
                .map(block_semantics_without_id)
                .collect::<Vec<_>>(),
            full_tree
                .blocks()
                .iter()
                .map(block_semantics_without_id)
                .collect::<Vec<_>>()
        );
        assert_eq!(tree.inline_spans(), full_tree.inline_spans());
    }

    #[test]
    fn incremental_reparse_matches_full_inline_and_projection_after_local_edits() {
        let source = concat!(
            "alpha **bold** &amp; [link](https://example.com)\n",
            "\n",
            "beta *em* &copy; text\n",
            "\n",
            "- [ ] task item\n",
            "\n",
            "> quoted marker line\n",
            "\n",
            "last line  \n",
            "continued\n",
        );
        let cases = [
            (
                source.find("alpha").unwrap() + "alpha".len()
                    ..source.find("alpha").unwrap() + "alpha".len(),
                " changed",
            ),
            (
                source.find("em").unwrap()..source.find("em").unwrap() + "em".len(),
                "emphasis",
            ),
            (
                source.find("task").unwrap()..source.find("task").unwrap() + "task".len(),
                "todo",
            ),
            (
                source.find("marker").unwrap()..source.find("marker").unwrap() + "marker".len(),
                "dependency",
            ),
            (
                source.find("continued").unwrap()..source.find("continued").unwrap(),
                "still ",
            ),
            (
                source.find("last line").unwrap() + "last line".len()
                    ..source.find("last line").unwrap() + "last line".len(),
                "\ninserted",
            ),
        ];

        for (old_range, replacement) in cases {
            let tree = MarkdownSyntaxTree::parse(source);
            let mut new_source = source.to_string();
            new_source.replace_range(old_range.clone(), replacement);
            let new_range = old_range.start..old_range.start + replacement.len();
            let tree = tree.reparse_after_edit_range(old_range, new_range, &new_source);
            let full_tree = MarkdownSyntaxTree::parse(&new_source);

            assert_eq!(
                tree.syntax_data()
                    .blocks
                    .iter()
                    .map(block_semantics_without_id)
                    .collect::<Vec<_>>(),
                full_tree
                    .syntax_data()
                    .blocks
                    .iter()
                    .map(block_semantics_without_id)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                tree.syntax_data().inline_spans,
                full_tree.syntax_data().inline_spans
            );
            assert_eq!(
                tree.syntax_data().projection_replacements,
                full_tree.syntax_data().projection_replacements
            );
            assert_eq!(
                tree.syntax_data().projection_marker_dependencies,
                full_tree.syntax_data().projection_marker_dependencies
            );
        }
    }

    #[test]
    fn line_starts_splice_matches_full_scan_after_local_edits() {
        let cases = [
            ("one\ntwo\nthree\n", 5..6, "XX"),
            ("one\ntwo\nthree\n", 3..3, "\ninserted"),
            ("one\ntwo\nthree\n", 3..9, ""),
            ("one\n\nthree\n", 4..5, "two\n"),
        ];

        for (old_source, old_range, replacement) in cases {
            let mut new_source = old_source.to_string();
            new_source.replace_range(old_range.clone(), replacement);
            let new_range = old_range.start..old_range.start + replacement.len();

            assert_eq!(
                line_starts_after_edit_range(
                    &line_starts(old_source),
                    old_range,
                    new_range,
                    &new_source,
                ),
                line_starts(&new_source),
            );
        }
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

        let active_quote = source.find("> ").expect("expected quote marker");
        let projection =
            tree.projection_for_visible_rows(0..4, Some(active_quote..active_quote + 1));

        assert_eq!(
            projection.project_source_text(source),
            "> quote\n☐ todo\nnested\nordered\n"
        );

        let active_quote_content = source.find("quote").expect("expected quote text");
        let projection = tree.projection_for_visible_rows(
            0..4,
            Some(active_quote_content + 1..active_quote_content + 2),
        );

        assert_eq!(
            projection.project_source_text(source),
            "quote\n☐ todo\nnested\nordered\n"
        );

        let active_list_content = source.find("todo").expect("expected list text");
        let projection = tree.projection_for_visible_rows(
            0..4,
            Some(active_list_content + 1..active_list_content + 2),
        );

        assert_eq!(
            projection.project_source_text(source),
            "quote\n☐ todo\nnested\nordered\n"
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
    fn projection_replaces_inactive_hard_breaks_with_forced_breaks() {
        let source = "first  \nsecond\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let projection = tree.projection_for_visible_rows(0..2, None);

        assert_eq!(projection.project_source_text(source), "first\nsecond\n");
        assert_eq!(projection.display_len(), "first\nsecond\n".len());
        assert_eq!(projection.source_to_display("first".len()), "first".len());
        assert_eq!(
            projection.source_to_display("first  \n".len()),
            "first\n".len()
        );
        assert_eq!(projection.display_to_source("first".len()), "first".len());
        assert_eq!(
            projection.display_to_source("first\n".len()),
            "first  \n".len()
        );
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
