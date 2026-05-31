use std::{
    collections::HashMap,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
};

#[cfg(test)]
use gpui::FontWeight;
use gpui::{
    App, ClipboardItem, Context, EventEmitter, FocusHandle, Focusable, IntoElement, KeyBinding,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Render, SharedString,
    TextAlign, Window, div, prelude::*, px,
};
#[cfg(test)]
use markdown_wysiwyg::MarkdownTableAlignment;
#[cfg(test)]
use markdown_wysiwyg::{MarkdownBlockKind, MarkdownInlineKind};
use md_assets::EDITOR_FONT_FAMILY;
use md_buffer::{Buffer, BufferSnapshot};
use md_settings::EditorSettings;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point, Selection, SelectionGoal};
use md_theme::{default_row_metrics, editor_palette, gutter_width};

mod block;
mod cache;
mod display_model;
mod display_row_builder;
mod display_space;
mod edit;
mod editor_actions;
mod editor_mouse;
mod editor_render;
mod formula_render;
mod inline_atom;
mod inline_layout;
mod interaction;
mod invalidation;
mod layout;
mod markdown_image;
mod movement;
mod render;
mod rendered_edit;
mod rendered_element;
mod selection;
mod table;
mod virtual_list;
mod visual_row;

use block::DisplayBlockLayout;
#[cfg(test)]
use block::{
    RENDERED_FORMULA_BLOCK_VERTICAL_PADDING, RENDERED_IMAGE_BLOCK_PLACEHOLDER_HEIGHT,
    RENDERED_IMAGE_BLOCK_VERTICAL_PADDING, RenderedFormulaBlock, RenderedFormulaBlockLayout,
    RenderedImageBlock, RenderedImageBlockLayout, image_block_size_for_size,
    image_block_source_offset_for_x, rendered_formula_block_for_row, rendered_image_block_for_row,
    rendered_source_block_layout_for_tests,
};
use cache::{
    DisplayCacheStore, InlineAtomMeasurementStore, RenderedPrewarmState, SourcePrewarmState,
};
use display_model::{DisplayRow, DisplayTextStyle, StyledDisplaySegment};
#[cfg(test)]
use display_row_builder::row_source_range;
pub use display_row_builder::{display_rows, row_text};
#[cfg(test)]
use display_row_builder::{
    display_rows_in_mode, rendered_display_index_for_tests, rendered_display_row_for_item_for_tests,
};
use display_row_builder::{display_rows_in_text_snapshot, row_text_in_text_snapshot};
pub use edit::{backspace_selection, current_line_indent, delete_selection, replace_selection};
use edit::{
    backspace_selection_in_mode, delete_selection_in_mode, insert_newline_in_mode,
    insert_soft_break_in_mode,
};
use inline_atom::{
    DisplayInlineAtom, DisplayInlineAtomKind, DisplayInlineFragment, DisplayInlineRowInputs,
    InlineAtomMeasurementKey, InlineAtomMeasurementState, render_text_piece,
};
#[cfg(test)]
use inline_atom::{
    INLINE_IMAGE_ATOM_SIZE, INLINE_IMAGE_PLACEHOLDER, INLINE_MATH_ATOM_EXTRA_HEIGHT,
    INLINE_MATH_ATOM_HORIZONTAL_PADDING, inline_image_atom_size_for_size,
};
#[cfg(test)]
use inline_layout::display_inline_row_inputs;
use interaction::{mouse_target_for_text_layout, task_checkbox_source_range_for_text_layout_click};
use invalidation::{
    EditLayoutInvalidation, LocalRenderedEditInvalidation, LocalSourceEditInvalidation,
};
use layout::{
    DisplayRowLayout, DisplayRowLayoutInputs, DisplayRowTextLayout, RowDisplayStyle,
    RowLayoutCacheKey, RowLayoutInputCacheKey, VisualDisplayRow, row_display_style_for_display_row,
    text_wrap_width,
};
#[cfg(test)]
use layout::{
    atom_range_containing_display_index, atomic_wrap_boundary_index,
    display_fragments_for_text_layout, display_inline_fragments, forced_break_visual_rows,
    inline_style, line_fragments_for_wrapping, source_display_fragments,
    text_segments_for_fragments, text_wrap_width_for_mode, unwrapped_visual_rows_if_fits,
    visual_row_height_for_range,
};
#[cfg(test)]
use markdown_image::MarkdownImageSource;
use md_projection::{RenderedCaretAffinity, RenderedDisplayIndex, RenderedProjectionState};
#[cfg(test)]
use render::{fragment_text_for_visual_row, selection_bounds_for_visual_row};
use render::{render_display_row_layout, render_row_text};
#[cfg(test)]
use rendered_element::RenderedElementDescriptor;
#[cfg(test)]
use rendered_element::RenderedElementKind;
#[cfg(test)]
use rendered_element::RenderedElementPlacement;
#[cfg(test)]
use rendered_element::rendered_element_range_at_cursor;
#[cfg(test)]
use rendered_element::source_offset_is_rendered_element_boundary;
use rendered_element::{
    active_source_range_for_selection, inactive_rendered_element_source_ranges_for_selection,
    rendered_element_source_range_is_active,
};
#[cfg(test)]
use selection::{
    HorizontalDirection, move_horizontal_in_mode, select_left_in_mode, select_right_in_mode,
};
use selection::{
    TransactionSelectionState, apply_rendered_active_source_range_change,
    apply_text_wrap_width_change, clip_cursor_in_text_snapshot, clip_selection_in_text_snapshot,
    collapsed_selection, collapsed_selection_with_goal, reveal_selection_head_row_in_text_snapshot,
    reveal_selection_item, select_to_point_in_text_snapshot_with_goal, select_to_point_with_goal,
    selection_byte_range_in_text_snapshot, selection_for_source_range, selection_without_goal,
    source_rows_for_active_range_change, transaction_selection_state_without_goals,
};
pub use selection::{
    clip_cursor, clip_selection, move_left, move_right, move_to_beginning_of_line,
    move_to_end_of_line, select_left, select_right, select_to_beginning_of_line,
    select_to_end_of_line, select_to_point, select_vertical, selection_byte_range,
};
#[cfg(test)]
use selection::{move_selection_left, move_vertical};
use table::{DisplayTableLayout, DisplayTableRowLayout, TableLayoutCacheKey};
#[cfg(test)]
use virtual_list::ListOffset;
use virtual_list::{ListAlignment, ListSizingBehavior, MdListState, md_list};
#[cfg(test)]
use visual_row::{
    VisualLineBoundary, desired_visual_x, display_x_for_offset, point_for_display_offset,
    point_for_visual_row_x, source_offset_for_display_offset, visual_horizontal_goal,
    visual_line_boundary_for_caret, visual_row_contains_caret, visual_row_index_containing_caret,
    visual_row_index_for_caret,
};

