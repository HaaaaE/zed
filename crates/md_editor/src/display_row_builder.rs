#[cfg(test)]
use std::sync::Arc;
use std::{ops::Range, path::Path};

use markdown_wysiwyg::{
    MarkdownBlock, MarkdownBlockKind, MarkdownInlineKind, MarkdownInlineSpan,
    MarkdownProjectionMap, MarkdownRangeSemantics,
};
use md_buffer::BufferSnapshot;
#[cfg(test)]
use md_text::Selection;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point};

#[cfg(test)]
use crate::rendered_projection_state;
use crate::{
    MarkdownEditorMode,
    display_model::{
        DisplayInsertion, DisplayRow, RenderedAdornment, RenderedAdornmentKind,
        RenderedAdornmentPlacement, RenderedContainerKind, RenderedItemPresentation,
        RenderedSpacing,
    },
    inline_atom::INLINE_IMAGE_PLACEHOLDER,
    range_contains, ranges_overlap,
    rendered_element::{
        RenderedElementDescriptor, RenderedElementPlacement,
        rendered_element_descriptor_for_inline_span_in_row,
    },
};
use md_projection::{self, DisplayItemId};
#[cfg(test)]
use md_projection::{RenderedDisplayIndex, RenderedTopology};

pub fn display_rows(snapshot: &BufferSnapshot, range: Range<usize>) -> Vec<DisplayRow> {
    display_rows_in_text_snapshot(snapshot.as_text_snapshot(), range)
}

pub(crate) fn display_rows_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    range: Range<usize>,
) -> Vec<DisplayRow> {
    let row_count = snapshot.row_count() as usize;
    let start = range.start.min(row_count);
    let end = range.end.min(row_count);
    (start..end)
        .map(|row| source_display_row_in_text_snapshot(snapshot, row as u32))
        .collect()
}

#[cfg(test)]
pub(crate) fn display_rows_in_mode(
    snapshot: &BufferSnapshot,
    range: Range<usize>,
    selection: Option<&Selection<Point>>,
    mode: MarkdownEditorMode,
) -> Vec<DisplayRow> {
    if mode == MarkdownEditorMode::Source {
        return display_rows_in_text_snapshot(snapshot.as_text_snapshot(), range);
    }

    let row_count = snapshot.row_count() as usize;
    let start = range.start.min(row_count);
    let end = range.end.min(row_count);
    let display_row_state = rendered_projection_state(snapshot, selection, mode);
    (start..end)
        .map(|row| {
            let row = row as u32;
            let source_range = row_source_range(snapshot, row);
            let range_semantics = snapshot.syntax_tree().range_semantics_for_source_range(
                source_range.clone(),
                display_row_state.active_source_range.clone(),
                &display_row_state.inactive_source_ranges,
            );
            rendered_display_row(
                snapshot,
                md_projection::source_display_item_id(&source_range, row as usize),
                row,
                row,
                source_range,
                row as usize..row as usize + 1,
                range_semantics,
                None,
            )
        })
        .collect()
}

#[cfg(test)]
pub(crate) fn rendered_display_index_for_tests(
    snapshot: &BufferSnapshot,
) -> Arc<RenderedDisplayIndex> {
    RenderedDisplayIndex::build(snapshot)
}

#[cfg(test)]
pub(crate) fn rendered_display_row_for_item_for_tests(
    snapshot: &BufferSnapshot,
    item_index: usize,
    selection: Option<&Selection<Point>>,
) -> DisplayRow {
    let index = RenderedDisplayIndex::build(snapshot);
    let item = index.item(item_index).expect("item should exist");
    let display_row_state =
        rendered_projection_state(snapshot, selection, MarkdownEditorMode::Rendered);
    let topology = RenderedTopology::new(snapshot, index.clone());
    let display_source_range = topology.item_display_source_range(item, &display_row_state);
    let range_semantics = snapshot.syntax_tree().range_semantics_for_source_range(
        display_source_range.source_range.clone(),
        display_row_state.active_source_range.clone(),
        &display_row_state.inactive_source_ranges,
    );

    rendered_display_row(
        snapshot,
        item.id,
        item.index as u32,
        display_source_range.row,
        display_source_range.source_range,
        display_source_range.source_row_range,
        range_semantics,
        None,
    )
}

