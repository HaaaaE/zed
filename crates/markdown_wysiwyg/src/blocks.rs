use std::ops::Range;

use tree_sitter::Node;

use super::{
    MarkdownBlock, MarkdownBlockKind, MarkdownNodeId, MarkdownStructure, MarkdownStructureBlock,
    inline::raw_html_tagfilter_disallowed, last_line_range, line_range, node_id,
    trim_ascii_whitespace, trim_line_end,
};

pub(super) fn collect_structure_blocks(
    source: &str,
    root: Node<'_>,
) -> Vec<MarkdownStructureBlock> {
    let mut blocks = Vec::new();
    collect_structure_block_nodes(source, root, &mut blocks);
    blocks.sort_by_key(|block| (block.source_range.start, block.source_range.end));
    blocks
}

pub(super) fn validate_structure_blocks(blocks: &[MarkdownStructureBlock]) {
    debug_assert!(blocks.windows(2).all(|pair| {
        let left = &pair[0];
        let right = &pair[1];
        (left.source_range.start, left.source_range.end)
            <= (right.source_range.start, right.source_range.end)
    }));
    debug_assert!(blocks.iter().all(|block| {
        let _kind = block.kind;
        block.source_range.start <= block.source_range.end
            && block.row_range.start < block.row_range.end
    }));
}

pub(super) fn collect_markdown_blocks(
    source: &str,
    line_starts: &[usize],
    structure: &MarkdownStructure,
) -> Vec<MarkdownBlock> {
    let mut blocks = structure
        .blocks()
        .iter()
        .map(MarkdownBlock::from_structure)
        .collect::<Vec<_>>();
    add_blank_structure_blocks(source, line_starts, &mut blocks);
    blocks.sort_by_key(|block| (block.source_range.start, block.source_range.end));
    blocks
}

pub(super) fn collect_incremental_blocks(
    source: &str,
    structure: &MarkdownStructure,
    line_starts: &[usize],
) -> Vec<MarkdownBlock> {
    let mut blocks = structure
        .blocks()
        .iter()
        .map(MarkdownBlock::from_structure)
        .collect::<Vec<_>>();
    add_blank_structure_blocks(source, line_starts, &mut blocks);
    blocks.sort_by_key(|block| {
        (
            block.source_range.start,
            block.source_range.end,
            block.row_range.start,
            block.row_range.end,
        )
    });
    blocks.dedup_by(|right, left| block_semantics_match(left, right));
    blocks
}

fn add_blank_structure_blocks(
    source: &str,
    line_starts: &[usize],
    blocks: &mut Vec<MarkdownBlock>,
) {
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
            tagfilter_disallowed: false,
        });
    }
}

fn block_semantics_match(left: &MarkdownBlock, right: &MarkdownBlock) -> bool {
    left.kind == right.kind
        && left.source_range == right.source_range
        && left.content_range == right.content_range
        && left.marker_ranges == right.marker_ranges
        && left.row_range == right.row_range
        && left.tagfilter_disallowed == right.tagfilter_disallowed
}

fn collect_structure_block_nodes(
    source: &str,
    node: Node<'_>,
    blocks: &mut Vec<MarkdownStructureBlock>,
) {
    if let Some(block) = structure_block_from_node(source, node) {
        let recurse = structure_block_node_has_children(node.kind());
        blocks.push(block);
        if !recurse {
            return;
        }
    }

    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        collect_structure_block_nodes(source, child, blocks);
    }
}

fn structure_block_from_node(source: &str, node: Node<'_>) -> Option<MarkdownStructureBlock> {
    match node.kind() {
        "atx_heading" => structure_atx_heading_block(source, node),
        "setext_heading" => structure_setext_heading_block(source, node),
        "block_quote" => Some(structure_block_quote_block(source, node)),
        "list" => Some(structure_list_block(source, node)),
        "list_item" => Some(structure_list_item_block(source, node)),
        "paragraph" => Some(MarkdownStructureBlock {
            id: node_id(node),
            kind: MarkdownBlockKind::Paragraph,
            source_range: node.byte_range(),
            content_range: trim_line_end(source, node.byte_range()),
            marker_ranges: Vec::new(),
            row_range: row_range_for_structure_node(node),
            tagfilter_disallowed: false,
        }),
        "thematic_break" => Some(MarkdownStructureBlock {
            id: node_id(node),
            kind: MarkdownBlockKind::ThematicBreak,
            source_range: node.byte_range(),
            content_range: node.start_byte()..node.start_byte(),
            marker_ranges: Vec::new(),
            row_range: row_range_for_structure_node(node),
            tagfilter_disallowed: false,
        }),
        "indented_code_block" => Some(MarkdownStructureBlock {
            id: node_id(node),
            kind: MarkdownBlockKind::IndentedCodeBlock,
            source_range: node.byte_range(),
            content_range: trim_line_end(source, node.byte_range()),
            marker_ranges: Vec::new(),
            row_range: row_range_for_structure_node(node),
            tagfilter_disallowed: false,
        }),
        "fenced_code_block" => Some(MarkdownStructureBlock {
            id: node_id(node),
            kind: MarkdownBlockKind::FencedCodeBlock,
            source_range: node.byte_range(),
            content_range: structure_fenced_code_content_range(source, node),
            marker_ranges: structure_fenced_code_marker_ranges(node),
            row_range: row_range_for_structure_node(node),
            tagfilter_disallowed: false,
        }),
        "html_block" => Some(MarkdownStructureBlock {
            id: node_id(node),
            kind: MarkdownBlockKind::HtmlBlock,
            source_range: node.byte_range(),
            content_range: trim_line_end(source, node.byte_range()),
            marker_ranges: Vec::new(),
            row_range: row_range_for_structure_node(node),
            tagfilter_disallowed: raw_html_tagfilter_disallowed(source, node.byte_range()),
        }),
        "link_reference_definition" => Some(MarkdownStructureBlock {
            id: node_id(node),
            kind: MarkdownBlockKind::LinkReferenceDefinition,
            source_range: node.byte_range(),
            content_range: trim_line_end(source, node.byte_range()),
            marker_ranges: Vec::new(),
            row_range: row_range_for_structure_node(node),
            tagfilter_disallowed: false,
        }),
        "pipe_table" => Some(MarkdownStructureBlock {
            id: node_id(node),
            kind: MarkdownBlockKind::PipeTable,
            source_range: node.byte_range(),
            content_range: trim_line_end(source, node.byte_range()),
            marker_ranges: structure_pipe_table_marker_ranges(node),
            row_range: row_range_for_structure_node(node),
            tagfilter_disallowed: false,
        }),
        _ => None,
    }
}

