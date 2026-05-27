use std::{collections::HashMap, fmt, ops::Range};

use tree_sitter::{InputEdit, Node, Parser, Point, Range as TreeSitterRange, Tree};

#[derive(Clone)]
pub struct MarkdownSyntaxTree {
    tree: MarkdownParseTree,
    source_len: usize,
    line_starts: Vec<usize>,
    blocks: Vec<MarkdownBlock>,
    inline_spans: Vec<MarkdownInlineSpan>,
    inline_span_prefix_maximum_ends: Vec<usize>,
    projection_marker_dependencies: Vec<ProjectionMarkerDependency>,
    projection_marker_prefix_maximum_ends: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct MarkdownParseTree {
    block_tree: Tree,
    inline_trees: Vec<MarkdownInlineTree>,
    inline_tree_by_parent_id: HashMap<usize, usize>,
}

#[derive(Clone, Debug)]
pub struct MarkdownInlineTree {
    pub parent_id: usize,
    pub parent_range: Range<usize>,
    tree: Tree,
}

impl fmt::Debug for MarkdownSyntaxTree {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MarkdownSyntaxTree")
            .field("source_len", &self.source_len)
            .field("line_starts", &self.line_starts)
            .field("blocks", &self.blocks)
            .field("inline_spans", &self.inline_spans)
            .finish_non_exhaustive()
    }
}

impl MarkdownParseTree {
    pub fn block_tree(&self) -> &Tree {
        &self.block_tree
    }

    pub fn inline_trees(&self) -> &[MarkdownInlineTree] {
        &self.inline_trees
    }

    pub fn inline_tree_for_parent(&self, parent: Node<'_>) -> Option<&Tree> {
        self.inline_tree_by_parent_id
            .get(&parent.id())
            .map(|index| &self.inline_trees[*index].tree)
    }

    fn edit(&mut self, edit: &InputEdit) {
        self.block_tree.edit(edit);
        for inline_tree in &mut self.inline_trees {
            inline_tree.tree.edit(edit);
        }
    }
}