pub(crate) fn rendered_display_row(
    snapshot: &BufferSnapshot,
    item_id: DisplayItemId,
    item_index: u32,
    row: u32,
    source_range: Range<usize>,
    source_row_range: Range<usize>,
    range_semantics: MarkdownRangeSemantics,
    document_path: Option<&Path>,
) -> DisplayRow {
    let source_text: String = snapshot
        .as_text_snapshot()
        .text_for_range(source_range.clone())
        .collect();
    let MarkdownRangeSemantics {
        blocks: markdown_blocks,
        inline_spans,
        projection,
        active_projection_source_ranges,
        rendered_element_candidates,
    } = range_semantics;

    let rendered_element_descriptors = rendered_element_descriptors_for_display_row(
        &rendered_element_candidates,
        &source_text,
        &source_range,
        document_path,
    );
    let (text, insertions) = project_display_row_text(
        &source_text,
        &source_range,
        &projection,
        &inline_spans,
        &rendered_element_descriptors,
        MarkdownEditorMode::Rendered,
    );
    let heading_level = heading_level_for_display_row(&markdown_blocks, row);
    let rendered_indent_level = rendered_indent_level_for_display_row(&markdown_blocks, row);
    let presentation = rendered_item_presentation_for_display_row(
        &markdown_blocks,
        rendered_indent_level,
        &source_range,
        row,
    );
    let adornments =
        rendered_adornments_for_display_row(&markdown_blocks, &source_text, &source_range, row);
    DisplayRow {
        item_id,
        item_index,
        row,
        source_row_range,
        text,
        source_text,
        source_range,
        active_projection_source_ranges,
        markdown_blocks,
        heading_level,
        rendered_indent_level,
        presentation,
        adornments,
        inline_spans,
        rendered_element_descriptors,
        rendered_element_descriptors_have_document_path: document_path.is_some(),
        projection,
        insertions,
    }
}

pub fn row_text(snapshot: &BufferSnapshot, row: u32) -> String {
    row_text_in_text_snapshot(snapshot.as_text_snapshot(), row)
}

pub(crate) fn row_text_in_text_snapshot(snapshot: &TextBufferSnapshot, row: u32) -> String {
    if row >= snapshot.row_count() {
        return String::new();
    }

    let start = snapshot.point_to_offset(Point::new(row, 0));
    let end = start + snapshot.line_len(row) as usize;
    snapshot.text_for_range(start..end).collect()
}

pub(crate) fn source_display_row_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    row: u32,
) -> DisplayRow {
    let source_range = row_source_range_in_text_snapshot(snapshot, row);
    let source_text: String = snapshot.text_for_range(source_range.clone()).collect();
    let projection = MarkdownProjectionMap::new(snapshot.len(), source_range.clone(), Vec::new());
    DisplayRow {
        item_id: md_projection::source_display_item_id(&source_range, row as usize),
        item_index: row,
        row,
        source_row_range: row as usize..row as usize + 1,
        text: source_text.clone(),
        source_text,
        source_range,
        active_projection_source_ranges: Vec::new(),
        markdown_blocks: Vec::new(),
        heading_level: None,
        rendered_indent_level: 0,
        presentation: RenderedItemPresentation::default(),
        adornments: Vec::new(),
        inline_spans: Vec::new(),
        rendered_element_descriptors: Vec::new(),
        rendered_element_descriptors_have_document_path: false,
        projection,
        insertions: Vec::new(),
    }
}

fn rendered_item_presentation_for_display_row(
    markdown_blocks: &[MarkdownBlock],
    blockquote_depth: u16,
    source_range: &Range<usize>,
    row: u32,
) -> RenderedItemPresentation {
    let spacing = rendered_spacing_for_display_row(markdown_blocks, row);
    let code_block = markdown_blocks.iter().find(|block| {
        matches!(
            block.kind,
            MarkdownBlockKind::FencedCodeBlock | MarkdownBlockKind::IndentedCodeBlock
        ) && ranges_overlap(&block.content_range, source_range)
    });
    if let Some(block) = code_block {
        let row = row as usize;
        let first_content_row = match block.kind {
            MarkdownBlockKind::FencedCodeBlock => block.row_range.start.saturating_add(1),
            MarkdownBlockKind::IndentedCodeBlock => block.row_range.start,
            _ => block.row_range.start,
        };
        let last_content_row = match block.kind {
            MarkdownBlockKind::FencedCodeBlock => block.row_range.end.saturating_sub(2),
            MarkdownBlockKind::IndentedCodeBlock => block.row_range.end.saturating_sub(1),
            _ => block.row_range.end.saturating_sub(1),
        };
        return RenderedItemPresentation {
            before_spacing: spacing.0,
            after_spacing: spacing.1,
            content_padding: crate::display_model::RenderedPadding {
                top: if row == first_content_row { 3 } else { 0 },
                right: 12,
                bottom: if row == last_content_row { 3 } else { 0 },
                left: 12,
            },
            background: Some(crate::display_model::RenderedBackgroundKind::CodeBlock),
            container: Some(RenderedContainerKind::CodeBlock),
            ..Default::default()
        };
    }

    if blockquote_depth > 0 {
        return RenderedItemPresentation {
            before_spacing: spacing.0,
            after_spacing: spacing.1,
            background: Some(crate::display_model::RenderedBackgroundKind::BlockQuote),
            container: Some(RenderedContainerKind::BlockQuote {
                depth: blockquote_depth,
            }),
            ..Default::default()
        };
    }

    RenderedItemPresentation {
        before_spacing: spacing.0,
        after_spacing: spacing.1,
        ..Default::default()
    }
}