fn structure_block_node_has_children(kind: &str) -> bool {
    matches!(kind, "block_quote" | "list" | "list_item")
}

fn structure_block_quote_block(source: &str, node: Node<'_>) -> MarkdownStructureBlock {
    let source_range = node.byte_range();
    MarkdownStructureBlock {
        id: node_id(node),
        kind: MarkdownBlockKind::BlockQuote,
        source_range: source_range.clone(),
        content_range: trim_line_end(source, source_range.clone()),
        marker_ranges: structure_block_quote_marker_ranges(source, source_range),
        row_range: row_range_for_structure_node(node),
        tagfilter_disallowed: false,
    }
}

fn structure_list_block(source: &str, node: Node<'_>) -> MarkdownStructureBlock {
    let source_range = node.byte_range();
    let content_range = trim_line_end(source, source_range.clone());
    let kind = if structure_list_source_starts_ordered_marker(source, content_range.clone()) {
        MarkdownBlockKind::OrderedList
    } else {
        MarkdownBlockKind::UnorderedList
    };

    MarkdownStructureBlock {
        id: node_id(node),
        kind,
        source_range,
        content_range,
        marker_ranges: Vec::new(),
        row_range: row_range_for_structure_node(node),
        tagfilter_disallowed: false,
    }
}

fn structure_list_item_block(source: &str, node: Node<'_>) -> MarkdownStructureBlock {
    let node_range = node.byte_range();
    let marker_range = structure_list_item_marker_range(source, node_range.clone());
    let source_start = marker_range
        .as_ref()
        .map_or(node_range.start, |range| range.start);
    let source_range = source_start..node_range.end;
    let content_start = marker_range
        .as_ref()
        .map_or(source_range.start, |range| range.end);
    let task_marker =
        structure_task_list_marker_after_list_marker(source, content_start, source_range.end);
    let (kind, content_start) = if let Some((checked, task_content_start)) = task_marker {
        (
            MarkdownBlockKind::TaskListItem { checked },
            task_content_start,
        )
    } else {
        (MarkdownBlockKind::ListItem, content_start)
    };

    MarkdownStructureBlock {
        id: node_id(node),
        kind,
        source_range: source_range.clone(),
        content_range: trim_line_end(source, content_start..source_range.end),
        marker_ranges: marker_range.into_iter().collect(),
        row_range: row_range_for_structure_node(node),
        tagfilter_disallowed: false,
    }
}

fn structure_setext_heading_block(source: &str, node: Node<'_>) -> Option<MarkdownStructureBlock> {
    let source_range = node.byte_range();
    let marker_range = last_line_range(source, source_range.clone())?;
    let marker_text = &source[trim_ascii_whitespace(source, marker_range.clone())];
    let level = match marker_text.as_bytes().first().copied()? {
        b'=' => 1,
        b'-' => 2,
        _ => return None,
    };

    Some(MarkdownStructureBlock {
        id: node_id(node),
        kind: MarkdownBlockKind::SetextHeading { level },
        source_range: source_range.clone(),
        content_range: trim_line_end(source, source_range.start..marker_range.start),
        marker_ranges: vec![marker_range],
        row_range: row_range_for_structure_node(node),
        tagfilter_disallowed: false,
    })
}

