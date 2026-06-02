use std::ops::Range;

#[cfg(any(test, perf_enabled))]
use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag};

#[cfg(any(test, perf_enabled))]
use super::{
    MarkdownBlockKind, MarkdownInlineParent, MarkdownNodeId,
    blocks::{
        row_range_for_byte_range, structure_block_quote_block_from_range,
        structure_fenced_code_block_from_range, structure_heading_block_from_range,
        structure_html_block_from_range, structure_indented_code_block_from_range,
        structure_link_reference_definition_block_from_range, structure_list_block_from_range,
        structure_list_item_block_from_range, structure_paragraph_block_from_range,
        structure_pipe_table_block_from_range, structure_thematic_break_block_from_range,
    },
    source::{line_range, line_starts, ranges_overlap, trim_line_end},
    structure::MarkdownStructureBlock,
    tables::table_cell_content_ranges_for_blocks,
};
use super::{
    MarkdownParseTree, MarkdownSyntaxData, MarkdownSyntaxTree,
    assembler::MarkdownSemanticsAssembler, blocks, parser::parse_markdown,
    record_timed_backend_prepare, record_timed_parse, record_timed_structure_build,
    record_timed_syntax_data_collect, structure::MarkdownStructure,
};

#[cfg(any(test, perf_enabled))]
use super::{
    parser::{
        InlineBackendKind, MarkdownInlineCache, parse_inline_for_parents,
        parse_markdown_with_inline_backend,
    },
    record_timed_block_parse, record_timed_inline_parent_scan,
};

#[cfg(test)]
use super::parser::parse_markdown_block_tree;

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

    #[cfg(any(test, perf_enabled))]
    pub(super) fn parse_syntax_data_with_inline_backend(
        source: &str,
        inline_backend: InlineBackendKind,
    ) -> MarkdownSyntaxData {
        let (parser_state, structure) = record_timed_backend_prepare(|| {
            let (parser_state, inline_semantics) = record_timed_parse(|| {
                parse_markdown_with_inline_backend(source, None, None, inline_backend)
            });
            let structure = record_timed_structure_build(|| {
                MarkdownStructure::from_parts(
                    blocks::collect_structure_blocks(source, parser_state.block_tree().root_node()),
                    inline_semantics,
                )
            });
            (parser_state, structure)
        });
        let _ = parser_state;
        record_timed_collect_syntax_data(|| {
            MarkdownSemanticsAssembler::assemble(source, &structure)
        })
    }
}

impl MarkdownBackend for TreeSitterMarkdownBackend {
    fn parse(
        source: &str,
        old_tree: Option<&MarkdownParseTree>,
        changed_range: Option<&Range<usize>>,
    ) -> MarkdownBackendOutput {
        let (parser_state, structure) = record_timed_backend_prepare(|| {
            let (parser_state, inline_semantics) =
                record_timed_parse(|| parse_markdown(source, old_tree, changed_range));
            let structure = record_timed_structure_build(|| {
                MarkdownStructure::from_parts(
                    blocks::collect_structure_blocks(source, parser_state.block_tree().root_node()),
                    inline_semantics,
                )
            });
            (parser_state, structure)
        });
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
        let (parser_state, structure) = record_timed_backend_prepare(|| {
            let (parser_state, inline_semantics) =
                record_timed_parse(|| parse_markdown(source, Some(edited_tree), Some(&new_range)));
            let structure = record_timed_structure_build(|| {
                MarkdownStructure::from_parts(
                    blocks::collect_structure_blocks(source, parser_state.block_tree().root_node()),
                    inline_semantics,
                )
            });
            (parser_state, structure)
        });
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
        Self::parse_syntax_data_with_inline_backend(source, InlineBackendKind::TreeSitter)
    }

    pub(super) fn parse_syntax_data_with_inline_backend(
        source: &str,
        inline_backend: InlineBackendKind,
    ) -> MarkdownSyntaxData {
        let structure = record_timed_backend_prepare(|| pulldown_structure(source, inline_backend));
        record_timed_collect_syntax_data(|| {
            MarkdownSemanticsAssembler::assemble(source, &structure)
        })
    }

    #[cfg(test)]
    pub(super) fn parse_syntax_tree(source: &str) -> MarkdownSyntaxTree {
        Self::parse_syntax_tree_with_inline_backend(source, InlineBackendKind::TreeSitter)
    }