gpui::actions!(
    md_editor,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        MoveToBeginningOfLine,
        MoveToEndOfLine,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectToBeginningOfLine,
        SelectToEndOfLine,
        SelectAll,
        Copy,
        Paste,
        Cut,
        Backspace,
        Delete,
        InsertNewline,
        InsertSoftBreak,
        Tab,
        ShiftTab,
        ToggleBold,
        ToggleItalic,
        ToggleInlineCode,
        ToggleStrikethrough,
        InsertLink,
        EditLink,
        ToggleSourceRevealCurrentBlock,
        Undo,
        Redo,
    ]
);

fn rendered_projection_state(
    snapshot: &BufferSnapshot,
    selection: Option<&Selection<Point>>,
    mode: MarkdownEditorMode,
) -> RenderedProjectionState {
    if mode != MarkdownEditorMode::Rendered {
        return RenderedProjectionState::default();
    }

    RenderedProjectionState {
        active_source_range: selection
            .and_then(|selection| active_source_range_for_selection(snapshot, selection)),
        inactive_source_ranges: selection
            .map(|selection| {
                inactive_rendered_element_source_ranges_for_selection(snapshot, selection)
            })
            .unwrap_or_default(),
        active_cursor: selection
            .filter(|selection| selection.is_empty())
            .map(|selection| selection.head()),
    }
}

const DISPLAY_LIST_OVERDRAW: gpui::Pixels = px(500.);
const RENDERED_LEFT_RAIL_WIDTH: gpui::Pixels = px(24.);

fn left_rail_width(mode: MarkdownEditorMode) -> gpui::Pixels {
    match mode {
        MarkdownEditorMode::Source => gutter_width(),
        MarkdownEditorMode::Rendered => RENDERED_LEFT_RAIL_WIDTH,
    }
}

