use std::ops::Range;

use super::{
    MarkdownBlock, MarkdownBlockKind, MarkdownInlineKind, MarkdownInlineSpan,
    MarkdownProjectionMap, MarkdownProjectionOperation, MarkdownProjectionReplacement,
    MarkdownRangeSemantics, MarkdownSyntaxTree,
    source::{range_contains, ranges_overlap},
};

impl MarkdownSyntaxTree {
    pub fn source_range_for_rows(&self, rows: Range<usize>) -> Range<usize> {
        let start = self
            .data
            .line_starts
            .get(rows.start)
            .copied()
            .unwrap_or(self.data.source_len);
        let end = self
            .data
            .line_starts
            .get(rows.end)
            .copied()
            .unwrap_or(self.data.source_len);
        start..end
    }

    fn projection_replacements_in_source_range(
        &self,
        range: Range<usize>,
    ) -> impl Iterator<Item = &MarkdownProjectionReplacement> {
        let start = range.start;
        let end = range.end;
        let start_index = self.partition_projection_replacements_by_prefix_end(start);
        self.data.projection_replacements[start_index..]
            .iter()
            .take_while(move |replacement| replacement.source_range.start < end)
            .filter(move |replacement| {
                replacement.source_range.start < end && replacement.source_range.end > start
            })
    }

    pub fn range_semantics_for_visible_rows(
        &self,
        rows: Range<usize>,
        active_source_range: Option<Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
    ) -> MarkdownRangeSemantics {
        self.range_semantics_for_source_range(
            self.source_range_for_rows(rows),
            active_source_range,
            inactive_source_ranges,
        )
    }

    pub fn range_semantics_for_source_range(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
    ) -> MarkdownRangeSemantics {
        let blocks = self
            .blocks_in_source_range(visible_source_range.clone())
            .cloned()
            .collect::<Vec<_>>();
        let inline_spans = self
            .inline_spans_in_source_range(visible_source_range.clone())
            .cloned()
            .collect::<Vec<_>>();
        let projection = self.projection_for_source_range_with_semantics(
            visible_source_range.clone(),
            active_source_range.as_ref(),
            inactive_source_ranges,
            blocks.iter(),
            inline_spans.iter(),
        );
        let active_projection_source_ranges = self
            .active_projection_source_ranges_for_source_range(
                visible_source_range,
                active_source_range,
                inactive_source_ranges,
            );
        let rendered_element_candidates = inline_spans
            .iter()
            .filter(|span| span.kind.is_rendered_element_candidate())
            .cloned()
            .collect();

        MarkdownRangeSemantics {
            blocks,
            inline_spans,
            projection,
            active_projection_source_ranges,
            rendered_element_candidates,
        }
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
        let blocks = self.blocks_in_source_range(visible_source_range.clone());
        let inline_spans = self.inline_spans_in_source_range(visible_source_range.clone());
        self.projection_for_source_range_with_semantics(
            visible_source_range,
            active_source_range.as_ref(),
            inactive_source_ranges,
            blocks,
            inline_spans,
        )
    }