impl MarkdownInlineTree {
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownBlock {
    pub id: MarkdownNodeId,
    pub kind: MarkdownBlockKind,
    pub source_range: Range<usize>,
    pub content_range: Range<usize>,
    pub marker_ranges: Vec<Range<usize>>,
    pub row_range: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MarkdownNodeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownBlockKind {
    Blank,
    Paragraph,
    AtxHeading { level: u8 },
    FencedCodeBlock,
    PipeTable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownInlineSpan {
    pub kind: MarkdownInlineKind,
    pub source_range: Range<usize>,
    pub content_ranges: Vec<Range<usize>>,
    pub marker_ranges: Vec<Range<usize>>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectionMarkerDependency {
    marker_range: Range<usize>,
    owner_source_range: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownInlineKind {
    Emphasis,
    Strong,
    InlineCode,
    Link,
    Strikethrough,
    Image,
    InlineMath,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkdownProjectionMap {
    source_len: usize,
    visible_source_range: Range<usize>,
    hidden_ranges: Vec<Range<usize>>,
}

impl MarkdownSyntaxTree {
    pub fn parse(source: &str) -> Self {
        Self::parse_with_previous_tree(source, None)
    }

    pub fn reparse_after_edit(
        &self,
        old_source: &str,
        old_range: Range<usize>,
        new_source: &str,
    ) -> Self {
        let old_line_starts = line_starts(old_source);
        let new_line_starts = line_starts(new_source);
        let inserted_len = new_source
            .len()
            .checked_sub(old_source.len() - (old_range.end - old_range.start))
            .expect("new source must match the supplied edit range");
        let new_end_byte = old_range.start + inserted_len;

        let mut edited_tree = self.tree.clone();
        edited_tree.edit(&InputEdit {
            start_byte: old_range.start,
            old_end_byte: old_range.end,
            new_end_byte,
            start_position: point_for_offset(&old_line_starts, old_range.start),
            old_end_position: point_for_offset(&old_line_starts, old_range.end),
            new_end_position: point_for_offset(&new_line_starts, new_end_byte),
        });

        Self::parse_with_previous_tree(new_source, Some(&edited_tree))
    }

    pub fn parse_tree(&self) -> &MarkdownParseTree {
        &self.tree
    }

    pub fn block_tree(&self) -> &Tree {
        self.tree.block_tree()
    }

    pub fn inline_trees(&self) -> &[MarkdownInlineTree] {
        self.tree.inline_trees()
    }

    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub fn blocks(&self) -> &[MarkdownBlock] {
        &self.blocks
    }

    pub fn inline_spans(&self) -> &[MarkdownInlineSpan] {
        &self.inline_spans
    }

    pub fn inline_spans_in_source_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = &MarkdownInlineSpan> {
        let start = range.start;
        let end = range.end;
        let start_index = self.partition_inline_spans_by_prefix_end(start);
        self.inline_spans[start_index..]
            .iter()
            .take_while(move |span| span.source_range.start < end)
            .filter(move |span| span.source_range.start < end && span.source_range.end > start)
    }

    pub fn blocks_in_source_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = &MarkdownBlock> {
        let start = self.partition_blocks_by_end(range.start);
        self.blocks[start..]
            .iter()
            .take_while(move |block| block.source_range.start < range.end)
    }

    pub fn source_range_for_rows(&self, rows: Range<usize>) -> Range<usize> {
        let start = self
            .line_starts
            .get(rows.start)
            .copied()
            .unwrap_or(self.source_len);
        let end = self
            .line_starts
            .get(rows.end)
            .copied()
            .unwrap_or(self.source_len);
        start..end
    }

    pub fn projection_for_visible_rows(
        &self,
        rows: Range<usize>,
        active_source_range: Option<Range<usize>>,
    ) -> MarkdownProjectionMap {
        self.projection_for_source_range(self.source_range_for_rows(rows), active_source_range)
    }

    pub fn projection_for_source_range(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<Range<usize>>,
    ) -> MarkdownProjectionMap {
        self.projection_for_source_range_with_inactive_ranges(
            visible_source_range,
            active_source_range,
            &[],
        )
    }

    pub fn projection_for_source_range_with_inactive_ranges(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
    ) -> MarkdownProjectionMap {
        let mut hidden_ranges = Vec::new();
        for block in self.blocks_in_source_range(visible_source_range.clone()) {
            if source_range_is_active(
                &block.source_range,
                active_source_range.as_ref(),
                inactive_source_ranges,
            ) {
                continue;
            }

            for marker_range in &block.marker_ranges {
                let start = marker_range.start.max(visible_source_range.start);
                let end = marker_range.end.min(visible_source_range.end);
                if start < end {
                    hidden_ranges.push(start..end);
                }
            }
        }

        for span in self.inline_spans_in_source_range(visible_source_range.clone()) {
            if source_range_is_active(
                &span.source_range,
                active_source_range.as_ref(),
                inactive_source_ranges,
            ) {
                continue;
            }

            for marker_range in &span.marker_ranges {
                let start = marker_range.start.max(visible_source_range.start);
                let end = marker_range.end.min(visible_source_range.end);
                if start < end {
                    hidden_ranges.push(start..end);
                }
            }
        }

        MarkdownProjectionMap::new(self.source_len, visible_source_range, hidden_ranges)
    }

    pub fn active_projection_source_ranges_for_source_range(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
    ) -> Vec<Range<usize>> {
        let Some(active_source_range) = active_source_range.as_ref() else {
            return Vec::new();
        };

        let start_index =
            self.partition_projection_marker_dependencies_by_prefix_end(visible_source_range.start);
        let mut source_ranges = self.projection_marker_dependencies[start_index..]
            .iter()
            .take_while(|dependency| dependency.marker_range.start < visible_source_range.end)
            .filter(|dependency| ranges_overlap(&dependency.marker_range, &visible_source_range))
            .filter(|dependency| {
                source_range_is_active(
                    &dependency.owner_source_range,
                    Some(active_source_range),
                    inactive_source_ranges,
                )
            })
            .map(|dependency| dependency.owner_source_range.clone())
            .collect::<Vec<_>>();

        source_ranges.sort_by_key(|source_range| (source_range.start, source_range.end));
        source_ranges.dedup();
        source_ranges
    }

    fn parse_with_previous_tree(source: &str, old_tree: Option<&MarkdownParseTree>) -> Self {
        let tree = parse_markdown(source, old_tree);
        let line_starts = line_starts(source);
        let blocks = collect_blocks(source, &line_starts, tree.block_tree());
        let inline_spans = collect_inline_spans(source, &tree);
        let inline_span_prefix_maximum_ends = inline_span_prefix_maximum_ends(&inline_spans);
        let projection_marker_dependencies = projection_marker_dependencies(&blocks, &inline_spans);
        let projection_marker_prefix_maximum_ends =
            projection_marker_prefix_maximum_ends(&projection_marker_dependencies);

        Self {
            tree,
            source_len: source.len(),
            line_starts,
            blocks,
            inline_spans,
            inline_span_prefix_maximum_ends,
            projection_marker_dependencies,
            projection_marker_prefix_maximum_ends,
        }
    }

    fn partition_blocks_by_end(&self, offset: usize) -> usize {
        self.blocks
            .partition_point(|block| block.source_range.end <= offset)
    }

    fn partition_inline_spans_by_prefix_end(&self, offset: usize) -> usize {
        self.inline_span_prefix_maximum_ends
            .partition_point(|end| *end <= offset)
    }

    fn partition_projection_marker_dependencies_by_prefix_end(&self, offset: usize) -> usize {
        self.projection_marker_prefix_maximum_ends
            .partition_point(|end| *end <= offset)
    }
}

impl MarkdownProjectionMap {
    pub fn new(
        source_len: usize,
        visible_source_range: Range<usize>,
        mut hidden_ranges: Vec<Range<usize>>,
    ) -> Self {
        hidden_ranges.sort_by_key(|range| (range.start, range.end));
        hidden_ranges.retain(|range| range.start < range.end);

        let mut merged_ranges: Vec<Range<usize>> = Vec::with_capacity(hidden_ranges.len());
        for range in hidden_ranges {
            if let Some(previous) = merged_ranges.last_mut() {
                if previous.end >= range.start {
                    previous.end = previous.end.max(range.end);
                    continue;
                }
            }
            merged_ranges.push(range);
        }

        Self {
            source_len,
            visible_source_range,
            hidden_ranges: merged_ranges,
        }
    }

    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub fn visible_source_range(&self) -> Range<usize> {
        self.visible_source_range.clone()
    }

    pub fn hidden_ranges(&self) -> &[Range<usize>] {
        &self.hidden_ranges
    }

    pub fn display_len(&self) -> usize {
        self.source_to_display(self.visible_source_range.end)
    }

    pub fn source_to_display(&self, source_offset: usize) -> usize {
        let clipped_offset = source_offset.clamp(
            self.visible_source_range.start,
            self.visible_source_range.end,
        );
        let mut display_offset = clipped_offset - self.visible_source_range.start;

        for hidden_range in &self.hidden_ranges {
            if hidden_range.start >= clipped_offset {
                break;
            }
            let hidden_start = hidden_range.start.max(self.visible_source_range.start);
            let hidden_end = hidden_range.end.min(clipped_offset);
            display_offset = display_offset.saturating_sub(hidden_end.saturating_sub(hidden_start));
        }

        display_offset
    }

    pub fn display_to_source(&self, display_offset: usize) -> usize {
        let mut source_offset = self.visible_source_range.start + display_offset;

        for hidden_range in &self.hidden_ranges {
            if source_offset < hidden_range.start {
                break;
            }
            source_offset += hidden_range.end - hidden_range.start;
        }

        source_offset.min(self.visible_source_range.end)
    }
}

fn parse_markdown(source: &str, old_tree: Option<&MarkdownParseTree>) -> MarkdownParseTree {
    let mut block_parser = Parser::new();
    let block_language = tree_sitter_md::LANGUAGE.into();
    block_parser
        .set_language(&block_language)
        .expect("failed to load tree-sitter markdown block grammar");
    let block_tree = block_parser
        .parse(source, old_tree.map(|tree| &tree.block_tree))
        .expect("tree-sitter markdown block parser was cancelled");

    let (inline_trees, inline_tree_by_parent_id) =
        parse_inline_trees(source, &block_tree, old_tree);

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
                old_tree.and_then(|tree| {
                    tree.inline_trees
                        .get(inline_trees.len())
                        .map(|tree| &tree.tree)
                }),
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

fn collect_blocks(source: &str, line_starts: &[usize], tree: &Tree) -> Vec<MarkdownBlock> {
    let mut blocks = Vec::new();
    collect_block_nodes(source, tree.root_node(), &mut blocks);
    add_blank_blocks(source, line_starts, &mut blocks);
    blocks.sort_by_key(|block| (block.source_range.start, block.source_range.end));
    blocks
}

fn collect_inline_spans(source: &str, parse_tree: &MarkdownParseTree) -> Vec<MarkdownInlineSpan> {
    let mut spans = Vec::new();
    for inline_tree in parse_tree.inline_trees() {
        collect_inline_span_nodes(source, inline_tree.tree().root_node(), &mut spans);
    }
    spans.sort_by_key(|span| (span.source_range.start, span.source_range.end));
    spans
}

fn inline_span_prefix_maximum_ends(inline_spans: &[MarkdownInlineSpan]) -> Vec<usize> {
    let mut maximum_end = 0;
    inline_spans
        .iter()
        .map(|span| {
            maximum_end = maximum_end.max(span.source_range.end);
            maximum_end
        })
        .collect()
}

fn projection_marker_dependencies(
    blocks: &[MarkdownBlock],
    inline_spans: &[MarkdownInlineSpan],
) -> Vec<ProjectionMarkerDependency> {
    let mut dependencies = Vec::new();
    for block in blocks {
        dependencies.extend(block.marker_ranges.iter().cloned().map(|marker_range| {
            ProjectionMarkerDependency {
                marker_range,
                owner_source_range: block.source_range.clone(),
            }
        }));
    }
    for span in inline_spans {
        dependencies.extend(span.marker_ranges.iter().cloned().map(|marker_range| {
            ProjectionMarkerDependency {
                marker_range,
                owner_source_range: span.source_range.clone(),
            }
        }));
    }
    dependencies.sort_by_key(|dependency| {
        (
            dependency.marker_range.start,
            dependency.marker_range.end,
            dependency.owner_source_range.start,
            dependency.owner_source_range.end,
        )
    });
    dependencies
}

fn projection_marker_prefix_maximum_ends(
    dependencies: &[ProjectionMarkerDependency],
) -> Vec<usize> {
    let mut maximum_end = 0;
    dependencies
        .iter()
        .map(|dependency| {
            maximum_end = maximum_end.max(dependency.marker_range.end);
            maximum_end
        })
        .collect()
}

fn collect_inline_span_nodes(source: &str, node: Node<'_>, spans: &mut Vec<MarkdownInlineSpan>) {
    if let Some(span) = inline_span_from_node(source, node) {
        spans.push(span);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_inline_span_nodes(source, child, spans);
    }
}

fn inline_span_from_node(source: &str, node: Node<'_>) -> Option<MarkdownInlineSpan> {
    let kind = match node.kind() {
        "emphasis" => MarkdownInlineKind::Emphasis,
        "strong_emphasis" => MarkdownInlineKind::Strong,
        "code_span" => MarkdownInlineKind::InlineCode,
        "inline_link"
        | "full_reference_link"
        | "collapsed_reference_link"
        | "shortcut_link"
        | "uri_autolink"
        | "email_autolink" => MarkdownInlineKind::Link,
        "strikethrough" => MarkdownInlineKind::Strikethrough,
        "image" => MarkdownInlineKind::Image,
        "latex_block" => MarkdownInlineKind::InlineMath,
        _ => return None,
    };

    let source_range = node.byte_range();
    let marker_ranges = inline_marker_ranges(source, node);
    let content_ranges = inline_content_ranges(source_range.clone(), &marker_ranges);
    let url = match kind {
        MarkdownInlineKind::Image | MarkdownInlineKind::Link => extract_link_url(source, &node),
        _ => None,
    };

    Some(MarkdownInlineSpan {
        kind,
        source_range,
        content_ranges,
        marker_ranges,
        url,
    })
}

fn inline_marker_ranges(source: &str, node: Node<'_>) -> Vec<Range<usize>> {
    if node.kind() == "latex_block" {
        return latex_block_marker_ranges(source, node);
    }

    let mut marker_ranges = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "latex_span_delimiter" => marker_ranges.push(child.byte_range()),
            "emphasis_delimiter"
            | "code_span_delimiter"
            | "link_destination"
            | "link_label"
            | "link_title" => marker_ranges.push(child.byte_range()),
            _ if !child.is_named() => marker_ranges.push(child.byte_range()),
            _ => {}
        }
    }
    marker_ranges.sort_by_key(|range| (range.start, range.end));
    marker_ranges
}

fn latex_block_marker_ranges(source: &str, node: Node<'_>) -> Vec<Range<usize>> {
    let source_range = node.byte_range();
    let delimiter_len = if source
        .get(source_range.clone())
        .is_some_and(|text| text.starts_with("$$") && text.ends_with("$$"))
    {
        2
    } else {
        1
    };

    let start = source_range.start;
    let end = source_range.end;
    let content_start = start.saturating_add(delimiter_len).min(end);
    let content_end = end.saturating_sub(delimiter_len).max(content_start);

    let mut marker_ranges = Vec::new();
    if start < content_start {
        marker_ranges.push(start..content_start);
    }
    if content_end < end {
        marker_ranges.push(content_end..end);
    }
    marker_ranges
}

fn extract_link_url(source: &str, node: &Node<'_>) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "link_destination" {
            let range = child.byte_range();
            return Some(source[range].to_string());
        }
    }
    None
}

fn inline_content_ranges(
    source_range: Range<usize>,
    marker_ranges: &[Range<usize>],
) -> Vec<Range<usize>> {
    let mut content_ranges = Vec::new();
    let mut start = source_range.start;
    for marker_range in marker_ranges {
        if start < marker_range.start {
            content_ranges.push(start..marker_range.start);
        }
        start = start.max(marker_range.end);
    }
    if start < source_range.end {
        content_ranges.push(start..source_range.end);
    }
    content_ranges
}

fn collect_block_nodes(source: &str, node: Node<'_>, blocks: &mut Vec<MarkdownBlock>) {
    if let Some(block) = block_from_node(source, node) {
        blocks.push(block);
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_block_nodes(source, child, blocks);
    }
}

fn block_from_node(source: &str, node: Node<'_>) -> Option<MarkdownBlock> {
    match node.kind() {
        "atx_heading" => atx_heading_block(source, node),
        "paragraph" => Some(MarkdownBlock {
            id: node_id(node),
            kind: MarkdownBlockKind::Paragraph,
            source_range: node.byte_range(),
            content_range: trim_line_end(source, node.byte_range()),
            marker_ranges: Vec::new(),
            row_range: row_range_for_node(node),
        }),
        "fenced_code_block" => Some(MarkdownBlock {
            id: node_id(node),
            kind: MarkdownBlockKind::FencedCodeBlock,
            source_range: node.byte_range(),
            content_range: fenced_code_content_range(source, node),
            marker_ranges: fenced_code_marker_ranges(node),
            row_range: row_range_for_node(node),
        }),
        "pipe_table" => {
            let marker_ranges = pipe_table_marker_ranges(node);
            let content_range = trim_line_end(source, node.byte_range());
            Some(MarkdownBlock {
                id: node_id(node),
                kind: MarkdownBlockKind::PipeTable,
                source_range: node.byte_range(),
                content_range,
                marker_ranges,
                row_range: row_range_for_node(node),
            })
        }
        _ => None,
    }
}

fn atx_heading_block(source: &str, node: Node<'_>) -> Option<MarkdownBlock> {
    let source_range = node.byte_range();
    let (level, marker_range, content_start) =
        atx_heading_marker_range(source, source_range.clone())?;

    let content_end = trim_line_end(source, content_start..source_range.end).end;

    Some(MarkdownBlock {
        id: node_id(node),
        kind: MarkdownBlockKind::AtxHeading { level },
        source_range: source_range.clone(),
        content_range: content_start..content_end,
        marker_ranges: vec![marker_range],
        row_range: row_range_for_node(node),
    })
}

fn atx_heading_marker_range(
    source: &str,
    source_range: Range<usize>,
) -> Option<(u8, Range<usize>, usize)> {
    let bytes = source.as_bytes();
    let mut marker_start = source_range.start;
    while marker_start < source_range.end && matches!(bytes[marker_start], b' ' | b'\t') {
        marker_start += 1;
    }

    let mut marker_end = marker_start;
    while marker_end < source_range.end && bytes[marker_end] == b'#' {
        marker_end += 1;
    }

    let level = marker_end - marker_start;
    if !(1..=6).contains(&level) {
        return None;
    }

    let mut content_start = marker_end;
    while content_start < source_range.end && matches!(bytes[content_start], b' ' | b'\t') {
        content_start += 1;
    }

    Some((level as u8, marker_start..content_start, content_start))
}

fn fenced_code_marker_ranges(node: Node<'_>) -> Vec<Range<usize>> {
    let mut marker_ranges = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "fenced_code_block_delimiter" | "info_string" => marker_ranges.push(child.byte_range()),
            _ => {}
        }
    }
    marker_ranges
}

fn pipe_table_marker_ranges(node: Node<'_>) -> Vec<Range<usize>> {
    let mut marker_ranges = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "pipe_table_delimiter_row" => marker_ranges.push(child.byte_range()),
            _ => {}
        }
        let mut inner_cursor = child.walk();
        for grandchild in child.children(&mut inner_cursor) {
            if !grandchild.is_named() && grandchild.kind() == "|" {
                marker_ranges.push(grandchild.byte_range());
            }
        }
    }
    marker_ranges.sort_by_key(|range| (range.start, range.end));
    marker_ranges
}

