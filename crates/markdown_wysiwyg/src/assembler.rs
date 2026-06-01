use std::ops::Range;

use super::{
    MarkdownSyntaxData, MarkdownSyntaxTree, blocks,
    inline::{
        collect_incremental_inline_spans, collect_incremental_projection_replacements,
        collect_structure_inline_spans, collect_structure_projection_replacements,
        inline_span_prefix_maximum_ends, projection_marker_dependencies,
        projection_marker_prefix_maximum_ends, projection_replacement_prefix_maximum_ends,
    },
    record_timed_block_collect, record_timed_inline_collect, record_timed_line_start_collect,
    record_timed_projection_collect, record_timed_table_collect,
    source::line_starts,
    structure::MarkdownStructure,
    tables::collect_structure_tables,
};

pub(super) struct MarkdownSemanticsAssembler;

impl MarkdownSemanticsAssembler {
    pub(super) fn assemble(source: &str, structure: &MarkdownStructure) -> MarkdownSyntaxData {
        blocks::validate_structure_blocks(structure.blocks());
        let line_starts = record_timed_line_start_collect(|| line_starts(source));
        let blocks = record_timed_block_collect(|| {
            blocks::collect_markdown_blocks(source, &line_starts, structure)
        });
        let tables = record_timed_table_collect(|| {
            collect_structure_tables(source, &line_starts, structure)
        });
        let inline_spans =
            record_timed_inline_collect(|| collect_structure_inline_spans(source, structure));
        let inline_span_prefix_maximum_ends = inline_span_prefix_maximum_ends(&inline_spans);
        let (
            projection_replacements,
            projection_replacement_prefix_maximum_ends,
            projection_marker_dependencies,
            projection_marker_prefix_maximum_ends,
        ) = record_timed_projection_collect(|| {
            let projection_replacements =
                collect_structure_projection_replacements(source, structure, &blocks);
            let projection_replacement_prefix_maximum_ends =
                projection_replacement_prefix_maximum_ends(&projection_replacements);
            let projection_marker_dependencies =
                projection_marker_dependencies(&blocks, &inline_spans, &projection_replacements);
            let projection_marker_prefix_maximum_ends =
                projection_marker_prefix_maximum_ends(&projection_marker_dependencies);
            (
                projection_replacements,
                projection_replacement_prefix_maximum_ends,
                projection_marker_dependencies,
                projection_marker_prefix_maximum_ends,
            )
        });

        MarkdownSyntaxData {
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

    pub(super) fn assemble_incremental(
        source: &str,
        previous: &MarkdownSyntaxTree,
        structure: &MarkdownStructure,
        old_range: &Range<usize>,
        new_range: &Range<usize>,
    ) -> MarkdownSyntaxData {
        blocks::validate_structure_blocks(structure.blocks());
        let line_starts = record_timed_line_start_collect(|| line_starts(source));
        let blocks = record_timed_block_collect(|| {
            blocks::collect_incremental_blocks(source, structure, &line_starts)
        });
        let tables = record_timed_table_collect(|| {
            collect_structure_tables(source, &line_starts, structure)
        });
        let inline_spans = record_timed_inline_collect(|| {
            collect_incremental_inline_spans(source, previous, structure, old_range, new_range)
        });
        let inline_span_prefix_maximum_ends = inline_span_prefix_maximum_ends(&inline_spans);
        let (
            projection_replacements,
            projection_replacement_prefix_maximum_ends,
            projection_marker_dependencies,
            projection_marker_prefix_maximum_ends,
        ) = record_timed_projection_collect(|| {
            let projection_replacements = collect_incremental_projection_replacements(
                source, previous, structure, &blocks, old_range, new_range,
            );
            let projection_replacement_prefix_maximum_ends =
                projection_replacement_prefix_maximum_ends(&projection_replacements);
            let projection_marker_dependencies =
                projection_marker_dependencies(&blocks, &inline_spans, &projection_replacements);
            let projection_marker_prefix_maximum_ends =
                projection_marker_prefix_maximum_ends(&projection_marker_dependencies);
            (
                projection_replacements,
                projection_replacement_prefix_maximum_ends,
                projection_marker_dependencies,
                projection_marker_prefix_maximum_ends,
            )
        });

        MarkdownSyntaxData {
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
}
