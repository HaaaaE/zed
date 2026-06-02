use std::ops::Range;

use super::{MarkdownBlockKind, MarkdownInlineSpan, MarkdownNodeId, MarkdownProjectionReplacement};

#[derive(Clone, Debug)]
pub(super) struct MarkdownStructure {
    inline_semantics: Vec<MarkdownInlineSemantics>,
    blocks: Vec<MarkdownStructureBlock>,
}

#[derive(Clone, Debug)]
pub(super) struct MarkdownInlineSemantics {
    pub(super) parent_id: usize,
    pub(super) parent_range: Range<usize>,
    pub(super) spans: Vec<MarkdownInlineSpan>,
    pub(super) replacements: Vec<MarkdownProjectionReplacement>,
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
    pub(super) fn from_parts(
        blocks: Vec<MarkdownStructureBlock>,
        inline_semantics: Vec<MarkdownInlineSemantics>,
    ) -> Self {
        Self {
            inline_semantics,
            blocks,
        }
    }

    pub(super) fn inline_semantics(&self) -> &[MarkdownInlineSemantics] {
        &self.inline_semantics
    }

    pub(super) fn blocks(&self) -> &[MarkdownStructureBlock] {
        &self.blocks
    }
}