    fn projection_for_source_range_with_semantics<'a>(
        &self,
        visible_source_range: Range<usize>,
        active_source_range: Option<&Range<usize>>,
        inactive_source_ranges: &[Range<usize>],
        blocks: impl IntoIterator<Item = &'a MarkdownBlock>,
        inline_spans: impl IntoIterator<Item = &'a MarkdownInlineSpan>,
    ) -> MarkdownProjectionMap {
        let mut operations = Vec::new();
        for block in blocks {
            let block_is_active = if block.kind == MarkdownBlockKind::PipeTable {
                table_row_source_range_is_active(
                    &block.source_range,
                    &visible_source_range,
                    active_source_range,
                    inactive_source_ranges,
                )
            } else {
                source_range_is_active(
                    &block.source_range,
                    active_source_range,
                    inactive_source_ranges,
                )
            };
            let block_markers_are_active = if block_markers_require_marker_hit(block.kind) {
                block.marker_ranges.iter().any(|marker_range| {
                    marker_range_is_hit(marker_range, active_source_range, inactive_source_ranges)
                })
            } else {
                block_is_active
            };
            if block_markers_are_active {
                continue;
            }

            for marker_range in &block.marker_ranges {
                let start = marker_range.start.max(visible_source_range.start);
                let end = marker_range.end.min(visible_source_range.end);
                if start < end {
                    operations.push(MarkdownProjectionOperation::Hide {
                        source_range: start..end,
                    });
                }
            }
        }

        let active_table_row = self.data.tables.iter().any(|table| {
            table_row_source_range_is_active(
                &table.source_range,
                &visible_source_range,
                active_source_range,
                inactive_source_ranges,
            )
        });
        for span in inline_spans {
            let span_is_active = if active_table_row {
                ranges_overlap(&span.source_range, &visible_source_range)
            } else {
                source_range_is_active(
                    &span.source_range,
                    active_source_range,
                    inactive_source_ranges,
                )
            };
            if span_is_active {
                continue;
            }

            if matches!(
                span.kind,
                MarkdownInlineKind::SoftBreak | MarkdownInlineKind::HardBreak
            ) {
                let start = span.source_range.start.max(visible_source_range.start);
                let end = span.source_range.end.min(visible_source_range.end);
                if start < end {
                    operations.push(MarkdownProjectionOperation::Replace {
                        source_range: start..end,
                        display_text: if span.kind == MarkdownInlineKind::HardBreak {
                            "\n".to_string()
                        } else {
                            " ".to_string()
                        },
                    });
                }
                continue;
            }

            for marker_range in &span.marker_ranges {
                let start = marker_range.start.max(visible_source_range.start);
                let end = marker_range.end.min(visible_source_range.end);
                if start < end {
                    operations.push(MarkdownProjectionOperation::Hide {
                        source_range: start..end,
                    });
                }
            }
        }

        for replacement in
            self.projection_replacements_in_source_range(visible_source_range.clone())
        {
            let replacement_is_active = if active_table_row {
                ranges_overlap(&replacement.source_range, &visible_source_range)
            } else {
                source_range_is_active(
                    &replacement.owner_source_range,
                    active_source_range,
                    inactive_source_ranges,
                )
            };
            if replacement_is_active {
                continue;
            }

            let start = replacement
                .source_range
                .start
                .max(visible_source_range.start);
            let end = replacement.source_range.end.min(visible_source_range.end);
            if start < end {
                operations.push(MarkdownProjectionOperation::Replace {
                    source_range: start..end,
                    display_text: replacement.display_text.clone(),
                });
            }
        }

        MarkdownProjectionMap::with_operations(
            self.data.source_len,
            visible_source_range,
            operations,
        )
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
        let mut source_ranges = self.data.projection_marker_dependencies[start_index..]
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

    fn partition_projection_marker_dependencies_by_prefix_end(&self, offset: usize) -> usize {
        self.data
            .projection_marker_prefix_maximum_ends
            .partition_point(|end| *end <= offset)
    }

    fn partition_projection_replacements_by_prefix_end(&self, offset: usize) -> usize {
        self.data
            .projection_replacement_prefix_maximum_ends
            .partition_point(|end| *end <= offset)
    }
}

impl MarkdownProjectionMap {
    pub fn new(
        source_len: usize,
        visible_source_range: Range<usize>,
        hidden_ranges: Vec<Range<usize>>,
    ) -> Self {
        Self::with_operations(
            source_len,
            visible_source_range,
            hidden_ranges
                .into_iter()
                .map(|source_range| MarkdownProjectionOperation::Hide { source_range }),
        )
    }

    pub fn with_operations(
        source_len: usize,
        visible_source_range: Range<usize>,
        operations: impl IntoIterator<Item = MarkdownProjectionOperation>,
    ) -> Self {
        let operations = normalize_projection_operations(operations);
        let hidden_ranges = projection_hidden_ranges(&operations);

        Self {
            source_len,
            visible_source_range,
            operations,
            hidden_ranges,
        }
    }

    pub fn source_len(&self) -> usize {
        self.source_len
    }

    pub fn visible_source_range(&self) -> Range<usize> {
        self.visible_source_range.clone()
    }

    pub fn operations(&self) -> &[MarkdownProjectionOperation] {
        &self.operations
    }

    pub fn hidden_ranges(&self) -> &[Range<usize>] {
        &self.hidden_ranges
    }

    pub fn display_len(&self) -> usize {
        self.source_to_display(self.visible_source_range.end)
    }

    pub fn project_source_text(&self, source_text: &str) -> String {
        if self.operations.is_empty() {
            return source_text.to_string();
        }

        let mut rendered_text = String::new();
        let mut cursor = self.visible_source_range.start;
        for operation in &self.operations {
            let operation_range = operation.source_range();
            let start = operation_range.start.max(self.visible_source_range.start);
            let end = operation_range.end.min(self.visible_source_range.end);
            if start >= end {
                continue;
            }

            if cursor < start {
                rendered_text.push_str(
                    &source_text[(cursor - self.visible_source_range.start)
                        ..(start - self.visible_source_range.start)],
                );
            }
            if let MarkdownProjectionOperation::Replace { display_text, .. } = operation {
                rendered_text.push_str(display_text);
            }
            cursor = cursor.max(end);
        }

        if cursor < self.visible_source_range.end {
            rendered_text.push_str(
                &source_text[(cursor - self.visible_source_range.start)
                    ..(self.visible_source_range.end - self.visible_source_range.start)],
            );
        }

        rendered_text
    }

