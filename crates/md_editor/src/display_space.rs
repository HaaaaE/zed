use md_buffer::BufferSnapshot;
use md_projection::RenderedDisplayIndex;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point};

use crate::MarkdownEditorMode;

pub(crate) fn display_item_count_for_mode(
    mode: MarkdownEditorMode,
    text_snapshot: &TextBufferSnapshot,
    rendered_index: Option<&RenderedDisplayIndex>,
) -> usize {
    match mode {
        MarkdownEditorMode::Source => text_snapshot.row_count() as usize,
        MarkdownEditorMode::Rendered => rendered_index
            .expect("rendered index is required for rendered mode")
            .item_count(),
    }
}

pub(crate) fn display_item_index_for_cursor(
    mode: MarkdownEditorMode,
    rendered_index: Option<&RenderedDisplayIndex>,
    cursor: Point,
) -> Option<usize> {
    match mode {
        MarkdownEditorMode::Source => Some(cursor.row as usize),
        MarkdownEditorMode::Rendered => rendered_index
            .expect("rendered index is required for rendered mode")
            .item_index_for_source_row(cursor.row as usize),
    }
}

pub(crate) fn rendered_item_index_for_cursor(
    snapshot: &BufferSnapshot,
    rendered_index: &RenderedDisplayIndex,
    cursor: Point,
) -> Option<usize> {
    let source_offset = snapshot.as_text_snapshot().point_to_offset(cursor);
    rendered_index.item_index_for_source_offset(snapshot, source_offset)
}