/// Construct editor keybindings from the single-source-of-truth in `md_settings`.
///
/// This reads `md_settings::DEFAULT_EDITOR_KEYBINDINGS` and dispatches on
/// the action name to produce typed `gpui::KeyBinding` values.
fn editor_keybindings() -> Vec<KeyBinding> {
    md_settings::DEFAULT_EDITOR_KEYBINDINGS
        .iter()
        .map(|spec| {
            let context = Some(spec.context);
            match spec.action {
                "MoveLeft" => KeyBinding::new(spec.keystroke, MoveLeft, context),
                "MoveRight" => KeyBinding::new(spec.keystroke, MoveRight, context),
                "MoveUp" => KeyBinding::new(spec.keystroke, MoveUp, context),
                "MoveDown" => KeyBinding::new(spec.keystroke, MoveDown, context),
                "MoveToBeginningOfLine" => {
                    KeyBinding::new(spec.keystroke, MoveToBeginningOfLine, context)
                }
                "MoveToEndOfLine" => KeyBinding::new(spec.keystroke, MoveToEndOfLine, context),
                "SelectLeft" => KeyBinding::new(spec.keystroke, SelectLeft, context),
                "SelectRight" => KeyBinding::new(spec.keystroke, SelectRight, context),
                "SelectUp" => KeyBinding::new(spec.keystroke, SelectUp, context),
                "SelectDown" => KeyBinding::new(spec.keystroke, SelectDown, context),
                "SelectToBeginningOfLine" => {
                    KeyBinding::new(spec.keystroke, SelectToBeginningOfLine, context)
                }
                "SelectToEndOfLine" => KeyBinding::new(spec.keystroke, SelectToEndOfLine, context),
                "SelectAll" => KeyBinding::new(spec.keystroke, SelectAll, context),
                "Copy" => KeyBinding::new(spec.keystroke, Copy, context),
                "Paste" => KeyBinding::new(spec.keystroke, Paste, context),
                "Cut" => KeyBinding::new(spec.keystroke, Cut, context),
                "Backspace" => KeyBinding::new(spec.keystroke, Backspace, context),
                "Delete" => KeyBinding::new(spec.keystroke, Delete, context),
                "InsertNewline" => KeyBinding::new(spec.keystroke, InsertNewline, context),
                "InsertSoftBreak" => KeyBinding::new(spec.keystroke, InsertSoftBreak, context),
                "Tab" => KeyBinding::new(spec.keystroke, Tab, context),
                "ShiftTab" => KeyBinding::new(spec.keystroke, ShiftTab, context),
                "ToggleBold" => KeyBinding::new(spec.keystroke, ToggleBold, context),
                "ToggleItalic" => KeyBinding::new(spec.keystroke, ToggleItalic, context),
                "ToggleInlineCode" => KeyBinding::new(spec.keystroke, ToggleInlineCode, context),
                "ToggleStrikethrough" => {
                    KeyBinding::new(spec.keystroke, ToggleStrikethrough, context)
                }
                "InsertLink" => KeyBinding::new(spec.keystroke, InsertLink, context),
                "EditLink" => KeyBinding::new(spec.keystroke, EditLink, context),
                "ToggleSourceRevealCurrentBlock" => {
                    KeyBinding::new(spec.keystroke, ToggleSourceRevealCurrentBlock, context)
                }
                "Undo" => KeyBinding::new(spec.keystroke, Undo, context),
                "Redo" => KeyBinding::new(spec.keystroke, Redo, context),
                _ => panic!(
                    "unknown editor action in DEFAULT_EDITOR_KEYBINDINGS: {}",
                    spec.action
                ),
            }
        })
        .collect()
}

pub fn init_standalone(cx: &mut App) {
    cx.bind_keys(editor_keybindings());
}

