use std::ops::Range;

#[cfg(any(test, perf_enabled))]
use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag};

#[cfg(any(test, perf_enabled))]
use super::{
    MarkdownBlockKind, MarkdownNodeId,
    source::{line_starts, trim_line_end},
    structure::MarkdownStructureBlock,
};
use super::{
    MarkdownParseTree, MarkdownSyntaxData, MarkdownSyntaxTree,
    assembler::MarkdownSemanticsAssembler, parser::parse_markdown, record_timed_parse,
    structure::MarkdownStructure,
};

pub(super) struct MarkdownBackendOutput {
    pub(super) structure: MarkdownStructure,
    pub(super) parser_state: MarkdownParseTree,
    pub(super) data: MarkdownSyntaxData,
}

trait MarkdownBackend {
    fn parse(
        source: &str,
        old_tree: Option<&MarkdownParseTree>,
        changed_range: Option<&Range<usize>>,
    ) -> MarkdownBackendOutput;

    fn parse_after_edit(
        source: &str,
        previous: &MarkdownSyntaxTree,
        edited_tree: &MarkdownParseTree,
        old_range: Range<usize>,
        new_range: Range<usize>,
    ) -> MarkdownBackendOutput;
}

pub(super) struct TreeSitterMarkdownBackend;

#[cfg(any(test, perf_enabled))]
pub(super) struct PulldownMarkdownBackend;

impl TreeSitterMarkdownBackend {
    pub(super) fn parse(
        source: &str,
        old_tree: Option<&MarkdownParseTree>,
        changed_range: Option<&Range<usize>>,
    ) -> MarkdownBackendOutput {
        <Self as MarkdownBackend>::parse(source, old_tree, changed_range)
    }

    pub(super) fn parse_after_edit(
        source: &str,
        previous: &MarkdownSyntaxTree,
        edited_tree: &MarkdownParseTree,
        old_range: Range<usize>,
        new_range: Range<usize>,
    ) -> MarkdownBackendOutput {
        <Self as MarkdownBackend>::parse_after_edit(
            source,
            previous,
            edited_tree,
            old_range,
            new_range,
        )
    }
}

impl MarkdownBackend for TreeSitterMarkdownBackend {
    fn parse(
        source: &str,
        old_tree: Option<&MarkdownParseTree>,
        changed_range: Option<&Range<usize>>,
    ) -> MarkdownBackendOutput {
        let parser_state = record_timed_parse(|| parse_markdown(source, old_tree, changed_range));
        let structure = MarkdownStructure::from_parse_tree(source, &parser_state);
        let data = record_timed_collect_syntax_data(|| {
            MarkdownSemanticsAssembler::assemble(source, &structure)
        });

        MarkdownBackendOutput {
            structure,
            parser_state,
            data,
        }
    }

    fn parse_after_edit(
        source: &str,
        previous: &MarkdownSyntaxTree,
        edited_tree: &MarkdownParseTree,
        old_range: Range<usize>,
        new_range: Range<usize>,
    ) -> MarkdownBackendOutput {
        let parser_state =
            record_timed_parse(|| parse_markdown(source, Some(edited_tree), Some(&new_range)));
        let structure = MarkdownStructure::from_parse_tree(source, &parser_state);
        let data = record_timed_collect_syntax_data(|| {
            MarkdownSemanticsAssembler::assemble_incremental(
                source, previous, &structure, &old_range, &new_range,
            )
        });

        MarkdownBackendOutput {
            structure,
            parser_state,
            data,
        }
    }
}

#[cfg(any(test, perf_enabled))]
impl PulldownMarkdownBackend {
    pub(super) fn parse_syntax_data(source: &str) -> MarkdownSyntaxData {
        let structure =
            MarkdownStructure::from_parts(collect_pulldown_structure_blocks(source), Vec::new());
        record_timed_collect_syntax_data(|| {
            MarkdownSemanticsAssembler::assemble(source, &structure)
        })
    }
}