    #[cfg(test)]
    pub(super) fn parse_syntax_tree_with_inline_backend(
        source: &str,
        inline_backend: InlineBackendKind,
    ) -> MarkdownSyntaxTree {
        let (structure, parser_state) = record_timed_backend_prepare(|| {
            pulldown_structure_and_parser_state(source, inline_backend)
        });
        let data = record_timed_collect_syntax_data(|| {
            MarkdownSemanticsAssembler::assemble(source, &structure)
        });

        MarkdownSyntaxTree { parser_state, data }
    }

    #[cfg(test)]
    pub(super) fn parse_syntax_tree_after_edit(
        source: &str,
        previous: &MarkdownSyntaxTree,
        old_range: Range<usize>,
        new_range: Range<usize>,
    ) -> MarkdownSyntaxTree {
        Self::parse_syntax_tree_after_edit_with_inline_backend(
            source,
            previous,
            old_range,
            new_range,
            InlineBackendKind::TreeSitter,
        )
    }

    #[cfg(test)]
    pub(super) fn parse_syntax_tree_after_edit_with_inline_backend(
        source: &str,
        previous: &MarkdownSyntaxTree,
        old_range: Range<usize>,
        new_range: Range<usize>,
        inline_backend: InlineBackendKind,
    ) -> MarkdownSyntaxTree {
        let (structure, parser_state) = record_timed_backend_prepare(|| {
            pulldown_structure_and_parser_state(source, inline_backend)
        });
        let data = record_timed_collect_syntax_data(|| {
            MarkdownSemanticsAssembler::assemble_incremental(
                source, previous, &structure, &old_range, &new_range,
            )
        });

        MarkdownSyntaxTree { parser_state, data }
    }
}

#[cfg(any(test, perf_enabled))]
fn pulldown_structure(source: &str, inline_backend: InlineBackendKind) -> MarkdownStructure {
    let source_line_starts = line_starts(source);
    let blocks =
        record_timed_block_parse(|| collect_pulldown_structure_blocks(source, &source_line_starts));
    let inline_semantics =
        pulldown_inline_semantics(source, &source_line_starts, &blocks, inline_backend).0;
    record_timed_structure_build(|| MarkdownStructure::from_parts(blocks, inline_semantics))
}

#[cfg(test)]
fn pulldown_structure_and_parser_state(
    source: &str,
    inline_backend: InlineBackendKind,
) -> (MarkdownStructure, MarkdownParseTree) {
    let source_line_starts = line_starts(source);
    let blocks =
        record_timed_block_parse(|| collect_pulldown_structure_blocks(source, &source_line_starts));
    let (inline_semantics, inline_trees) =
        pulldown_inline_semantics(source, &source_line_starts, &blocks, inline_backend);
    let inline_tree_by_parent_id = inline_trees
        .iter()
        .enumerate()
        .map(|(index, inline_tree)| (inline_tree.parent_id, index))
        .collect();
    let parser_state = MarkdownParseTree {
        block_tree: parse_markdown_block_tree(source, None),
        inline_trees: inline_trees.clone(),
        inline_tree_by_parent_id,
    };

    (
        record_timed_structure_build(|| MarkdownStructure::from_parts(blocks, inline_semantics)),
        parser_state,
    )
}

#[cfg(any(test, perf_enabled))]
fn pulldown_inline_semantics(
    source: &str,
    source_line_starts: &[usize],
    blocks: &[MarkdownStructureBlock],
    inline_backend: InlineBackendKind,
) -> (
    Vec<super::structure::MarkdownInlineSemantics>,
    Vec<super::MarkdownInlineTree>,
) {
    let inline_parents = pulldown_inline_parents(source, source_line_starts, blocks);
    let output = parse_inline_for_parents(source, &inline_parents, inline_backend);
    let inline_trees = match output.cache {
        MarkdownInlineCache::TreeSitter { inline_trees, .. } => inline_trees,
        MarkdownInlineCache::None => Vec::new(),
    };
    (output.semantics, inline_trees)
}

