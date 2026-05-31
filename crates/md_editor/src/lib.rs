use std::{
    collections::{HashMap, HashSet, VecDeque},
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
mod edit;
mod formula_render;
mod inline_atom;
mod inline_layout;
mod interaction;
mod layout;
mod markdown_image;
mod movement;
mod render;
mod rendered_element;
mod rendered_index;
mod rendered_topology;
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
use layout::{
    DisplayRowCacheKey, DisplayRowLayout, DisplayRowLayoutInputs, DisplayRowProjectionState,
    DisplayRowTextLayout, RowLayoutCacheKey, RowLayoutInputCacheKey, VisualDisplayRow,
    row_display_style_for_display_row, text_wrap_width,
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
use rendered_index::RenderedDisplayIndex;
use rendered_topology::RenderedCaretAffinity;
#[cfg(test)]
use selection::{
    HorizontalDirection, move_horizontal_in_mode, select_left_in_mode, select_right_in_mode,
};
use selection::{
    apply_rendered_active_source_range_change, apply_text_wrap_width_change,
    clip_cursor_in_text_snapshot, clip_selection_in_text_snapshot, collapsed_selection,
    collapsed_selection_with_goal, reveal_selection_head_row_in_text_snapshot,
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
        Undo,
        Redo,
    ]
);

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
    selection_history: HashMap<md_text::TransactionId, TransactionSelectionState>,
    settings: EditorSettings,
    last_text_wrap_width: Option<gpui::Pixels>,
    display_row_cache: HashMap<DisplayRowCacheKey, Arc<DisplayRow>>,
    row_layout_input_cache: HashMap<RowLayoutInputCacheKey, DisplayRowLayoutInputs>,
    row_layout_cache: HashMap<RowLayoutCacheKey, DisplayRowLayout>,
    table_layout_cache: HashMap<TableLayoutCacheKey, Arc<DisplayTableLayout>>,
    inline_atom_measurement_cache: HashMap<InlineAtomMeasurementKey, InlineAtomMeasurementState>,
    pending_inline_atom_rows: HashMap<InlineAtomMeasurementKey, HashSet<usize>>,
    pending_inline_atom_remeasure_rows: HashSet<usize>,
    inline_atom_remeasure_scheduled: bool,
    source_prewarm: Option<SourcePrewarmState>,
    rendered_prewarm: Option<RenderedPrewarmState>,
    rendered_display_index: Option<Arc<RenderedDisplayIndex>>,
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransactionSelectionState {
    before: Selection<Point>,
    after: Selection<Point>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditLayoutInvalidation {
    Conservative,
    LocalSourceSelection { byte_delta: Option<isize> },
}

#[derive(Clone, Debug, PartialEq)]
struct LocalSourceEditInvalidation {
    rows: Range<usize>,
    byte_delta: Option<isize>,
}

struct SourcePrewarmState {
    version: md_text::Global,
    wrap_width: gpui::Pixels,
    row_style: RowDisplayStyle,
    anchor_row: usize,
    rows: VecDeque<usize>,
    scheduled: bool,
}

struct RenderedPrewarmState {
    version: md_text::Global,
    wrap_width: gpui::Pixels,
    #[allow(dead_code)]
    selection: Selection<Point>,
    anchor_row: usize,
    rows: VecDeque<usize>,
    scheduled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct RowDisplayStyle {
    min_height: gpui::Pixels,
    text_size: gpui::Pixels,
    line_height: gpui::Pixels,
    caret_height: gpui::Pixels,
}

impl From<md_theme::RowMetrics> for RowDisplayStyle {
    fn from(metrics: md_theme::RowMetrics) -> Self {
        Self {
            min_height: metrics.min_height,
            text_size: metrics.text_size,
            line_height: metrics.line_height,
            caret_height: metrics.caret_height,
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
            selection_history: HashMap::default(),
            settings: EditorSettings::default(),
            last_text_wrap_width: None,
            display_row_cache: HashMap::default(),
            row_layout_input_cache: HashMap::default(),
            row_layout_cache: HashMap::default(),
            table_layout_cache: HashMap::default(),
            inline_atom_measurement_cache: HashMap::default(),
            pending_inline_atom_rows: HashMap::default(),
            pending_inline_atom_remeasure_rows: HashSet::default(),
            inline_atom_remeasure_scheduled: false,
            source_prewarm: None,
            rendered_prewarm: None,
            rendered_display_index: None,
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
        self.inline_atom_measurement_cache.clear();
        self.pending_inline_atom_rows.clear();
        self.pending_inline_atom_remeasure_rows.clear();
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
        self.clear_display_row_cache();
        self.clear_row_layout_cache();
        self.sync_display_list_state(row_count_before, &self.selection.clone());
        self.reveal_cursor_row();
        cx.notify();
    }

    pub fn toggle_mode(&mut self, cx: &mut Context<Self>) {
        self.set_mode(self.mode.toggle(), cx);
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
            MarkdownEditorMode::Source => self.buffer.as_text_snapshot().row_count() as usize,
            MarkdownEditorMode::Rendered => {
                let snapshot = self.buffer.snapshot();
                self.rendered_display_index(&snapshot).item_count()
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
            MarkdownEditorMode::Source => Some(cursor.row as usize),
            MarkdownEditorMode::Rendered => self
                .rendered_display_index(snapshot)
                .item_index_for_source_row(cursor.row as usize),
        }
    }

    pub(crate) fn rendered_display_index(
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

    pub fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        let max_point = self.buffer.as_text_snapshot().max_point();
        self.selection = Selection {
            id: 0,
            start: Point::zero(),
            end: max_point,
            reversed: false,
            goal: SelectionGoal::None,
        };
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = self.selected_text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    pub fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) else {
            return;
        };
        self.replace_current_selection(&text, cx);
    }

    pub fn cut(&mut self, _: &Cut, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.selected_text() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.replace_current_selection("", cx);
    }

    pub fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let buffer_len_before = self.buffer.len();
        let (selection, transaction_id) =
            backspace_selection_in_mode(&mut self.buffer, &self.selection, self.mode);
        let changed = transaction_id.is_some();
        let byte_delta = buffer_byte_delta(buffer_len_before, self.buffer.len());
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(
            changed,
            row_count_before,
            &previous_selection,
            EditLayoutInvalidation::LocalSourceSelection { byte_delta },
            cx,
        );
    }

    pub fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let buffer_len_before = self.buffer.len();
        let (selection, transaction_id) =
            delete_selection_in_mode(&mut self.buffer, &self.selection, self.mode);
        let changed = transaction_id.is_some();
        let byte_delta = buffer_byte_delta(buffer_len_before, self.buffer.len());
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(
            changed,
            row_count_before,
            &previous_selection,
            EditLayoutInvalidation::LocalSourceSelection { byte_delta },
            cx,
        );
    }

    pub fn insert_newline(&mut self, _: &InsertNewline, _: &mut Window, cx: &mut Context<Self>) {
        self.insert_line_break(cx, false);
    }

    pub fn insert_soft_break(
        &mut self,
        _: &InsertSoftBreak,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.insert_line_break(cx, true);
    }

    pub fn tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        let tab_text = if self.settings.use_soft_tabs {
            " ".repeat(self.settings.tab_size)
        } else {
            "\t".to_string()
        };
        self.replace_current_selection(&tab_text, cx);
    }

    /// Update editor settings at runtime.
    pub fn set_settings(&mut self, settings: EditorSettings, _cx: &mut Context<Self>) {
        self.settings = settings;
    }

    /// Get a reference to the current editor settings.
    pub fn settings(&self) -> &EditorSettings {
        &self.settings
    }

    pub fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let buffer_len_before = self.buffer.len();
        let mut changed = false;
        let mut invalidation = EditLayoutInvalidation::Conservative;
        if let Some(transaction_id) = self.buffer.undo() {
            let fallback = collapsed_selection(clip_cursor_in_text_snapshot(
                self.buffer.as_text_snapshot(),
                self.cursor(),
            ));
            let previous = self
                .selection_history
                .get(&transaction_id)
                .map(|state| state.before.clone());
            if previous.is_some() {
                let byte_delta = buffer_byte_delta(buffer_len_before, self.buffer.len());
                invalidation = EditLayoutInvalidation::LocalSourceSelection { byte_delta };
            }
            self.selection = previous.unwrap_or(fallback);
            changed = true;
        }
        self.notify_after_edit(
            changed,
            row_count_before,
            &previous_selection,
            invalidation,
            cx,
        );
    }

    pub fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let buffer_len_before = self.buffer.len();
        let mut changed = false;
        let mut invalidation = EditLayoutInvalidation::Conservative;
        if let Some(transaction_id) = self.buffer.redo() {
            let fallback = collapsed_selection(clip_cursor_in_text_snapshot(
                self.buffer.as_text_snapshot(),
                self.cursor(),
            ));
            let next = self
                .selection_history
                .get(&transaction_id)
                .map(|state| state.after.clone());
            if next.is_some() {
                let byte_delta = buffer_byte_delta(buffer_len_before, self.buffer.len());
                invalidation = EditLayoutInvalidation::LocalSourceSelection { byte_delta };
            }
            self.selection = next.unwrap_or(fallback);
            changed = true;
        }
        self.notify_after_edit(
            changed,
            row_count_before,
            &previous_selection,
            invalidation,
            cx,
        );
    }

    fn key_down(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.modifiers.control
            || event.keystroke.modifiers.platform
            || event.keystroke.modifiers.function
            || event.keystroke.modifiers.alt
        {
            return;
        }

        let Some(text) = event.keystroke.key_char.as_deref() else {
            return;
        };
        if text.is_empty() {
            return;
        }

        cx.stop_propagation();
        self.replace_current_selection(text, cx);
    }

    fn selected_text(&self) -> Option<String> {
        let snapshot = self.buffer.as_text_snapshot();
        let selection = clip_selection_in_text_snapshot(snapshot, &self.selection);
        if selection.is_empty() {
            return None;
        }

        let range = selection_byte_range_in_text_snapshot(snapshot, &selection);
        Some(snapshot.text_for_range(range).collect())
    }

    fn replace_current_selection(&mut self, text: &str, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let buffer_len_before = self.buffer.len();
        let (selection, transaction_id) =
            replace_selection(&mut self.buffer, &self.selection, text);
        let changed = transaction_id.is_some();
        let byte_delta = buffer_byte_delta(buffer_len_before, self.buffer.len());
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(
            changed,
            row_count_before,
            &previous_selection,
            EditLayoutInvalidation::LocalSourceSelection { byte_delta },
            cx,
        );
    }

    fn insert_line_break(&mut self, cx: &mut Context<Self>, soft_break: bool) {
        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let buffer_len_before = self.buffer.len();
        let (selection, transaction_id) = if soft_break {
            insert_soft_break_in_mode(&mut self.buffer, &self.selection, self.mode)
        } else {
            insert_newline_in_mode(&mut self.buffer, &self.selection, self.mode)
        };
        let changed = transaction_id.is_some();
        let byte_delta = buffer_byte_delta(buffer_len_before, self.buffer.len());
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(
            changed,
            row_count_before,
            &previous_selection,
            EditLayoutInvalidation::LocalSourceSelection { byte_delta },
            cx,
        );
    }

    fn toggle_task_checkbox_source_range(
        &mut self,
        source_range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> bool {
        let (marker, marker_selection) = {
            let snapshot = self.buffer.snapshot();
            let text_snapshot = snapshot.as_text_snapshot();
            let marker = text_snapshot
                .text_for_range(source_range.clone())
                .collect::<String>();
            let marker_selection =
                selection_for_source_range(&snapshot, self.selection.id, source_range);
            (marker, marker_selection)
        };

        let replacement = match marker.as_str() {
            "[ ]" => "[x]",
            "[x]" | "[X]" => "[ ]",
            _ => return false,
        };

        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let buffer_len_before = self.buffer.len();
        let (_selection, transaction_id) =
            replace_selection(&mut self.buffer, &marker_selection, replacement);
        let changed = transaction_id.is_some();
        let byte_delta = buffer_byte_delta(buffer_len_before, self.buffer.len());
        self.selection = selection_before.clone();
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(
            changed,
            row_count_before,
            &previous_selection,
            EditLayoutInvalidation::LocalSourceSelection { byte_delta },
            cx,
        );
        changed
    }

    fn record_selection_history(
        &mut self,
        transaction_id: Option<md_text::TransactionId>,
        before: Selection<Point>,
        after: Selection<Point>,
    ) {
        let Some(transaction_id) = transaction_id else {
            return;
        };
        self.selection_history.insert(
            transaction_id,
            transaction_selection_state_without_goals(before, after),
        );
    }

    fn notify_after_edit(
        &mut self,
        changed: bool,
        row_count_before: usize,
        previous_selection: &Selection<Point>,
        invalidation: EditLayoutInvalidation,
        cx: &mut Context<Self>,
    ) {
        let row_count_after = self.buffer.as_text_snapshot().row_count() as usize;
        let local_source_edit_invalidation = if changed {
            match invalidation {
                EditLayoutInvalidation::LocalSourceSelection { byte_delta } => {
                    local_source_edit_invalidation_rows(
                        self.mode,
                        row_count_before,
                        row_count_after,
                        previous_selection,
                        &self.selection,
                    )
                    .map(|rows| LocalSourceEditInvalidation { rows, byte_delta })
                }
                EditLayoutInvalidation::Conservative => None,
            }
        } else {
            None
        };

        if changed {
            if self.mode == MarkdownEditorMode::Rendered {
                self.table_layout_cache.clear();
            }
            if let Some(invalidation) = local_source_edit_invalidation.as_ref() {
                let version = self.buffer.as_text_snapshot().version().clone();
                self.rekey_source_display_row_cache_for_local_edit(invalidation, version.clone());
                self.rekey_source_row_layout_input_cache_for_local_edit(invalidation, version);
                self.clear_row_layout_input_cache_for_rows(invalidation.rows.clone());
                self.clear_row_layout_cache_for_rows(invalidation.rows.clone());
            } else {
                self.clear_display_row_cache();
                self.clear_row_layout_cache();
            }
        }
        self.sync_display_list_state(row_count_before, previous_selection);
        if let Some(invalidation) = local_source_edit_invalidation {
            self.display_list_state.remeasure_items(invalidation.rows);
        } else if changed && self.mode == MarkdownEditorMode::Rendered {
            self.remeasure_rendered_items_for_selection_change(previous_selection);
        }
        self.reveal_cursor_row();
        if changed {
            self.emit_dirty_state(cx);
        } else {
            cx.notify();
        }
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
    ) {
        let row_count_after = self.display_item_count_for_mode(self.mode);
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
        self.sync_rendered_rows_for_selection_change(previous_selection);
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
            let source_offset = snapshot.as_text_snapshot().point_to_offset(cursor);
            let item_index = self
                .rendered_display_index(&snapshot)
                .item_index_for_source_offset(&snapshot, source_offset);
            reveal_selection_item(&self.display_list_state, item_index);
        } else {
            reveal_selection_head_row_in_text_snapshot(
                &self.display_list_state,
                self.buffer.as_text_snapshot(),
                &self.selection,
            );
        }
    }

    fn mouse_left_down_on_row(
        &mut self,
        display_row: &DisplayRow,
        visual_row_index: usize,
        visual_row: &VisualDisplayRow,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle.clone());
        self.is_selecting_with_mouse = true;

        if self.mode == MarkdownEditorMode::Source {
            let snapshot = self.buffer.text_snapshot();
            let wrap_width = text_wrap_width(window);
            let row_style = default_row_metrics().into();
            let text_layout = self.cached_source_text_layout(
                display_row,
                row_style,
                wrap_width,
                false,
                window,
                cx,
            );
            let (point, goal) = mouse_target_for_text_layout(
                &snapshot,
                display_row,
                visual_row_index,
                visual_row,
                event.position.x,
                left_rail_width(MarkdownEditorMode::Source),
                &text_layout,
            );
            let previous_selection = self.selection.clone();
            self.selection = if event.modifiers.shift {
                select_to_point_in_text_snapshot_with_goal(&snapshot, &self.selection, point, goal)
            } else {
                collapsed_selection_with_goal(point, goal)
            };
            self.notify_after_selection_change(&previous_selection, cx);
            return;
        }

        let snapshot = self.buffer.snapshot();
        let wrap_width = text_wrap_width(window);
        let row_style = row_display_style_for_display_row(&snapshot, display_row, self.mode);
        let selection = self.selection.clone();
        let (point, goal) = match self.cached_row_layout(
            &snapshot,
            display_row,
            &selection,
            self.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        ) {
            DisplayRowLayout::Text(text_layout) => {
                if !event.modifiers.shift
                    && let Some(source_range) = task_checkbox_source_range_for_text_layout_click(
                        display_row,
                        visual_row,
                        event.position.x,
                        left_rail_width(self.mode),
                        &text_layout,
                    )
                    && self.toggle_task_checkbox_source_range(source_range, cx)
                {
                    self.is_selecting_with_mouse = false;
                    return;
                }

                mouse_target_for_text_layout(
                    snapshot.as_text_snapshot(),
                    display_row,
                    visual_row_index,
                    visual_row,
                    event.position.x,
                    left_rail_width(self.mode),
                    &text_layout,
                )
            }
            DisplayRowLayout::Block(_) => (
                clip_cursor(&snapshot, selection.head()),
                SelectionGoal::None,
            ),
            DisplayRowLayout::TableRow(table_layout) => table_layout
                .mouse_target_for_x_with_indent(
                    &snapshot,
                    event.position.x,
                    display_row.rendered_indent_width(),
                ),
        };
        let previous_selection = self.selection.clone();
        self.selection = if event.modifiers.shift {
            select_to_point_with_goal(&snapshot, &self.selection, point, goal)
        } else {
            collapsed_selection_with_goal(point, goal)
        };
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn mouse_move_on_row(
        &mut self,
        display_row: &DisplayRow,
        visual_row_index: usize,
        visual_row: &VisualDisplayRow,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting_with_mouse || event.pressed_button != Some(MouseButton::Left) {
            return;
        }

        if self.mode == MarkdownEditorMode::Source {
            let snapshot = self.buffer.text_snapshot();
            let wrap_width = text_wrap_width(window);
            let row_style = default_row_metrics().into();
            let text_layout = self.cached_source_text_layout(
                display_row,
                row_style,
                wrap_width,
                false,
                window,
                cx,
            );
            let (point, goal) = mouse_target_for_text_layout(
                &snapshot,
                display_row,
                visual_row_index,
                visual_row,
                event.position.x,
                left_rail_width(MarkdownEditorMode::Source),
                &text_layout,
            );
            let previous_selection = self.selection.clone();
            self.selection =
                select_to_point_in_text_snapshot_with_goal(&snapshot, &self.selection, point, goal);
            self.notify_after_selection_change(&previous_selection, cx);
            return;
        }

        let snapshot = self.buffer.snapshot();
        let wrap_width = text_wrap_width(window);
        let row_style = row_display_style_for_display_row(&snapshot, display_row, self.mode);
        let selection = self.selection.clone();
        let (point, goal) = match self.cached_row_layout(
            &snapshot,
            display_row,
            &selection,
            self.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        ) {
            DisplayRowLayout::Text(text_layout) => mouse_target_for_text_layout(
                snapshot.as_text_snapshot(),
                display_row,
                visual_row_index,
                visual_row,
                event.position.x,
                left_rail_width(self.mode),
                &text_layout,
            ),
            DisplayRowLayout::Block(_) => (
                clip_cursor(&snapshot, selection.head()),
                SelectionGoal::None,
            ),
            DisplayRowLayout::TableRow(table_layout) => table_layout
                .mouse_target_for_x_with_indent(
                    &snapshot,
                    event.position.x,
                    display_row.rendered_indent_width(),
                ),
        };
        let previous_selection = self.selection.clone();
        self.selection = select_to_point_with_goal(&snapshot, &self.selection, point, goal);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn mouse_left_down_on_block(
        &mut self,
        block_layout: &DisplayBlockLayout,
        indent_width: gpui::Pixels,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle.clone());
        self.is_selecting_with_mouse = true;

        let snapshot = self.buffer.snapshot();
        let (point, goal) =
            block_layout.mouse_target_for_x_with_indent(&snapshot, event.position.x, indent_width);
        let previous_selection = self.selection.clone();
        self.selection = if event.modifiers.shift {
            select_to_point_with_goal(&snapshot, &self.selection, point, goal)
        } else {
            collapsed_selection_with_goal(point, goal)
        };
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn mouse_move_on_block(
        &mut self,
        block_layout: &DisplayBlockLayout,
        indent_width: gpui::Pixels,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting_with_mouse || event.pressed_button != Some(MouseButton::Left) {
            return;
        }

        let snapshot = self.buffer.snapshot();
        let (point, goal) =
            block_layout.mouse_target_for_x_with_indent(&snapshot, event.position.x, indent_width);
        let previous_selection = self.selection.clone();
        self.selection = select_to_point_with_goal(&snapshot, &self.selection, point, goal);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn mouse_left_down_on_table_row(
        &mut self,
        table_layout: &DisplayTableRowLayout,
        indent_width: gpui::Pixels,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle.clone());
        self.is_selecting_with_mouse = true;

        let snapshot = self.buffer.snapshot();
        let (point, goal) =
            table_layout.mouse_target_for_x_with_indent(&snapshot, event.position.x, indent_width);
        let previous_selection = self.selection.clone();
        self.selection = if event.modifiers.shift {
            select_to_point_with_goal(&snapshot, &self.selection, point, goal)
        } else {
            collapsed_selection_with_goal(point, goal)
        };
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn mouse_move_on_table_row(
        &mut self,
        table_layout: &DisplayTableRowLayout,
        indent_width: gpui::Pixels,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting_with_mouse || event.pressed_button != Some(MouseButton::Left) {
            return;
        }

        let snapshot = self.buffer.snapshot();
        let (point, goal) =
            table_layout.mouse_target_for_x_with_indent(&snapshot, event.position.x, indent_width);
        let previous_selection = self.selection.clone();
        self.selection = select_to_point_with_goal(&snapshot, &self.selection, point, goal);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn mouse_left_up(&mut self, _: &MouseUpEvent, _: &mut Window, _: &mut Context<Self>) {
        self.is_selecting_with_mouse = false;
    }
}

impl Focusable for MarkdownEditor {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for MarkdownEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = self.mode;
        let palette = editor_palette();
        let default_metrics = default_row_metrics();
        let wrap_width = text_wrap_width(window);
        if apply_text_wrap_width_change(
            &mut self.last_text_wrap_width,
            &mut self.selection,
            wrap_width,
        ) {
            self.clear_row_layout_cache();
        }
        let list_element = match mode {
            MarkdownEditorMode::Source => {
                let snapshot = self.buffer.text_snapshot();
                let selection = clip_selection_in_text_snapshot(&snapshot, &self.selection);
                let cursor = selection.head();
                self.schedule_source_cache_prewarm(wrap_width, default_metrics.into(), window, cx);

                md_list(
                    self.display_list_state.clone(),
                    cx.processor(move |this, row, window, _cx| {
                        let Some(display_row) = this.cached_source_display_row(&snapshot, row)
                        else {
                            return div().into_any_element();
                        };

                        let is_cursor_row = display_row.contains_source_point(cursor);
                        let row_style = default_metrics.into();
                        let text_layout = this.cached_source_text_layout(
                            &display_row,
                            row_style,
                            wrap_width,
                            true,
                            window,
                            _cx,
                        );
                        let content_min_height = text_layout.height(row_style);
                        let row_min_height = row_style.min_height.max(content_min_height);
                        let row_contents = render_row_text(
                            &snapshot,
                            &display_row,
                            &text_layout,
                            &selection,
                            row_style,
                            _cx,
                        );

                        render_editor_row(
                            &display_row,
                            mode,
                            is_cursor_row,
                            row_style,
                            row_min_height,
                            content_min_height,
                            row_contents,
                            _cx,
                        )
                    }),
                )
                .with_sizing_behavior(ListSizingBehavior::Auto)
                .size_full()
                .into_any_element()
            }
            MarkdownEditorMode::Rendered => {
                let snapshot = self.buffer.snapshot();
                let selection = clip_selection(&snapshot, &self.selection);
                let display_row_state =
                    DisplayRowProjectionState::new(&snapshot, Some(&selection), mode);
                let cursor = selection.head();
                self.schedule_rendered_cache_prewarm(wrap_width, selection.clone(), window, cx);

                md_list(
                    self.display_list_state.clone(),
                    cx.processor(move |this, row, window, _cx| {
                        let Some(display_row) =
                            this.cached_display_row(&snapshot, row, mode, &display_row_state)
                        else {
                            return div().into_any_element();
                        };

                        let is_cursor_row = display_row.contains_source_point(cursor);
                        let row_style =
                            row_display_style_for_display_row(&snapshot, &display_row, mode);
                        let row_layout = this.cached_row_layout(
                            &snapshot,
                            &display_row,
                            &selection,
                            mode,
                            row_style,
                            wrap_width,
                            true,
                            window,
                            _cx,
                        );
                        let row_min_height = row_layout.row_min_height(row_style);
                        let content_min_height = row_layout.content_min_height(row_style);
                        let row_contents = render_display_row_layout(
                            &snapshot,
                            &display_row,
                            &row_layout,
                            &selection,
                            row_style,
                            _cx,
                        );

                        render_editor_row(
                            &display_row,
                            mode,
                            is_cursor_row,
                            row_style,
                            row_min_height,
                            content_min_height,
                            row_contents,
                            _cx,
                        )
                    }),
                )
                .with_sizing_behavior(ListSizingBehavior::Auto)
                .size_full()
                .into_any_element()
            }
        };

        div()
            .id("md-editor")
            .size_full()
            .key_context("MarkdownEditor")
            .track_focus(&self.focus_handle(cx))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_to_beginning_of_line))
            .on_action(cx.listener(Self::move_to_end_of_line))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_to_beginning_of_line))
            .on_action(cx.listener(Self::select_to_end_of_line))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::insert_newline))
            .on_action(cx.listener(Self::insert_soft_break))
            .on_action(cx.listener(Self::tab))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_key_down(cx.listener(Self::key_down))
            .bg(palette.background)
            .text_color(palette.text)
            .font_family(EDITOR_FONT_FAMILY)
            .text_size(default_metrics.text_size)
            .line_height(default_metrics.line_height)
            .overflow_hidden()
            .child(list_element)
            .into_any_element()
    }
}