#[cfg(any(test, perf_enabled))]
fn collect_pulldown_structure_blocks(source: &str) -> Vec<MarkdownStructureBlock> {
    let line_starts = line_starts(source);
    let mut blocks = Vec::new();
    for (event, range) in Parser::new_ext(source, Options::all()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Some(block) = pulldown_structure_block_from_start_tag(
                    source,
                    &line_starts,
                    tag,
                    range,
                    blocks.len(),
                ) {
                    blocks.push(block);
                }
            }
            Event::Rule => blocks.push(MarkdownStructureBlock {
                id: pulldown_node_id(blocks.len()),
                kind: MarkdownBlockKind::ThematicBreak,
                content_range: range.start..range.start,
                row_range: row_range_for_byte_range(&line_starts, range.clone()),
                source_range: range,
                marker_ranges: Vec::new(),
                tagfilter_disallowed: false,
            }),
            _ => {}
        }
    }
    blocks.sort_by_key(|block| (block.source_range.start, block.source_range.end));
    blocks
}

#[cfg(any(test, perf_enabled))]
fn pulldown_structure_block_from_start_tag(
    source: &str,
    line_starts: &[usize],
    tag: Tag<'_>,
    range: Range<usize>,
    index: usize,
) -> Option<MarkdownStructureBlock> {
    let kind = match tag {
        Tag::Paragraph => MarkdownBlockKind::Paragraph,
        Tag::Heading { level, .. } => MarkdownBlockKind::AtxHeading {
            level: heading_level(level),
        },
        Tag::BlockQuote(_) => MarkdownBlockKind::BlockQuote,
        Tag::CodeBlock(CodeBlockKind::Indented) => MarkdownBlockKind::IndentedCodeBlock,
        Tag::CodeBlock(CodeBlockKind::Fenced(_)) => MarkdownBlockKind::FencedCodeBlock,
        Tag::HtmlBlock => MarkdownBlockKind::HtmlBlock,
        Tag::List(Some(_)) => MarkdownBlockKind::OrderedList,
        Tag::List(None) => MarkdownBlockKind::UnorderedList,
        Tag::Item => MarkdownBlockKind::ListItem,
        Tag::Table(_) => MarkdownBlockKind::PipeTable,
        Tag::FootnoteDefinition(_)
        | Tag::DefinitionList
        | Tag::DefinitionListTitle
        | Tag::DefinitionListDefinition
        | Tag::TableHead
        | Tag::TableRow
        | Tag::TableCell
        | Tag::Emphasis
        | Tag::Strong
        | Tag::Strikethrough
        | Tag::Superscript
        | Tag::Subscript
        | Tag::Link { .. }
        | Tag::Image { .. }
        | Tag::MetadataBlock(_) => return None,
    };

    Some(MarkdownStructureBlock {
        id: pulldown_node_id(index),
        kind,
        content_range: trim_line_end(source, range.clone()),
        marker_ranges: Vec::new(),
        row_range: row_range_for_byte_range(line_starts, range.clone()),
        source_range: range,
        tagfilter_disallowed: false,
    })
}

#[cfg(any(test, perf_enabled))]
fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(any(test, perf_enabled))]
fn row_range_for_byte_range(line_starts: &[usize], range: Range<usize>) -> Range<usize> {
    let start = line_starts.partition_point(|line_start| *line_start <= range.start) - 1;
    let end_offset = range
        .end
        .saturating_sub(usize::from(range.end > range.start));
    let end = line_starts.partition_point(|line_start| *line_start <= end_offset);
    start..end.max(start + 1)
}

#[cfg(any(test, perf_enabled))]
fn pulldown_node_id(index: usize) -> MarkdownNodeId {
    MarkdownNodeId(1 << 62 | index as u64)
}

fn record_timed_collect_syntax_data<T>(run: impl FnOnce() -> T) -> T {
    run()
}
