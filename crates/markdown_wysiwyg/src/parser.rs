use std::collections::HashMap;

use tree_sitter::{Node, Parser, Range as TreeSitterRange, Tree};

use super::{
    MarkdownInlineTree, MarkdownParseTree, record_timed_block_parse,
    record_timed_inline_parent_scan, record_timed_inline_parse, record_timed_inline_range_build,
    record_timed_inline_reuse_index,
};
pub(super) fn parse_markdown(
    source: &str,
    old_tree: Option<&MarkdownParseTree>,
    changed_range: Option<&std::ops::Range<usize>>,
) -> MarkdownParseTree {
    let block_tree = record_timed_block_parse(|| {
        let mut block_parser = Parser::new();
        let block_language = tree_sitter_md::LANGUAGE.into();
        block_parser
            .set_language(&block_language)
            .expect("failed to load tree-sitter markdown block grammar");
        block_parser
            .parse(source, old_tree.map(|tree| &tree.block_tree))
            .expect("tree-sitter markdown block parser was cancelled")
    });

    let (inline_trees, inline_tree_by_parent_id) =
        parse_inline_trees(source, &block_tree, old_tree, changed_range);

    MarkdownParseTree {
        block_tree,
        inline_trees,
        inline_tree_by_parent_id,
    }
}

fn parse_inline_trees(
    source: &str,
    block_tree: &Tree,
    old_tree: Option<&MarkdownParseTree>,
    changed_range: Option<&std::ops::Range<usize>>,
) -> (Vec<MarkdownInlineTree>, HashMap<usize, usize>) {
    let mut inline_parser = Parser::new();
    let inline_language = tree_sitter_md::INLINE_LANGUAGE.into();
    inline_parser
        .set_language(&inline_language)
        .expect("failed to load tree-sitter markdown inline grammar");

    let mut inline_trees = Vec::new();
    let mut inline_tree_by_parent_id = HashMap::new();
    let inline_parent_nodes = record_timed_inline_parent_scan(|| inline_parent_nodes(block_tree));
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

    for parent_node in inline_parent_nodes {
        if let Some(inline_tree) = reusable_inline_tree(
            old_tree,
            old_inline_tree_by_parent_range.as_ref(),
            parent_node,
            changed_range,
        ) {
            inline_tree_by_parent_id.insert(parent_node.id(), inline_trees.len());
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
        inline_tree_by_parent_id.insert(parent_node.id(), inline_trees.len());
        inline_trees.push(MarkdownInlineTree {
            parent_id: parent_node.id(),
            parent_range: parent_node.byte_range(),
            tree: inline_tree,
        });
    }

    (inline_trees, inline_tree_by_parent_id)
}

fn reusable_inline_tree<'a>(
    old_tree: Option<&'a MarkdownParseTree>,
    old_inline_tree_by_parent_range: Option<&HashMap<(usize, usize), usize>>,
    parent_node: Node<'_>,
    changed_range: Option<&std::ops::Range<usize>>,
) -> Option<&'a MarkdownInlineTree> {
    let inline_tree =
        old_inline_tree_for_parent(old_tree, old_inline_tree_by_parent_range, parent_node)?;
    if inline_tree.parent_range != parent_node.byte_range() {
        return None;
    }
    if changed_range.is_some_and(|range| ranges_touch(&inline_tree.parent_range, range)) {
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

fn ranges_touch(left: &std::ops::Range<usize>, right: &std::ops::Range<usize>) -> bool {
    left.start <= right.end && right.start <= left.end
}

fn inline_parent_nodes(block_tree: &Tree) -> Vec<Node<'_>> {
    let mut nodes = Vec::new();
    collect_inline_parent_nodes(block_tree.root_node(), &mut nodes);
    nodes
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
