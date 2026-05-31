use std::{
    collections::{HashMap, HashSet, VecDeque},
    ops::Range,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{Context, Window};
use md_buffer::BufferSnapshot;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point, Selection};

use super::{
    DisplayBlockLayout, DisplayInlineAtomKind, DisplayRow, DisplayRowLayout,
    DisplayRowLayoutInputs, DisplayRowTextLayout, DisplayTableLayout, InlineAtomMeasurementKey,
    InlineAtomMeasurementState, MarkdownEditor, MarkdownEditorMode, RowDisplayStyle,
    RowLayoutCacheKey, RowLayoutInputCacheKey, TableLayoutCacheKey, clip_selection,
    invalidation::LocalSourceEditInvalidation, layout::DisplayRowCacheKey, ranges_overlap,
    rendered_projection_state,
};
use crate::display_row_builder::{rendered_display_row, source_display_row_in_text_snapshot};
use crate::layout::{
    display_row_layout_inputs, effective_text_wrap_width, source_display_row_layout_inputs,
    text_layout_for_display_row_inputs,
};
use md_projection::{
    RenderedDisplayIndex, RenderedProjectionState, RenderedTopology, source_display_item_id,
};

pub(crate) struct DisplayCacheStore {
    pub(super) display_row_cache: HashMap<DisplayRowCacheKey, Arc<DisplayRow>>,
    pub(super) row_layout_input_cache: HashMap<RowLayoutInputCacheKey, DisplayRowLayoutInputs>,
    pub(super) row_layout_cache: HashMap<RowLayoutCacheKey, DisplayRowLayout>,
    pub(super) table_layout_cache: HashMap<TableLayoutCacheKey, Arc<DisplayTableLayout>>,
    pub(super) rendered_display_index: Option<Arc<RenderedDisplayIndex>>,
}

impl Default for DisplayCacheStore {
    fn default() -> Self {
        Self {
            display_row_cache: HashMap::default(),
            row_layout_input_cache: HashMap::default(),
            row_layout_cache: HashMap::default(),
            table_layout_cache: HashMap::default(),
            rendered_display_index: None,
        }
    }
}

impl DisplayCacheStore {
    pub(crate) fn clear_all_display_rows(&mut self) {
        self.display_row_cache.clear();
        self.row_layout_input_cache.clear();
        self.table_layout_cache.clear();
        self.rendered_display_index = None;
    }

    pub(crate) fn clear_all_layouts(&mut self) {
        self.row_layout_cache.clear();
        self.table_layout_cache.clear();
    }

    pub(crate) fn clear_table_layouts(&mut self) {
        self.table_layout_cache.clear();
    }

    pub(crate) fn clear_rows(&mut self, row_ranges: &[Range<usize>]) {
        self.display_row_cache.retain(|key, _| {
            !row_ranges
                .iter()
                .any(|rows| ranges_overlap(rows, &key.source_row_range))
        });
        self.clear_layout_inputs_for_row_ranges(row_ranges);
    }

    pub(crate) fn clear_layout_rows(&mut self, rows: Range<usize>) {
        self.row_layout_cache
            .retain(|key, _| !ranges_overlap(&rows, &key.source_row_range));
    }

    pub(crate) fn clear_layout_inputs_for_rows(&mut self, rows: Range<usize>) {
        self.row_layout_input_cache
            .retain(|key, _| !ranges_overlap(&rows, &key.source_row_range));
    }

    pub(crate) fn clear_layout_rows_for_row_ranges(&mut self, row_ranges: &[Range<usize>]) {
        self.row_layout_cache.retain(|key, _| {
            !row_ranges
                .iter()
                .any(|rows| ranges_overlap(rows, &key.source_row_range))
        });
    }

    pub(crate) fn clear_layout_inputs_for_row_ranges(&mut self, row_ranges: &[Range<usize>]) {
        self.row_layout_input_cache.retain(|key, _| {
            !row_ranges
                .iter()
                .any(|rows| ranges_overlap(rows, &key.source_row_range))
        });
    }

