use std::{ops::Range, sync::Arc};

use gpui::{Context, Window};
use md_buffer::BufferSnapshot;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point, Selection};

use super::{
    DisplayBlockLayout, DisplayInlineAtomKind, DisplayRow, DisplayRowLayout,
    DisplayRowLayoutInputs, DisplayRowProjectionState, DisplayRowTextLayout,
    InlineAtomMeasurementKey, InlineAtomMeasurementState, LocalSourceEditInvalidation,
    MarkdownEditor, MarkdownEditorMode, RowDisplayStyle, RowLayoutCacheKey, RowLayoutInputCacheKey,
    active_projection_source_ranges, display_row_in_mode, layout::DisplayRowCacheKey,
    row_source_range, source_display_row_in_text_snapshot,
};
use crate::layout::{
    display_row_layout_inputs, source_display_row_layout_inputs, text_layout_for_display_row_inputs,
};

impl MarkdownEditor {
    pub(crate) fn clear_row_layout_cache(&mut self) {
        self.row_layout_cache.clear();
    }

    pub(crate) fn clear_display_row_cache(&mut self) {
        self.display_row_cache.clear();
        self.row_layout_input_cache.clear();
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

    pub(crate) fn rekey_source_row_layout_input_cache_for_local_edit(
        &mut self,
        invalidation: &LocalSourceEditInvalidation,
        version: md_text::Global,
    ) {
        self.row_layout_input_cache = self
            .row_layout_input_cache
            .drain()
            .filter_map(|(key, inputs)| {
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
                    RowLayoutInputCacheKey {
                        version: version.clone(),
                        ..key
                    },
                    inputs,
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
        self.clear_row_layout_input_cache_for_row_ranges(row_ranges);
    }

    pub(crate) fn clear_row_layout_cache_for_rows(&mut self, rows: Range<usize>) {
        self.row_layout_cache
            .retain(|key, _| !rows.contains(&(key.row as usize)));
    }

    pub(crate) fn clear_row_layout_input_cache_for_rows(&mut self, rows: Range<usize>) {
        self.row_layout_input_cache
            .retain(|key, _| !rows.contains(&(key.row as usize)));
    }

    pub(crate) fn clear_row_layout_cache_for_row_ranges(&mut self, row_ranges: &[Range<usize>]) {
        self.row_layout_cache.retain(|key, _| {
            !row_ranges
                .iter()
                .any(|rows| rows.contains(&(key.row as usize)))
        });
    }

    pub(crate) fn clear_row_layout_input_cache_for_row_ranges(
        &mut self,
        row_ranges: &[Range<usize>],
    ) {
        self.row_layout_input_cache.retain(|key, _| {
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
            self.document_path(),
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
            row_style,
            wrap_width,
            active_projection_source_ranges: display_row.active_projection_source_ranges.clone(),
        };

        if let Some(cached_layout) = self.row_layout_cache.get(&cache_key) {
            return cached_layout.clone();
        }

        let layout = if let Some(block_layout) = DisplayBlockLayout::for_display_row(
            snapshot,
            display_row,
            selection,
            mode,
            self.document_path(),
            wrap_width,
            row_style,
            measure_inline_atoms,
            window,
            cx,
        ) {
            DisplayRowLayout::Block(block_layout)
        } else {
            let inputs =
                self.cached_row_layout_inputs(snapshot, display_row, mode, row_style, window);
            let atom_measurements = self.inline_atom_measurement_states_for_layout(
                display_row.row as usize,
                &inputs,
                row_style,
                measure_inline_atoms,
                window,
                cx,
            );
            DisplayRowLayout::Text(text_layout_for_display_row_inputs(
                &display_row.text,
                &inputs,
                row_style,
                wrap_width,
                &atom_measurements,
                window,
                cx,
            ))
        };
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
            row_style,
            wrap_width,
            active_projection_source_ranges: Vec::new(),
        };

        if let Some(DisplayRowLayout::Text(cached_layout)) = self.row_layout_cache.get(&cache_key) {
            return cached_layout.clone();
        }

        let inputs = self.cached_source_row_layout_inputs(display_row, row_style, window);
        let atom_measurements = self.inline_atom_measurement_states_for_layout(
            display_row.row as usize,
            &inputs,
            row_style,
            measure_inline_atoms,
            window,
            cx,
        );
        let layout = text_layout_for_display_row_inputs(
            &display_row.text,
            &inputs,
            row_style,
            wrap_width,
            &atom_measurements,
            window,
            cx,
        );
        if layout.cacheable {
            self.row_layout_cache
                .insert(cache_key, DisplayRowLayout::Text(layout.clone()));
        }
        layout
    }

    fn cached_row_layout_inputs(
        &mut self,
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        mode: MarkdownEditorMode,
        row_style: RowDisplayStyle,
        window: &mut Window,
    ) -> DisplayRowLayoutInputs {
        let cache_key = RowLayoutInputCacheKey {
            version: snapshot.version().clone(),
            row: display_row.row,
            mode,
            active_projection_source_ranges: display_row.active_projection_source_ranges.clone(),
            row_style,
        };

        if let Some(inputs) = self.row_layout_input_cache.get(&cache_key) {
            return inputs.clone();
        }

        let inputs = display_row_layout_inputs(
            snapshot,
            display_row,
            mode,
            row_style,
            self.document_path(),
            window,
        );
        self.row_layout_input_cache
            .insert(cache_key, inputs.clone());
        inputs
    }

    fn cached_source_row_layout_inputs(
        &mut self,
        display_row: &DisplayRow,
        row_style: RowDisplayStyle,
        window: &mut Window,
    ) -> DisplayRowLayoutInputs {
        let cache_key = RowLayoutInputCacheKey {
            version: self.buffer.as_text_snapshot().version().clone(),
            row: display_row.row,
            mode: MarkdownEditorMode::Source,
            active_projection_source_ranges: Vec::new(),
            row_style,
        };

        if let Some(inputs) = self.row_layout_input_cache.get(&cache_key) {
            return inputs.clone();
        }

        let inputs = source_display_row_layout_inputs(display_row, row_style, window);
        self.row_layout_input_cache
            .insert(cache_key, inputs.clone());
        inputs
    }

    fn inline_atom_measurement_states_for_layout(
        &mut self,
        row: usize,
        inputs: &DisplayRowLayoutInputs,
        row_style: RowDisplayStyle,
        measure_inline_atoms: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<InlineAtomMeasurementState> {
        let atoms = inputs.inline_atoms().cloned().collect::<Vec<_>>();
        atoms
            .iter()
            .zip(inputs.inline_atom_keys.iter())
            .map(|(atom, key)| {
                let fallback_size = atom.fallback_size(&inputs.shaped_line, row_style);
                if let Some(measurement) = self.inline_atom_measurement_cache.get(key).copied() {
                    if measure_inline_atoms && atom.kind() == DisplayInlineAtomKind::InlineImage {
                        let measured =
                            atom.measure_size_state(fallback_size, row_style, window, cx);
                        match measured {
                            InlineAtomMeasurementState::Ready(_)
                            | InlineAtomMeasurementState::Invalid(_) => {
                                self.update_inline_atom_measurement_cache(
                                    row,
                                    key.clone(),
                                    measured,
                                    window,
                                    cx,
                                );
                                return measured;
                            }
                            InlineAtomMeasurementState::Pending(_) => {
                                self.pending_inline_atom_rows
                                    .entry(key.clone())
                                    .or_default()
                                    .insert(row);
                            }
                        }
                    }
                    return measurement;
                }

                if !measure_inline_atoms {
                    return InlineAtomMeasurementState::Pending(fallback_size);
                }

                let measurement = atom.measure_size_state(fallback_size, row_style, window, cx);
                match measurement {
                    InlineAtomMeasurementState::Ready(_)
                    | InlineAtomMeasurementState::Invalid(_) => {
                        self.update_inline_atom_measurement_cache(
                            row,
                            key.clone(),
                            measurement,
                            window,
                            cx,
                        );
                    }
                    InlineAtomMeasurementState::Pending(_) => {
                        self.pending_inline_atom_rows
                            .entry(key.clone())
                            .or_default()
                            .insert(row);
                    }
                }
                measurement
            })
            .collect()
    }

    pub(crate) fn update_inline_atom_measurement_cache(
        &mut self,
        row: usize,
        key: InlineAtomMeasurementKey,
        measurement: InlineAtomMeasurementState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous = self
            .inline_atom_measurement_cache
            .insert(key.clone(), measurement);
        let mut rows_to_remeasure = self
            .pending_inline_atom_rows
            .remove(&key)
            .unwrap_or_default();
        let changed = previous.is_some_and(|previous| previous.size() != measurement.size());
        if changed {
            rows_to_remeasure.insert(row);
        }
        for row in rows_to_remeasure {
            self.queue_inline_atom_row_remeasure(row, window, cx);
        }
    }

    pub(crate) fn queue_inline_atom_row_remeasure(
        &mut self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pending_inline_atom_remeasure_rows.insert(row);
        if self.inline_atom_remeasure_scheduled {
            return;
        }

        self.inline_atom_remeasure_scheduled = true;
        cx.on_next_frame(window, |this, _window, cx| {
            this.flush_inline_atom_row_remeasures(cx);
        });
    }

    pub(crate) fn flush_inline_atom_row_remeasures(&mut self, cx: &mut Context<Self>) {
        self.inline_atom_remeasure_scheduled = false;
        let mut rows = self
            .pending_inline_atom_remeasure_rows
            .drain()
            .collect::<Vec<_>>();
        if rows.is_empty() {
            return;
        }

        rows.sort_unstable();
        rows.dedup();
        for row in rows {
            let rows = row..row.saturating_add(1);
            self.clear_row_layout_cache_for_rows(rows.clone());
            self.display_list_state.remeasure_items(rows);
        }
        cx.notify();
    }
}