pub struct MarkdownEditor {
    buffer: Buffer,
    document_path: Option<PathBuf>,
    focus_handle: FocusHandle,
    display_list_state: MdListState,
    mode: MarkdownEditorMode,
    selection: Selection<Point>,
    is_selecting_with_mouse: bool,
    rendered_drag_projection_state: Option<RenderedProjectionState>,
    selection_history: HashMap<md_text::TransactionId, TransactionSelectionState>,
    settings: EditorSettings,
    last_text_wrap_width: Option<gpui::Pixels>,
    display_cache: DisplayCacheStore,
    inline_atoms: InlineAtomMeasurementStore,
    source_prewarm: Option<SourcePrewarmState>,
    rendered_prewarm: Option<RenderedPrewarmState>,
    #[cfg(perf_enabled)]
    layout_computation_counts: LayoutComputationCounts,
}

#[cfg(perf_enabled)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LayoutComputationCounts {
    pub(crate) display_rows_created: usize,
    pub(crate) row_layout_inputs_created: usize,
    pub(crate) row_layouts_created: usize,
    pub(crate) text_shaping_calls: usize,
    pub(crate) rendered_block_queries: usize,
    pub(crate) rendered_inline_span_queries: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum MarkdownEditorMode {
    Source,
    Rendered,
}

impl MarkdownEditorMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Source => "Source",
            Self::Rendered => "Rendered",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            Self::Source => Self::Rendered,
            Self::Rendered => Self::Source,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownEditorEvent {
    DirtyChanged(bool),
}

impl EventEmitter<MarkdownEditorEvent> for MarkdownEditor {}

impl MarkdownEditor {
    pub fn new(buffer: Buffer, cx: &mut Context<Self>) -> Self {
        let row_count = buffer.as_text_snapshot().row_count() as usize;
        let row_size_hint = gpui::size(px(0.), default_row_metrics().min_height);
        Self {
            buffer,
            document_path: None,
            focus_handle: cx.focus_handle(),
            display_list_state: MdListState::new(
                row_count,
                ListAlignment::Top,
                DISPLAY_LIST_OVERDRAW,
            )
            .with_default_size_hint(row_size_hint),
            mode: MarkdownEditorMode::Source,
            selection: collapsed_selection(Point::zero()),
            is_selecting_with_mouse: false,
            rendered_drag_projection_state: None,
            selection_history: HashMap::default(),
            settings: EditorSettings::default(),
            last_text_wrap_width: None,
            display_cache: DisplayCacheStore::default(),
            inline_atoms: InlineAtomMeasurementStore::default(),
            source_prewarm: None,
            rendered_prewarm: None,
            #[cfg(perf_enabled)]
            layout_computation_counts: LayoutComputationCounts::default(),
        }
    }

    pub fn for_text(text: impl Into<String>, cx: &mut Context<Self>) -> Self {
        Self::new(Buffer::local(text), cx)
    }

    #[cfg(perf_enabled)]
    pub(crate) fn reset_layout_computation_counts(&mut self) {
        self.layout_computation_counts = LayoutComputationCounts::default();
    }

    #[cfg(perf_enabled)]
    pub(crate) fn layout_computation_counts(&self) -> LayoutComputationCounts {
        self.layout_computation_counts
    }

    pub fn for_text_with_document_path(
        text: impl Into<String>,
        path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut editor = Self::for_text(text, cx);
        editor.document_path = path;
        editor
    }

    pub fn document_path(&self) -> Option<&Path> {
        self.document_path.as_deref()
    }

    pub fn set_document_path(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.document_path == path {
            return;
        }

        self.document_path = path;
        self.clear_display_row_cache();
        self.clear_row_layout_cache();
        self.clear_inline_atom_measurement_cache();
        self.display_list_state.remeasure();
        cx.notify();
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffer
    }

    pub fn is_dirty(&self) -> bool {
        self.buffer.is_dirty()
    }

    pub fn serialized_text(&self) -> String {
        self.buffer.serialized_text()
    }

