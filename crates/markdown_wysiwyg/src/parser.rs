use std::{cell::RefCell, collections::HashMap, ops::Range};

use tree_sitter::{Node, Parser, Range as TreeSitterRange, Tree};

use super::{
    MarkdownInlineTree, MarkdownParseTree, inline, record_timed_block_parse,
    record_timed_inline_parent_scan, record_timed_inline_parse, record_timed_inline_range_build,
    record_timed_inline_reuse_index, source::ranges_touch, structure::MarkdownInlineSemantics,
};

#[cfg(any(test, perf_enabled))]
use super::source::point_for_offset;

thread_local! {
    static BLOCK_PARSER: RefCell<Parser> = RefCell::new(markdown_block_parser());
    static INLINE_PARSER: RefCell<Parser> = RefCell::new(markdown_inline_parser());
}

fn markdown_block_parser() -> Parser {
    let mut parser = Parser::new();
    let language = tree_sitter_md::LANGUAGE.into();
    parser
        .set_language(&language)
        .expect("failed to load tree-sitter markdown block grammar");
    parser
}

fn markdown_inline_parser() -> Parser {
    let mut parser = Parser::new();
    let language = tree_sitter_md::INLINE_LANGUAGE.into();
    parser
        .set_language(&language)
        .expect("failed to load tree-sitter markdown inline grammar");
    parser
}

pub(super) fn parse_markdown(
    source: &str,
    old_tree: Option<&MarkdownParseTree>,
    changed_range: Option<&Range<usize>>,
) -> (MarkdownParseTree, Vec<MarkdownInlineSemantics>) {
    parse_markdown_with_inline_backend(
        source,
        old_tree,
        changed_range,
        InlineBackendKind::TreeSitter,
    )
}