    pub(crate) fn rekey_source_rows_for_local_edit(
        &mut self,
        invalidation: &LocalSourceEditInvalidation,
        version: md_text::Global,
    ) {
        self.rekey_source_display_rows_for_local_edit(invalidation, version.clone());
        self.rekey_source_layout_inputs_for_local_edit(invalidation, version);
    }

    pub(crate) fn rendered_index(
        &mut self,
        snapshot: &BufferSnapshot,
    ) -> Arc<RenderedDisplayIndex> {
        if let Some(index) = &self.rendered_display_index
            && index.version() == snapshot.version()
        {
            return index.clone();
        }

        let index = RenderedDisplayIndex::build(snapshot);
        self.rendered_display_index = Some(index.clone());
        index
    }

    fn rekey_source_display_rows_for_local_edit(
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

                let row = key.item_index as usize;
                if invalidation.rows.contains(&row)
                    || (invalidation.byte_delta != Some(0) && row >= invalidation.rows.start)
                {
                    return None;
                }

                Some((
                    DisplayRowCacheKey {
                        version: version.clone(),
                        item_id: source_display_item_id(&key.source_range, row),
                        ..key
                    },
                    display_row,
                ))
            })
            .collect();
    }

    fn rekey_source_layout_inputs_for_local_edit(
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

                let row = key.item_index as usize;
                if invalidation.rows.contains(&row)
                    || (invalidation.byte_delta != Some(0) && row >= invalidation.rows.start)
                {
                    return None;
                }

                Some((
                    RowLayoutInputCacheKey {
                        version: version.clone(),
                        item_id: source_display_item_id(&key.source_range, row),
                        ..key
                    },
                    inputs,
                ))
            })
            .collect();
    }
}