    pub fn mode(&self) -> MarkdownEditorMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: MarkdownEditorMode, cx: &mut Context<Self>) {
        if self.mode == mode {
            return;
        }

        let row_count_before = self.display_list_state.item_count();
        self.mode = mode;
        self.selection = selection_without_goal(&self.selection);
        self.rendered_drag_projection_state = None;
        self.clear_display_row_cache();
        self.clear_row_layout_cache();
        self.sync_display_list_state(row_count_before, &self.selection.clone(), None, false);
        self.reveal_cursor_row();
        cx.notify();
    }

    pub fn toggle_mode(&mut self, cx: &mut Context<Self>) {
        self.set_mode(self.mode.toggle(), cx);
    }

    fn current_rendered_projection_state(
        &self,
        snapshot: &BufferSnapshot,
        selection: &Selection<Point>,
    ) -> RenderedProjectionState {
        if self.mode == MarkdownEditorMode::Rendered
            && let Some(state) = self.rendered_drag_projection_state.as_ref()
        {
            return state.clone();
        }

        rendered_projection_state(snapshot, Some(selection), self.mode)
    }

    fn freeze_rendered_drag_projection(&mut self, snapshot: &BufferSnapshot) {
        if self.mode != MarkdownEditorMode::Rendered
            || self.rendered_drag_projection_state.is_some()
        {
            return;
        }

        let selection = clip_selection(snapshot, &self.selection);
        self.rendered_drag_projection_state = Some(rendered_projection_state(
            snapshot,
            Some(&selection),
            self.mode,
        ));
    }

    pub fn mark_saved(&mut self, cx: &mut Context<Self>) {
        self.buffer.did_save_at_current_version();
        self.emit_dirty_state(cx);
    }

    pub fn row_count(&mut self) -> u32 {
        self.buffer.as_text_snapshot().row_count()
    }

    pub(crate) fn display_item_count_for_mode(&mut self, mode: MarkdownEditorMode) -> usize {
        match mode {
            MarkdownEditorMode::Source => display_space::display_item_count_for_mode(
                mode,
                self.buffer.as_text_snapshot(),
                None,
            ),
            MarkdownEditorMode::Rendered => {
                let snapshot = self.buffer.snapshot();
                let index = self.rendered_display_index(&snapshot);
                display_space::display_item_count_for_mode(
                    mode,
                    snapshot.as_text_snapshot(),
                    Some(&index),
                )
            }
        }
    }

    pub(crate) fn display_item_index_for_cursor(
        &mut self,
        snapshot: &BufferSnapshot,
        cursor: Point,
        mode: MarkdownEditorMode,
    ) -> Option<usize> {
        match mode {
            MarkdownEditorMode::Source => {
                display_space::display_item_index_for_cursor(mode, None, cursor)
            }
            MarkdownEditorMode::Rendered => {
                let index = self.rendered_display_index(snapshot);
                display_space::display_item_index_for_cursor(mode, Some(&index), cursor)
            }
        }
    }

    pub(crate) fn rendered_display_index(
        &mut self,
        snapshot: &BufferSnapshot,
    ) -> Arc<RenderedDisplayIndex> {
        self.display_cache.rendered_index(snapshot)
    }

    pub fn row_text(&mut self, row: u32) -> String {
        row_text_in_text_snapshot(self.buffer.as_text_snapshot(), row)
    }

    pub fn display_rows(&mut self, range: Range<usize>) -> Vec<DisplayRow> {
        display_rows_in_text_snapshot(self.buffer.as_text_snapshot(), range)
    }

    pub fn cursor(&self) -> Point {
        self.selection.head()
    }

    pub fn selection(&self) -> &Selection<Point> {
        &self.selection
    }

    pub fn set_cursor(&mut self, cursor: Point) {
        let previous_selection = self.selection.clone();
        let cursor = clip_cursor_in_text_snapshot(self.buffer.as_text_snapshot(), cursor);
        self.selection = collapsed_selection(
            self.normalize_rendered_caret(cursor, RenderedCaretAffinity::After),
        );
        self.sync_rendered_rows_for_selection_change(&previous_selection);
        self.reveal_cursor_row();
    }

    pub(crate) fn notify_after_selection_change(
        &mut self,
        previous_selection: &Selection<Point>,
        cx: &mut Context<Self>,
    ) {
        self.sync_rendered_rows_for_selection_change(previous_selection);
        self.reveal_cursor_row();
        cx.notify();
    }

    fn sync_display_list_state(
        &mut self,
        row_count_before: usize,
        previous_selection: &Selection<Point>,
        row_count_after: Option<usize>,
        skip_selection_sync: bool,
    ) {
        let row_count_after =
            row_count_after.unwrap_or_else(|| self.display_item_count_for_mode(self.mode));
        if row_count_before != row_count_after {
            if let Some((old_range, count)) = row_count_change_splice(
                row_count_before,
                row_count_after,
                previous_selection,
                &self.selection,
            ) {
                self.display_list_state.splice(old_range, count);
            } else {
                self.display_list_state
                    .splice(0..row_count_before, row_count_after);
            }
            return;
        }
        if !skip_selection_sync {
            self.sync_rendered_rows_for_selection_change(previous_selection);
        }
    }

    fn remeasure_rendered_items_for_selection_change(
        &mut self,
        previous_selection: &Selection<Point>,
    ) {
        let snapshot = self.buffer.snapshot();
        let index = self.rendered_display_index(&snapshot);
        let mut items = [
            previous_selection.start,
            previous_selection.end,
            self.selection.start,
            self.selection.end,
        ]
        .into_iter()
        .filter_map(|point| {
            let cursor = clip_cursor(&snapshot, point);
            let source_offset = snapshot.as_text_snapshot().point_to_offset(cursor);
            index.item_index_for_source_offset(&snapshot, source_offset)
        })
        .collect::<Vec<_>>();
        items.sort_unstable();
        items.dedup();

        if items.is_empty() {
            self.display_list_state.remeasure();
            return;
        }

        for item in items {
            self.display_list_state
                .remeasure_items(item..item.saturating_add(1));
        }
    }

    fn sync_rendered_rows_for_selection_change(&mut self, previous_selection: &Selection<Point>) {
        if self.mode != MarkdownEditorMode::Rendered {
            return;
        }

        let snapshot = self.buffer.snapshot();
        let previous_active = active_source_range_for_selection(&snapshot, previous_selection);
        let current_active = active_source_range_for_selection(&snapshot, &self.selection);
        let current_goal = self.selection.goal;
        let preserve_wrapped_visual_goal =
            matches!(current_goal, SelectionGoal::WrappedHorizontalPosition(_))
                && self.selection_rows_share_rendered_item(&snapshot, previous_selection);
        let active_rows = source_rows_for_active_range_change(
            &snapshot,
            previous_active.as_ref(),
            current_active.as_ref(),
        );
        if apply_rendered_active_source_range_change(
            &mut self.selection,
            previous_active.as_ref(),
            current_active.as_ref(),
        ) {
            self.clear_display_row_cache_for_row_ranges(&active_rows);
            self.clear_row_layout_cache_for_row_ranges(&active_rows);
            if preserve_wrapped_visual_goal {
                self.selection.goal = current_goal;
            }
        }
        for rows in active_rows {
            self.display_list_state.remeasure_items(rows);
        }
    }

    fn selection_rows_share_rendered_item(
        &mut self,
        snapshot: &BufferSnapshot,
        previous_selection: &Selection<Point>,
    ) -> bool {
        let previous_row = previous_selection.head().row;
        let current_row = self.selection.head().row;
        if previous_row == current_row {
            return true;
        }

        let index = self.rendered_display_index(snapshot);
        index.item_index_for_source_row(previous_row as usize)
            == index.item_index_for_source_row(current_row as usize)
    }

    fn emit_dirty_state(&mut self, cx: &mut Context<Self>) {
        cx.emit(MarkdownEditorEvent::DirtyChanged(self.buffer.is_dirty()));
        cx.notify();
    }

    fn reveal_cursor_row(&mut self) {
        if self.mode == MarkdownEditorMode::Rendered {
            let snapshot = self.buffer.snapshot();
            let cursor = clip_cursor(&snapshot, self.selection.head());
            let index = self.rendered_display_index(&snapshot);
            let item_index =
                display_space::rendered_item_index_for_cursor(&snapshot, &index, cursor);
            reveal_selection_item(&self.display_list_state, item_index);
        } else {
            reveal_selection_head_row_in_text_snapshot(
                &self.display_list_state,
                self.buffer.as_text_snapshot(),
                &self.selection,
            );
        }
    }

    fn reveal_cursor_row_with_rendered_index(&mut self, index: Option<&RenderedDisplayIndex>) {
        if self.mode == MarkdownEditorMode::Rendered
            && let Some(index) = index
        {
            let cursor =
                clip_cursor_in_text_snapshot(self.buffer.as_text_snapshot(), self.selection.head());
            let item_index = display_space::rendered_item_index_for_cursor_in_text_snapshot(
                self.buffer.as_text_snapshot(),
                index,
                cursor,
            );
            reveal_selection_item(&self.display_list_state, item_index);
        } else {
            self.reveal_cursor_row();
        }
    }
}