pub(super) fn parse_markdown_with_inline_backend(
    source: &str,
    old_tree: Option<&MarkdownParseTree>,
    changed_range: Option<&Range<usize>>,
    inline_backend: InlineBackendKind,
) -> (MarkdownParseTree, Vec<MarkdownInlineSemantics>) {
    let block_tree = record_timed_block_parse(|| {
        BLOCK_PARSER.with(|parser| {
            parser
                .borrow_mut()
                .parse(source, old_tree.map(|tree| &tree.block_tree))
                .expect("tree-sitter markdown block parser was cancelled")
        })
    });

    let InlineParseOutput { semantics, cache } =
        parse_inline(source, &block_tree, old_tree, changed_range, inline_backend);
    let (inline_trees, inline_tree_by_parent_id) = match cache {
        MarkdownInlineCache::TreeSitter {
            inline_trees,
            inline_tree_by_parent_id,
        } => (inline_trees, inline_tree_by_parent_id),
        MarkdownInlineCache::None => (Vec::new(), HashMap::new()),
    };

    let parser_state = MarkdownParseTree {
        block_tree,
        inline_trees,
        inline_tree_by_parent_id,
    };
    (parser_state, semantics)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InlineBackendKind {
    TreeSitter,
    #[cfg(any(test, perf_enabled))]
    Comrak,
}

pub(super) struct InlineParseOutput {
    pub(super) semantics: Vec<MarkdownInlineSemantics>,
    pub(super) cache: MarkdownInlineCache,
}

pub(super) enum MarkdownInlineCache {
    TreeSitter {
        inline_trees: Vec<MarkdownInlineTree>,
        inline_tree_by_parent_id: HashMap<usize, usize>,
    },
    #[allow(dead_code)]
    None,
}

fn parse_inline(
    source: &str,
    block_tree: &Tree,
    old_tree: Option<&MarkdownParseTree>,
    changed_range: Option<&Range<usize>>,
    inline_backend: InlineBackendKind,
) -> InlineParseOutput {
    match inline_backend {
        InlineBackendKind::TreeSitter => {
            parse_tree_sitter_inline(source, block_tree, old_tree, changed_range)
        }
        #[cfg(any(test, perf_enabled))]
        InlineBackendKind::Comrak => {
            parse_tree_sitter_inline(source, block_tree, old_tree, changed_range)
        }
    }
}

fn parse_tree_sitter_inline(
    source: &str,
    block_tree: &Tree,
    old_tree: Option<&MarkdownParseTree>,
    changed_range: Option<&Range<usize>>,
) -> InlineParseOutput {
    let dirty_ranges = inline_dirty_ranges(block_tree, old_tree, changed_range);
    let mut inline_trees = Vec::new();

    if let (Some(old_tree), Some(dirty_ranges)) = (old_tree, dirty_ranges.as_deref()) {
        inline_trees.extend(
            old_tree
                .inline_trees()
                .iter()
                .filter(|inline_tree| !ranges_touch_any(&inline_tree.parent_range, dirty_ranges))
                .cloned(),
        );
    }

    let inline_parent_nodes = record_timed_inline_parent_scan(|| {
        if let Some(dirty_ranges) = dirty_ranges.as_deref() {
            inline_parent_nodes_touching_ranges(block_tree, dirty_ranges)
        } else {
            inline_parent_nodes(block_tree)
        }
    });
    let old_inline_tree_by_parent_range = record_timed_inline_reuse_index(|| {
        old_tree.map(|tree| {
            tree.inline_trees()
                .iter()
                .enumerate()
                .map(|(index, inline_tree)| {
                    (
                        (inline_tree.parent_range.start, inline_tree.parent_range.end),
                        index,
                    )
                })
                .collect::<HashMap<_, _>>()
        })
    });

    INLINE_PARSER.with(|parser| {
        let mut inline_parser = parser.borrow_mut();
        for parent_node in inline_parent_nodes {
            if let Some(inline_tree) = reusable_inline_tree(
                old_tree,
                old_inline_tree_by_parent_range.as_ref(),
                parent_node,
                dirty_ranges.as_deref(),
            ) {
                inline_trees.push(inline_tree.clone());
                continue;
            }

            let ranges = record_timed_inline_range_build(|| inline_included_ranges(parent_node));
            if ranges
                .iter()
                .all(|range| range.start_byte == range.end_byte)
            {
                continue;
            }

            let inline_tree = record_timed_inline_parse(|| {
                inline_parser
                    .set_included_ranges(&ranges)
                    .expect("failed to set markdown inline parse ranges");
                inline_parser
                    .parse(
                        source,
                        old_inline_tree_for_parent(
                            old_tree,
                            old_inline_tree_by_parent_range.as_ref(),
                            parent_node,
                        )
                        .map(|tree| &tree.tree),
                    )
                    .expect("tree-sitter markdown inline parser was cancelled")
            });
            inline_trees.push(MarkdownInlineTree {
                parent_id: parent_node.id(),
                parent_range: parent_node.byte_range(),
                tree: inline_tree,
            });
        }
    });

    inline_trees.sort_by_key(|inline_tree| {
        (
            inline_tree.parent_range.start,
            inline_tree.parent_range.end,
            inline_tree.parent_id,
        )
    });
    inline_trees.dedup_by_key(|inline_tree| {
        (
            inline_tree.parent_range.start,
            inline_tree.parent_range.end,
            inline_tree.parent_id,
        )
    });
    let inline_tree_by_parent_id = inline_trees
        .iter()
        .enumerate()
        .map(|(index, inline_tree)| (inline_tree.parent_id, index))
        .collect();
    let semantics = inline::collect_inline_semantics_for_inline_trees(source, &inline_trees);

    InlineParseOutput {
        semantics,
        cache: MarkdownInlineCache::TreeSitter {
            inline_trees,
            inline_tree_by_parent_id,
        },
    }
}

#[cfg(any(test, perf_enabled))]
pub(super) fn parse_inline_trees_for_ranges(
    source: &str,
    parent_ranges: impl IntoIterator<Item = Range<usize>>,
) -> Vec<MarkdownInlineTree> {
    let line_starts = super::source::line_starts(source);
    let mut inline_trees = Vec::new();

    INLINE_PARSER.with(|parser| {
        let mut inline_parser = parser.borrow_mut();
        for (index, parent_range) in parent_ranges.into_iter().enumerate() {
            if parent_range.is_empty() {
                continue;
            }

            let ranges = record_timed_inline_range_build(|| {
                [TreeSitterRange {
                    start_byte: parent_range.start,
                    start_point: point_for_offset(&line_starts, parent_range.start),
                    end_byte: parent_range.end,
                    end_point: point_for_offset(&line_starts, parent_range.end),
                }]
            });
            let inline_tree = record_timed_inline_parse(|| {
                inline_parser
                    .set_included_ranges(&ranges)
                    .expect("failed to set markdown inline parse ranges");
                inline_parser
                    .parse(source, None)
                    .expect("tree-sitter markdown inline parser was cancelled")
            });
            inline_trees.push(MarkdownInlineTree {
                parent_id: 1 << 61 | index,
                parent_range,
                tree: inline_tree,
            });
        }
    });

    inline_trees.sort_by_key(|inline_tree| {
        (
            inline_tree.parent_range.start,
            inline_tree.parent_range.end,
            inline_tree.parent_id,
        )
    });
    inline_trees
}

fn inline_dirty_ranges(
    block_tree: &Tree,
    old_tree: Option<&MarkdownParseTree>,
    changed_range: Option<&Range<usize>>,
) -> Option<Vec<Range<usize>>> {
    let old_tree = old_tree?;
    let mut dirty_ranges = Vec::new();
    if let Some(changed_range) = changed_range {
        dirty_ranges.push(changed_range.clone());
    }
    dirty_ranges.extend(
        old_tree
            .block_tree
            .changed_ranges(block_tree)
            .map(|range| range.start_byte..range.end_byte),
    );
    Some(merge_ranges(dirty_ranges))
}

fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in ranges {
        if let Some(previous) = merged.last_mut()
            && range.start <= previous.end
        {
            previous.end = previous.end.max(range.end);
            continue;
        }
        merged.push(range);
    }
    merged
}