#[cfg(any(test, perf_enabled))]
fn pulldown_inline_parents(
    source: &str,
    source_line_starts: &[usize],
    blocks: &[MarkdownStructureBlock],
) -> Vec<MarkdownInlineParent> {
    record_timed_inline_parent_scan(|| {
        pulldown_inline_parent_ranges(source, source_line_starts, blocks)
            .into_iter()
            .enumerate()
            .map(|(index, parent_range)| MarkdownInlineParent {
                parent_id: 1 << 61 | index,
                parent_range,
            })
            .collect()
    })
}

#[cfg(any(test, perf_enabled))]
fn collect_pulldown_structure_blocks(
    source: &str,
    line_starts: &[usize],
) -> Vec<MarkdownStructureBlock> {
    let mut blocks = Vec::new();
    for (event, range) in Parser::new_ext(source, Options::all()).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                if let Some(block) = pulldown_structure_block_from_start_tag(
                    source,
                    line_starts,
                    tag,
                    range,
                    blocks.len(),
                ) {
                    blocks.push(block);
                }
            }
            Event::Rule => {
                blocks.push(structure_thematic_break_block_from_range(
                    line_starts,
                    pulldown_node_id(blocks.len()),
                    range,
                ));
            }
            _ => {}
        }
    }
    add_pulldown_link_reference_definition_blocks(source, line_starts, &mut blocks);
    blocks.sort_by_key(|block| (block.source_range.start, block.source_range.end));
    synthesize_pulldown_paragraph_blocks(source, line_starts, &mut blocks);
    blocks.sort_by_key(|block| (block.source_range.start, block.source_range.end));
    blocks
}

#[cfg(any(test, perf_enabled))]
fn add_pulldown_link_reference_definition_blocks(
    source: &str,
    line_starts: &[usize],
    blocks: &mut Vec<MarkdownStructureBlock>,
) {
    let definition_ranges = pulldown_link_reference_definition_ranges(source, line_starts, blocks);
    blocks.retain(|block| {
        block.kind != MarkdownBlockKind::Paragraph
            || !definition_ranges
                .iter()
                .any(|range| ranges_overlap(&block.source_range, range))
    });

    let first_id = blocks.len();
    blocks.extend(
        definition_ranges
            .into_iter()
            .enumerate()
            .map(|(index, range)| {
                structure_link_reference_definition_block_from_range(
                    source,
                    line_starts,
                    pulldown_node_id(first_id + index),
                    range,
                )
            }),
    );
}

#[cfg(any(test, perf_enabled))]
fn pulldown_link_reference_definition_ranges(
    source: &str,
    line_starts: &[usize],
    blocks: &[MarkdownStructureBlock],
) -> Vec<Range<usize>> {
    (0..line_starts.len())
        .filter_map(|row| {
            let range = line_range(source, line_starts, row);
            pulldown_link_reference_definition_range_for_line(source, range)
        })
        .filter(|range| {
            !blocks.iter().any(|block| {
                block_excludes_link_reference_scan(block)
                    && ranges_overlap(&block.source_range, range)
            })
        })
        .collect()
}

#[cfg(any(test, perf_enabled))]
fn block_excludes_link_reference_scan(block: &MarkdownStructureBlock) -> bool {
    matches!(
        block.kind,
        MarkdownBlockKind::FencedCodeBlock
            | MarkdownBlockKind::IndentedCodeBlock
            | MarkdownBlockKind::HtmlBlock
            | MarkdownBlockKind::PipeTable
    )
}

#[cfg(any(test, perf_enabled))]
fn pulldown_link_reference_definition_range_for_line(
    source: &str,
    source_range: Range<usize>,
) -> Option<Range<usize>> {
    let trimmed_range = trim_line_end(source, source_range.clone());
    let line = source.get(trimmed_range)?;
    let bytes = line.as_bytes();

    let mut cursor = 0;
    while bytes.get(cursor) == Some(&b' ') {
        cursor += 1;
    }
    if cursor > 3 || bytes.get(cursor) != Some(&b'[') {
        return None;
    }

    let label_start = cursor + 1;
    let label_end = line[label_start..]
        .find("]:")
        .map(|offset| label_start + offset)?;
    if label_end == label_start {
        return None;
    }

    Some(source_range)
}