fn fenced_code_content_range(source: &str, node: Node<'_>) -> Range<usize> {
    let mut content_range = trim_line_end(source, node.byte_range());
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "fenced_code_block_delimiter" | "info_string" => {
                if child.start_byte() == content_range.start {
                    content_range.start = child.end_byte();
                    while content_range.start < content_range.end
                        && matches!(source.as_bytes()[content_range.start], b'\r' | b'\n')
                    {
                        content_range.start += 1;
                    }
                } else if child.end_byte() == content_range.end {
                    content_range.end = child.start_byte();
                    content_range = trim_line_end(source, content_range);
                }
            }
            "code_fence_content" => return child.byte_range(),
            _ => {}
        }
    }
    content_range
}

fn add_blank_blocks(source: &str, line_starts: &[usize], blocks: &mut Vec<MarkdownBlock>) {
    let mut covered_rows = vec![false; line_starts.len()];
    for block in blocks.iter() {
        for row in block.row_range.clone() {
            if let Some(covered) = covered_rows.get_mut(row) {
                *covered = true;
            }
        }
    }

    for row in 0..line_starts.len() {
        if covered_rows[row] {
            continue;
        }

        let range = line_range(source, line_starts, row);
        if range.is_empty() || !source[range.clone()].trim().is_empty() {
            continue;
        }

        blocks.push(MarkdownBlock {
            id: MarkdownNodeId(1 << 63 | row as u64),
            kind: MarkdownBlockKind::Blank,
            source_range: range.clone(),
            content_range: range.start..range.start,
            marker_ranges: Vec::new(),
            row_range: row..row + 1,
        });
    }
}