fn rendered_spacing_for_display_row(
    markdown_blocks: &[MarkdownBlock],
    row: u32,
) -> (RenderedSpacing, RenderedSpacing) {
    let row = row as usize;
    if let Some(level) = markdown_blocks.iter().find_map(|block| match block.kind {
        MarkdownBlockKind::AtxHeading { level } | MarkdownBlockKind::SetextHeading { level }
            if block.row_range.contains(&row) =>
        {
            Some(level)
        }
        _ => None,
    }) {
        let (before, after) = match level {
            1 => (10, 6),
            2 => (8, 4),
            3 => (6, 3),
            _ => (4, 2),
        };
        return (
            RenderedSpacing { px: before },
            RenderedSpacing { px: after },
        );
    }

    if markdown_blocks.iter().any(|block| {
        matches!(
            block.kind,
            MarkdownBlockKind::ListItem | MarkdownBlockKind::TaskListItem { .. }
        ) && block.row_range.end.saturating_sub(1) == row
    }) {
        return (RenderedSpacing::default(), RenderedSpacing { px: 2 });
    }

    if let Some(blockquote) = markdown_blocks.iter().find(|block| {
        matches!(block.kind, MarkdownBlockKind::BlockQuote) && block.row_range.contains(&row)
    }) {
        let before = if blockquote.row_range.start == row {
            6
        } else {
            0
        };
        let after = if blockquote.row_range.end.saturating_sub(1) == row {
            6
        } else {
            0
        };
        return (
            RenderedSpacing { px: before },
            RenderedSpacing { px: after },
        );
    }

    if markdown_blocks.iter().any(|block| {
        matches!(block.kind, MarkdownBlockKind::Paragraph)
            && block.row_range.end.saturating_sub(1) == row
    }) {
        return (RenderedSpacing::default(), RenderedSpacing { px: 6 });
    }

    (RenderedSpacing::default(), RenderedSpacing::default())
}

fn rendered_adornments_for_display_row(
    markdown_blocks: &[MarkdownBlock],
    source_text: &str,
    source_range: &Range<usize>,
    row: u32,
) -> Vec<RenderedAdornment> {
    let mut adornments = Vec::new();
    let mut quote_depth = 0;
    for block in markdown_blocks {
        if !block.row_range.contains(&(row as usize)) {
            continue;
        }

        match block.kind {
            MarkdownBlockKind::BlockQuote => {
                quote_depth += 1;
                let source_range = block
                    .marker_ranges
                    .iter()
                    .find(|marker_range| ranges_overlap(marker_range, source_range))
                    .cloned();
                adornments.push(RenderedAdornment {
                    kind: RenderedAdornmentKind::QuoteBar { depth: quote_depth },
                    source_range,
                    row_range: row as usize..row as usize + 1,
                    placement: RenderedAdornmentPlacement::BlockEdge,
                });
            }
            MarkdownBlockKind::ListItem => {
                if let Some((marker_range, marker_text)) =
                    list_marker_for_block(block, source_text, source_range)
                {
                    let marker_text = marker_text.trim().to_string();
                    let kind = if marker_text
                        .as_bytes()
                        .first()
                        .is_some_and(u8::is_ascii_digit)
                    {
                        RenderedAdornmentKind::OrderedMarker { text: marker_text }
                    } else {
                        RenderedAdornmentKind::ListBullet
                    };
                    adornments.push(RenderedAdornment {
                        kind,
                        source_range: Some(marker_range),
                        row_range: row as usize..row as usize + 1,
                        placement: RenderedAdornmentPlacement::Leading,
                    });
                }
            }
            MarkdownBlockKind::TaskListItem { checked } => {
                if let Some((marker_range, marker_text)) =
                    list_marker_for_block(block, source_text, source_range)
                {
                    let task_marker_range = task_marker_source_range(&marker_range, &marker_text)
                        .unwrap_or(marker_range);
                    adornments.push(RenderedAdornment {
                        kind: RenderedAdornmentKind::TaskCheckbox { checked },
                        source_range: Some(task_marker_range),
                        row_range: row as usize..row as usize + 1,
                        placement: RenderedAdornmentPlacement::Leading,
                    });
                }
            }
            _ => {}
        }
    }
    adornments
}