fn render_editor_row(
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    is_cursor_row: bool,
    row_style: RowDisplayStyle,
    row_min_height: gpui::Pixels,
    content_min_height: gpui::Pixels,
    row_contents: Vec<gpui::AnyElement>,
    cx: &mut Context<MarkdownEditor>,
) -> gpui::AnyElement {
    let palette = editor_palette();

    div()
        .id(display_row.row as usize)
        .w_full()
        .min_h(row_min_height)
        .flex()
        .items_center()
        .when(is_cursor_row, |this| {
            this.bg(palette.current_row_background)
        })
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(MarkdownEditor::mouse_left_up),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(MarkdownEditor::mouse_left_up),
        )
        .child(
            div()
                .w(left_rail_width(mode))
                .flex_none()
                .pr_2()
                .text_align(TextAlign::Right)
                .text_color(if is_cursor_row {
                    palette.gutter_current_text
                } else {
                    palette.gutter_text
                })
                .when(mode == MarkdownEditorMode::Source, |this| {
                    this.child(SharedString::from((display_row.row + 1).to_string()))
                })
                .when(
                    mode == MarkdownEditorMode::Rendered && is_cursor_row,
                    |this| this.child(SharedString::from("\u{2022}")),
                ),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .relative()
                .text_size(row_style.text_size)
                .line_height(row_style.line_height)
                .min_h(content_min_height)
                .children(row_contents),
        )
        .into_any_element()
}

fn buffer_byte_delta(before_len: usize, after_len: usize) -> Option<isize> {
    let before_len = isize::try_from(before_len).ok()?;
    let after_len = isize::try_from(after_len).ok()?;
    after_len.checked_sub(before_len)
}

fn local_source_edit_invalidation_rows(
    mode: MarkdownEditorMode,
    row_count_before: usize,
    row_count_after: usize,
    previous_selection: &Selection<Point>,
    current_selection: &Selection<Point>,
) -> Option<Range<usize>> {
    if mode != MarkdownEditorMode::Source || row_count_before != row_count_after {
        return None;
    }

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
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
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
