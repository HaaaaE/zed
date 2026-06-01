use std::ops::Range;

use tree_sitter::{InputEdit, Node, Point};

use super::MarkdownNodeId;

pub(super) fn edit_byte_range(range: Range<usize>, edit: &InputEdit) -> Range<usize> {
    let start = edit.start_byte;
    let old_end = edit.old_end_byte;
    let new_end = edit.new_end_byte;

    if range.end <= start {
        return range;
    }

    if range.start >= old_end {
        return shift_byte_range(range, old_end, new_end);
    }

    let edited_start = range.start.min(start);
    let edited_end = if range.end >= old_end {
        shift_byte_offset(range.end, old_end, new_end)
    } else {
        new_end
    };
    edited_start..edited_end
}

fn shift_byte_range(range: Range<usize>, old_end: usize, new_end: usize) -> Range<usize> {
    shift_byte_offset(range.start, old_end, new_end)..shift_byte_offset(range.end, old_end, new_end)
}

fn shift_byte_offset(offset: usize, old_end: usize, new_end: usize) -> usize {
    if new_end >= old_end {
        offset + (new_end - old_end)
    } else {
        offset.saturating_sub(old_end - new_end)
    }
}

pub(super) fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' {
            starts.push(index + 1);
        }
    }
    starts
}

pub(super) fn line_starts_after_edit_range(
    old_line_starts: &[usize],
    old_range: Range<usize>,
    new_range: Range<usize>,
    new_source: &str,
) -> Vec<usize> {
    let byte_delta = new_range.len() as isize - old_range.len() as isize;
    let mut starts = Vec::with_capacity(
        old_line_starts.len().saturating_add(
            new_source[new_range.clone()]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count(),
        ),
    );

    starts.extend(
        old_line_starts
            .iter()
            .copied()
            .take_while(|line_start| *line_start <= old_range.start),
    );
    starts.extend(
        new_source[new_range.clone()]
            .bytes()
            .enumerate()
            .filter_map(|(index, byte)| (byte == b'\n').then_some(new_range.start + index + 1)),
    );
    starts.extend(
        old_line_starts
            .iter()
            .copied()
            .filter(|line_start| *line_start > old_range.end)
            .map(|line_start| shift_offset(line_start, byte_delta)),
    );
    starts.dedup();
    starts
}

fn shift_offset(offset: usize, byte_delta: isize) -> usize {
    if byte_delta >= 0 {
        offset + byte_delta as usize
    } else {
        offset - byte_delta.unsigned_abs()
    }
}

pub(super) fn line_range(source: &str, line_starts: &[usize], row: usize) -> Range<usize> {
    let start = line_starts[row];
    let end = line_starts.get(row + 1).copied().unwrap_or(source.len());
    start..end
}

pub(super) fn line_range_checked(
    source: &str,
    line_starts: &[usize],
    row: usize,
) -> Option<Range<usize>> {
    let start = line_starts.get(row).copied()?;
    let end = line_starts.get(row + 1).copied().unwrap_or(source.len());
    Some(start..end)
}

pub(super) fn trim_line_end(source: &str, mut range: Range<usize>) -> Range<usize> {
    while range.end > range.start && matches!(source.as_bytes()[range.end - 1], b'\r' | b'\n') {
        range.end -= 1;
    }
    range
}

pub(super) fn last_line_range(source: &str, range: Range<usize>) -> Option<Range<usize>> {
    let trimmed = trim_line_end(source, range.clone());
    if trimmed.is_empty() {
        return None;
    }

    let start = source[range.start..trimmed.end]
        .rfind('\n')
        .map(|index| range.start + index + 1)
        .unwrap_or(range.start);
    Some(start..trimmed.end)
}

pub(super) fn trim_ascii_whitespace(source: &str, mut range: Range<usize>) -> Range<usize> {
    while range.start < range.end && matches!(source.as_bytes()[range.start], b' ' | b'\t') {
        range.start += 1;
    }
    while range.end > range.start && matches!(source.as_bytes()[range.end - 1], b' ' | b'\t') {
        range.end -= 1;
    }
    range
}

pub(super) fn point_for_offset(line_starts: &[usize], offset: usize) -> Point {
    let row = line_starts.partition_point(|line_start| *line_start <= offset) - 1;
    Point {
        row,
        column: offset - line_starts[row],
    }
}

pub(super) fn node_id(node: Node<'_>) -> MarkdownNodeId {
    MarkdownNodeId(node.id() as u64)
}

pub(super) fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

pub(super) fn range_contains(container: &Range<usize>, candidate: &Range<usize>) -> bool {
    container.start <= candidate.start && container.end >= candidate.end
}

pub(super) fn ranges_touch(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start <= right.end && right.start <= left.end
}

pub(super) fn old_range_for_clean_new_range(
    new_parent_range: &Range<usize>,
    new_range: &Range<usize>,
    old_range: &Range<usize>,
) -> Range<usize> {
    if new_parent_range.end <= new_range.start {
        return new_parent_range.clone();
    }

    debug_assert!(new_parent_range.start >= new_range.end);
    shift_byte_range(new_parent_range.clone(), new_range.end, old_range.end)
}

pub(super) fn shift_clean_old_range_to_new(
    range: Range<usize>,
    old_range: &Range<usize>,
    new_range: &Range<usize>,
) -> Range<usize> {
    if range.end <= old_range.start {
        return range;
    }

    debug_assert!(range.start >= old_range.end);
    shift_byte_range(range, old_range.end, new_range.end)
}
