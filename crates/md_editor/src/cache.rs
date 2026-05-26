use std::{ops::Range, sync::Arc};

use gpui::{Context, Window};
use md_buffer::BufferSnapshot;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point, Selection};

use super::{
    DisplayRow, DisplayRowLayout, DisplayRowProjectionState, DisplayRowTextLayout,
    LocalSourceEditInvalidation, MarkdownEditor, MarkdownEditorMode, RowDisplayStyle,
    RowLayoutCacheKey, active_projection_source_ranges, compute_display_row_layout,
    display_row_in_mode, layout::DisplayRowCacheKey, row_source_range,
    source_display_row_in_text_snapshot, source_text_layout_for_display_row,
};

impl MarkdownEditor {
    pub(crate) fn clear_row_layout_cache(&mut self) {
        self.row_layout_cache.clear();
    }

    pub(crate) fn clear_display_row_cache(&mut self) {
        self.display_row_cache.clear();
    }

    pub(crate) fn rekey_source_display_row_cache_for_local_edit(
        &mut self,
        invalidation: &LocalSourceEditInvalidation,
        version: md_text::Global,
    ) {
        self.display_row_cache = self
            .display_row_cache
            .drain()
            .filter_map(|(key, display_row)| {
                if key.mode != MarkdownEditorMode::Source {
                    return None;
                }

                let row = key.row as usize;
                if invalidation.rows.contains(&row)
                    || (invalidation.byte_delta != Some(0) && row >= invalidation.rows.start)
                {
                    return None;
                }

                Some((
                    DisplayRowCacheKey {
                        version: version.clone(),
                        ..key
                    },
                    display_row,
                ))
            })
            .collect();
    }

    pub(crate) fn clear_display_row_cache_for_row_ranges(&mut self, row_ranges: &[Range<usize>]) {
        self.display_row_cache.retain(|key, _| {
            !row_ranges
                .iter()
                .any(|rows| rows.contains(&(key.row as usize)))
        });
    }

    pub(crate) fn clear_row_layout_cache_for_rows(&mut self, rows: Range<usize>) {
        self.row_layout_cache
            .retain(|key, _| !rows.contains(&(key.row as usize)));
    }

    pub(crate) fn clear_row_layout_cache_for_row_ranges(&mut self, row_ranges: &[Range<usize>]) {
        self.row_layout_cache.retain(|key, _| {
            !row_ranges
                .iter()
                .any(|rows| rows.contains(&(key.row as usize)))
        });
    }

    pub(crate) fn cached_display_row(
        &mut self,
        snapshot: &BufferSnapshot,
        row: usize,
        mode: MarkdownEditorMode,
        display_row_state: &DisplayRowProjectionState,
    ) -> Option<Arc<DisplayRow>> {
        if mode == MarkdownEditorMode::Source {
            return self.cached_source_display_row(snapshot.as_text_snapshot(), row);
        }

        let row_count = snapshot.row_count() as usize;
        if row >= row_count {
            return None;
        }

        let row = row as u32;
        let source_range = row_source_range(snapshot, row);
        let active_projection_source_ranges =
            active_projection_source_ranges(snapshot, &source_range, display_row_state, mode);
        let cache_key = DisplayRowCacheKey {
            version: snapshot.version().clone(),
            row,
            mode,
            active_projection_source_ranges: active_projection_source_ranges.clone(),
        };

        if let Some(display_row) = self.display_row_cache.get(&cache_key) {
            return Some(display_row.clone());
        }

        let display_row = Arc::new(display_row_in_mode(
            snapshot,
            row,
            mode,
            display_row_state,
            source_range,
            active_projection_source_ranges,
        ));
        self.display_row_cache
            .insert(cache_key, display_row.clone());
        Some(display_row)
    }

    pub(crate) fn cached_source_display_row(
        &mut self,
        snapshot: &TextBufferSnapshot,
        row: usize,
    ) -> Option<Arc<DisplayRow>> {
        let row_count = snapshot.row_count() as usize;
        if row >= row_count {
            return None;
        }

        let row = row as u32;
        let cache_key = DisplayRowCacheKey {
            version: snapshot.version().clone(),
            row,
            mode: MarkdownEditorMode::Source,
            active_projection_source_ranges: Vec::new(),
        };

        if let Some(display_row) = self.display_row_cache.get(&cache_key) {
            return Some(display_row.clone());
        }

        let display_row = Arc::new(source_display_row_in_text_snapshot(snapshot, row));
        self.display_row_cache
            .insert(cache_key, display_row.clone());
        Some(display_row)
    }

    pub(crate) fn cached_row_layout(
        &mut self,
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        selection: &Selection<Point>,
        mode: MarkdownEditorMode,
        row_style: RowDisplayStyle,
        wrap_width: gpui::Pixels,
        measure_inline_atoms: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DisplayRowLayout {
        let cache_key = RowLayoutCacheKey {
            row: display_row.row,
            mode,
            wrap_width,
            active_projection_source_ranges: display_row.active_projection_source_ranges.clone(),
        };

        if let Some(cached_layout) = self.row_layout_cache.get(&cache_key) {
            return cached_layout.clone();
        }

        let layout = compute_display_row_layout(
            snapshot,
            display_row,
            selection,
            mode,
            row_style,
            wrap_width,
            measure_inline_atoms,
            window,
            cx,
        );
        if layout.cacheable() {
            self.row_layout_cache.insert(cache_key, layout.clone());
        }
        layout
    }

    pub(crate) fn cached_source_text_layout(
        &mut self,
        display_row: &DisplayRow,
        row_style: RowDisplayStyle,
        wrap_width: gpui::Pixels,
        measure_inline_atoms: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DisplayRowTextLayout {
        let cache_key = RowLayoutCacheKey {
            row: display_row.row,
            mode: MarkdownEditorMode::Source,
            wrap_width,
            active_projection_source_ranges: Vec::new(),
        };

        if let Some(DisplayRowLayout::Text(cached_layout)) = self.row_layout_cache.get(&cache_key) {
            return cached_layout.clone();
        }

        let layout = source_text_layout_for_display_row(
            display_row,
            row_style,
            wrap_width,
            measure_inline_atoms,
            window,
            cx,
        );
        if layout.cacheable {
            self.row_layout_cache
                .insert(cache_key, DisplayRowLayout::Text(layout.clone()));
        }
        layout
    }
}