impl Focusable for MarkdownEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn local_edit_invalidation_rows(
    mode: MarkdownEditorMode,
    row_count_before: usize,
    row_count_after: usize,
    previous_selection: &Selection<Point>,
    current_selection: &Selection<Point>,
    rendered_index: Option<&RenderedDisplayIndex>,
) -> Option<Range<usize>> {
    if row_count_before != row_count_after {
        return None;
    }

    match mode {
        MarkdownEditorMode::Source => {
            let row = previous_selection.start.row;
            if previous_selection.end.row != row
                || current_selection.start.row != row
                || current_selection.end.row != row
            {
                return None;
            }

            let row = row as usize;
            if row >= row_count_after {
                return None;
            }

            Some(row..row.saturating_add(1))
        }
        MarkdownEditorMode::Rendered => {
            let index = rendered_index?;
            let rows = [
                previous_selection.start.row,
                previous_selection.end.row,
                current_selection.start.row,
                current_selection.end.row,
            ];
            let item_index = index.item_index_for_source_row(rows[0] as usize)?;
            if rows
                .into_iter()
                .any(|row| index.item_index_for_source_row(row as usize) != Some(item_index))
            {
                return None;
            }

            let item = index.item(item_index)?;
            Some(item.row_range.clone())
        }
    }
}