#[cfg(any(test, perf_enabled))]
fn pulldown_inline_parent_ranges(
    source: &str,
    line_starts: &[usize],
    blocks: &[MarkdownStructureBlock],
) -> Vec<Range<usize>> {
    let mut ranges = blocks
        .iter()
        .filter_map(|block| match block.kind {
            MarkdownBlockKind::Paragraph
            | MarkdownBlockKind::AtxHeading { .. }
            | MarkdownBlockKind::SetextHeading { .. } => Some(pulldown_inline_parent_range(
                source,
                block.content_range.clone(),
            )),
            _ => None,
        })
        .filter(|range| !range.is_empty())
        .collect::<Vec<_>>();
    ranges.extend(table_cell_content_ranges_for_blocks(
        source,
        line_starts,
        blocks,
    ));
    ranges.sort_by_key(|range| (range.start, range.end));
    ranges.dedup();
    ranges
}

#[cfg(any(test, perf_enabled))]
fn pulldown_inline_parent_range(source: &str, mut range: Range<usize>) -> Range<usize> {
    loop {
        let trimmed =
            trim_trailing_continuation_whitespace(source, trim_line_end(source, range.clone()));
        let Some(last_line) = last_line_before_range_end(source, range.start..trimmed.end) else {
            return trimmed;
        };
        if last_line.start == range.start
            || !line_is_blockquote_marker_only(source, last_line.clone())
        {
            return trimmed;
        }

        range.end = last_line.start;
    }
}

#[cfg(any(test, perf_enabled))]
fn trim_trailing_continuation_whitespace(source: &str, mut range: Range<usize>) -> Range<usize> {
    loop {
        let Some(line_start) = source[range.start..range.end]
            .rfind('\n')
            .map(|offset| range.start + offset + 1)
        else {
            return range;
        };

        if !source[line_start..range.end]
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t'))
        {
            return range;
        }

        range.end = line_start.saturating_sub(1);
        range = trim_line_end(source, range);
    }
}

#[cfg(any(test, perf_enabled))]
fn last_line_before_range_end(source: &str, range: Range<usize>) -> Option<Range<usize>> {
    if range.is_empty() {
        return None;
    }

    let start = source[range.start..range.end]
        .rfind('\n')
        .map_or(range.start, |offset| range.start + offset + 1);
    Some(start..range.end)
}

#[cfg(any(test, perf_enabled))]
fn line_is_blockquote_marker_only(source: &str, range: Range<usize>) -> bool {
    let bytes = source.as_bytes();
    let mut cursor = range.start;
    let mut saw_marker = false;

    loop {
        let mut leading_spaces = 0;
        while cursor < range.end && bytes[cursor] == b' ' && leading_spaces < 4 {
            cursor += 1;
            leading_spaces += 1;
        }

        if cursor < range.end && bytes[cursor] == b'>' {
            saw_marker = true;
            cursor += 1;
            if cursor < range.end && bytes[cursor] == b' ' {
                cursor += 1;
            }
            continue;
        }

        return saw_marker && source[cursor..range.end].trim().is_empty();
    }
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
            pulldown_indented_code_block_range(source, line_starts, range),
        )),
        Tag::CodeBlock(CodeBlockKind::Fenced(_)) => Some(structure_fenced_code_block_from_range(
            source,
            line_starts,
            id,
            pulldown_fenced_code_block_range(source, range),
        )),
        Tag::HtmlBlock => Some(structure_html_block_from_range(
            source,
            line_starts,
            id,
            pulldown_html_block_range(source, range),
        )),
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
fn pulldown_html_block_range(source: &str, mut range: Range<usize>) -> Range<usize> {
    if html_block_is_blank_line_terminated(source, range.clone())
        && source.as_bytes().get(range.end).copied() == Some(b'\n')
    {
        range.end += 1;
    }
    range
}

#[cfg(any(test, perf_enabled))]
fn html_block_is_blank_line_terminated(source: &str, range: Range<usize>) -> bool {
    let first_line_end = source[range.clone()]
        .find('\n')
        .map_or(range.end, |offset| range.start + offset);
    let first_line = source[trim_line_end(source, range.start..first_line_end)]
        .trim_start_matches([' ', '\t'])
        .to_ascii_lowercase();

    !html_block_has_explicit_end_condition(&first_line)
}

