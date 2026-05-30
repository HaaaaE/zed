use std::ops::Range;

use super::{MarkdownProjectionMap, MarkdownProjectionOperation};

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
