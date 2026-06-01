use std::ops::Range;

use super::{
    MarkdownBlock, MarkdownBlockKind, MarkdownSyntaxTree, MarkdownTable, MarkdownTableAlignment,
    MarkdownTableCell, MarkdownTableRow,
    source::{line_range_checked, ranges_overlap, trim_ascii_whitespace, trim_line_end},
    structure::MarkdownStructure,
};

#[cfg(any(test, perf_enabled))]
use super::structure::MarkdownStructureBlock;

impl MarkdownSyntaxTree {
    pub fn tables(&self) -> &[MarkdownTable] {
        &self.data.tables
    }

    pub fn table_for_source_row(&self, row: usize) -> Option<&MarkdownTable> {
        self.data
            .tables
            .iter()
            .find(|table| table.row_range.contains(&row))
    }

    pub fn table_for_source_range(&self, range: Range<usize>) -> Option<&MarkdownTable> {
        self.data
            .tables
            .iter()
            .find(|table| ranges_overlap(&table.source_range, &range))
    }

    pub fn table_row_for_source_row(
        &self,
        row: usize,
    ) -> Option<(&MarkdownTable, &MarkdownTableRow)> {
        let table = self.table_for_source_row(row)?;
        table
            .rows()
            .find(|table_row| table_row.row == row)
            .map(|table_row| (table, table_row))
    }
}

pub(super) fn collect_structure_tables(
    source: &str,
    line_starts: &[usize],
    structure: &MarkdownStructure,
) -> Vec<MarkdownTable> {
    structure
        .blocks()
        .iter()
        .filter(|block| block.kind == MarkdownBlockKind::PipeTable)
        .map(MarkdownBlock::from_structure)
        .filter_map(|block| table_from_block(source, line_starts, &block))
        .collect()
}

#[cfg(any(test, perf_enabled))]
pub(super) fn table_cell_content_ranges_for_blocks(
    source: &str,
    line_starts: &[usize],
    blocks: &[MarkdownStructureBlock],
) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for block in blocks
        .iter()
        .filter(|block| block.kind == MarkdownBlockKind::PipeTable)
    {
        for (index, row) in block.row_range.clone().enumerate() {
            if index == 1 {
                continue;
            }

            let Some(row) = table_row_from_source_row(source, line_starts, row, false) else {
                continue;
            };
            ranges.extend(
                row.cells
                    .into_iter()
                    .map(|cell| cell.content_range)
                    .filter(|range| !range.is_empty()),
            );
        }
    }
    ranges
}

fn table_from_block(
    source: &str,
    line_starts: &[usize],
    block: &MarkdownBlock,
) -> Option<MarkdownTable> {
    if block.row_range.len() < 2 {
        return None;
    }

    let mut rows = block
        .row_range
        .clone()
        .enumerate()
        .map(|(index, row)| table_row_from_source_row(source, line_starts, row, index == 1))
        .collect::<Option<Vec<_>>>()?;
    if rows.len() < 2 {
        return None;
    }

    let header = rows.remove(0);
    let delimiter = rows.remove(0);
    if delimiter.delimiter_marker_ranges.is_empty() {
        return None;
    }
    let alignments = delimiter
        .cells
        .iter()
        .map(|cell| alignment_for_delimiter_cell(source, cell.content_range.clone()))
        .collect::<Vec<_>>();
    let pipe_marker_ranges = std::iter::once(&header)
        .chain(std::iter::once(&delimiter))
        .chain(rows.iter())
        .flat_map(|row| row.pipe_marker_ranges.iter().cloned())
        .collect();
    let delimiter_marker_ranges = delimiter.delimiter_marker_ranges.clone();

    Some(MarkdownTable {
        id: block.id,
        source_range: block.source_range.clone(),
        row_range: block.row_range.clone(),
        header,
        delimiter,
        body: rows,
        alignments,
        pipe_marker_ranges,
        delimiter_marker_ranges,
    })
}

fn table_row_from_source_row(
    source: &str,
    line_starts: &[usize],
    row: usize,
    is_delimiter_row: bool,
) -> Option<MarkdownTableRow> {
    let source_range = trim_line_end(source, line_range_checked(source, line_starts, row)?);
    let line = source.get(source_range.clone())?;
    let pipe_offsets = line
        .match_indices('|')
        .map(|(offset, _)| source_range.start + offset)
        .collect::<Vec<_>>();
    let pipe_marker_ranges = pipe_offsets
        .iter()
        .map(|offset| *offset..*offset + 1)
        .collect::<Vec<_>>();
    let cells = table_cells_for_line(source, source_range.clone(), &pipe_offsets);
    let delimiter_marker_ranges = if is_delimiter_row {
        cells
            .iter()
            .map(|cell| cell.content_range.clone())
            .filter(|range| source.get(range.clone()).is_some_and(is_delimiter_cell))
            .collect()
    } else {
        Vec::new()
    };

    Some(MarkdownTableRow {
        source_range,
        row,
        cells,
        pipe_marker_ranges,
        delimiter_marker_ranges,
    })
}

fn table_cells_for_line(
    source: &str,
    line_range: Range<usize>,
    pipe_offsets: &[usize],
) -> Vec<MarkdownTableCell> {
    let leading_pipe = pipe_offsets
        .first()
        .is_some_and(|pipe| source[line_range.start..*pipe].trim().is_empty());
    let trailing_pipe = pipe_offsets
        .last()
        .is_some_and(|pipe| source[*pipe + 1..line_range.end].trim().is_empty());

    let mut cells = Vec::new();
    let mut cell_start = if leading_pipe {
        pipe_offsets[0] + 1
    } else {
        line_range.start
    };

    let first_separator = usize::from(leading_pipe);
    let last_separator = pipe_offsets
        .len()
        .saturating_sub(usize::from(trailing_pipe));
    for pipe in &pipe_offsets[first_separator..last_separator] {
        cells.push(table_cell(source, cell_start..*pipe));
        cell_start = *pipe + 1;
    }

    let cell_end = if trailing_pipe {
        *pipe_offsets.last().unwrap_or(&line_range.end)
    } else {
        line_range.end
    };
    if cell_start <= cell_end {
        cells.push(table_cell(source, cell_start..cell_end));
    }

    cells
}

fn table_cell(source: &str, source_range: Range<usize>) -> MarkdownTableCell {
    MarkdownTableCell {
        content_range: trim_ascii_whitespace(source, source_range.clone()),
        source_range,
    }
}

fn alignment_for_delimiter_cell(
    source: &str,
    content_range: Range<usize>,
) -> MarkdownTableAlignment {
    let Some(delimiter) = source.get(content_range) else {
        return MarkdownTableAlignment::Left;
    };
    let delimiter = delimiter.trim();
    match (delimiter.starts_with(':'), delimiter.ends_with(':')) {
        (true, true) => MarkdownTableAlignment::Center,
        (false, true) => MarkdownTableAlignment::Right,
        _ => MarkdownTableAlignment::Left,
    }
}

fn is_delimiter_cell(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| matches!(byte, b':' | b'-' | b' ' | b'\t'))
        && text.bytes().any(|byte| byte == b'-')
}
