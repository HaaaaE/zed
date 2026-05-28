use std::{
    collections::{HashSet, VecDeque},
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{Context, Window};
use md_buffer::BufferSnapshot;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point, Selection};

use super::{
    DisplayBlockLayout, DisplayInlineAtomKind, DisplayRow, DisplayRowLayout,
    DisplayRowLayoutInputs, DisplayRowProjectionState, DisplayRowTextLayout,
    InlineAtomMeasurementKey, InlineAtomMeasurementState, LocalSourceEditInvalidation,
    MarkdownEditor, MarkdownEditorMode, RowDisplayStyle, RowLayoutCacheKey, RowLayoutInputCacheKey,
    clip_selection, display_row_in_mode, inline_spans_for_display_row, layout::DisplayRowCacheKey,
    markdown_blocks_for_display_row, row_source_range, source_display_row_in_text_snapshot,
};
use crate::layout::{
    display_row_layout_inputs, source_display_row_layout_inputs, text_layout_for_display_row_inputs,
};

impl MarkdownEditor {
    pub(crate) fn clear_row_layout_cache(&mut self) {
        self.row_layout_cache.clear();
        self.table_layout_cache.clear();
    }

    pub(crate) fn clear_display_row_cache(&mut self) {
        self.display_row_cache.clear();
        self.row_layout_input_cache.clear();
        self.table_layout_cache.clear();
        self.source_prewarm = None;
        self.rendered_prewarm = None;
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
        let active_projection_source_ranges = snapshot
            .syntax_tree()
            .active_projection_source_ranges_for_source_range(
                source_range.clone(),
                display_row_state.active_source_range.clone(),
                &display_row_state.inactive_source_ranges,
            );
        let cache_key = DisplayRowCacheKey {
            version: snapshot.version().clone(),
            row,
            mode,
            active_projection_source_ranges: active_projection_source_ranges.clone(),
        };

        if let Some(display_row) = self.display_row_cache.get(&cache_key) {
            return Some(display_row.clone());
        }

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.display_rows_created += 1;
            self.layout_computation_counts.rendered_block_queries += 1;
            self.layout_computation_counts.rendered_inline_span_queries += 1;
        }
        let markdown_blocks = markdown_blocks_for_display_row(snapshot, source_range.clone(), mode);
        let inline_spans = inline_spans_for_display_row(snapshot, source_range.clone(), mode);
        let display_row = Arc::new(display_row_in_mode(
            snapshot,
            row,
            mode,
            display_row_state,
            source_range,
            active_projection_source_ranges,
            markdown_blocks,
            inline_spans,
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

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.display_rows_created += 1;
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

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.row_layouts_created += 1;
        }
        let layout = if let Some(table_layout) = self.cached_table_row_layout(
            snapshot,
            display_row,
            selection,
            mode,
            wrap_width,
            row_style,
            window,
        ) {
            DisplayRowLayout::TableRow(Arc::new(table_layout))
        } else if let Some(block_layout) = DisplayBlockLayout::for_display_row(
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
            DisplayRowLayout::Block(Arc::new(block_layout))
        } else {
            let inputs =
                self.cached_row_layout_inputs(snapshot, display_row, mode, row_style, window);
            let atom_measurements = self.inline_atom_measurement_states_for_layout(
                display_row.row as usize,
                &inputs,
                row_style,
                wrap_width,
                measure_inline_atoms,
                window,
                cx,
            );
            #[cfg(perf_enabled)]
            {
                self.layout_computation_counts.text_shaping_calls += 1;
            }
            DisplayRowLayout::Text(Arc::new(text_layout_for_display_row_inputs(
                &display_row.text,
                &inputs,
                row_style,
                wrap_width,
                &atom_measurements,
                window,
                cx,
            )))
        };
        if layout.cacheable() {
            self.row_layout_cache.insert(cache_key, layout.clone());
        }
        layout
    }