#[cfg(any(test, perf_enabled))]
fn html_block_has_explicit_end_condition(first_line: &str) -> bool {
    html_start_tag_name_is(first_line, "script")
        || html_start_tag_name_is(first_line, "pre")
        || html_start_tag_name_is(first_line, "style")
        || first_line.starts_with("<!--")
        || first_line.starts_with("<?")
        || first_line.starts_with("<![cdata[")
        || first_line
            .as_bytes()
            .get(2)
            .is_some_and(|byte| first_line.starts_with("<!") && byte.is_ascii_uppercase())
}

#[cfg(any(test, perf_enabled))]
fn html_start_tag_name_is(line: &str, tag_name: &str) -> bool {
    let Some(rest) = line.strip_prefix('<') else {
        return false;
    };
    let Some(after_tag) = rest.strip_prefix(tag_name) else {
        return false;
    };

    matches!(
        after_tag.as_bytes().first(),
        None | Some(b' ' | b'\t' | b'>' | b'/')
    )
}

#[cfg(any(test, perf_enabled))]
fn pulldown_indented_code_block_range(
    source: &str,
    line_starts: &[usize],
    range: Range<usize>,
) -> Range<usize> {
    let row = line_starts.partition_point(|line_start| *line_start <= range.start) - 1;
    let mut end = range.end;
    while end < source.len() {
        let next_row = line_starts.partition_point(|line_start| *line_start <= end) - 1;
        let next_line = line_range(source, line_starts, next_row);
        if !source[trim_line_end(source, next_line.clone())]
            .trim()
            .is_empty()
        {
            break;
        }
        end = next_line.end;
    }
    line_starts[row]..end
}

#[cfg(any(test, perf_enabled))]
fn synthesize_pulldown_paragraph_blocks(
    source: &str,
    line_starts: &[usize],
    blocks: &mut Vec<MarkdownStructureBlock>,
) {
    extend_blockquote_paragraphs_to_next_child_marker(source, line_starts, blocks);

    let mut synthesized = Vec::new();
    for item in blocks.iter() {
        if !matches!(
            item.kind,
            MarkdownBlockKind::ListItem | MarkdownBlockKind::TaskListItem { .. }
        ) {
            continue;
        }

        let paragraph_start = item.content_range.start;
        let paragraph_end = pulldown_list_item_paragraph_end(source, blocks, item, paragraph_start);
        if paragraph_start >= paragraph_end
            || source[paragraph_start..paragraph_end].trim().is_empty()
            || blocks.iter().any(|block| {
                block.kind == MarkdownBlockKind::Paragraph
                    && ranges_overlap(&block.source_range, &(paragraph_start..paragraph_end))
            })
        {
            continue;
        }

        synthesized.push(structure_paragraph_block_from_range(
            source,
            line_starts,
            pulldown_node_id(blocks.len() + synthesized.len()),
            paragraph_start..paragraph_end,
        ));
    }

    blocks.extend(synthesized);
    extend_blockquote_paragraphs_to_next_child_marker(source, line_starts, blocks);
    extend_blockquote_child_blocks_to_quoted_gaps(source, line_starts, blocks);
}

#[cfg(any(test, perf_enabled))]
fn extend_blockquote_child_blocks_to_quoted_gaps(
    source: &str,
    line_starts: &[usize],
    blocks: &mut [MarkdownStructureBlock],
) {
    let original_blocks = blocks.to_vec();
    for block in blocks {
        if matches!(
            block.kind,
            MarkdownBlockKind::BlockQuote | MarkdownBlockKind::FencedCodeBlock
        ) || !block_is_inside_blockquote(&original_blocks, &block.source_range)
        {
            continue;
        }

        let Some(extended_end) =
            quoted_gap_extension_end(source, line_starts, &block.source_range, block.kind).or_else(
                || {
                    block_extends_quoted_child_marker(block.kind).then(|| {
                        next_blockquote_child_marker_start_after_range(
                            source,
                            &original_blocks,
                            &block.source_range,
                        )
                    })?
                },
            )
        else {
            continue;
        };
        if extended_end <= block.source_range.end {
            continue;
        }

        block.source_range.end = extended_end;
        block.content_range.end =
            trim_line_end(source, block.content_range.start..extended_end).end;
        block.row_range = row_range_for_byte_range(line_starts, block.source_range.clone());
    }
}