fn row_count_change_splice(
    row_count_before: usize,
    row_count_after: usize,
    previous_selection: &Selection<Point>,
    current_selection: &Selection<Point>,
) -> Option<(Range<usize>, usize)> {
    if row_count_before == 0 || row_count_before == row_count_after {
        return None;
    }

    let start = [
        previous_selection.start.row,
        previous_selection.end.row,
        current_selection.start.row,
        current_selection.end.row,
    ]
    .into_iter()
    .map(|row| row as usize)
    .min()?
    .min(row_count_before.saturating_sub(1));

    let delta = row_count_after as isize - row_count_before as isize;
    let old_count = if delta < 0 {
        1 + delta.unsigned_abs()
    } else {
        1
    };
    let new_count = if delta > 0 { 1 + delta as usize } else { 1 };
    let old_end = start.saturating_add(old_count).min(row_count_before);

    Some((start..old_end, new_count))
}

fn range_contains(container: &Range<usize>, candidate: &Range<usize>) -> bool {
    container.start <= candidate.start && container.end >= candidate.end
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn selected_range_for_row_in_text_snapshot(
    snapshot: &TextBufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
) -> Option<Range<usize>> {
    if selection.is_empty() {
        return None;
    }

    let selection_range = selection_byte_range_in_text_snapshot(snapshot, selection);
    let row_range = display_row.projection.visible_source_range();
    if !selection_intersects_visible_row_range(&selection_range, &row_range) {
        return None;
    }

    let start = selection_range.start.max(row_range.start);
    let end = selection_range.end.min(row_range.end);
    let start = display_row.source_to_display(start);
    let end = display_row.source_to_display(end);

    Some(start.min(end)..end.max(start))
}

fn selection_intersects_visible_row_range(
    selection_range: &Range<usize>,
    row_range: &Range<usize>,
) -> bool {
    if row_range.is_empty() {
        selection_range.start <= row_range.start && row_range.start < selection_range.end
    } else {
        selection_range.end > row_range.start && selection_range.start < row_range.end
    }
}

fn source_range_to_row_range(
    snapshot: &BufferSnapshot,
    source_range: &Range<usize>,
) -> Option<Range<usize>> {
    if source_range.start >= source_range.end {
        return None;
    }

    let text_snapshot = snapshot.as_text_snapshot();
    let start_point = text_snapshot.offset_to_point(source_range.start);
    let end_point = text_snapshot.offset_to_point(source_range.end.saturating_sub(1));
    let start_row = start_point.row as usize;
    let end_row_exclusive = end_point.row as usize + 1;
    Some(start_row..end_row_exclusive)
}

fn merge_overlapping_row_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    if ranges.is_empty() {
        return ranges;
    }

    ranges.sort_by_key(|range| (range.start, range.end));
    let mut merged = Vec::with_capacity(ranges.len());
    let mut current = ranges[0].clone();

    for range in ranges.into_iter().skip(1) {
        if range.start <= current.end {
            current.end = current.end.max(range.end);
        } else {
            merged.push(current);
            current = range;
        }
    }
    merged.push(current);
    merged
}