fn structure_atx_heading_block(source: &str, node: Node<'_>) -> Option<MarkdownStructureBlock> {
    let source_range = node.byte_range();
    let (level, marker_range, content_start) =
        structure_atx_heading_marker_range(source, source_range.clone())?;
    let content_end = trim_line_end(source, content_start..source_range.end).end;

    Some(MarkdownStructureBlock {
        id: node_id(node),
        kind: MarkdownBlockKind::AtxHeading { level },
        source_range,
        content_range: content_start..content_end,
        marker_ranges: vec![marker_range],
        row_range: row_range_for_structure_node(node),
        tagfilter_disallowed: false,
    })
}

fn structure_atx_heading_marker_range(
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

fn structure_task_list_marker_after_list_marker(
    source: &str,
    content_start: usize,
    source_end: usize,
) -> Option<(bool, usize)> {
    let bytes = source.as_bytes();
    let marker_end = content_start.checked_add(3)?;
    if marker_end > source_end {
        return None;
    }
    if bytes[content_start] != b'[' || bytes[content_start + 2] != b']' {
        return None;
    }

    let checked = match bytes[content_start + 1] {
        b' ' => false,
        b'x' | b'X' => true,
        _ => return None,
    };

    let line_end = source[content_start..source_end]
        .find('\n')
        .map_or(source_end, |offset| content_start + offset);
    let trimmed_line_end = trim_line_end(source, content_start..line_end).end;
    if marker_end < trimmed_line_end && !matches!(bytes[marker_end], b' ' | b'\t') {
        return None;
    }

    let mut task_content_start = marker_end;
    while task_content_start < source_end && matches!(bytes[task_content_start], b' ' | b'\t') {
        task_content_start += 1;
    }

    Some((checked, task_content_start))
}

fn structure_list_source_starts_ordered_marker(source: &str, range: Range<usize>) -> bool {
    let bytes = source.as_bytes();
    let mut cursor = range.start;

    while cursor < range.end && matches!(bytes[cursor], b' ' | b'\t') {
        cursor += 1;
    }

    let digit_start = cursor;
    while cursor < range.end && bytes[cursor].is_ascii_digit() {
        cursor += 1;
    }

    cursor > digit_start && cursor < range.end && matches!(bytes[cursor], b'.' | b')')
}

fn structure_block_quote_marker_ranges(
    source: &str,
    source_range: Range<usize>,
) -> Vec<Range<usize>> {
    let bytes = source.as_bytes();
    let mut ranges = Vec::new();
    let mut line_start = source_range.start;

    while line_start < source_range.end {
        let line_end = source[line_start..source_range.end]
            .find('\n')
            .map_or(source_range.end, |offset| line_start + offset + 1);
        let trimmed_line = trim_line_end(source, line_start..line_end);
        let mut cursor = trimmed_line.start;
        let mut leading_spaces = 0;

        while cursor < trimmed_line.end && bytes[cursor] == b' ' && leading_spaces < 4 {
            cursor += 1;
            leading_spaces += 1;
        }

        if cursor < trimmed_line.end && bytes[cursor] == b'>' {
            let marker_start = cursor;
            cursor += 1;
            if cursor < trimmed_line.end && bytes[cursor] == b' ' {
                cursor += 1;
            }
            ranges.push(marker_start..cursor);
        }

        line_start = line_end;
    }

    ranges
}

fn structure_list_item_marker_range(
    source: &str,
    source_range: Range<usize>,
) -> Option<Range<usize>> {
    let bytes = source.as_bytes();
    let marker_start = structure_list_item_marker_start(source, source_range.start);
    let mut cursor = marker_start;

    while cursor < source_range.end && matches!(bytes[cursor], b' ' | b'\t') {
        cursor += 1;
    }

    if cursor < source_range.end && matches!(bytes[cursor], b'-' | b'+' | b'*') {
        cursor += 1;
    } else {
        let digit_start = cursor;
        while cursor < source_range.end && bytes[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == digit_start
            || cursor >= source_range.end
            || !matches!(bytes[cursor], b'.' | b')')
        {
            return None;
        }
        cursor += 1;
    }

    while cursor < source_range.end && matches!(bytes[cursor], b' ' | b'\t') {
        cursor += 1;
    }

    (marker_start < cursor).then_some(marker_start..cursor)
}

fn structure_list_item_marker_start(source: &str, node_start: usize) -> usize {
    let bytes = source.as_bytes();
    let mut cursor = source[..node_start]
        .rfind('\n')
        .map_or(0, |offset| offset + 1);

    loop {
        let marker_start = cursor;
        let mut leading_spaces = 0;
        while cursor < node_start && bytes[cursor] == b' ' && leading_spaces < 4 {
            cursor += 1;
            leading_spaces += 1;
        }

        if cursor < node_start && bytes[cursor] == b'>' {
            cursor += 1;
            if cursor < node_start && bytes[cursor] == b' ' {
                cursor += 1;
            }
            continue;
        }

        return marker_start;
    }
}

fn structure_fenced_code_marker_ranges(node: Node<'_>) -> Vec<Range<usize>> {
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

fn structure_pipe_table_marker_ranges(node: Node<'_>) -> Vec<Range<usize>> {
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

fn structure_fenced_code_content_range(source: &str, node: Node<'_>) -> Range<usize> {
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

fn row_range_for_structure_node(node: Node<'_>) -> Range<usize> {
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