    fn cached_table_row_layout(
        &mut self,
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        selection: &Selection<Point>,
        mode: MarkdownEditorMode,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
        window: &mut Window,
    ) -> Option<super::DisplayTableRowLayout> {
        if !super::DisplayTableRowLayout::is_inactive_table_row(
            snapshot,
            display_row,
            selection,
            mode,
        ) {
            return None;
        }

        let (table, table_row) = snapshot
            .syntax_tree()
            .table_row_for_source_row(display_row.row as usize)?;
        let cache_key = super::TableLayoutCacheKey {
            version: snapshot.version().clone(),
            table_source_range: table.source_range.clone(),
            wrap_width,
            row_style,
        };
        let table_layout = if let Some(table_layout) = self.table_layout_cache.get(&cache_key) {
            table_layout.clone()
        } else {
            let table_layout = Arc::new(super::DisplayTableLayout::new(
                snapshot, table, wrap_width, row_style, window,
            ));
            self.table_layout_cache
                .insert(cache_key, table_layout.clone());
            table_layout
        };

        Some(super::DisplayTableRowLayout::new(
            snapshot,
            table,
            table_row,
            &table_layout,
            row_style,
            window,
        ))
    }

    pub(crate) fn cached_source_text_layout(
        &mut self,
        display_row: &DisplayRow,
        row_style: RowDisplayStyle,
        wrap_width: gpui::Pixels,
        measure_inline_atoms: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Arc<DisplayRowTextLayout> {
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

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.row_layouts_created += 1;
        }
        let inputs = self.cached_source_row_layout_inputs(display_row, row_style, window);
        let atom_measurements = self.inline_atom_measurement_states_for_layout(
            display_row.row as usize,
            &inputs,
            row_style,
            wrap_width,
            measure_inline_atoms,
            window,
            cx,
        );
        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.text_shaping_calls += 1;
        }
        let layout = Arc::new(text_layout_for_display_row_inputs(
            &display_row.text,
            &inputs,
            row_style,
            wrap_width,
            &atom_measurements,
            window,
            cx,
        ));
        if layout.cacheable {
            self.row_layout_cache
                .insert(cache_key, DisplayRowLayout::Text(layout.clone()));
        }
        layout
    }

    pub(crate) fn schedule_source_cache_prewarm(
        &mut self,
        wrap_width: gpui::Pixels,
        row_style: RowDisplayStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode != MarkdownEditorMode::Source {
            self.source_prewarm = None;
            return;
        }

        let snapshot = self.buffer.as_text_snapshot();
        let version = snapshot.version().clone();
        let row_count = snapshot.row_count() as usize;
        if row_count == 0 {
            self.source_prewarm = None;
            return;
        }

        let start_row = self.display_list_state.logical_scroll_top().item_ix;
        let reset_queue = self.source_prewarm.as_ref().is_none_or(|state| {
            state.version != version
                || state.wrap_width != wrap_width
                || state.row_style != row_style
                || start_row.abs_diff(state.anchor_row) > PREWARM_ANCHOR_RESET_ROWS
        });
        if reset_queue {
            let byte_len = self.buffer.len();
            self.source_prewarm = Some(super::SourcePrewarmState {
                version,
                wrap_width,
                row_style,
                anchor_row: start_row,
                rows: cache_prewarm_rows(row_count, byte_len, start_row),
                scheduled: false,
            });
        }

        if self
            .source_prewarm
            .as_ref()
            .is_some_and(|state| state.scheduled || state.rows.is_empty())
        {
            return;
        }

        if let Some(state) = self.source_prewarm.as_mut() {
            state.scheduled = true;
        }
        cx.on_next_frame(window, |this, window, cx| {
            this.flush_source_cache_prewarm(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn flush_source_cache_prewarm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.buffer.text_snapshot();
        let Some(state) = self.source_prewarm.as_mut() else {
            return;
        };
        state.scheduled = false;
        if self.mode != MarkdownEditorMode::Source || state.version != *snapshot.version() {
            self.source_prewarm = None;
            return;
        }

        let deadline = Instant::now() + PREWARM_FRAME_BUDGET;
        let mut rows = Vec::new();
        while rows.len() < SOURCE_PREWARM_ROWS_PER_FRAME && Instant::now() < deadline {
            let Some(row) = state.rows.pop_front() else {
                break;
            };
            rows.push(row);
        }
        let has_more_rows = !state.rows.is_empty();
        let wrap_width = state.wrap_width;
        let row_style = state.row_style;

        for row in rows {
            let Some(display_row) = self.cached_source_display_row(&snapshot, row) else {
                continue;
            };
            let text_layout = self.cached_source_text_layout(
                &display_row,
                row_style,
                wrap_width,
                false,
                window,
                cx,
            );
            self.display_list_state.set_item_size_hint(
                row,
                gpui::size(
                    gpui::px(0.),
                    row_style.min_height.max(text_layout.height(row_style)),
                ),
            );
        }

        if has_more_rows {
            self.schedule_source_cache_prewarm(wrap_width, row_style, window, cx);
        }
    }

    pub(crate) fn schedule_rendered_cache_prewarm(
        &mut self,
        wrap_width: gpui::Pixels,
        selection: Selection<Point>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.mode != MarkdownEditorMode::Rendered {
            self.rendered_prewarm = None;
            return;
        }

        let snapshot = self.buffer.as_text_snapshot();
        let version = snapshot.version().clone();
        let row_count = snapshot.row_count() as usize;
        if row_count == 0 {
            self.rendered_prewarm = None;
            return;
        }

        let start_row = self.display_list_state.logical_scroll_top().item_ix;
        let reset_queue = self.rendered_prewarm.as_ref().is_none_or(|state| {
            state.version != version
                || state.wrap_width != wrap_width
                || state.selection != selection
                || start_row.abs_diff(state.anchor_row) > PREWARM_ANCHOR_RESET_ROWS
        });
        if reset_queue {
            let byte_len = self.buffer.len();
            self.rendered_prewarm = Some(super::RenderedPrewarmState {
                version,
                wrap_width,
                selection,
                anchor_row: start_row,
                rows: cache_prewarm_rows(row_count, byte_len, start_row),
                scheduled: false,
            });
        }

        if self
            .rendered_prewarm
            .as_ref()
            .is_some_and(|state| state.scheduled || state.rows.is_empty())
        {
            return;
        }

        if let Some(state) = self.rendered_prewarm.as_mut() {
            state.scheduled = true;
        }
        cx.on_next_frame(window, |this, window, cx| {
            this.flush_rendered_cache_prewarm(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn flush_rendered_cache_prewarm(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let snapshot = self.buffer.snapshot();
        let selection = clip_selection(&snapshot, &self.selection);
        let Some(state) = self.rendered_prewarm.as_mut() else {
            return;
        };
        state.scheduled = false;
        if self.mode != MarkdownEditorMode::Rendered
            || state.version != *snapshot.version()
            || state.selection != selection
        {
            self.rendered_prewarm = None;
            return;
        }

        let deadline = Instant::now() + PREWARM_FRAME_BUDGET;
        let mut rows = Vec::new();
        while rows.len() < RENDERED_PREWARM_ROWS_PER_FRAME && Instant::now() < deadline {
            let Some(row) = state.rows.pop_front() else {
                break;
            };
            rows.push(row);
        }
        let has_more_rows = !state.rows.is_empty();
        let wrap_width = state.wrap_width;

        let display_row_state = DisplayRowProjectionState::new(
            &snapshot,
            Some(&selection),
            MarkdownEditorMode::Rendered,
        );
        for row in rows {
            let Some(display_row) = self.cached_display_row(
                &snapshot,
                row,
                MarkdownEditorMode::Rendered,
                &display_row_state,
            ) else {
                continue;
            };
            let row_style = super::row_display_style_for_display_row(
                &snapshot,
                &display_row,
                MarkdownEditorMode::Rendered,
            );
            let row_layout = self.cached_row_layout(
                &snapshot,
                &display_row,
                &selection,
                MarkdownEditorMode::Rendered,
                row_style,
                wrap_width,
                false,
                window,
                cx,
            );
            self.display_list_state.set_item_size_hint(
                row,
                gpui::size(gpui::px(0.), row_layout.row_min_height(row_style)),
            );
        }

        if has_more_rows {
            self.schedule_rendered_cache_prewarm(wrap_width, selection, window, cx);
        }
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

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.row_layout_inputs_created += 1;
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

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.row_layout_inputs_created += 1;
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
        wrap_width: gpui::Pixels,
        measure_inline_atoms: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<InlineAtomMeasurementState> {
        let atoms = inputs.inline_atoms().cloned().collect::<Vec<_>>();
        atoms
            .iter()
            .map(|atom| {
                let fallback_size = atom.fallback_size(&inputs.shaped_line, row_style);
                let image_max_width = (atom.kind() == DisplayInlineAtomKind::InlineImage)
                    .then_some(wrap_width.max(gpui::px(1.)));
                let key = atom.measurement_key_with_scale(
                    row_style,
                    window.scale_factor(),
                    image_max_width,
                );
                if let Some(measurement) = self.inline_atom_measurement_cache.get(&key).copied() {
                    if measure_inline_atoms && atom.kind() == DisplayInlineAtomKind::InlineImage {
                        let measured = atom.measure_size_state(
                            fallback_size,
                            row_style,
                            image_max_width,
                            window,
                            cx,
                        );
                        match measured {
                            InlineAtomMeasurementState::Ready(_)
                            | InlineAtomMeasurementState::Invalid(_) => {
                                self.update_inline_atom_measurement_cache(
                                    row, key, measured, window, cx,
                                );
                                return measured;
                            }
                            InlineAtomMeasurementState::Pending(_) => {
                                self.pending_inline_atom_rows
                                    .entry(key)
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

                let measurement =
                    atom.measure_size_state(fallback_size, row_style, image_max_width, window, cx);
                match measurement {
                    InlineAtomMeasurementState::Ready(_)
                    | InlineAtomMeasurementState::Invalid(_) => {
                        self.update_inline_atom_measurement_cache(
                            row,
                            key,
                            measurement,
                            window,
                            cx,
                        );
                    }
                    InlineAtomMeasurementState::Pending(_) => {
                        self.pending_inline_atom_rows
                            .entry(key)
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

const SMALL_FILE_MAX_BYTES: usize = 64 * 1024;
const SMALL_FILE_MAX_ROWS: usize = 1_000;
const NEARBY_FORWARD_ROWS: usize = 256;
const NEARBY_BACKWARD_ROWS: usize = 64;
const PREWARM_ANCHOR_RESET_ROWS: usize = 128;
const SOURCE_PREWARM_ROWS_PER_FRAME: usize = 32;
const RENDERED_PREWARM_ROWS_PER_FRAME: usize = 16;
const PREWARM_FRAME_BUDGET: Duration = Duration::from_millis(1);

fn cache_prewarm_rows(row_count: usize, byte_len: usize, start_row: usize) -> VecDeque<usize> {
    let mut rows = VecDeque::new();
    let mut queued = HashSet::new();
    let mut push_row = |row: usize, rows: &mut VecDeque<usize>| {
        if row < row_count && queued.insert(row) {
            rows.push_back(row);
        }
    };

    let start_row = start_row.min(row_count.saturating_sub(1));
    for row in start_row..row_count.min(start_row.saturating_add(NEARBY_FORWARD_ROWS)) {
        push_row(row, &mut rows);
    }
    let before_start = start_row.saturating_sub(NEARBY_BACKWARD_ROWS);
    for row in before_start..start_row {
        push_row(row, &mut rows);
    }

    if byte_len <= SMALL_FILE_MAX_BYTES && row_count <= SMALL_FILE_MAX_ROWS {
        for row in 0..row_count {
            push_row(row, &mut rows);
        }
    }

    rows
}