#[cfg(any(test, perf_enabled))]
fn block_extends_quoted_child_marker(kind: MarkdownBlockKind) -> bool {
    matches!(
        kind,
        MarkdownBlockKind::OrderedList
            | MarkdownBlockKind::UnorderedList
            | MarkdownBlockKind::ListItem
            | MarkdownBlockKind::TaskListItem { .. }
    )
}

#[cfg(any(test, perf_enabled))]
fn block_is_inside_blockquote(blocks: &[MarkdownStructureBlock], range: &Range<usize>) -> bool {
    blocks.iter().any(|block| {
        block.kind == MarkdownBlockKind::BlockQuote
            && block.source_range.start <= range.start
            && block.source_range.end >= range.end
    })
}

#[cfg(any(test, perf_enabled))]
fn quoted_gap_extension_end(
    source: &str,
    line_starts: &[usize],
    source_range: &Range<usize>,
    kind: MarkdownBlockKind,
) -> Option<usize> {
    let extends_container = matches!(
        kind,
        MarkdownBlockKind::OrderedList
            | MarkdownBlockKind::UnorderedList
            | MarkdownBlockKind::ListItem
            | MarkdownBlockKind::TaskListItem { .. }
    );
    let mut cursor = source_range.end;
    let mut extended_blank = false;

    loop {
        let line = line_range_for_offset(source, line_starts, cursor)?;
        if line.start != cursor {
            return None;
        }

        let prefix = quoted_line_prefix(source, line.clone())?;
        if prefix.content_is_blank {
            if extends_container {
                cursor = line.end;
                extended_blank = true;
                continue;
            } else {
                return Some(prefix.end);
            }
        }

        return (extends_container
            && (extended_blank
                || block_ends_after_quoted_blank_line(source, line_starts, source_range)))
        .then_some(prefix.end);
    }
}

#[cfg(any(test, perf_enabled))]
fn block_ends_after_quoted_blank_line(
    source: &str,
    line_starts: &[usize],
    source_range: &Range<usize>,
) -> bool {
    if source_range.end == 0 {
        return false;
    }

    let row = line_starts.partition_point(|line_start| *line_start < source_range.end) - 1;
    let line = line_range(source, line_starts, row);
    if line.end != source_range.end {
        return false;
    }

    quoted_line_prefix(source, line).is_some_and(|prefix| prefix.content_is_blank)
}

#[cfg(any(test, perf_enabled))]
fn line_range_for_offset(
    source: &str,
    line_starts: &[usize],
    offset: usize,
) -> Option<Range<usize>> {
    if offset >= source.len() {
        return None;
    }

    let row = line_starts.partition_point(|line_start| *line_start <= offset) - 1;
    Some(line_range(source, line_starts, row))
}

#[cfg(any(test, perf_enabled))]
struct QuotedLinePrefix {
    end: usize,
    content_is_blank: bool,
}

#[cfg(any(test, perf_enabled))]
fn quoted_line_prefix(source: &str, line_range: Range<usize>) -> Option<QuotedLinePrefix> {
    let bytes = source.as_bytes();
    let trimmed_line = trim_line_end(source, line_range);
    let mut cursor = trimmed_line.start;
    let mut leading_spaces = 0;

    while cursor < trimmed_line.end && bytes[cursor] == b' ' && leading_spaces < 4 {
        cursor += 1;
        leading_spaces += 1;
    }

    if cursor >= trimmed_line.end || bytes[cursor] != b'>' {
        return None;
    }

    cursor += 1;
    if cursor < trimmed_line.end && bytes[cursor] == b' ' {
        cursor += 1;
    }

    Some(QuotedLinePrefix {
        end: cursor,
        content_is_blank: source[cursor..trimmed_line.end].trim().is_empty(),
    })
}