fn list_marker_for_block(
    block: &MarkdownBlock,
    source_text: &str,
    source_range: &Range<usize>,
) -> Option<(Range<usize>, String)> {
    let marker_range = block
        .marker_ranges
        .iter()
        .find(|marker_range| ranges_overlap(marker_range, source_range))?
        .clone();
    let local_start = marker_range.start.checked_sub(source_range.start)?;
    let local_end = marker_range.end.checked_sub(source_range.start)?;
    Some((
        marker_range,
        source_text.get(local_start..local_end)?.to_string(),
    ))
}

fn task_marker_source_range(
    marker_range: &Range<usize>,
    marker_text: &str,
) -> Option<Range<usize>> {
    let marker_start = marker_text
        .find("[ ]")
        .or_else(|| marker_text.find("[x]").or_else(|| marker_text.find("[X]")))?;
    Some(marker_range.start + marker_start..marker_range.start + marker_start + 3)
}

fn rendered_indent_level_for_display_row(markdown_blocks: &[MarkdownBlock], row: u32) -> u16 {
    markdown_blocks
        .iter()
        .filter(|block| {
            matches!(
                block.kind,
                MarkdownBlockKind::BlockQuote
                    | MarkdownBlockKind::ListItem
                    | MarkdownBlockKind::TaskListItem { .. }
            ) && block.row_range.contains(&(row as usize))
        })
        .count()
        .try_into()
        .unwrap_or(u16::MAX)
}

fn heading_level_for_display_row(markdown_blocks: &[MarkdownBlock], row: u32) -> Option<u8> {
    markdown_blocks.iter().find_map(|block| match block.kind {
        MarkdownBlockKind::AtxHeading { level } | MarkdownBlockKind::SetextHeading { level }
            if block.row_range.start == row as usize =>
        {
            Some(level)
        }
        _ => None,
    })
}

fn rendered_element_descriptors_for_display_row(
    rendered_element_candidates: &[MarkdownInlineSpan],
    source_text: &str,
    row_source_range: &Range<usize>,
    document_path: Option<&Path>,
) -> Vec<RenderedElementDescriptor> {
    rendered_element_candidates
        .iter()
        .filter_map(|span| {
            rendered_element_descriptor_for_inline_span_in_row(
                span,
                source_text,
                row_source_range,
                document_path,
            )
        })
        .collect()
}

pub(crate) fn row_source_range(snapshot: &BufferSnapshot, row: u32) -> Range<usize> {
    row_source_range_in_text_snapshot(snapshot.as_text_snapshot(), row)
}

pub(crate) fn row_source_range_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    row: u32,
) -> Range<usize> {
    if row >= snapshot.row_count() {
        let end = snapshot.len();
        return end..end;
    }

    let start = snapshot.point_to_offset(Point::new(row, 0));
    let end = start + snapshot.line_len(row) as usize;
    start..end
}

fn project_display_row_text(
    source_text: &str,
    row_source_range: &Range<usize>,
    projection: &MarkdownProjectionMap,
    inline_spans: &[MarkdownInlineSpan],
    rendered_element_descriptors: &[RenderedElementDescriptor],
    mode: MarkdownEditorMode,
) -> (String, Vec<DisplayInsertion>) {
    if mode != MarkdownEditorMode::Rendered {
        return (project_row_text(source_text, projection), Vec::new());
    }

    let mut display_text = project_rendered_row_text(source_text, row_source_range, projection);
    restore_rendered_soft_breaks(
        &mut display_text,
        row_source_range,
        inline_spans,
        projection,
    );
    restore_rendered_trailing_soft_break(&mut display_text, source_text);
    let mut insertions = Vec::new();
    for span in inline_spans {
        let descriptor = rendered_element_descriptors
            .iter()
            .find(|descriptor| descriptor.source_range == span.source_range);
        if span.kind != MarkdownInlineKind::Image
            || descriptor
                .is_some_and(|descriptor| descriptor.placement == RenderedElementPlacement::Block)
            || !range_contains(&row_source_range, &span.source_range)
            || !span.marker_ranges.iter().any(|marker_range| {
                projection
                    .hidden_ranges()
                    .iter()
                    .any(|hidden_range| ranges_overlap(marker_range, hidden_range))
            })
        {
            continue;
        }

        let display_start = projection.source_to_display(span.source_range.start);
        let display_end = projection.source_to_display(span.source_range.end);
        if display_start != display_end {
            continue;
        }

        let inserted_len = insertions
            .iter()
            .filter(|insertion: &&DisplayInsertion| {
                span.source_range.start > insertion.source_range.start
            })
            .map(|insertion| insertion.display_range.len())
            .sum::<usize>();
        let display_start = display_start + inserted_len;
        display_text.insert_str(display_start, INLINE_IMAGE_PLACEHOLDER);
        insertions.push(DisplayInsertion {
            source_range: span.source_range.clone(),
            display_range: display_start..display_start + INLINE_IMAGE_PLACEHOLDER.len(),
        });
    }

    (display_text, insertions)
}

