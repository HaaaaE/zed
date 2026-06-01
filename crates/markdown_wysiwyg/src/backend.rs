use std::ops::Range;

#[cfg(any(test, perf_enabled))]
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

#[cfg(any(test, perf_enabled))]
use super::{
    MarkdownNodeId,
    blocks::{
        structure_block_quote_block_from_range, structure_fenced_code_block_from_range,
        structure_heading_block_from_range, structure_html_block_from_range,
        structure_indented_code_block_from_range, structure_list_block_from_range,
        structure_list_item_block_from_range, structure_paragraph_block_from_range,
        structure_pipe_table_block_from_range, structure_thematic_break_block_from_range,
    },
    source::line_starts,
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
            Event::Rule => {
                blocks.push(structure_thematic_break_block_from_range(
                    &line_starts,
                    pulldown_node_id(blocks.len()),
                    range,
                ));
            }
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
    let id = pulldown_node_id(index);
    match tag {
        Tag::Paragraph => Some(structure_paragraph_block_from_range(
            source,
            line_starts,
            id,
            range,
        )),
        Tag::Heading { .. } => structure_heading_block_from_range(source, line_starts, id, range),
        Tag::BlockQuote(_) => Some(structure_block_quote_block_from_range(
            source,
            line_starts,
            id,
            range,
        )),
        Tag::CodeBlock(CodeBlockKind::Indented) => Some(structure_indented_code_block_from_range(
            source,
            line_starts,
            id,
            range,
        )),
        Tag::CodeBlock(CodeBlockKind::Fenced(_)) => Some(structure_fenced_code_block_from_range(
            source,
            line_starts,
            id,
            pulldown_fenced_code_block_range(source, range),
        )),
        Tag::HtmlBlock => Some(structure_html_block_from_range(source, line_starts, id, range)),
        Tag::List(_) => Some(structure_list_block_from_range(
            source,
            line_starts,
            id,
            range,
        )),
        Tag::Item => Some(structure_list_item_block_from_range(
            source,
            line_starts,
            id,
            range,
        )),
        Tag::Table(_) => Some(structure_pipe_table_block_from_range(
            source,
            line_starts,
            id,
            range,
        )),
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
        | Tag::MetadataBlock(_) => None,
    }
}

#[cfg(any(test, perf_enabled))]
fn pulldown_fenced_code_block_range(source: &str, mut range: Range<usize>) -> Range<usize> {
    if source.as_bytes().get(range.end).copied() == Some(b'\n') {
        range.end += 1;
    }
    range
}

#[cfg(any(test, perf_enabled))]
fn pulldown_node_id(index: usize) -> MarkdownNodeId {
    MarkdownNodeId(1 << 62 | index as u64)
}

fn record_timed_collect_syntax_data<T>(run: impl FnOnce() -> T) -> T {
    run()
}