#[cfg(any(test, perf_enabled))]
fn extend_blockquote_paragraphs_to_next_child_marker(
    source: &str,
    line_starts: &[usize],
    blocks: &mut [MarkdownStructureBlock],
) {
    let original_blocks = blocks.to_vec();
    for block in blocks {
        if block.kind != MarkdownBlockKind::Paragraph {
            continue;
        }

        let Some(next_child_start) = next_blockquote_child_marker_start_after_range(
            source,
            &original_blocks,
            &block.source_range,
        ) else {
            continue;
        };
        if next_child_start <= block.source_range.end
            || !source[block.source_range.end..next_child_start]
                .bytes()
                .all(|byte| matches!(byte, b'>' | b' ' | b'\t'))
        {
            continue;
        }

        block.source_range.end = next_child_start;
        block.content_range = block.source_range.clone();
        block.row_range = row_range_for_byte_range(line_starts, block.source_range.clone());
    }
}

#[cfg(any(test, perf_enabled))]
fn next_blockquote_child_marker_start_after_range(
    source: &str,
    blocks: &[MarkdownStructureBlock],
    range: &Range<usize>,
) -> Option<usize> {
    let blockquote = blocks.iter().find(|block| {
        block.kind == MarkdownBlockKind::BlockQuote
            && block.source_range.start <= range.start
            && block.source_range.end >= range.end
    })?;

    blocks
        .iter()
        .filter(|block| {
            block.source_range.start >= range.end
                && block.source_range.start <= blockquote.source_range.end
                && block.kind != MarkdownBlockKind::Paragraph
        })
        .min_by_key(|block| block.source_range.start)
        .map(|block| blockquote_child_marker_start(source, block))
}

#[cfg(any(test, perf_enabled))]
fn blockquote_child_marker_start(source: &str, block: &MarkdownStructureBlock) -> usize {
    if matches!(
        block.kind,
        MarkdownBlockKind::OrderedList
            | MarkdownBlockKind::UnorderedList
            | MarkdownBlockKind::ListItem
            | MarkdownBlockKind::TaskListItem { .. }
    ) {
        list_marker_start(source, block.source_range.clone()).unwrap_or(block.source_range.start)
    } else {
        block.source_range.start
    }
}

#[cfg(any(test, perf_enabled))]
fn list_marker_start(source: &str, range: Range<usize>) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut cursor = range.start;

    while cursor < range.end && matches!(bytes[cursor], b' ' | b'\t') {
        cursor += 1;
    }

    if cursor < range.end && matches!(bytes[cursor], b'-' | b'+' | b'*') {
        return Some(cursor);
    }

    let digit_start = cursor;
    while cursor < range.end && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }

    (cursor > digit_start && cursor < range.end && matches!(bytes[cursor], b'.' | b')'))
        .then_some(digit_start)
}

#[cfg(any(test, perf_enabled))]
fn pulldown_list_item_paragraph_end(
    source: &str,
    blocks: &[MarkdownStructureBlock],
    item: &MarkdownStructureBlock,
    paragraph_start: usize,
) -> usize {
    blocks
        .iter()
        .filter(|block| {
            matches!(
                block.kind,
                MarkdownBlockKind::BlockQuote
                    | MarkdownBlockKind::OrderedList
                    | MarkdownBlockKind::UnorderedList
                    | MarkdownBlockKind::FencedCodeBlock
                    | MarkdownBlockKind::IndentedCodeBlock
            ) && block.source_range.start > paragraph_start
                && block.source_range.start < item.source_range.end
        })
        .map(|block| block.source_range.start)
        .min()
        .unwrap_or_else(|| {
            paragraph_end_before_blank_line(source, paragraph_start, item.source_range.end)
        })
}

#[cfg(any(test, perf_enabled))]
fn paragraph_end_before_blank_line(source: &str, mut cursor: usize, limit: usize) -> usize {
    while cursor < limit {
        let line_end = source[cursor..limit]
            .find('\n')
            .map_or(limit, |newline| cursor + newline + 1);
        if source[trim_line_end(source, cursor..line_end)]
            .trim()
            .is_empty()
        {
            return cursor;
        }
        if let Some(prefix) = quoted_line_prefix(source, cursor..line_end) {
            if prefix.content_is_blank {
                return prefix.end;
            }
        }
        cursor = line_end;
    }

    limit
}

#[cfg(any(test, perf_enabled))]
fn pulldown_node_id(index: usize) -> MarkdownNodeId {
    MarkdownNodeId(1 << 62 | index as u64)
}

fn record_timed_collect_syntax_data<T>(run: impl FnOnce() -> T) -> T {
    record_timed_syntax_data_collect(run)
}
