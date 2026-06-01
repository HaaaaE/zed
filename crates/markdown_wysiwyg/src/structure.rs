use std::ops::Range;

use super::{MarkdownBlockKind, MarkdownInlineTree, MarkdownNodeId, MarkdownParseTree, blocks};

#[derive(Clone, Debug)]
pub(super) struct MarkdownStructure {
    inline_trees: Vec<MarkdownInlineTree>,
    blocks: Vec<MarkdownStructureBlock>,
}

#[derive(Clone, Debug)]
pub(super) struct MarkdownStructureBlock {
    pub(super) id: MarkdownNodeId,
    pub(super) kind: MarkdownBlockKind,
    pub(super) source_range: Range<usize>,
    pub(super) content_range: Range<usize>,
    pub(super) marker_ranges: Vec<Range<usize>>,
    pub(super) row_range: Range<usize>,
    pub(super) tagfilter_disallowed: bool,
}

impl MarkdownStructure {
    pub(super) fn from_parse_tree(source: &str, parser_state: &MarkdownParseTree) -> Self {
        Self::from_parts(
            blocks::collect_structure_blocks(source, parser_state.block_tree().root_node()),
            parser_state.inline_trees().to_vec(),
        )
    }

    pub(super) fn from_parts(
        blocks: Vec<MarkdownStructureBlock>,
        inline_trees: Vec<MarkdownInlineTree>,
    ) -> Self {
        Self {
            inline_trees,
            blocks,
        }
    }

    pub(super) fn inline_trees(&self) -> &[MarkdownInlineTree] {
        &self.inline_trees
    }

    pub(super) fn blocks(&self) -> &[MarkdownStructureBlock] {
        &self.blocks
    }
}
