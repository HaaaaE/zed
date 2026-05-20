use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkdownSyntaxTree {
    source_len: usize,
    line_starts: Vec<usize>,
    blocks: Vec<MarkdownBlock>,
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
pub struct MarkdownNodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownBlockKind {
    Blank,
    Paragraph,
    AtxHeading { level: u8 },
    FencedCodeBlock,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MarkdownProjectionMap {
    source_len: usize,
    visible_source_range: Range<usize>,
    hidden_ranges: Vec<Range<usize>>,
}

impl MarkdownSyntaxTree {
    pub fn parse(source: &str) -> Self {
        let line_starts = line_starts(source);
        let mut blocks = Vec::new();
        let mut row = 0;
        let mut next_id = 0;
        let mut fenced_code_start: Option<(usize, usize)> = None;

        while row < line_starts.len() {
            let source_range = line_range(source, &line_starts, row);
            let line = &source[source_range.clone()];
            let line_without_newline = line.trim_end_matches(['\r', '\n']);
            let trimmed_start = line_without_newline.len() - line_without_newline.trim_start().len();
            let content_start = source_range.start + trimmed_start;
            let trimmed = &source[content_start..source_range.start + line_without_newline.len()];

            if let Some((start_row, start_offset)) = fenced_code_start {
                if is_fence_marker(trimmed) {
                    let end_range = line_range(source, &line_starts, row);
                    blocks.push(MarkdownBlock {
                        id: MarkdownNodeId(next_id),
                        kind: MarkdownBlockKind::FencedCodeBlock,
                        source_range: start_offset..end_range.end,
                        content_range: start_offset..end_range.end,
                        marker_ranges: Vec::new(),
                        row_range: start_row..row + 1,
                    });
                    next_id += 1;
                    fenced_code_start = None;
                }
                row += 1;
                continue;
            }

            if trimmed.is_empty() {
                blocks.push(MarkdownBlock {
                    id: MarkdownNodeId(next_id),
                    kind: MarkdownBlockKind::Blank,
                    source_range: source_range.clone(),
                    content_range: source_range.start..source_range.start,
                    marker_ranges: Vec::new(),
                    row_range: row..row + 1,
                });
                next_id += 1;
                row += 1;
                continue;
            }

            if is_fence_marker(trimmed) {
                fenced_code_start = Some((row, source_range.start));
                row += 1;
                continue;
            }

            if let Some((level, marker_len)) = atx_heading_marker(trimmed) {
                let marker_start = content_start;
                let marker_end = marker_start + marker_len;
                blocks.push(MarkdownBlock {
                    id: MarkdownNodeId(next_id),
                    kind: MarkdownBlockKind::AtxHeading { level },
                    source_range: source_range.clone(),
                    content_range: marker_end..source_range.start + line_without_newline.len(),
                    marker_ranges: vec![marker_start..marker_end],
                    row_range: row..row + 1,
                });
                next_id += 1;
                row += 1;
                continue;
            }

            let paragraph_start_row = row;
            let paragraph_start_offset = source_range.start;
            let mut paragraph_end_offset = source_range.end;
            row += 1;

            while row < line_starts.len() {
                let next_range = line_range(source, &line_starts, row);
                let next_line = &source[next_range.clone()];
                let next_without_newline = next_line.trim_end_matches(['\r', '\n']);
                let next_trimmed_start = next_without_newline.len() - next_without_newline.trim_start().len();
                let next_content_start = next_range.start + next_trimmed_start;
                let next_trimmed =
                    &source[next_content_start..next_range.start + next_without_newline.len()];

                if next_trimmed.is_empty()
                    || atx_heading_marker(next_trimmed).is_some()
                    || is_fence_marker(next_trimmed)
                {
                    break;
                }

                paragraph_end_offset = next_range.end;
                row += 1;
            }

            blocks.push(MarkdownBlock {
                id: MarkdownNodeId(next_id),
                kind: MarkdownBlockKind::Paragraph,
                source_range: paragraph_start_offset..paragraph_end_offset,
                content_range: paragraph_start_offset..paragraph_end_offset,
                marker_ranges: Vec::new(),
                row_range: paragraph_start_row..row,
            });
            next_id += 1;
        }

        if let Some((start_row, start_offset)) = fenced_code_start {
            blocks.push(MarkdownBlock {
                id: MarkdownNodeId(next_id),
                kind: MarkdownBlockKind::FencedCodeBlock,
                source_range: start_offset..source.len(),
                content_range: start_offset..source.len(),
                marker_ranges: Vec::new(),
                row_range: start_row..line_starts.len(),
            });
        }

        Self {
            source_len: source.len(),
            line_starts,
            blocks,
        }
    }

    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub fn blocks(&self) -> &[MarkdownBlock] {
        &self.blocks
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
        let start = self.line_starts.get(rows.start).copied().unwrap_or(self.source_len);
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
        let mut hidden_ranges = Vec::new();
        for block in self.blocks_in_source_range(visible_source_range.clone()) {
            let is_active = active_source_range
                .as_ref()
                .is_some_and(|active| ranges_overlap(&block.source_range, active));

            if is_active {
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

        MarkdownProjectionMap::new(self.source_len, visible_source_range, hidden_ranges)
    }

    fn partition_blocks_by_end(&self, offset: usize) -> usize {
        self.blocks
            .partition_point(|block| block.source_range.end <= offset)
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
        let clipped_offset = source_offset.clamp(self.visible_source_range.start, self.visible_source_range.end);
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

fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    for (index, byte) in source.bytes().enumerate() {
        if byte == b'\n' && index + 1 < source.len() {
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

fn atx_heading_marker(line: &str) -> Option<(u8, usize)> {
    let mut level = 0;
    for byte in line.bytes() {
        if byte == b'#' {
            level += 1;
            if level > 6 {
                return None;
            }
        } else {
            break;
        }
    }

    if level == 0 {
        return None;
    }

    let next = line.as_bytes().get(level);
    if !matches!(next, Some(b' ' | b'\t')) {
        return None;
    }

    let mut marker_len = level;
    while matches!(line.as_bytes().get(marker_len), Some(b' ' | b'\t')) {
        marker_len += 1;
    }

    Some((level as u8, marker_len))
}

fn is_fence_marker(line: &str) -> bool {
    let bytes = line.as_bytes();
    let Some(marker) = bytes.first().copied() else {
        return false;
    };
    if marker != b'`' && marker != b'~' {
        return false;
    }

    bytes.iter().take_while(|byte| **byte == marker).count() >= 3
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_atx_headings_without_allocating_source_text() {
        let tree = MarkdownSyntaxTree::parse("# Title\n\nText\n");
        assert_eq!(tree.source_len(), 14);
        assert_eq!(tree.blocks().len(), 3);
        assert_eq!(tree.blocks()[0].kind, MarkdownBlockKind::AtxHeading { level: 1 });
        assert_eq!(tree.blocks()[0].source_range, 0..8);
        assert_eq!(tree.blocks()[0].content_range, 2..7);
        assert_eq!(tree.blocks()[0].marker_ranges, vec![0..2]);
        assert_eq!(tree.blocks()[1].kind, MarkdownBlockKind::Blank);
        assert_eq!(tree.blocks()[2].kind, MarkdownBlockKind::Paragraph);
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
        assert_eq!(tree.blocks()[1].kind, MarkdownBlockKind::AtxHeading { level: 1 });
    }
}