fn restore_rendered_trailing_soft_break(display_text: &mut String, source_text: &str) {
    let trailing_line_break_count = trailing_line_break_count(source_text);
    if trailing_line_break_count == 0 {
        return;
    }

    let trailing_spaces = " ".repeat(trailing_line_break_count);
    if !display_text.ends_with(&trailing_spaces) {
        return;
    }

    let trailing_space_start = display_text.len().saturating_sub(trailing_line_break_count);
    display_text.replace_range(
        trailing_space_start..,
        &"\n".repeat(trailing_line_break_count),
    );
}

fn trailing_line_break_count(text: &str) -> usize {
    let mut count = 0;
    let mut end = text.len();
    while end > 0 {
        if text[..end].ends_with("\r\n") {
            count += 1;
            end = end.saturating_sub("\r\n".len());
        } else if text[..end].ends_with(['\n', '\r']) {
            count += 1;
            end = end.saturating_sub('\n'.len_utf8());
        } else {
            break;
        }
    }
    count
}

fn restore_rendered_soft_breaks(
    display_text: &mut String,
    row_source_range: &Range<usize>,
    inline_spans: &[MarkdownInlineSpan],
    projection: &MarkdownProjectionMap,
) {
    let mut replacements = inline_spans
        .iter()
        .filter(|span| {
            span.kind == MarkdownInlineKind::SoftBreak
                && range_contains(row_source_range, &span.source_range)
        })
        .filter_map(|span| {
            let display_start = projection.source_to_display(span.source_range.start);
            let display_end = projection.source_to_display(span.source_range.end);
            (display_start < display_end && display_end <= display_text.len())
                .then_some(display_start..display_end)
        })
        .collect::<Vec<_>>();

    replacements.sort_by_key(|range| range.start);
    for range in replacements.into_iter().rev() {
        display_text.replace_range(range, "\n");
    }
}

fn project_row_text(source_text: &str, projection: &MarkdownProjectionMap) -> String {
    projection.project_source_text(source_text)
}

fn project_rendered_row_text(
    source_text: &str,
    row_source_range: &Range<usize>,
    projection: &MarkdownProjectionMap,
) -> String {
    let mut rendered_text = String::new();
    let mut cursor = row_source_range.start;
    for operation in projection.operations() {
        let operation_range = operation.source_range();
        let start = operation_range.start.max(row_source_range.start);
        let end = operation_range.end.min(row_source_range.end);
        if start >= end {
            continue;
        }

        if cursor < start {
            push_rendered_source_text_chunk(
                &mut rendered_text,
                source_text,
                row_source_range,
                cursor..start,
            );
        }
        if let markdown_wysiwyg::MarkdownProjectionOperation::Replace { display_text, .. } =
            operation
        {
            rendered_text.push_str(display_text);
        }
        cursor = cursor.max(end);
    }

    if cursor < row_source_range.end {
        push_rendered_source_text_chunk(
            &mut rendered_text,
            source_text,
            row_source_range,
            cursor..row_source_range.end,
        );
    }

    rendered_text
}

fn push_rendered_source_text_chunk(
    rendered_text: &mut String,
    source_text: &str,
    row_source_range: &Range<usize>,
    source_range: Range<usize>,
) {
    let local_start = source_range.start - row_source_range.start;
    let local_end = source_range.end - row_source_range.start;
    if let Some(text) = source_text.get(local_start..local_end) {
        rendered_text.push_str(
            text.replace("\r\n", " ")
                .replace(['\n', '\r'], " ")
                .as_str(),
        );
    }
}