    pub fn source_to_display(&self, source_offset: usize) -> usize {
        let clipped_offset = source_offset.clamp(
            self.visible_source_range.start,
            self.visible_source_range.end,
        );
        let mut display_offset = 0;
        let mut source_cursor = self.visible_source_range.start;

        for operation in &self.operations {
            let operation_range = operation.source_range();
            let start = operation_range.start.max(self.visible_source_range.start);
            let end = operation_range.end.min(self.visible_source_range.end);
            if start >= end {
                continue;
            }
            if start >= clipped_offset {
                break;
            }

            display_offset += start.saturating_sub(source_cursor);
            if clipped_offset < end {
                return display_offset;
            }

            display_offset += operation.display_len();
            source_cursor = end;
        }

        display_offset + clipped_offset.saturating_sub(source_cursor)
    }

    pub fn display_to_source(&self, display_offset: usize) -> usize {
        let mut display_cursor = 0;
        let mut source_cursor = self.visible_source_range.start;

        for operation in &self.operations {
            let operation_range = operation.source_range();
            let start = operation_range.start.max(self.visible_source_range.start);
            let end = operation_range.end.min(self.visible_source_range.end);
            if start >= end {
                continue;
            }

            let visible_source_len = start.saturating_sub(source_cursor);
            if display_offset < display_cursor + visible_source_len {
                return source_cursor + (display_offset - display_cursor);
            }
            display_cursor += visible_source_len;

            let operation_display_len = operation.display_len();
            if display_offset <= display_cursor + operation_display_len {
                return match operation {
                    MarkdownProjectionOperation::Hide { .. } => end,
                    MarkdownProjectionOperation::Replace { .. }
                        if display_offset == display_cursor =>
                    {
                        start
                    }
                    MarkdownProjectionOperation::Replace { .. } => end,
                };
            }
            display_cursor += operation_display_len;
            source_cursor = end;
        }

        (source_cursor + display_offset.saturating_sub(display_cursor))
            .min(self.visible_source_range.end)
    }
}

impl MarkdownProjectionOperation {
    pub fn source_range(&self) -> &Range<usize> {
        match self {
            Self::Hide { source_range } | Self::Replace { source_range, .. } => source_range,
        }
    }

    pub fn display_len(&self) -> usize {
        match self {
            Self::Hide { .. } => 0,
            Self::Replace { display_text, .. } => display_text.len(),
        }
    }
}

fn normalize_projection_operations(
    operations: impl IntoIterator<Item = MarkdownProjectionOperation>,
) -> Vec<MarkdownProjectionOperation> {
    let mut operations = operations
        .into_iter()
        .filter(|operation| operation.source_range().start < operation.source_range().end)
        .collect::<Vec<_>>();
    operations
        .sort_by_key(|operation| (operation.source_range().start, operation.source_range().end));

    let mut normalized = Vec::with_capacity(operations.len());
    for operation in operations {
        if let Some(MarkdownProjectionOperation::Hide { source_range }) = normalized.last_mut()
            && let MarkdownProjectionOperation::Hide {
                source_range: next_range,
            } = &operation
            && source_range.end >= next_range.start
        {
            source_range.end = source_range.end.max(next_range.end);
            continue;
        }
        normalized.push(operation);
    }

    normalized
}

fn projection_hidden_ranges(operations: &[MarkdownProjectionOperation]) -> Vec<Range<usize>> {
    let hidden_ranges = operations
        .iter()
        .map(|operation| operation.source_range().clone())
        .collect::<Vec<_>>();

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

    merged_ranges
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

fn marker_range_is_hit(
    marker_range: &Range<usize>,
    active_source_range: Option<&Range<usize>>,
    inactive_source_ranges: &[Range<usize>],
) -> bool {
    let Some(active_source_range) = active_source_range else {
        return false;
    };
    let overlaps_or_touches = ranges_overlap(marker_range, active_source_range)
        || active_source_range.start == marker_range.end
        || active_source_range.end == marker_range.start;
    if !overlaps_or_touches {
        return false;
    }

    !inactive_source_ranges
        .iter()
        .any(|inactive_source_range| range_contains(inactive_source_range, marker_range))
}

fn table_row_source_range_is_active(
    table_source_range: &Range<usize>,
    row_source_range: &Range<usize>,
    active_source_range: Option<&Range<usize>>,
    inactive_source_ranges: &[Range<usize>],
) -> bool {
    let Some(active_source_range) = active_source_range else {
        return false;
    };
    if !ranges_overlap(table_source_range, active_source_range)
        || !ranges_overlap(row_source_range, active_source_range)
    {
        return false;
    }

    !inactive_source_ranges
        .iter()
        .any(|inactive_source_range| range_contains(inactive_source_range, row_source_range))
}

fn block_markers_require_marker_hit(kind: MarkdownBlockKind) -> bool {
    matches!(
        kind,
        MarkdownBlockKind::BlockQuote
            | MarkdownBlockKind::OrderedList
            | MarkdownBlockKind::UnorderedList
            | MarkdownBlockKind::ListItem
            | MarkdownBlockKind::TaskListItem { .. }
    )
}