fn reusable_inline_tree<'a>(
    old_tree: Option<&'a MarkdownParseTree>,
    old_inline_tree_by_parent_range: Option<&HashMap<(usize, usize), usize>>,
    parent_node: Node<'_>,
    dirty_ranges: Option<&[Range<usize>]>,
) -> Option<&'a MarkdownInlineTree> {
    let inline_tree =
        old_inline_tree_for_parent(old_tree, old_inline_tree_by_parent_range, parent_node)?;
    if inline_tree.parent_range != parent_node.byte_range() {
        return None;
    }
    if dirty_ranges.is_some_and(|ranges| ranges_touch_any(&inline_tree.parent_range, ranges)) {
        return None;
    }
    Some(inline_tree)
}

fn old_inline_tree_for_parent<'a>(
    old_tree: Option<&'a MarkdownParseTree>,
    old_inline_tree_by_parent_range: Option<&HashMap<(usize, usize), usize>>,
    parent_node: Node<'_>,
) -> Option<&'a MarkdownInlineTree> {
    let old_tree = old_tree?;
    if let Some(inline_tree) = old_tree
        .inline_tree_by_parent_id
        .get(&parent_node.id())
        .and_then(|index| old_tree.inline_trees.get(*index))
    {
        return Some(inline_tree);
    }

    old_inline_tree_by_parent_range
        .and_then(|index_by_range| {
            index_by_range.get(&(parent_node.start_byte(), parent_node.end_byte()))
        })
        .and_then(|index| old_tree.inline_trees.get(*index))
}

fn ranges_touch_any(range: &Range<usize>, dirty_ranges: &[Range<usize>]) -> bool {
    dirty_ranges
        .iter()
        .any(|dirty_range| ranges_touch(range, dirty_range))
}

fn inline_parent_nodes(block_tree: &Tree) -> Vec<Node<'_>> {
    let mut nodes = Vec::new();
    collect_inline_parent_nodes(block_tree.root_node(), &mut nodes);
    nodes
}

fn inline_parent_nodes_touching_ranges<'tree>(
    block_tree: &'tree Tree,
    dirty_ranges: &[Range<usize>],
) -> Vec<Node<'tree>> {
    let mut nodes = Vec::new();
    for dirty_range in dirty_ranges {
        collect_inline_parent_nodes_touching_range(block_tree.root_node(), dirty_range, &mut nodes);
    }
    nodes.sort_by_key(|node| (node.start_byte(), node.end_byte(), node.id()));
    nodes.dedup_by_key(|node| (node.start_byte(), node.end_byte(), node.id()));
    nodes
}

fn collect_inline_parent_nodes_touching_range<'tree>(
    node: Node<'tree>,
    dirty_range: &Range<usize>,
    nodes: &mut Vec<Node<'tree>>,
) {
    if !ranges_touch(&node.byte_range(), dirty_range) {
        return;
    }
    if matches!(node.kind(), "inline" | "pipe_table_cell") {
        nodes.push(node);
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_inline_parent_nodes_touching_range(child, dirty_range, nodes);
    }
}

fn collect_inline_parent_nodes<'tree>(node: Node<'tree>, nodes: &mut Vec<Node<'tree>>) {
    if matches!(node.kind(), "inline" | "pipe_table_cell") {
        nodes.push(node);
        return;
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_inline_parent_nodes(child, nodes);
    }
}

fn inline_included_ranges(parent_node: Node<'_>) -> Vec<TreeSitterRange> {
    let mut ranges = Vec::new();
    let mut range = parent_node.range();
    let mut cursor = parent_node.walk();

    for child in parent_node.named_children(&mut cursor) {
        let child_range = child.range();
        if range.start_byte < child_range.start_byte {
            ranges.push(TreeSitterRange {
                start_byte: range.start_byte,
                start_point: range.start_point,
                end_byte: child_range.start_byte,
                end_point: child_range.start_point,
            });
        }
        range.start_byte = child_range.end_byte;
        range.start_point = child_range.end_point;
    }

    if range.start_byte < range.end_byte {
        ranges.push(range);
    }

    ranges
}
