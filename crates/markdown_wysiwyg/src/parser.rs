use std::collections::HashMap;

use tree_sitter::{Node, Parser, Range as TreeSitterRange, Tree};

use super::{MarkdownInlineTree, MarkdownParseTree};
pub(super) fn parse_markdown(
    source: &str,
    old_tree: Option<&MarkdownParseTree>,
    changed_range: Option<&std::ops::Range<usize>>,
) -> MarkdownParseTree {
    let mut block_parser = Parser::new();
    let block_language = tree_sitter_md::LANGUAGE.into();
    block_parser
        .set_language(&block_language)
        .expect("failed to load tree-sitter markdown block grammar");
    let block_tree = block_parser
        .parse(source, old_tree.map(|tree| &tree.block_tree))
        .expect("tree-sitter markdown block parser was cancelled");

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
    let inline_parent_nodes = inline_parent_nodes(block_tree);

    for parent_node in inline_parent_nodes {
        if let Some(inline_tree) = reusable_inline_tree(old_tree, parent_node, changed_range) {
            inline_tree_by_parent_id.insert(parent_node.id(), inline_trees.len());
            inline_trees.push(inline_tree.clone());
            continue;
        }

        let ranges = inline_included_ranges(parent_node);
        if ranges
            .iter()
            .all(|range| range.start_byte == range.end_byte)
        {
            continue;
        }

        inline_parser
            .set_included_ranges(&ranges)
            .expect("failed to set markdown inline parse ranges");
        let inline_tree = inline_parser
            .parse(
                source,
                old_inline_tree_for_parent(old_tree, parent_node).map(|tree| &tree.tree),
            )
            .expect("tree-sitter markdown inline parser was cancelled");
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
    parent_node: Node<'_>,
    changed_range: Option<&std::ops::Range<usize>>,
) -> Option<&'a MarkdownInlineTree> {
    let inline_tree = old_inline_tree_for_parent(old_tree, parent_node)?;
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
    parent_node: Node<'_>,
) -> Option<&'a MarkdownInlineTree> {
    let old_tree = old_tree?;
    old_tree
        .inline_tree_by_parent_id
        .get(&parent_node.id())
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
    for child in node.children(&mut cursor) {
        collect_inline_parent_nodes(child, nodes);
    }
}

fn inline_included_ranges(parent_node: Node<'_>) -> Vec<TreeSitterRange> {
    let mut ranges = Vec::new();
    let mut range = parent_node.range();
    let mut cursor = parent_node.walk();

    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() {
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

            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    if range.start_byte < range.end_byte {
        ranges.push(range);
    }

    ranges
}
