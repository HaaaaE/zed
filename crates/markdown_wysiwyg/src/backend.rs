use std::ops::Range;

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

fn record_timed_collect_syntax_data<T>(run: impl FnOnce() -> T) -> T {
    run()
}