pub(crate) struct InlineAtomMeasurementStore {
    pub(super) measurement_cache: HashMap<InlineAtomMeasurementKey, InlineAtomMeasurementState>,
    pub(super) pending_rows: HashMap<InlineAtomMeasurementKey, HashSet<usize>>,
    pub(super) pending_remeasure_rows: HashSet<usize>,
    pub(super) remeasure_scheduled: bool,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DisplayCacheStats {
    pub(crate) display_row_count: usize,
    pub(crate) row_layout_input_count: usize,
    pub(crate) row_layout_count: usize,
    pub(crate) table_layout_count: usize,
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InlineAtomStats {
    pub(crate) measurement_count: usize,
    pub(crate) pending_row_key_count: usize,
    pub(crate) remeasure_scheduled: bool,
}

impl Default for InlineAtomMeasurementStore {
    fn default() -> Self {
        Self {
            measurement_cache: HashMap::default(),
            pending_rows: HashMap::default(),
            pending_remeasure_rows: HashSet::default(),
            remeasure_scheduled: false,
        }
    }
}

impl InlineAtomMeasurementStore {
    pub(crate) fn clear_all(&mut self) {
        self.measurement_cache.clear();
        self.pending_rows.clear();
        self.pending_remeasure_rows.clear();
        self.remeasure_scheduled = false;
    }
}

pub(crate) struct SourcePrewarmState {
    pub(crate) version: md_text::Global,
    pub(crate) wrap_width: gpui::Pixels,
    pub(crate) row_style: RowDisplayStyle,
    pub(crate) anchor_row: usize,
    pub(crate) rows: VecDeque<usize>,
    pub(crate) scheduled: bool,
}

pub(crate) struct RenderedPrewarmState {
    pub(crate) version: md_text::Global,
    pub(crate) wrap_width: gpui::Pixels,
    #[allow(dead_code)]
    pub(crate) selection: Selection<Point>,
    pub(crate) anchor_row: usize,
    pub(crate) rows: VecDeque<usize>,
    pub(crate) scheduled: bool,
}

fn row_source_range_in_text_snapshot_for_cache(
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

impl MarkdownEditor {
    #[cfg(test)]
    pub(crate) fn display_cache_stats_for_tests(&self) -> DisplayCacheStats {
        DisplayCacheStats {
            display_row_count: self.display_cache.display_row_cache.len(),
            row_layout_input_count: self.display_cache.row_layout_input_cache.len(),
            row_layout_count: self.display_cache.row_layout_cache.len(),
            table_layout_count: self.display_cache.table_layout_cache.len(),
        }
    }

    #[cfg(test)]
    pub(crate) fn row_layout_cache_has_item_index_for_tests(&self, item_index: u32) -> bool {
        self.display_cache
            .row_layout_cache
            .keys()
            .any(|key| key.item_index == item_index)
    }

    #[cfg(test)]
    pub(crate) fn inline_atom_stats_for_tests(&self) -> InlineAtomStats {
        InlineAtomStats {
            measurement_count: self.inline_atoms.measurement_cache.len(),
            pending_row_key_count: self.inline_atoms.pending_rows.len(),
            remeasure_scheduled: self.inline_atoms.remeasure_scheduled,
        }
    }

    #[cfg(test)]
    pub(crate) fn inline_atom_measurements_all_ready_for_tests(&self) -> bool {
        self.inline_atoms
            .measurement_cache
            .values()
            .all(|state| matches!(state, InlineAtomMeasurementState::Ready(_)))
    }

    #[cfg(test)]
    pub(crate) fn track_pending_inline_atom_row_for_tests(
        &mut self,
        key: InlineAtomMeasurementKey,
        row: usize,
    ) {
        self.inline_atoms
            .pending_rows
            .entry(key)
            .or_default()
            .insert(row);
    }

    pub(crate) fn clear_row_layout_cache(&mut self) {
        self.display_cache.clear_all_layouts();
    }

    pub(crate) fn clear_table_layout_cache(&mut self) {
        self.display_cache.clear_table_layouts();
    }

    pub(crate) fn clear_display_row_cache(&mut self) {
        self.display_cache.clear_all_display_rows();
        self.source_prewarm = None;
        self.rendered_prewarm = None;
    }

    pub(crate) fn clear_inline_atom_measurement_cache(&mut self) {
        self.inline_atoms.clear_all();
    }

    pub(crate) fn rekey_source_rows_for_local_edit(
        &mut self,
        invalidation: &LocalSourceEditInvalidation,
        version: md_text::Global,
    ) {
        self.display_cache
            .rekey_source_rows_for_local_edit(invalidation, version);
    }

    pub(crate) fn clear_display_row_cache_for_row_ranges(&mut self, row_ranges: &[Range<usize>]) {
        self.display_cache.clear_rows(row_ranges);
    }

    pub(crate) fn clear_row_layout_cache_for_rows(&mut self, rows: Range<usize>) {
        self.display_cache.clear_layout_rows(rows);
    }

    pub(crate) fn clear_row_layout_input_cache_for_rows(&mut self, rows: Range<usize>) {
        self.display_cache.clear_layout_inputs_for_rows(rows);
    }

    pub(crate) fn clear_row_layout_cache_for_row_ranges(&mut self, row_ranges: &[Range<usize>]) {
        self.display_cache
            .clear_layout_rows_for_row_ranges(row_ranges);
    }

    pub(crate) fn cached_display_row(
        &mut self,
        snapshot: &BufferSnapshot,
        item_index: usize,
        mode: MarkdownEditorMode,
        display_row_state: &RenderedProjectionState,
    ) -> Option<Arc<DisplayRow>> {
        if mode == MarkdownEditorMode::Source {
            return self.cached_source_display_row(snapshot.as_text_snapshot(), item_index);
        }

        let index = self.rendered_display_index(snapshot);
        let item = index.item(item_index)?.clone();
        let topology = RenderedTopology::new(snapshot, index.clone());
        let display_source_range = topology.item_display_source_range(&item, display_row_state);
        let active_projection_source_ranges = snapshot
            .syntax_tree()
            .active_projection_source_ranges_for_source_range(
                display_source_range.source_range.clone(),
                display_row_state.active_source_range.clone(),
                &display_row_state.inactive_source_ranges,
            );
        let cache_key = DisplayRowCacheKey {
            version: snapshot.version().clone(),
            item_id: item.id,
            item_index: item.index as u32,
            source_range: display_source_range.source_range.clone(),
            source_row_range: display_source_range.source_row_range.clone(),
            mode,
            active_projection_source_ranges: active_projection_source_ranges.clone(),
        };

        if let Some(display_row) = self.display_cache.display_row_cache.get(&cache_key) {
            return Some(display_row.clone());
        }

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.display_rows_created += 1;
            self.layout_computation_counts.rendered_block_queries += 1;
            self.layout_computation_counts.rendered_inline_span_queries += 1;
        }
        let range_semantics = snapshot.syntax_tree().range_semantics_for_source_range(
            display_source_range.source_range.clone(),
            display_row_state.active_source_range.clone(),
            &display_row_state.inactive_source_ranges,
        );
        let display_row = Arc::new(rendered_display_row(
            snapshot,
            item.id,
            item.index as u32,
            display_source_range.row,
            display_source_range.source_range,
            display_source_range.source_row_range,
            range_semantics,
            self.document_path(),
        ));
        self.display_cache
            .display_row_cache
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
        let source_range = row_source_range_in_text_snapshot_for_cache(snapshot, row);
        let cache_key = DisplayRowCacheKey {
            version: snapshot.version().clone(),
            item_id: source_display_item_id(&source_range, row as usize),
            item_index: row,
            source_range: source_range.clone(),
            source_row_range: row as usize..row as usize + 1,
            mode: MarkdownEditorMode::Source,
            active_projection_source_ranges: Vec::new(),
        };

        if let Some(display_row) = self.display_cache.display_row_cache.get(&cache_key) {
            return Some(display_row.clone());
        }

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.display_rows_created += 1;
        }
        let display_row = Arc::new(source_display_row_in_text_snapshot(snapshot, row));
        self.display_cache
            .display_row_cache
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
        let content_wrap_width = effective_text_wrap_width(display_row, wrap_width);
        let cache_key = RowLayoutCacheKey {
            item_id: display_row.item_id,
            item_index: display_row.item_index,
            source_range: display_row.source_range.clone(),
            source_row_range: display_row.source_row_range.clone(),
            mode,
            row_style,
            wrap_width: content_wrap_width,
            active_projection_source_ranges: display_row.active_projection_source_ranges.clone(),
        };

        if let Some(cached_layout) = self.display_cache.row_layout_cache.get(&cache_key) {
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
            content_wrap_width,
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
            content_wrap_width,
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
                display_row.item_index as usize,
                &inputs,
                row_style,
                content_wrap_width,
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
                content_wrap_width,
                &atom_measurements,
                window,
                cx,
            )))
        };
        if layout.cacheable() {
            self.display_cache
                .row_layout_cache
                .insert(cache_key, layout.clone());
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
        let table_layout =
            if let Some(table_layout) = self.display_cache.table_layout_cache.get(&cache_key) {
                table_layout.clone()
            } else {
                let table_layout = Arc::new(super::DisplayTableLayout::new(
                    snapshot, table, wrap_width, row_style, window,
                ));
                self.display_cache
                    .table_layout_cache
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
            item_id: display_row.item_id,
            item_index: display_row.item_index,
            source_range: display_row.source_range.clone(),
            source_row_range: display_row.source_row_range.clone(),
            mode: MarkdownEditorMode::Source,
            row_style,
            wrap_width,
            active_projection_source_ranges: Vec::new(),
        };

        if let Some(DisplayRowLayout::Text(cached_layout)) =
            self.display_cache.row_layout_cache.get(&cache_key)
        {
            return cached_layout.clone();
        }

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.row_layouts_created += 1;
        }
        let inputs = self.cached_source_row_layout_inputs(display_row, row_style, window);
        let atom_measurements = self.inline_atom_measurement_states_for_layout(
            display_row.item_index as usize,
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
            self.display_cache
                .row_layout_cache
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

        let snapshot = self.buffer.snapshot();
        let version = snapshot.version().clone();
        let item_count = self.rendered_display_index(&snapshot).item_count();
        if item_count == 0 {
            self.rendered_prewarm = None;
            return;
        }

        let start_item = self.display_list_state.logical_scroll_top().item_ix;
        let reset_queue = self.rendered_prewarm.as_ref().is_none_or(|state| {
            state.version != version
                || state.wrap_width != wrap_width
                || start_item.abs_diff(state.anchor_row) > PREWARM_ANCHOR_RESET_ROWS
        });
        if reset_queue {
            let byte_len = self.buffer.len();
            self.rendered_prewarm = Some(super::RenderedPrewarmState {
                version,
                wrap_width,
                selection,
                anchor_row: start_item,
                rows: cache_prewarm_rows(item_count, byte_len, start_item),
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
        if self.mode != MarkdownEditorMode::Rendered || state.version != *snapshot.version() {
            self.rendered_prewarm = None;
            return;
        }

        let deadline = Instant::now() + PREWARM_FRAME_BUDGET;
        let mut items = Vec::new();
        while items.len() < RENDERED_PREWARM_ROWS_PER_FRAME && Instant::now() < deadline {
            let Some(item) = state.rows.pop_front() else {
                break;
            };
            items.push(item);
        }
        let has_more_rows = !state.rows.is_empty();
        let wrap_width = state.wrap_width;

        let display_row_state =
            rendered_projection_state(&snapshot, Some(&selection), MarkdownEditorMode::Rendered);
        for item in items {
            let Some(display_row) = self.cached_display_row(
                &snapshot,
                item,
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
                item,
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
            item_id: display_row.item_id,
            item_index: display_row.item_index,
            source_range: display_row.source_range.clone(),
            source_row_range: display_row.source_row_range.clone(),
            mode,
            active_projection_source_ranges: display_row.active_projection_source_ranges.clone(),
            row_style,
        };

        if let Some(inputs) = self.display_cache.row_layout_input_cache.get(&cache_key) {
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
        self.display_cache
            .row_layout_input_cache
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
            item_id: display_row.item_id,
            item_index: display_row.item_index,
            source_range: display_row.source_range.clone(),
            source_row_range: display_row.source_row_range.clone(),
            mode: MarkdownEditorMode::Source,
            active_projection_source_ranges: Vec::new(),
            row_style,
        };

        if let Some(inputs) = self.display_cache.row_layout_input_cache.get(&cache_key) {
            return inputs.clone();
        }

        #[cfg(perf_enabled)]
        {
            self.layout_computation_counts.row_layout_inputs_created += 1;
        }
        let inputs = source_display_row_layout_inputs(display_row, row_style, window);
        self.display_cache
            .row_layout_input_cache
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
                if let Some(measurement) = self.inline_atoms.measurement_cache.get(&key).copied() {
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
                                self.inline_atoms
                                    .pending_rows
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
                        self.inline_atoms
                            .pending_rows
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
            .inline_atoms
            .measurement_cache
            .insert(key.clone(), measurement);
        let mut rows_to_remeasure = self
            .inline_atoms
            .pending_rows
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
        self.inline_atoms.pending_remeasure_rows.insert(row);
        if self.inline_atoms.remeasure_scheduled {
            return;
        }

        self.inline_atoms.remeasure_scheduled = true;
        cx.on_next_frame(window, |this, _window, cx| {
            this.flush_inline_atom_row_remeasures(cx);
        });
    }

    pub(crate) fn flush_inline_atom_row_remeasures(&mut self, cx: &mut Context<Self>) {
        self.inline_atoms.remeasure_scheduled = false;
        let mut rows = self
            .inline_atoms
            .pending_remeasure_rows
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
