use std::ops::Range;

use super::{MarkdownBlockKind, MarkdownInlineTree, MarkdownNodeId, MarkdownParseTree, blocks};

#[derive(Clone, Debug)]
pub(super) struct MarkdownStructure {
    parser_state: MarkdownParseTree,
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
        let mut structure = Self {
            parser_state: parser_state.clone(),
            blocks: Vec::new(),
        };
        structure.collect_blocks_from_parse_tree(source);
        structure
    }

    pub(super) fn inline_trees(&self) -> &[MarkdownInlineTree] {
        self.parser_state.inline_trees()
    }

    pub(super) fn blocks(&self) -> &[MarkdownStructureBlock] {
        &self.blocks
    }

    fn collect_blocks_from_parse_tree(&mut self, source: &str) {
        self.blocks =
            blocks::collect_structure_blocks(source, self.parser_state.block_tree().root_node());
    }
}