#[cfg(test)]
mod test_support {
    pub(super) use gpui::{LineFragment, TextRun, font};

    pub(super) use super::*;
    pub(super) use crate::display_model::{
        RenderedAdornmentKind, RenderedAdornmentPlacement, RenderedBackgroundKind,
        RenderedContainerKind,
    };
    pub(super) use crate::layout::text_runs_on_char_boundaries;
    pub(super) use crate::render::caret_position_for_visual_row;

    pub(super) fn image_descriptor(
        source_range: Range<usize>,
        url: impl Into<String>,
        alt_text: impl Into<String>,
        placement: RenderedElementPlacement,
    ) -> RenderedElementDescriptor {
        RenderedElementDescriptor {
            kind: RenderedElementKind::Image {
                image_source: MarkdownImageSource::resolve(url, alt_text, None),
            },
            placement,
            source_range,
        }
    }

    pub(super) fn markdown_image_source(
        url: impl Into<String>,
        alt_text: impl Into<String>,
    ) -> MarkdownImageSource {
        MarkdownImageSource::resolve(url, alt_text, None)
    }

    pub(super) fn math_descriptor(
        source_range: Range<usize>,
        tex: impl Into<String>,
    ) -> RenderedElementDescriptor {
        RenderedElementDescriptor {
            kind: RenderedElementKind::Math { tex: tex.into() },
            placement: RenderedElementPlacement::Inline,
            source_range,
        }
    }

    pub(super) fn rendered_formula_block(
        source_range: Range<usize>,
        tex: impl Into<String>,
    ) -> RenderedFormulaBlock {
        let tex = tex.into();
        RenderedFormulaBlock {
            descriptor: RenderedElementDescriptor {
                kind: RenderedElementKind::Math { tex: tex.clone() },
                placement: RenderedElementPlacement::Block,
                source_range: source_range.clone(),
            },
            tex,
            source_range,
        }
    }

    pub(super) fn image_block_layout(
        source_range: Range<usize>,
        width: gpui::Pixels,
    ) -> DisplayBlockLayout {
        DisplayBlockLayout::RemoteImage(RenderedImageBlockLayout {
            image_block: rendered_image_block(source_range, "alt"),
            width,
            image_height: px(120.),
            cacheable: true,
        })
    }

    pub(super) fn formula_block_layout(
        source_range: Range<usize>,
        width: gpui::Pixels,
    ) -> DisplayBlockLayout {
        DisplayBlockLayout::Formula(RenderedFormulaBlockLayout {
            formula_block: rendered_formula_block(source_range, "x + y"),
            width,
            height: px(36.),
            rendered_formula: None,
            cacheable: true,
        })
    }

    pub(super) fn rendered_image_block(
        source_range: Range<usize>,
        alt_text: impl Into<String>,
    ) -> RenderedImageBlock {
        let alt_text = alt_text.into();
        RenderedImageBlock {
            descriptor: image_descriptor(
                source_range.clone(),
                "https://example.com/cat.png",
                alt_text.clone(),
                RenderedElementPlacement::Block,
            ),
            image_source: MarkdownImageSource::resolve(
                "https://example.com/cat.png",
                alt_text,
                None,
            ),
            source_range,
        }
    }

    pub(super) fn cached_row_text_for_current_selection(
        editor: &mut MarkdownEditor,
        row: usize,
    ) -> String {
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            editor.current_rendered_projection_state(&snapshot, &editor.selection);
        editor
            .cached_display_row(&snapshot, row, editor.mode, &display_row_state)
            .expect("display row should exist")
            .text
            .clone()
    }
}

#[cfg(test)]
#[path = "lib_test/interaction_tests.rs"]
mod interaction_tests;
#[cfg(test)]
#[path = "lib_test/layout_tests.rs"]
mod layout_tests;
#[cfg(perf_enabled)]
#[path = "lib_test/perf_tests.rs"]
mod perf_tests;
#[cfg(test)]
#[path = "lib_test/render_tests.rs"]
mod render_tests;
#[cfg(test)]
#[path = "lib_test/visual_row_tests.rs"]
mod visual_row_tests;