fn row_range_for_node(node: Node<'_>) -> Range<usize> {
    let start = node.start_position().row;
    let end_position = node.end_position();
    let mut end = if end_position.column == 0 {
        end_position.row
    } else {
        end_position.row + 1
    };
    if end <= start {
        end = start + 1;
    }
    start..end
}

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

fn line_range(source: &str, line_starts: &[usize], row: usize) -> Range<usize> {
    let start = line_starts[row];
    let end = line_starts.get(row + 1).copied().unwrap_or(source.len());
    start..end
}

fn trim_line_end(source: &str, mut range: Range<usize>) -> Range<usize> {
    while range.end > range.start && matches!(source.as_bytes()[range.end - 1], b'\r' | b'\n') {
        range.end -= 1;
    }
    range
}

fn point_for_offset(line_starts: &[usize], offset: usize) -> Point {
    let row = line_starts.partition_point(|line_start| *line_start <= offset) - 1;
    Point {
        row,
        column: offset - line_starts[row],
    }
}

fn node_id(node: Node<'_>) -> MarkdownNodeId {
    MarkdownNodeId(node.id() as u64)
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn range_contains(container: &Range<usize>, candidate: &Range<usize>) -> bool {
    container.start <= candidate.start && container.end >= candidate.end
}

fn source_range_is_active(
    source_range: &Range<usize>,
    active_source_range: Option<&Range<usize>>,
    inactive_source_ranges: &[Range<usize>],
) -> bool {
    let Some(active_source_range) = active_source_range else {
        return false;
    };
    if !ranges_overlap(source_range, active_source_range) {
        return false;
    }

    !inactive_source_ranges
        .iter()
        .any(|inactive_source_range| range_contains(inactive_source_range, source_range))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_atx_headings_with_tree_sitter() {
        let tree = MarkdownSyntaxTree::parse("# Title\n\nText\n");
        assert_eq!(tree.block_tree().root_node().kind(), "document");
        assert_eq!(tree.source_len(), 14);
        assert_eq!(tree.blocks().len(), 3);
        assert_eq!(
            tree.blocks()[0].kind,
            MarkdownBlockKind::AtxHeading { level: 1 }
        );
        assert_eq!(tree.blocks()[0].source_range, 0..8);
        assert_eq!(tree.blocks()[0].content_range, 2..7);
        assert_eq!(tree.blocks()[0].marker_ranges, vec![0..2]);
        assert_eq!(tree.blocks()[1].kind, MarkdownBlockKind::Blank);
        assert_eq!(tree.blocks()[2].kind, MarkdownBlockKind::Paragraph);
    }

    #[test]
    fn reparses_after_edit_with_tree_sitter() {
        let old_source = "# Title\nBody\n";
        let tree = MarkdownSyntaxTree::parse(old_source);
        let new_source = "# Title!\nBody\n";
        let tree = tree.reparse_after_edit(old_source, 7..7, new_source);

        assert_eq!(tree.source_len(), new_source.len());
        assert_eq!(
            tree.blocks()[0].kind,
            MarkdownBlockKind::AtxHeading { level: 1 }
        );
        assert_eq!(tree.blocks()[0].source_range, 0..9);
        assert_eq!(tree.blocks()[0].content_range, 2..8);
    }

    #[test]
    fn parses_inline_trees_with_tree_sitter() {
        let tree = MarkdownSyntaxTree::parse("Text with **bold** and [link](https://zed.dev).\n");

        assert_eq!(tree.inline_trees().len(), 1);
        let inline_root = tree.inline_trees()[0].tree().root_node();
        let mut cursor = inline_root.walk();
        let inline_kinds = inline_root
            .named_children(&mut cursor)
            .map(|node| node.kind())
            .collect::<Vec<_>>();

        assert!(inline_kinds.contains(&"strong_emphasis"));
        assert!(inline_kinds.contains(&"inline_link"));
        assert_eq!(tree.inline_spans().len(), 2);
        assert_eq!(tree.inline_spans()[0].kind, MarkdownInlineKind::Strong);
        assert_eq!(tree.inline_spans()[1].kind, MarkdownInlineKind::Link);
    }

    #[test]
    fn inline_spans_in_source_range_returns_overlapping_spans() {
        let source = "before **bold**\nafter [link](url)\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let second_row_start = source.find("after").expect("expected second row");

        let first_row_spans = tree
            .inline_spans_in_source_range(0..second_row_start)
            .map(|span| span.kind)
            .collect::<Vec<_>>();
        let second_row_spans = tree
            .inline_spans_in_source_range(second_row_start..source.len())
            .map(|span| span.kind)
            .collect::<Vec<_>>();

        assert_eq!(first_row_spans, vec![MarkdownInlineKind::Strong]);
        assert_eq!(second_row_spans, vec![MarkdownInlineKind::Link]);
    }

    #[test]
    fn inline_spans_in_source_range_includes_spans_starting_before_range() {
        let source = "before **bold\nstill bold** after\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let second_row_start = source.find("still").expect("expected second row");

        let second_row_spans = tree
            .inline_spans_in_source_range(second_row_start..source.len())
            .map(|span| span.kind)
            .collect::<Vec<_>>();

        assert_eq!(second_row_spans, vec![MarkdownInlineKind::Strong]);
    }

    #[test]
    fn hides_inactive_heading_markers_in_projection() {
        let tree = MarkdownSyntaxTree::parse("# Title\nBody\n");
        let projection = tree.projection_for_visible_rows(0..2, None);
        assert_eq!(projection.hidden_ranges(), &[0..2]);
        assert_eq!(projection.source_to_display(0), 0);
        assert_eq!(projection.source_to_display(2), 0);
        assert_eq!(projection.source_to_display(7), 5);
        assert_eq!(projection.display_to_source(0), 2);
        assert_eq!(projection.display_to_source(5), 7);
    }

    #[test]
    fn reveals_active_block_markers() {
        let tree = MarkdownSyntaxTree::parse("# Title\n## Other\n");
        let projection = tree.projection_for_visible_rows(0..2, Some(0..1));
        assert_eq!(projection.hidden_ranges(), &[8..11]);
        assert_eq!(projection.source_to_display(2), 2);
        assert_eq!(projection.display_to_source(0), 0);
    }

    #[test]
    fn hides_inactive_inline_markers_in_projection() {
        let tree = MarkdownSyntaxTree::parse("Before **bold** after\n");
        let projection = tree.projection_for_visible_rows(0..1, None);
        assert_eq!(projection.hidden_ranges(), &[7..9, 13..15]);
        assert_eq!(projection.display_len(), "Before bold after\n".len());
    }

    #[test]
    fn reveals_active_inline_markers() {
        let tree = MarkdownSyntaxTree::parse("Before **bold** after\n");
        let projection = tree.projection_for_visible_rows(0..1, Some(10..11));
        assert!(projection.hidden_ranges().is_empty());
        assert_eq!(projection.display_to_source(7), 7);
    }

    #[test]
    fn inactive_ranges_keep_markers_hidden_inside_active_range() {
        let tree = MarkdownSyntaxTree::parse("Before **bold** and $x$ after\n");
        let projection =
            tree.projection_for_source_range_with_inactive_ranges(0..30, Some(7..23), &[20..23]);

        assert_eq!(projection.hidden_ranges(), &[20..21, 22..23]);
        assert_eq!(projection.display_len(), 28);
    }

    #[test]
    fn active_projection_source_ranges_uses_marker_dependencies() {
        let tree = MarkdownSyntaxTree::parse("```rust\nlet x = 1;\n```\n");
        let content_start = "```rust\n".len();
        let content_end = content_start + "let x = 1;".len();
        let closing_marker_start = content_end + "\n".len();

        assert_eq!(
            tree.active_projection_source_ranges_for_source_range(
                closing_marker_start..closing_marker_start + "```".len(),
                Some(content_start..content_end),
                &[],
            ),
            vec![tree.blocks()[0].source_range.clone()]
        );
    }

    #[test]
    fn clips_projection_to_visible_rows() {
        let tree = MarkdownSyntaxTree::parse("# One\n# Two\n# Three\n");
        let projection = tree.projection_for_visible_rows(1..2, None);
        assert_eq!(projection.visible_source_range(), 6..12);
        assert_eq!(projection.hidden_ranges(), &[6..8]);
        assert_eq!(projection.display_len(), 4);
    }

    #[test]
    fn keeps_fenced_code_block_as_one_block() {
        let source = "```rust\n# not heading\n```\n# Heading\n";
        let tree = MarkdownSyntaxTree::parse(source);
        assert_eq!(tree.blocks().len(), 2);
        assert_eq!(tree.blocks()[0].kind, MarkdownBlockKind::FencedCodeBlock);
        assert_eq!(tree.blocks()[0].row_range, 0..3);
        assert_eq!(
            tree.blocks()[1].kind,
            MarkdownBlockKind::AtxHeading { level: 1 }
        );
    }

    #[test]
    fn parses_pipe_table() {
        let source = "# Title\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let table_block = tree
            .blocks()
            .iter()
            .find(|b| b.kind == MarkdownBlockKind::PipeTable);
        assert!(table_block.is_some(), "expected a PipeTable block");
        let table = table_block.unwrap();
        assert!(
            !table.marker_ranges.is_empty(),
            "table should have marker ranges"
        );
        let has_pipe_marker = table
            .marker_ranges
            .iter()
            .any(|r| &source[r.clone()] == "|");
        assert!(
            has_pipe_marker,
            "table markers should include pipe characters"
        );
    }

    #[test]
    fn parses_image_and_math_inline() {
        let source = "text ![alt](url) $math$\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let spans = tree.inline_spans();
        assert!(spans.iter().any(|s| s.kind == MarkdownInlineKind::Image));
        assert!(
            spans
                .iter()
                .any(|s| s.kind == MarkdownInlineKind::InlineMath)
        );
    }

    #[test]
    fn parses_block_math_markers_and_content_ranges() {
        let source = "$$x + y$$\n";
        let tree = MarkdownSyntaxTree::parse(source);
        let span = tree
            .inline_spans()
            .iter()
            .find(|span| span.kind == MarkdownInlineKind::InlineMath)
            .expect("expected block math span");

        assert_eq!(span.source_range, 0.."$$x + y$$".len());
        assert_eq!(span.marker_ranges, vec![0..2, 7..9]);
        assert_eq!(span.content_ranges, vec![2..7]);
    }

    #[test]
    fn inline_math_projection_keeps_operator_content() {
        let tree = MarkdownSyntaxTree::parse("Before $x + y$ after\n");
        let projection = tree.projection_for_visible_rows(0..1, None);

        assert_eq!(projection.hidden_ranges(), &[7..8, 13..14]);
        assert_eq!(projection.display_len(), "Before x + y after\n".len());
    }

    #[test]
    fn block_math_projection_hides_double_dollar_markers() {
        let tree = MarkdownSyntaxTree::parse("$$x + y$$\n");
        let projection = tree.projection_for_visible_rows(0..1, None);

        assert_eq!(projection.hidden_ranges(), &[0..2, 7..9]);
        assert_eq!(projection.display_len(), "x + y\n".len());
    }
}
