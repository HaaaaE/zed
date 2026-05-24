use std::{collections::HashMap, hash::Hash, ops::Range};

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, FontStyle, FontWeight, ImgResourceLoader,
    IntoElement, KeyBinding, KeyDownEvent, ListAlignment, ListSizingBehavior, ListState,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Render, Resource, SharedString,
    StrikethroughStyle, TextAlign, TextRun, UnderlineStyle, Window, div, font, img, list,
    prelude::*, px,
};
use markdown_wysiwyg::{MarkdownBlockKind, MarkdownInlineKind, MarkdownProjectionMap};
use md_assets::EDITOR_FONT_FAMILY;
use md_buffer::{Buffer, BufferSnapshot};
use md_settings::EditorSettings;
use md_text::{Bias, Point, Selection, SelectionGoal};
use md_theme::{default_row_metrics, editor_palette, gutter_width, heading_row_metrics};

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
        Backspace,
        Delete,
        InsertNewline,
        Tab,
        Undo,
        Redo,
    ]
);

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
                "Backspace" => KeyBinding::new(spec.keystroke, Backspace, context),
                "Delete" => KeyBinding::new(spec.keystroke, Delete, context),
                "InsertNewline" => KeyBinding::new(spec.keystroke, InsertNewline, context),
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
    focus_handle: FocusHandle,
    display_list_state: ListState,
    mode: MarkdownEditorMode,
    selection: Selection<Point>,
    is_selecting_with_mouse: bool,
    selection_history: HashMap<md_text::TransactionId, TransactionSelectionState>,
    settings: EditorSettings,
    last_text_wrap_width: Option<gpui::Pixels>,
    row_layout_cache: HashMap<RowLayoutCacheKey, DisplayRowLayout>,
    block_row_heights: HashMap<u32, gpui::Pixels>,
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

#[derive(Clone, Debug)]
pub struct DisplayRow {
    pub row: u32,
    pub text: String,
    projection: MarkdownProjectionMap,
}

impl PartialEq for DisplayRow {
    fn eq(&self, other: &Self) -> bool {
        self.row == other.row && self.text == other.text
    }
}

impl Eq for DisplayRow {}

#[derive(Clone, Debug, PartialEq)]
struct TransactionSelectionState {
    before: Selection<Point>,
    after: Selection<Point>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct DisplayTextStyle {
    color: Option<gpui::Hsla>,
    font_weight: Option<FontWeight>,
    text_background: Option<gpui::Hsla>,
    italic: bool,
    underline: bool,
    line_through: bool,
}

#[derive(Clone, Debug, PartialEq)]
struct StyledDisplaySegment {
    display_range: Range<usize>,
    text: String,
    style: DisplayTextStyle,
}

#[derive(Clone, Debug, PartialEq)]
enum DisplayInlineFragment {
    Text(StyledDisplaySegment),
    Atom(DisplayInlineAtom),
}

#[derive(Clone, Debug, PartialEq)]
struct DisplayInlineAtom {
    kind: DisplayInlineAtomKind,
    source_range: Range<usize>,
    display_range: Range<usize>,
    fallback_text: String,
    style: DisplayTextStyle,
    height: gpui::Pixels,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DisplayInlineAtomKind {
    InlineMath,
}

#[derive(Clone, Debug, PartialEq)]
struct VisualDisplayRow {
    display_range: Range<usize>,
    line_start_x: gpui::Pixels,
    top: gpui::Pixels,
    height: gpui::Pixels,
}

#[derive(Clone, Debug)]
struct DisplayRowTextLayout {
    fragments: Vec<DisplayInlineFragment>,
    visual_rows: Vec<VisualDisplayRow>,
    shaped_line: gpui::ShapedLine,
    text_len: usize,
}

impl DisplayRowTextLayout {
    fn height(&self, row_style: RowDisplayStyle) -> gpui::Pixels {
        let height = self
            .visual_rows
            .iter()
            .fold(px(0.), |height, visual_row| height + visual_row.height);

        height.max(row_style.line_height)
    }
}

const RENDERED_IMAGE_BLOCK_MAX_WIDTH: gpui::Pixels = px(600.);
const RENDERED_IMAGE_BLOCK_PLACEHOLDER_HEIGHT: gpui::Pixels = px(120.);
const INLINE_MATH_ATOM_EXTRA_HEIGHT: gpui::Pixels = px(4.);

#[derive(Clone, Debug, PartialEq)]
enum DisplayBlockLayout {
    RemoteImage(RenderedImageBlockLayout),
}

impl DisplayBlockLayout {
    fn height(&self) -> gpui::Pixels {
        match self {
            Self::RemoteImage(image_layout) => image_layout.height(),
        }
    }
}

#[derive(Clone, Debug)]
enum DisplayRowLayout {
    Text(DisplayRowTextLayout),
    Block(DisplayBlockLayout),
}

impl DisplayRowLayout {
    fn row_min_height(&self, row_style: RowDisplayStyle) -> gpui::Pixels {
        match self {
            Self::Text(text_layout) => row_style.min_height.max(text_layout.height(row_style)),
            Self::Block(block_layout) => row_style.min_height.max(block_layout.height()),
        }
    }

    fn content_min_height(&self, row_style: RowDisplayStyle) -> gpui::Pixels {
        match self {
            Self::Text(text_layout) => text_layout.height(row_style),
            Self::Block(block_layout) => row_style.min_height.max(block_layout.height()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RenderedImageBlock {
    url: String,
    alt_text: String,
    source_range: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
struct RenderedImageBlockLayout {
    image_block: RenderedImageBlock,
    width: gpui::Pixels,
    height: gpui::Pixels,
}

impl RenderedImageBlockLayout {
    fn height(&self) -> gpui::Pixels {
        self.height
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct RowLayoutCacheKey {
    row: u32,
    mode: MarkdownEditorMode,
    wrap_width: gpui::Pixels,
    active_source_range: Option<Range<usize>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
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
    pub fn new(mut buffer: Buffer, cx: &mut Context<Self>) -> Self {
        let row_count = buffer.snapshot().row_count() as usize;
        Self {
            buffer,
            focus_handle: cx.focus_handle(),
            display_list_state: ListState::new(row_count, ListAlignment::Top, px(1000.)),
            mode: MarkdownEditorMode::Source,
            selection: collapsed_selection(Point::zero()),
            is_selecting_with_mouse: false,
            selection_history: HashMap::default(),
            settings: EditorSettings::default(),
            last_text_wrap_width: None,
            row_layout_cache: HashMap::default(),
            block_row_heights: HashMap::default(),
        }
    }

    pub fn for_text(text: impl Into<String>, cx: &mut Context<Self>) -> Self {
        Self::new(Buffer::local(text), cx)
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

        self.mode = mode;
        self.clear_row_layout_cache();
        self.display_list_state.remeasure();
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
        self.buffer.snapshot().row_count()
    }

    pub fn row_text(&mut self, row: u32) -> String {
        row_text(&self.buffer.snapshot(), row)
    }

    pub fn display_rows(&mut self, range: Range<usize>) -> Vec<DisplayRow> {
        display_rows(&self.buffer.snapshot(), range)
    }

    pub fn cursor(&self) -> Point {
        self.selection.head()
    }

    pub fn selection(&self) -> &Selection<Point> {
        &self.selection
    }

    pub fn set_cursor(&mut self, cursor: Point) {
        let previous_selection = self.selection.clone();
        self.selection = collapsed_selection(clip_cursor(&self.buffer.snapshot(), cursor));
        self.sync_rendered_rows_for_selection_change(&previous_selection);
    }

    pub fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection = move_selection_left(&self.buffer.snapshot(), &self.selection);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection = move_selection_right(&self.buffer.snapshot(), &self.selection);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection = self.move_selection_visual_vertical(window, cx, -1, false);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection = self.move_selection_visual_vertical(window, cx, 1, false);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn move_to_beginning_of_line(
        &mut self,
        _: &MoveToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_selection = self.selection.clone();
        self.selection =
            move_selection_to_beginning_of_line(&self.buffer.snapshot(), &self.selection);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn move_to_end_of_line(
        &mut self,
        _: &MoveToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_selection = self.selection.clone();
        self.selection = move_selection_to_end_of_line(&self.buffer.snapshot(), &self.selection);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection = select_left(&self.buffer.snapshot(), &self.selection);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection = select_right(&self.buffer.snapshot(), &self.selection);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection = self.move_selection_visual_vertical(window, cx, -1, true);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection = self.move_selection_visual_vertical(window, cx, 1, true);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn move_selection_visual_vertical(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        delta_visual_rows: i32,
        extend_selection: bool,
    ) -> Selection<Point> {
        let snapshot = self.buffer.snapshot();
        let selection = clip_selection(&snapshot, &self.selection);
        let fallback = if extend_selection {
            select_vertical(&snapshot, &selection, delta_visual_rows)
        } else {
            move_selection_vertical(&snapshot, &selection, delta_visual_rows)
        };

        let Some((target, goal)) =
            self.visual_vertical_target_point(&snapshot, &selection, delta_visual_rows, window, cx)
        else {
            return fallback;
        };

        if extend_selection {
            let mut updated = selection.clone();
            updated.set_head(target, goal);
            updated
        } else {
            let mut updated = selection.clone();
            updated.collapse_to(target, goal);
            updated
        }
    }

    fn visual_vertical_target_point(
        &mut self,
        snapshot: &BufferSnapshot,
        selection: &Selection<Point>,
        delta_visual_rows: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Point, SelectionGoal)> {
        if delta_visual_rows == 0 {
            return Some((selection.head(), selection.goal));
        }

        let cursor = clip_cursor(snapshot, selection.head());
        let display_row = display_rows_in_mode(
            snapshot,
            cursor.row as usize..cursor.row as usize + 1,
            Some(selection),
            self.mode,
        )
        .into_iter()
        .next()?;
        let row_style = row_display_style(snapshot, display_row.row, self.mode);
        let wrap_width = text_wrap_width(window);
        let current_layout = self.cached_row_layout(
            snapshot,
            &display_row,
            selection,
            self.mode,
            row_style,
            wrap_width,
            window,
            cx,
        );

        let source_offset = snapshot.as_text_snapshot().point_to_offset(cursor);
        let desired_x = match current_layout {
            DisplayRowLayout::Text(text_layout) => {
                let display_offset = display_row
                    .projection
                    .source_to_display(source_offset)
                    .min(text_layout.text_len);
                let visual_row_index = visual_row_index_containing_caret(
                    &text_layout.visual_rows,
                    display_offset,
                    text_layout.text_len,
                )?;
                let visual_row = &text_layout.visual_rows[visual_row_index];
                let cursor_x =
                    text_layout.shaped_line.x_for_index(display_offset) - visual_row.line_start_x;
                let desired_x = desired_visual_x(selection.goal, cursor_x);

                let target_visual_row_index = visual_row_index as i32 + delta_visual_rows;
                if target_visual_row_index >= 0
                    && (target_visual_row_index as usize) < text_layout.visual_rows.len()
                {
                    let target_visual_row_index = target_visual_row_index as usize;
                    return point_for_visual_row_x(
                        snapshot,
                        &display_row,
                        &text_layout,
                        &text_layout.visual_rows[target_visual_row_index],
                        desired_x,
                    )
                    .map(|point| {
                        (
                            point,
                            visual_horizontal_goal(target_visual_row_index, desired_x),
                        )
                    });
                }

                desired_x
            }
            DisplayRowLayout::Block(DisplayBlockLayout::RemoteImage(image_layout)) => {
                desired_visual_x(
                    selection.goal,
                    image_block_x_for_source_offset(&image_layout, source_offset),
                )
            }
        };

        let target_row = if delta_visual_rows.is_negative() {
            display_row.row.checked_sub(1)?
        } else {
            let next_row = display_row.row.saturating_add(1);
            if next_row >= snapshot.row_count() {
                return None;
            }
            next_row
        };
        let target_display_row = display_rows_in_mode(
            snapshot,
            target_row as usize..target_row as usize + 1,
            Some(selection),
            self.mode,
        )
        .into_iter()
        .next()?;
        let target_row_style = row_display_style(snapshot, target_display_row.row, self.mode);
        let target_layout = self.cached_row_layout(
            snapshot,
            &target_display_row,
            selection,
            self.mode,
            target_row_style,
            wrap_width,
            window,
            cx,
        );
        match target_layout {
            DisplayRowLayout::Text(target_text_layout) => {
                let target_visual_row = if delta_visual_rows.is_negative() {
                    target_text_layout.visual_rows.last()?
                } else {
                    target_text_layout.visual_rows.first()?
                };
                let target_visual_row_index = if delta_visual_rows.is_negative() {
                    target_text_layout.visual_rows.len().saturating_sub(1)
                } else {
                    0
                };
                point_for_visual_row_x(
                    snapshot,
                    &target_display_row,
                    &target_text_layout,
                    target_visual_row,
                    desired_x,
                )
                .map(|point| {
                    (
                        point,
                        visual_horizontal_goal(target_visual_row_index, desired_x),
                    )
                })
            }
            DisplayRowLayout::Block(DisplayBlockLayout::RemoteImage(image_layout)) => Some((
                point_for_image_block_x(
                    snapshot,
                    &image_layout.image_block,
                    image_layout.width,
                    desired_x,
                ),
                visual_horizontal_goal(0, desired_x),
            )),
        }
    }

    pub fn select_to_beginning_of_line(
        &mut self,
        _: &SelectToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_selection = self.selection.clone();
        self.selection = select_to_beginning_of_line(&self.buffer.snapshot(), &self.selection);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn select_to_end_of_line(
        &mut self,
        _: &SelectToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_selection = self.selection.clone();
        self.selection = select_to_end_of_line(&self.buffer.snapshot(), &self.selection);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        let max_point = self.buffer.snapshot().as_text_snapshot().max_point();
        self.selection = Selection {
            id: 0,
            start: Point::zero(),
            end: max_point,
            reversed: false,
            goal: SelectionGoal::None,
        };
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let (selection, transaction_id) = backspace_selection(&mut self.buffer, &self.selection);
        let changed = transaction_id.is_some();
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(changed, row_count_before, &previous_selection, cx);
    }

    pub fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let (selection, transaction_id) = delete_selection(&mut self.buffer, &self.selection);
        let changed = transaction_id.is_some();
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(changed, row_count_before, &previous_selection, cx);
    }

    pub fn insert_newline(&mut self, _: &InsertNewline, _: &mut Window, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let current_line_indent = current_line_indent(&self.buffer.snapshot(), self.cursor());
        let insert_text = format!("\n{current_line_indent}");
        let (selection, transaction_id) =
            replace_selection(&mut self.buffer, &self.selection, &insert_text);
        let changed = transaction_id.is_some();
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(changed, row_count_before, &previous_selection, cx);
    }

    pub fn tab(&mut self, _: &Tab, _: &mut Window, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let tab_text = if self.settings.use_soft_tabs {
            " ".repeat(self.settings.tab_size)
        } else {
            "\t".to_string()
        };
        let (selection, transaction_id) =
            replace_selection(&mut self.buffer, &self.selection, &tab_text);
        let changed = transaction_id.is_some();
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(changed, row_count_before, &previous_selection, cx);
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
        let mut changed = false;
        if let Some(transaction_id) = self.buffer.undo() {
            let fallback = collapsed_selection(clip_cursor(&self.buffer.snapshot(), self.cursor()));
            self.selection = self
                .selection_history
                .get(&transaction_id)
                .map(|state| state.before.clone())
                .unwrap_or(fallback);
            changed = true;
        }
        self.notify_after_edit(changed, row_count_before, &previous_selection, cx);
    }

    pub fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let mut changed = false;
        if let Some(transaction_id) = self.buffer.redo() {
            let fallback = collapsed_selection(clip_cursor(&self.buffer.snapshot(), self.cursor()));
            self.selection = self
                .selection_history
                .get(&transaction_id)
                .map(|state| state.after.clone())
                .unwrap_or(fallback);
            changed = true;
        }
        self.notify_after_edit(changed, row_count_before, &previous_selection, cx);
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

        let selection_before = self.selection.clone();
        let previous_selection = self.selection.clone();
        let row_count_before = self.display_list_state.item_count();
        let (selection, transaction_id) =
            replace_selection(&mut self.buffer, &self.selection, text);
        let changed = transaction_id.is_some();
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        cx.stop_propagation();
        self.notify_after_edit(changed, row_count_before, &previous_selection, cx);
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
        self.selection_history
            .insert(transaction_id, TransactionSelectionState { before, after });
    }

    fn notify_after_edit(
        &mut self,
        changed: bool,
        row_count_before: usize,
        previous_selection: &Selection<Point>,
        cx: &mut Context<Self>,
    ) {
        self.clear_row_layout_cache();
        self.sync_display_list_state(row_count_before, previous_selection);
        if changed {
            self.emit_dirty_state(cx);
        } else {
            cx.notify();
        }
    }

    fn notify_after_selection_change(
        &mut self,
        previous_selection: &Selection<Point>,
        cx: &mut Context<Self>,
    ) {
        self.sync_rendered_rows_for_selection_change(previous_selection);
        cx.notify();
    }

    fn sync_display_list_state(
        &mut self,
        row_count_before: usize,
        previous_selection: &Selection<Point>,
    ) {
        let row_count_after = self.buffer.snapshot().row_count() as usize;
        if row_count_before != row_count_after {
            self.display_list_state
                .splice(0..row_count_before, row_count_after);
            if self.mode == MarkdownEditorMode::Rendered {
                self.display_list_state.remeasure();
            }
            return;
        }
        self.sync_rendered_rows_for_selection_change(previous_selection);
    }

    fn sync_rendered_rows_for_selection_change(&mut self, previous_selection: &Selection<Point>) {
        if self.mode != MarkdownEditorMode::Rendered {
            return;
        }

        let snapshot = self.buffer.snapshot();
        let previous_active = active_source_range_for_selection(&snapshot, previous_selection);
        let current_active = active_source_range_for_selection(&snapshot, &self.selection);
        if previous_active != current_active {
            self.clear_row_layout_cache();
        }
        self.remeasure_rows_from_source_ranges(
            &snapshot,
            previous_active.as_ref(),
            current_active.as_ref(),
        );
    }

    fn remeasure_rows_from_source_ranges(
        &self,
        snapshot: &BufferSnapshot,
        previous_source_range: Option<&Range<usize>>,
        current_source_range: Option<&Range<usize>>,
    ) {
        let mut ranges = Vec::new();
        if let Some(previous_source_range) = previous_source_range
            && let Some(rows) = source_range_to_row_range(snapshot, previous_source_range)
        {
            ranges.push(rows);
        }
        if let Some(current_source_range) = current_source_range
            && let Some(rows) = source_range_to_row_range(snapshot, current_source_range)
        {
            ranges.push(rows);
        }

        for rows in merge_overlapping_row_ranges(ranges) {
            self.display_list_state.remeasure_items(rows);
        }
    }

    fn emit_dirty_state(&mut self, cx: &mut Context<Self>) {
        cx.emit(MarkdownEditorEvent::DirtyChanged(self.buffer.is_dirty()));
        cx.notify();
    }

    fn clear_row_layout_cache(&mut self) {
        self.row_layout_cache.clear();
        self.block_row_heights.clear();
    }

    fn cached_row_layout(
        &mut self,
        snapshot: &BufferSnapshot,
        display_row: &DisplayRow,
        selection: &Selection<Point>,
        mode: MarkdownEditorMode,
        row_style: RowDisplayStyle,
        wrap_width: gpui::Pixels,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> DisplayRowLayout {
        let cache_key = RowLayoutCacheKey {
            row: display_row.row,
            mode,
            wrap_width,
            active_source_range: if mode == MarkdownEditorMode::Rendered {
                active_source_range_for_selection(snapshot, selection)
            } else {
                None
            },
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
            window,
            cx,
        );
        if matches!(layout, DisplayRowLayout::Text(_)) {
            self.row_layout_cache.insert(cache_key, layout.clone());
        }
        layout
    }

    fn mouse_left_down_on_row(
        &mut self,
        display_row: &DisplayRow,
        visual_row: &VisualDisplayRow,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle.clone(), cx);
        self.is_selecting_with_mouse = true;

        let snapshot = self.buffer.snapshot();
        let wrap_width = text_wrap_width(window);
        let row_style = row_display_style(&snapshot, display_row.row, self.mode);
        let selection = self.selection.clone();
        let point = match self.cached_row_layout(
            &snapshot,
            display_row,
            &selection,
            self.mode,
            row_style,
            wrap_width,
            window,
            cx,
        ) {
            DisplayRowLayout::Text(text_layout) => point_for_mouse_x_in_text_layout(
                &snapshot,
                display_row,
                visual_row,
                event.position.x,
                &text_layout,
            ),
            DisplayRowLayout::Block(_) => clip_cursor(&snapshot, selection.head()),
        };
        let previous_selection = self.selection.clone();
        self.selection = if event.modifiers.shift {
            select_to_point(&snapshot, &self.selection, point)
        } else {
            collapsed_selection(point)
        };
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn mouse_move_on_row(
        &mut self,
        display_row: &DisplayRow,
        visual_row: &VisualDisplayRow,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting_with_mouse || event.pressed_button != Some(MouseButton::Left) {
            return;
        }

        let snapshot = self.buffer.snapshot();
        let wrap_width = text_wrap_width(window);
        let row_style = row_display_style(&snapshot, display_row.row, self.mode);
        let selection = self.selection.clone();
        let point = match self.cached_row_layout(
            &snapshot,
            display_row,
            &selection,
            self.mode,
            row_style,
            wrap_width,
            window,
            cx,
        ) {
            DisplayRowLayout::Text(text_layout) => point_for_mouse_x_in_text_layout(
                &snapshot,
                display_row,
                visual_row,
                event.position.x,
                &text_layout,
            ),
            DisplayRowLayout::Block(_) => clip_cursor(&snapshot, selection.head()),
        };
        let previous_selection = self.selection.clone();
        self.selection = select_to_point(&snapshot, &self.selection, point);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn mouse_left_down_on_image_block(
        &mut self,
        image_block: &RenderedImageBlock,
        image_width: gpui::Pixels,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle.clone(), cx);
        self.is_selecting_with_mouse = true;

        let snapshot = self.buffer.snapshot();
        let point =
            point_for_image_block_mouse_x(&snapshot, image_block, image_width, event.position.x);
        let previous_selection = self.selection.clone();
        self.selection = if event.modifiers.shift {
            select_to_point(&snapshot, &self.selection, point)
        } else {
            collapsed_selection(point)
        };
        self.notify_after_selection_change(&previous_selection, cx);
    }

    fn mouse_move_on_image_block(
        &mut self,
        image_block: &RenderedImageBlock,
        image_width: gpui::Pixels,
        event: &MouseMoveEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting_with_mouse || event.pressed_button != Some(MouseButton::Left) {
            return;
        }

        let snapshot = self.buffer.snapshot();
        let point =
            point_for_image_block_mouse_x(&snapshot, image_block, image_width, event.position.x);
        let previous_selection = self.selection.clone();
        self.selection = select_to_point(&snapshot, &self.selection, point);
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
        let selection = self.selection.clone();
        let palette = editor_palette();
        let default_metrics = default_row_metrics();
        let wrap_width = text_wrap_width(window);
        if self.last_text_wrap_width != Some(wrap_width) {
            self.last_text_wrap_width = Some(wrap_width);
            self.clear_row_layout_cache();
            self.display_list_state.remeasure();
        }

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
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::insert_newline))
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
            .child(
                list(
                    self.display_list_state.clone(),
                    cx.processor(move |this, row, window, _cx| {
                        let snapshot = this.buffer.snapshot();
                        let selection = clip_selection(&snapshot, &selection);
                        let cursor = selection.head();
                        let Some(display_row) = display_rows_in_mode(
                            &snapshot,
                            row..row.saturating_add(1),
                            Some(&selection),
                            mode,
                        )
                        .into_iter()
                        .next() else {
                            return div().into_any_element();
                        };

                        let is_cursor_row = display_row.row == cursor.row;
                        let row_style = row_display_style(&snapshot, display_row.row, mode);
                        let row_layout = this.cached_row_layout(
                            &snapshot,
                            &display_row,
                            &selection,
                            mode,
                            row_style,
                            wrap_width,
                            window,
                            _cx,
                        );
                        let block_height_changed = if let DisplayRowLayout::Block(block_layout) =
                            &row_layout
                        {
                            let height = block_layout.height();
                            this.block_row_heights.insert(display_row.row, height) != Some(height)
                        } else {
                            false
                        };
                        if block_height_changed {
                            this.display_list_state
                                .remeasure_items(row..row.saturating_add(1));
                        }
                        let row_min_height = row_layout.row_min_height(row_style);
                        let content_min_height = row_layout.content_min_height(row_style);
                        let row_contents = render_display_row_layout(
                            &snapshot,
                            &display_row,
                            row_layout,
                            &selection,
                            row_style,
                            _cx,
                        );

                        div()
                            .id(display_row.row as usize)
                            .min_h(row_min_height)
                            .flex()
                            .items_center()
                            .when(is_cursor_row, |this| {
                                this.bg(palette.current_row_background)
                            })
                            .on_mouse_up(MouseButton::Left, _cx.listener(Self::mouse_left_up))
                            .on_mouse_up_out(MouseButton::Left, _cx.listener(Self::mouse_left_up))
                            .child(
                                div()
                                    .w(gutter_width())
                                    .pr_2()
                                    .text_align(TextAlign::Right)
                                    .text_color(if is_cursor_row {
                                        palette.gutter_current_text
                                    } else {
                                        palette.gutter_text
                                    })
                                    .child(SharedString::from((display_row.row + 1).to_string())),
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
                    }),
                )
                .with_sizing_behavior(ListSizingBehavior::Auto)
                .size_full(),
            )
    }
}

pub fn display_rows(snapshot: &BufferSnapshot, range: Range<usize>) -> Vec<DisplayRow> {
    display_rows_in_mode(snapshot, range, None, MarkdownEditorMode::Source)
}

fn display_rows_in_mode(
    snapshot: &BufferSnapshot,
    range: Range<usize>,
    selection: Option<&Selection<Point>>,
    mode: MarkdownEditorMode,
) -> Vec<DisplayRow> {
    let row_count = snapshot.row_count() as usize;
    let start = range.start.min(row_count);
    let end = range.end.min(row_count);
    let active_source_range = if mode == MarkdownEditorMode::Rendered {
        selection.and_then(|selection| active_source_range_for_selection(snapshot, selection))
    } else {
        None
    };

    (start..end)
        .map(|row| {
            let row = row as u32;
            let source_text = row_text(snapshot, row);
            let source_range = row_source_range(snapshot, row);
            let projection = match mode {
                MarkdownEditorMode::Source => MarkdownProjectionMap::new(
                    snapshot.as_text_snapshot().len(),
                    source_range.clone(),
                    Vec::new(),
                ),
                MarkdownEditorMode::Rendered => snapshot
                    .syntax_tree()
                    .projection_for_source_range(source_range.clone(), active_source_range.clone()),
            };

            DisplayRow {
                row,
                text: project_row_text(&source_text, &projection),
                projection,
            }
        })
        .collect()
}

pub fn row_text(snapshot: &BufferSnapshot, row: u32) -> String {
    let text_snapshot = snapshot.as_text_snapshot();
    if row >= text_snapshot.row_count() {
        return String::new();
    }

    let start = text_snapshot.point_to_offset(Point::new(row, 0));
    let end = start + text_snapshot.line_len(row) as usize;
    text_snapshot.text_for_range(start..end).collect()
}

fn row_source_range(snapshot: &BufferSnapshot, row: u32) -> Range<usize> {
    let text_snapshot = snapshot.as_text_snapshot();
    if row >= text_snapshot.row_count() {
        return text_snapshot.len()..text_snapshot.len();
    }

    let start = text_snapshot.point_to_offset(Point::new(row, 0));
    let end = start + text_snapshot.line_len(row) as usize;
    start..end
}

fn project_row_text(source_text: &str, projection: &MarkdownProjectionMap) -> String {
    if projection.hidden_ranges().is_empty() {
        return source_text.to_string();
    }

    let visible_source_range = projection.visible_source_range();
    let mut rendered_text = String::new();
    let mut cursor = visible_source_range.start;

    for hidden_range in projection.hidden_ranges() {
        let start = hidden_range.start.max(visible_source_range.start);
        let end = hidden_range.end.min(visible_source_range.end);
        if cursor < start {
            rendered_text.push_str(
                &source_text
                    [(cursor - visible_source_range.start)..(start - visible_source_range.start)],
            );
        }
        cursor = cursor.max(end);
    }

    if cursor < visible_source_range.end {
        rendered_text.push_str(
            &source_text[(cursor - visible_source_range.start)
                ..(visible_source_range.end - visible_source_range.start)],
        );
    }

    rendered_text
}

pub fn clip_cursor(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    snapshot.as_text_snapshot().clip_point(cursor, Bias::Left)
}

pub fn clip_selection(snapshot: &BufferSnapshot, selection: &Selection<Point>) -> Selection<Point> {
    let head = clip_cursor(snapshot, selection.head());
    let tail = clip_cursor(snapshot, selection.tail());
    let mut clipped = selection.clone();
    clipped.set_head_tail(head, tail, selection.goal);
    clipped.id = selection.id;
    clipped
}

pub fn collapsed_selection(point: Point) -> Selection<Point> {
    Selection {
        id: 0,
        start: point,
        end: point,
        reversed: false,
        goal: SelectionGoal::None,
    }
}

pub fn move_left(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    let text_snapshot = snapshot.as_text_snapshot();
    let offset = text_snapshot.point_to_offset(clip_cursor(snapshot, cursor));
    if offset == 0 {
        return Point::zero();
    }

    text_snapshot.offset_to_point(
        text_snapshot
            .as_rope()
            .floor_char_boundary(offset.saturating_sub(1)),
    )
}

pub fn move_right(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    let text_snapshot = snapshot.as_text_snapshot();
    let offset = text_snapshot.point_to_offset(clip_cursor(snapshot, cursor));
    if offset >= text_snapshot.len() {
        return text_snapshot.max_point();
    }

    text_snapshot.offset_to_point(
        text_snapshot
            .as_rope()
            .ceil_char_boundary(offset.saturating_add(1)),
    )
}

pub fn move_vertical(snapshot: &BufferSnapshot, cursor: Point, delta_rows: i32) -> Point {
    let text_snapshot = snapshot.as_text_snapshot();
    let current = clip_cursor(snapshot, cursor);
    let max_row = text_snapshot.row_count().saturating_sub(1);
    let target_row = if delta_rows.is_negative() {
        current.row.saturating_sub(delta_rows.unsigned_abs())
    } else {
        current.row.saturating_add(delta_rows as u32).min(max_row)
    };

    point_for_row_and_column(snapshot, target_row, current.column)
}

pub fn move_to_beginning_of_line(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    Point::new(clip_cursor(snapshot, cursor).row, 0)
}

pub fn move_to_end_of_line(snapshot: &BufferSnapshot, cursor: Point) -> Point {
    let row = clip_cursor(snapshot, cursor).row;
    Point::new(row, snapshot.as_text_snapshot().line_len(row))
}

pub fn move_selection_left(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    if selection.is_empty() {
        collapsed_selection(move_left(snapshot, selection.head()))
    } else {
        collapsed_selection(selection.start)
    }
}

pub fn move_selection_right(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    if selection.is_empty() {
        collapsed_selection(move_right(snapshot, selection.head()))
    } else {
        collapsed_selection(selection.end)
    }
}

pub fn move_selection_vertical(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    delta_rows: i32,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    collapsed_selection(move_vertical(snapshot, selection.head(), delta_rows))
}

pub fn move_selection_to_beginning_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    collapsed_selection(move_to_beginning_of_line(snapshot, selection.head()))
}

pub fn move_selection_to_end_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    collapsed_selection(move_to_end_of_line(snapshot, selection.head()))
}

pub fn select_left(snapshot: &BufferSnapshot, selection: &Selection<Point>) -> Selection<Point> {
    select_to_point(snapshot, selection, move_left(snapshot, selection.head()))
}

pub fn select_right(snapshot: &BufferSnapshot, selection: &Selection<Point>) -> Selection<Point> {
    select_to_point(snapshot, selection, move_right(snapshot, selection.head()))
}

pub fn select_vertical(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    delta_rows: i32,
) -> Selection<Point> {
    select_to_point(
        snapshot,
        selection,
        move_vertical(snapshot, selection.head(), delta_rows),
    )
}

pub fn select_to_beginning_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_point(
        snapshot,
        selection,
        move_to_beginning_of_line(snapshot, selection.head()),
    )
}

pub fn select_to_end_of_line(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Selection<Point> {
    select_to_point(
        snapshot,
        selection,
        move_to_end_of_line(snapshot, selection.head()),
    )
}

pub fn select_to_point(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
    head: Point,
) -> Selection<Point> {
    let selection = clip_selection(snapshot, selection);
    let mut updated = selection.clone();
    updated.set_head(head, SelectionGoal::None);
    updated
}

pub fn selection_byte_range(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Range<usize> {
    let selection = clip_selection(snapshot, selection);
    let text_snapshot = snapshot.as_text_snapshot();
    text_snapshot.point_to_offset(selection.start)..text_snapshot.point_to_offset(selection.end)
}

pub fn replace_selection(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
    text: &str,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let snapshot = buffer.snapshot();
    let selection = clip_selection(&snapshot, selection);
    let range = selection_byte_range(&snapshot, &selection);

    if range.is_empty() && text.is_empty() {
        return (selection, None);
    }

    buffer.start_transaction();
    buffer.edit([(range.clone(), text)]);
    let transaction_id = buffer.end_transaction();

    let snapshot = buffer.snapshot();
    let cursor = snapshot
        .as_text_snapshot()
        .offset_to_point(range.start.saturating_add(text.len()));
    (collapsed_selection(cursor), transaction_id)
}

pub fn backspace_selection(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let snapshot = buffer.snapshot();
    let selection = clip_selection(&snapshot, selection);
    if !selection.is_empty() {
        return replace_selection(buffer, &selection, "");
    }

    let text_snapshot = snapshot.as_text_snapshot();
    let offset = text_snapshot.point_to_offset(selection.head());
    if offset == 0 {
        return (selection, None);
    }

    let previous_offset = text_snapshot
        .as_rope()
        .floor_char_boundary(offset.saturating_sub(1));
    replace_selection(
        buffer,
        &Selection {
            id: selection.id,
            start: text_snapshot.offset_to_point(previous_offset),
            end: selection.head(),
            reversed: false,
            goal: SelectionGoal::None,
        },
        "",
    )
}

pub fn delete_selection(
    buffer: &mut Buffer,
    selection: &Selection<Point>,
) -> (Selection<Point>, Option<md_text::TransactionId>) {
    let snapshot = buffer.snapshot();
    let selection = clip_selection(&snapshot, selection);
    if !selection.is_empty() {
        return replace_selection(buffer, &selection, "");
    }

    let text_snapshot = snapshot.as_text_snapshot();
    let offset = text_snapshot.point_to_offset(selection.head());
    if offset >= text_snapshot.len() {
        return (selection, None);
    }

    let next_offset = text_snapshot
        .as_rope()
        .ceil_char_boundary(offset.saturating_add(1));
    replace_selection(
        buffer,
        &Selection {
            id: selection.id,
            start: selection.head(),
            end: text_snapshot.offset_to_point(next_offset),
            reversed: false,
            goal: SelectionGoal::None,
        },
        "",
    )
}

fn point_for_row_and_column(snapshot: &BufferSnapshot, row: u32, column: u32) -> Point {
    let text_snapshot = snapshot.as_text_snapshot();
    let row = row.min(text_snapshot.row_count().saturating_sub(1));
    let row_start = text_snapshot.point_to_offset(Point::new(row, 0));
    let row_end = row_start + text_snapshot.line_len(row) as usize;
    let target = row_start.saturating_add(column as usize).min(row_end);
    let target = text_snapshot
        .as_rope()
        .floor_char_boundary(target)
        .max(row_start);

    text_snapshot.offset_to_point(target)
}

/// Compute the leading whitespace (indent) of the line containing the given cursor position.
/// Returns a string of spaces and/or tabs from the start of the line up to the first
/// non-whitespace character.
pub fn current_line_indent(snapshot: &BufferSnapshot, cursor: Point) -> String {
    let text = row_text(snapshot, cursor.row);
    text.chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn point_for_mouse_x_in_text_layout(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    visual_row: &VisualDisplayRow,
    x: gpui::Pixels,
    text_layout: &DisplayRowTextLayout,
) -> Point {
    let text_x = (x - gutter_width()).max(px(0.));
    let display_offset = text_layout
        .shaped_line
        .closest_index_for_x(text_x + visual_row.line_start_x)
        .clamp(visual_row.display_range.start, visual_row.display_range.end);
    let source_offset = display_row.projection.display_to_source(display_offset);
    let source_offset = snapshot
        .as_text_snapshot()
        .as_rope()
        .floor_char_boundary(source_offset);

    clip_cursor(
        snapshot,
        snapshot.as_text_snapshot().offset_to_point(source_offset),
    )
}

fn render_row_text(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    text_layout: DisplayRowTextLayout,
    selection: &Selection<Point>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> Vec<gpui::AnyElement> {
    let selected_range = if selection.is_empty() {
        None
    } else {
        selected_range_for_row(snapshot, display_row, selection)
            .filter(|selected_range| !selected_range.is_empty())
    };

    let mut elements = Vec::new();
    for visual_row in text_layout.visual_rows.clone() {
        elements.push(render_visual_text_row(
            snapshot,
            display_row,
            &text_layout,
            visual_row,
            selection,
            selected_range.as_ref(),
            row_style,
            cx,
        ));
    }
    elements
}

fn render_display_row_layout(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    row_layout: DisplayRowLayout,
    selection: &Selection<Point>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> Vec<gpui::AnyElement> {
    match row_layout {
        DisplayRowLayout::Text(text_layout) => {
            render_row_text(snapshot, display_row, text_layout, selection, row_style, cx)
        }
        DisplayRowLayout::Block(DisplayBlockLayout::RemoteImage(image_block)) => {
            vec![render_image_block(image_block, cx)]
        }
    }
}

fn text_wrap_width(window: &Window) -> gpui::Pixels {
    (window.bounds().size.width - gutter_width()).max(px(1.))
}

fn compute_display_row_layout(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    window: &mut Window,
    cx: &mut App,
) -> DisplayRowLayout {
    if let Some(image_block) = rendered_image_block_for_row(snapshot, display_row, selection, mode)
    {
        return DisplayRowLayout::Block(DisplayBlockLayout::RemoteImage(
            rendered_image_block_layout(image_block, wrap_width, window, cx),
        ));
    }

    DisplayRowLayout::Text(text_layout_for_display_row(
        snapshot,
        display_row,
        mode,
        row_style,
        wrap_width,
        window,
    ))
}

fn rendered_image_block_layout(
    image_block: RenderedImageBlock,
    wrap_width: gpui::Pixels,
    window: &mut Window,
    cx: &mut App,
) -> RenderedImageBlockLayout {
    let width = wrap_width.max(px(1.)).min(RENDERED_IMAGE_BLOCK_MAX_WIDTH);
    let resource = Resource::Uri(image_block.url.clone().into());
    let height = window
        .use_asset::<ImgResourceLoader>(&resource, cx)
        .and_then(|image| {
            let image = image.ok()?;
            let size = image.size(0);
            image_block_height_for_size(width, size.width.0, size.height.0)
        })
        .unwrap_or(RENDERED_IMAGE_BLOCK_PLACEHOLDER_HEIGHT);

    RenderedImageBlockLayout {
        image_block,
        width,
        height,
    }
}

fn image_block_height_for_size(
    width: gpui::Pixels,
    image_width: i32,
    image_height: i32,
) -> Option<gpui::Pixels> {
    if image_width <= 0 || image_height <= 0 {
        return None;
    }

    Some(width * (image_height as f32 / image_width as f32))
}

fn text_layout_for_display_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
    wrap_width: gpui::Pixels,
    window: &mut Window,
) -> DisplayRowTextLayout {
    let fragments = display_inline_fragments(snapshot, display_row, mode, row_style);
    let segments = text_segments_for_fragments(&fragments);
    let text_runs = text_runs_for_segments(&segments);
    let shaped_line = window.text_system().shape_line(
        SharedString::from(display_row.text.clone()),
        row_style.text_size,
        &text_runs,
        None,
    );
    let visual_rows = match window.text_system().shape_text(
        SharedString::from(display_row.text.clone()),
        row_style.text_size,
        &text_runs,
        Some(wrap_width),
        None,
    ) {
        Ok(wrapped_lines) => wrapped_lines
            .first()
            .map(|wrapped_line| visual_rows_for_wrapped_line(wrapped_line, &fragments, row_style))
            .unwrap_or_else(|| fallback_visual_rows(display_row.text.len(), &fragments, row_style)),
        Err(_) => fallback_visual_rows(display_row.text.len(), &fragments, row_style),
    };

    DisplayRowTextLayout {
        fragments,
        visual_rows,
        shaped_line,
        text_len: display_row.text.len(),
    }
}

fn visual_rows_for_wrapped_line(
    wrapped_line: &gpui::WrappedLine,
    fragments: &[DisplayInlineFragment],
    row_style: RowDisplayStyle,
) -> Vec<VisualDisplayRow> {
    let mut rows = Vec::new();
    let mut start = 0;
    let mut start_x = px(0.);
    let mut top = px(0.);

    for wrap_boundary in wrapped_line.wrap_boundaries() {
        let Some(glyph) = wrap_boundary_glyph(wrapped_line, *wrap_boundary) else {
            return fallback_visual_rows(wrapped_line.len(), fragments, row_style);
        };
        if glyph.index < start {
            if atom_range_containing_display_index(fragments, glyph.index)
                .is_some_and(|atom_range| atom_range.end <= start)
            {
                continue;
            }
            return fallback_visual_rows(wrapped_line.len(), fragments, row_style);
        }
        let boundary_index = atomic_wrap_boundary_index(fragments, glyph.index, start);
        if boundary_index < start {
            return fallback_visual_rows(wrapped_line.len(), fragments, row_style);
        }
        if boundary_index == start {
            continue;
        }
        let display_range = start..boundary_index;
        let height = visual_row_height_for_range(fragments, &display_range, row_style);
        rows.push(VisualDisplayRow {
            display_range,
            line_start_x: start_x,
            top,
            height,
        });
        start = boundary_index;
        start_x = wrapped_line.unwrapped_layout.x_for_index(boundary_index);
        top += height;
    }

    let display_range = start..wrapped_line.len();
    rows.push(VisualDisplayRow {
        height: visual_row_height_for_range(fragments, &display_range, row_style),
        display_range,
        line_start_x: start_x,
        top,
    });
    rows
}

fn atomic_wrap_boundary_index(
    fragments: &[DisplayInlineFragment],
    boundary_index: usize,
    row_start: usize,
) -> usize {
    let Some(atom_range) = atom_range_containing_display_index(fragments, boundary_index) else {
        return boundary_index;
    };

    if atom_range.start > row_start {
        atom_range.start
    } else {
        atom_range.end
    }
}

fn atom_range_containing_display_index(
    fragments: &[DisplayInlineFragment],
    display_index: usize,
) -> Option<Range<usize>> {
    fragments.iter().find_map(|fragment| match fragment {
        DisplayInlineFragment::Text(_) => None,
        DisplayInlineFragment::Atom(atom)
            if atom.display_range.start < display_index
                && display_index < atom.display_range.end =>
        {
            Some(atom.display_range.clone())
        }
        DisplayInlineFragment::Atom(_) => None,
    })
}

fn wrap_boundary_glyph(
    wrapped_line: &gpui::WrappedLine,
    wrap_boundary: gpui::WrapBoundary,
) -> Option<&gpui::ShapedGlyph> {
    wrapped_line
        .unwrapped_layout
        .runs
        .get(wrap_boundary.run_ix)
        .and_then(|run| run.glyphs.get(wrap_boundary.glyph_ix))
}

fn fallback_visual_rows(
    text_len: usize,
    fragments: &[DisplayInlineFragment],
    row_style: RowDisplayStyle,
) -> Vec<VisualDisplayRow> {
    let display_range = 0..text_len;
    vec![VisualDisplayRow {
        height: visual_row_height_for_range(fragments, &display_range, row_style),
        display_range,
        line_start_x: px(0.),
        top: px(0.),
    }]
}

fn visual_row_height_for_range(
    fragments: &[DisplayInlineFragment],
    display_range: &Range<usize>,
    row_style: RowDisplayStyle,
) -> gpui::Pixels {
    fragments
        .iter()
        .filter_map(|fragment| match fragment {
            DisplayInlineFragment::Text(_) => None,
            DisplayInlineFragment::Atom(atom)
                if ranges_overlap(&atom.display_range, display_range) =>
            {
                Some(atom.height)
            }
            DisplayInlineFragment::Atom(_) => None,
        })
        .fold(row_style.line_height, |height, atom_height| {
            height.max(atom_height)
        })
}

fn text_runs_for_segments(segments: &[StyledDisplaySegment]) -> Vec<TextRun> {
    let palette = editor_palette();
    let mut runs = Vec::new();

    for segment in segments {
        if segment.text.is_empty() {
            continue;
        }

        let mut run_font = font(EDITOR_FONT_FAMILY);
        if let Some(font_weight) = segment.style.font_weight {
            run_font.weight = font_weight;
        }
        if segment.style.italic {
            run_font.style = FontStyle::Italic;
        }

        let color = segment.style.color.unwrap_or(palette.text);
        runs.push(TextRun {
            len: segment.text.len(),
            font: run_font,
            color,
            background_color: segment.style.text_background,
            underline: segment.style.underline.then_some(UnderlineStyle {
                thickness: px(1.),
                color: Some(color),
                wavy: false,
            }),
            strikethrough: segment.style.line_through.then_some(StrikethroughStyle {
                thickness: px(1.),
                color: Some(color),
            }),
        });
    }

    if runs.is_empty() {
        runs.push(TextRun {
            len: 0,
            font: font(EDITOR_FONT_FAMILY),
            color: palette.text,
            background_color: None,
            underline: None,
            strikethrough: None,
        });
    }

    runs
}

fn render_visual_text_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    text_layout: &DisplayRowTextLayout,
    visual_row: VisualDisplayRow,
    selection: &Selection<Point>,
    selected_range: Option<&Range<usize>>,
    row_style: RowDisplayStyle,
    cx: &mut Context<MarkdownEditor>,
) -> gpui::AnyElement {
    let mouse_down_row = display_row.clone();
    let mouse_down_visual_row = visual_row.clone();
    let mouse_move_row = display_row.clone();
    let mouse_move_visual_row = visual_row.clone();

    div()
        .h(visual_row.height)
        .flex()
        .items_center()
        .relative()
        .overflow_hidden()
        .whitespace_nowrap()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, event, window, cx| {
                this.mouse_left_down_on_row(
                    &mouse_down_row,
                    &mouse_down_visual_row,
                    event,
                    window,
                    cx,
                )
            }),
        )
        .on_mouse_move(cx.listener(move |this, event, window, cx| {
            this.mouse_move_on_row(&mouse_move_row, &mouse_move_visual_row, event, window, cx)
        }))
        .children(selection_elements_for_visual_row(
            text_layout,
            selected_range,
            &visual_row,
        ))
        .children(render_fragments_for_visual_row(
            &text_layout.fragments,
            &visual_row,
        ))
        .when_some(
            caret_position_for_visual_row(
                snapshot,
                display_row,
                selection,
                text_layout,
                &visual_row,
            ),
            |this, caret_x| this.child(caret_element(caret_x, row_style)),
        )
        .into_any_element()
}

fn selection_elements_for_visual_row(
    text_layout: &DisplayRowTextLayout,
    selected_range: Option<&Range<usize>>,
    visual_row: &VisualDisplayRow,
) -> Vec<gpui::AnyElement> {
    let Some(selected_range) = selected_range else {
        return Vec::new();
    };

    let start = selected_range.start.max(visual_row.display_range.start);
    let end = selected_range.end.min(visual_row.display_range.end);
    if start >= end {
        return Vec::new();
    }

    let palette = editor_palette();
    let start_x = text_layout.shaped_line.x_for_index(start) - visual_row.line_start_x;
    let end_x = text_layout.shaped_line.x_for_index(end) - visual_row.line_start_x;
    let width = (end_x - start_x).max(px(1.));

    vec![
        div()
            .absolute()
            .left(start_x)
            .top_0()
            .h(visual_row.height)
            .w(width)
            .bg(palette.selection_background)
            .into_any_element(),
    ]
}

fn render_fragments_for_visual_row(
    fragments: &[DisplayInlineFragment],
    visual_row: &VisualDisplayRow,
) -> Vec<gpui::AnyElement> {
    let mut elements = Vec::new();

    for fragment in fragments {
        match fragment {
            DisplayInlineFragment::Text(segment) => {
                let Some(text) = fragment_text_for_visual_row(
                    &segment.display_range,
                    segment.text.as_str(),
                    visual_row,
                ) else {
                    continue;
                };
                elements.push(render_text_piece(text, &segment.style));
            }
            DisplayInlineFragment::Atom(atom) => {
                let Some(text) = fragment_text_for_visual_row(
                    &atom.display_range,
                    atom.fallback_text.as_str(),
                    visual_row,
                ) else {
                    continue;
                };
                elements.push(render_inline_atom_piece(atom, text));
            }
        }
    }

    if elements.is_empty() {
        elements.push(SharedString::from(String::new()).into_any_element());
    }

    elements
}

fn fragment_text_for_visual_row(
    display_range: &Range<usize>,
    text: &str,
    visual_row: &VisualDisplayRow,
) -> Option<String> {
    if display_range.end <= visual_row.display_range.start
        || display_range.start >= visual_row.display_range.end
    {
        return None;
    }

    let start = display_range.start.max(visual_row.display_range.start);
    let end = display_range.end.min(visual_row.display_range.end);
    if start >= end {
        return None;
    }

    let local_start = start - display_range.start;
    let local_end = end - display_range.start;
    let text = text.get(local_start..local_end)?;
    if text.is_empty() {
        return None;
    }

    Some(text.to_string())
}

fn caret_position_for_visual_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    text_layout: &DisplayRowTextLayout,
    visual_row: &VisualDisplayRow,
) -> Option<gpui::Pixels> {
    if !selection.is_empty() {
        return None;
    }

    let cursor = selection.head();
    if display_row.row != cursor.row {
        return None;
    }

    let cursor_offset = snapshot
        .as_text_snapshot()
        .point_to_offset(clip_cursor(snapshot, cursor));
    let display_offset = display_row
        .projection
        .source_to_display(cursor_offset)
        .min(text_layout.text_len);
    if !visual_row_contains_caret(visual_row, display_offset, text_layout.text_len) {
        return None;
    }

    Some(text_layout.shaped_line.x_for_index(display_offset) - visual_row.line_start_x)
}

fn visual_row_contains_caret(
    visual_row: &VisualDisplayRow,
    display_offset: usize,
    text_len: usize,
) -> bool {
    if display_offset < visual_row.display_range.start
        || display_offset > visual_row.display_range.end
    {
        return false;
    }
    display_offset < visual_row.display_range.end || visual_row.display_range.end == text_len
}

fn visual_row_index_containing_caret(
    visual_rows: &[VisualDisplayRow],
    display_offset: usize,
    text_len: usize,
) -> Option<usize> {
    visual_rows
        .iter()
        .position(|visual_row| visual_row_contains_caret(visual_row, display_offset, text_len))
}

fn desired_visual_x(goal: SelectionGoal, cursor_x: gpui::Pixels) -> gpui::Pixels {
    match goal {
        SelectionGoal::HorizontalPosition(x) if x.is_finite() => px(x as f32),
        SelectionGoal::WrappedHorizontalPosition((_, x)) if x.is_finite() => px(x),
        _ => cursor_x,
    }
}

fn visual_horizontal_goal(visual_row_index: usize, x: gpui::Pixels) -> SelectionGoal {
    SelectionGoal::WrappedHorizontalPosition((
        u32::try_from(visual_row_index).unwrap_or(u32::MAX),
        f32::from(x),
    ))
}

fn point_for_visual_row_x(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    text_layout: &DisplayRowTextLayout,
    visual_row: &VisualDisplayRow,
    x: gpui::Pixels,
) -> Option<Point> {
    let display_offset =
        display_offset_for_visual_row_x(&display_row.text, text_layout, visual_row, x);
    let source_offset = display_row.projection.display_to_source(display_offset);
    let source_offset = snapshot
        .as_text_snapshot()
        .as_rope()
        .floor_char_boundary(source_offset);
    Some(snapshot.as_text_snapshot().offset_to_point(source_offset))
}

fn display_offset_for_visual_row_x(
    display_text: &str,
    text_layout: &DisplayRowTextLayout,
    visual_row: &VisualDisplayRow,
    x: gpui::Pixels,
) -> usize {
    let display_offset = text_layout
        .shaped_line
        .closest_index_for_x(x.max(px(0.)) + visual_row.line_start_x)
        .clamp(visual_row.display_range.start, visual_row.display_range.end);

    if display_offset == visual_row.display_range.end
        && visual_row.display_range.end < text_layout.text_len
    {
        return display_text
            .floor_char_boundary(visual_row.display_range.end.saturating_sub(1))
            .max(visual_row.display_range.start);
    }

    display_offset
}

fn point_for_image_block_mouse_x(
    snapshot: &BufferSnapshot,
    image_block: &RenderedImageBlock,
    image_width: gpui::Pixels,
    x: gpui::Pixels,
) -> Point {
    point_for_image_block_x(snapshot, image_block, image_width, x - gutter_width())
}

fn point_for_image_block_x(
    snapshot: &BufferSnapshot,
    image_block: &RenderedImageBlock,
    image_width: gpui::Pixels,
    x: gpui::Pixels,
) -> Point {
    let offset = image_block_source_offset_for_x(image_block, image_width, x);
    let offset = snapshot
        .as_text_snapshot()
        .as_rope()
        .floor_char_boundary(offset);

    clip_cursor(
        snapshot,
        snapshot.as_text_snapshot().offset_to_point(offset),
    )
}

fn image_block_source_offset_for_x(
    image_block: &RenderedImageBlock,
    image_width: gpui::Pixels,
    x: gpui::Pixels,
) -> usize {
    let image_x = x.max(px(0.));
    if image_x < image_width * 0.5 {
        image_block.source_range.start
    } else {
        image_block.source_range.end
    }
}

fn image_block_x_for_source_offset(
    image_layout: &RenderedImageBlockLayout,
    source_offset: usize,
) -> gpui::Pixels {
    let source_range = &image_layout.image_block.source_range;
    if source_offset <= source_range.start {
        px(0.)
    } else if source_offset >= source_range.end {
        image_layout.width
    } else {
        image_layout.width * 0.5
    }
}

fn render_image_block(
    image_layout: RenderedImageBlockLayout,
    cx: &mut Context<MarkdownEditor>,
) -> gpui::AnyElement {
    let palette = editor_palette();
    let image_block = image_layout.image_block;
    let mouse_down_image_block = image_block.clone();
    let mouse_move_image_block = image_block.clone();
    let image_width = image_layout.width;
    let fallback_label = if image_block.alt_text.trim().is_empty() {
        image_block
            .url
            .split('?')
            .next()
            .unwrap_or(&image_block.url)
            .to_string()
    } else {
        image_block.alt_text
    };

    div()
        .w_full()
        .py_1()
        .child(
            div()
                .w(image_layout.width)
                .h(image_layout.height)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event, window, cx| {
                        this.mouse_left_down_on_image_block(
                            &mouse_down_image_block,
                            image_width,
                            event,
                            window,
                            cx,
                        )
                    }),
                )
                .on_mouse_move(cx.listener(move |this, event, window, cx| {
                    this.mouse_move_on_image_block(
                        &mouse_move_image_block,
                        image_width,
                        event,
                        window,
                        cx,
                    )
                }))
                .rounded_md()
                .border_1()
                .border_color(palette.gutter_text)
                .bg(palette.fenced_code_background)
                .overflow_hidden()
                .child(
                    img(image_block.url)
                        .size_full()
                        .object_fit(gpui::ObjectFit::Contain)
                        .with_fallback(move || {
                            div()
                                .size_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .px_3()
                                .text_color(palette.muted_text)
                                .child(SharedString::from(fallback_label.clone()))
                                .into_any_element()
                        }),
                ),
        )
        .into_any_element()
}

fn caret_element(caret_x: gpui::Pixels, row_style: RowDisplayStyle) -> gpui::AnyElement {
    let palette = editor_palette();
    div()
        .absolute()
        .left(caret_x)
        .top_0()
        .bottom_0()
        .w(px(1.))
        .flex()
        .items_center()
        .child(div().w(px(1.)).h(row_style.caret_height).bg(palette.caret))
        .into_any_element()
}

fn rendered_image_block_for_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> Option<RenderedImageBlock> {
    if mode != MarkdownEditorMode::Rendered {
        return None;
    }

    let active_source_range = active_source_range_for_selection(snapshot, selection);
    let row_source_range = row_source_range(snapshot, display_row.row);
    let source_text = row_text(snapshot, display_row.row);
    let mut matching_spans = snapshot.syntax_tree().inline_spans().iter().filter(|span| {
        span.kind == MarkdownInlineKind::Image
            && span
                .url
                .as_ref()
                .is_some_and(|url| is_remote_image_url(url))
            && span.source_range.start >= row_source_range.start
            && span.source_range.end <= row_source_range.end
    });

    let span = matching_spans.next()?;
    if matching_spans.next().is_some() {
        return None;
    }
    if active_source_range
        .as_ref()
        .is_some_and(|active_source_range| ranges_overlap(&span.source_range, active_source_range))
    {
        return None;
    }

    let local_start = span.source_range.start - row_source_range.start;
    let local_end = span.source_range.end - row_source_range.start;
    if !source_text[..local_start].trim().is_empty() || !source_text[local_end..].trim().is_empty()
    {
        return None;
    }

    Some(RenderedImageBlock {
        url: span.url.clone()?,
        alt_text: display_row.text.trim().to_string(),
        source_range: span.source_range.clone(),
    })
}

fn render_text_piece(text: String, style: &DisplayTextStyle) -> gpui::AnyElement {
    let mut element = div().child(SharedString::from(text));

    if let Some(color) = style.color {
        element = element.text_color(color);
    }
    if let Some(weight) = style.font_weight {
        element = element.font_weight(weight);
    }
    if let Some(text_background) = style.text_background {
        element = element.text_bg(text_background);
    }
    if style.italic {
        element = element.italic();
    }
    if style.underline {
        element = element.underline();
        if let Some(color) = style.color {
            element = element.text_decoration_color(color);
        }
    }
    if style.line_through {
        element = element.line_through();
    }

    element.into_any_element()
}

fn render_inline_atom_piece(atom: &DisplayInlineAtom, text: String) -> gpui::AnyElement {
    match atom.kind {
        DisplayInlineAtomKind::InlineMath => {
            let palette = editor_palette();
            div()
                .h(atom.height)
                .flex()
                .items_center()
                .rounded_sm()
                .bg(palette.inline_math_text.opacity(0.08))
                .child(render_text_piece(text, &atom.style))
                .into_any_element()
        }
    }
}

fn display_inline_fragments(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
) -> Vec<DisplayInlineFragment> {
    let source_range = display_row.projection.visible_source_range();
    let source_text = row_text(snapshot, display_row.row);
    if source_text.is_empty() {
        return vec![DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..0,
            text: String::new(),
            style: DisplayTextStyle::default(),
        })];
    }

    let style_ranges = markdown_style_ranges_for_row(snapshot, source_range.clone(), mode);
    let atom_ranges = inline_atom_ranges_for_row(snapshot, display_row, mode, row_style);
    let hidden_ranges = display_row.projection.hidden_ranges();
    let mut breakpoints = vec![source_range.start, source_range.end];
    for hidden_range in hidden_ranges {
        breakpoints.push(hidden_range.start.max(source_range.start));
        breakpoints.push(hidden_range.end.min(source_range.end));
    }
    for (style_range, _) in &style_ranges {
        breakpoints.push(style_range.start);
        breakpoints.push(style_range.end);
    }
    for atom_range in &atom_ranges {
        breakpoints.push(atom_range.source_range.start);
        breakpoints.push(atom_range.source_range.end);
    }
    breakpoints.sort_unstable();
    breakpoints.dedup();

    let mut fragments: Vec<DisplayInlineFragment> = Vec::new();
    for window in breakpoints.windows(2) {
        let interval = window[0]..window[1];
        if interval.start >= interval.end
            || hidden_ranges
                .iter()
                .any(|hidden_range| range_contains(hidden_range, &interval))
        {
            continue;
        }

        if let Some(atom) = atom_ranges
            .iter()
            .find(|atom| range_contains(&atom.source_range, &interval))
        {
            if !fragments.iter().any(|fragment| {
                matches!(fragment, DisplayInlineFragment::Atom(existing) if existing.source_range == atom.source_range)
            }) {
                fragments.push(DisplayInlineFragment::Atom(atom.clone()));
            }
            continue;
        }

        let local_start = interval.start - source_range.start;
        let local_end = interval.end - source_range.start;
        let text = source_text[local_start..local_end].to_string();
        if text.is_empty() {
            continue;
        }

        let style = combined_style_for_range(&style_ranges, &interval);
        let display_range = display_row.projection.source_to_display(interval.start)
            ..display_row.projection.source_to_display(interval.end);

        if let Some(DisplayInlineFragment::Text(previous)) = fragments.last_mut()
            && previous.style == style
            && previous.display_range.end == display_range.start
        {
            previous.text.push_str(&text);
            previous.display_range.end = display_range.end;
        } else {
            fragments.push(DisplayInlineFragment::Text(StyledDisplaySegment {
                display_range,
                text,
                style,
            }));
        }
    }

    if fragments.is_empty() {
        vec![DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..display_row.text.len(),
            text: display_row.text.clone(),
            style: DisplayTextStyle::default(),
        })]
    } else {
        fragments
    }
}

fn text_segments_for_fragments(fragments: &[DisplayInlineFragment]) -> Vec<StyledDisplaySegment> {
    let mut segments: Vec<StyledDisplaySegment> = Vec::new();
    for fragment in fragments {
        let segment = match fragment {
            DisplayInlineFragment::Text(segment) => segment.clone(),
            DisplayInlineFragment::Atom(atom) => StyledDisplaySegment {
                display_range: atom.display_range.clone(),
                text: atom.fallback_text.clone(),
                style: atom.style.clone(),
            },
        };

        if let Some(previous) = segments.last_mut()
            && previous.style == segment.style
            && previous.display_range.end == segment.display_range.start
        {
            previous.text.push_str(&segment.text);
            previous.display_range.end = segment.display_range.end;
        } else {
            segments.push(segment);
        }
    }

    segments
}

fn inline_atom_ranges_for_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
    row_style: RowDisplayStyle,
) -> Vec<DisplayInlineAtom> {
    if mode != MarkdownEditorMode::Rendered {
        return Vec::new();
    }

    let row_source_range = display_row.projection.visible_source_range();
    let hidden_ranges = display_row.projection.hidden_ranges();
    snapshot
        .syntax_tree()
        .inline_spans()
        .iter()
        .filter(|span| {
            span.kind == MarkdownInlineKind::InlineMath
                && range_contains(&row_source_range, &span.source_range)
                && span.marker_ranges.iter().any(|marker_range| {
                    hidden_ranges
                        .iter()
                        .any(|hidden_range| ranges_overlap(marker_range, hidden_range))
                })
        })
        .filter_map(|span| inline_math_atom_for_span(display_row, span, row_style))
        .collect()
}

fn inline_math_atom_for_span(
    display_row: &DisplayRow,
    span: &markdown_wysiwyg::MarkdownInlineSpan,
    row_style: RowDisplayStyle,
) -> Option<DisplayInlineAtom> {
    let display_range = display_row
        .projection
        .source_to_display(span.source_range.start)
        ..display_row
            .projection
            .source_to_display(span.source_range.end);
    let fallback_text = display_row.text.get(display_range.clone())?.to_string();
    if fallback_text.is_empty() {
        return None;
    }

    Some(DisplayInlineAtom {
        kind: DisplayInlineAtomKind::InlineMath,
        source_range: span.source_range.clone(),
        display_range,
        fallback_text,
        style: inline_style(MarkdownInlineKind::InlineMath),
        height: inline_atom_height(DisplayInlineAtomKind::InlineMath, row_style),
    })
}

fn inline_atom_height(kind: DisplayInlineAtomKind, row_style: RowDisplayStyle) -> gpui::Pixels {
    match kind {
        DisplayInlineAtomKind::InlineMath => row_style.line_height + INLINE_MATH_ATOM_EXTRA_HEIGHT,
    }
}

fn markdown_style_ranges_for_row(
    snapshot: &BufferSnapshot,
    row_source_range: Range<usize>,
    mode: MarkdownEditorMode,
) -> Vec<(Range<usize>, DisplayTextStyle)> {
    if mode == MarkdownEditorMode::Source {
        return Vec::new();
    }

    let mut style_ranges = Vec::new();
    for block in snapshot
        .syntax_tree()
        .blocks_in_source_range(row_source_range.clone())
    {
        match block.kind {
            MarkdownBlockKind::AtxHeading { level } => {
                push_style_range(
                    &mut style_ranges,
                    row_source_range.clone(),
                    block.content_range.clone(),
                    heading_style(level),
                );
            }
            MarkdownBlockKind::FencedCodeBlock => {
                push_style_range(
                    &mut style_ranges,
                    row_source_range.clone(),
                    block.content_range.clone(),
                    fenced_code_style(),
                );
            }
            MarkdownBlockKind::PipeTable => {
                push_style_range(
                    &mut style_ranges,
                    row_source_range.clone(),
                    block.content_range.clone(),
                    pipe_table_style(),
                );
            }
            MarkdownBlockKind::Blank | MarkdownBlockKind::Paragraph => {}
        }
    }

    for span in snapshot.syntax_tree().inline_spans() {
        if !ranges_overlap(&span.source_range, &row_source_range) {
            continue;
        }
        let style = inline_style(span.kind);
        for content_range in &span.content_ranges {
            push_style_range(
                &mut style_ranges,
                row_source_range.clone(),
                content_range.clone(),
                style.clone(),
            );
        }
    }

    style_ranges
}

fn push_style_range(
    style_ranges: &mut Vec<(Range<usize>, DisplayTextStyle)>,
    row_source_range: Range<usize>,
    style_range: Range<usize>,
    style: DisplayTextStyle,
) {
    let clipped_start = style_range.start.max(row_source_range.start);
    let clipped_end = style_range.end.min(row_source_range.end);
    if clipped_start < clipped_end {
        style_ranges.push((clipped_start..clipped_end, style));
    }
}

fn combined_style_for_range(
    style_ranges: &[(Range<usize>, DisplayTextStyle)],
    interval: &Range<usize>,
) -> DisplayTextStyle {
    let mut combined = DisplayTextStyle::default();
    for (style_range, style) in style_ranges {
        if range_contains(style_range, interval) {
            combined = combined.merge(style);
        }
    }
    combined
}

fn range_contains(container: &Range<usize>, candidate: &Range<usize>) -> bool {
    container.start <= candidate.start && container.end >= candidate.end
}

fn is_remote_image_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn heading_style(level: u8) -> DisplayTextStyle {
    let palette = editor_palette();
    DisplayTextStyle {
        color: Some(match level {
            1 | 2 => palette.heading_primary,
            3 => palette.heading_accent,
            _ => palette.heading_muted,
        }),
        font_weight: Some(match level {
            1 => FontWeight::BLACK,
            2 => FontWeight::EXTRA_BOLD,
            3 => FontWeight::BOLD,
            _ => FontWeight::SEMIBOLD,
        }),
        ..Default::default()
    }
}

fn row_display_style(
    snapshot: &BufferSnapshot,
    display_row: u32,
    mode: MarkdownEditorMode,
) -> RowDisplayStyle {
    if mode == MarkdownEditorMode::Rendered {
        if let Some(level) = heading_level_for_row(snapshot, display_row) {
            return heading_row_metrics(level).into();
        }
    }

    default_row_metrics().into()
}

fn heading_level_for_row(snapshot: &BufferSnapshot, row: u32) -> Option<u8> {
    let source_range = row_source_range(snapshot, row);
    let row = row as usize;
    snapshot
        .syntax_tree()
        .blocks_in_source_range(source_range)
        .find_map(|block| match block.kind {
            MarkdownBlockKind::AtxHeading { level } if block.row_range.start == row => Some(level),
            _ => None,
        })
}

fn inline_style(kind: MarkdownInlineKind) -> DisplayTextStyle {
    let palette = editor_palette();
    match kind {
        MarkdownInlineKind::Strong => DisplayTextStyle {
            font_weight: Some(FontWeight::BOLD),
            ..Default::default()
        },
        MarkdownInlineKind::Emphasis => DisplayTextStyle {
            italic: true,
            ..Default::default()
        },
        MarkdownInlineKind::InlineCode => DisplayTextStyle {
            color: Some(palette.inline_code_text),
            text_background: Some(palette.inline_code_background),
            font_weight: Some(FontWeight::MEDIUM),
            ..Default::default()
        },
        MarkdownInlineKind::Link | MarkdownInlineKind::Image => DisplayTextStyle {
            color: Some(palette.link_text),
            underline: true,
            ..Default::default()
        },
        MarkdownInlineKind::Strikethrough => DisplayTextStyle {
            line_through: true,
            color: Some(palette.muted_text),
            ..Default::default()
        },
        MarkdownInlineKind::InlineMath => DisplayTextStyle {
            color: Some(palette.inline_math_text),
            italic: true,
            ..Default::default()
        },
    }
}

fn fenced_code_style() -> DisplayTextStyle {
    let palette = editor_palette();
    DisplayTextStyle {
        text_background: Some(palette.fenced_code_background),
        ..Default::default()
    }
}

fn pipe_table_style() -> DisplayTextStyle {
    let palette = editor_palette();
    DisplayTextStyle {
        color: Some(palette.pipe_table_text),
        text_background: Some(palette.pipe_table_background),
        ..Default::default()
    }
}

impl DisplayTextStyle {
    fn merge(mut self, overlay: &DisplayTextStyle) -> Self {
        if let Some(color) = overlay.color {
            self.color = Some(color);
        }
        if let Some(text_background) = overlay.text_background {
            self.text_background = Some(text_background);
        }
        self.font_weight = match (self.font_weight, overlay.font_weight) {
            (Some(current), Some(overlay)) => {
                Some(if current >= overlay { current } else { overlay })
            }
            (None, Some(overlay)) => Some(overlay),
            (current, None) => current,
        };
        self.italic |= overlay.italic;
        self.underline |= overlay.underline;
        self.line_through |= overlay.line_through;
        self
    }
}

fn selected_range_for_row(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
) -> Option<Range<usize>> {
    if selection.is_empty() {
        return None;
    }

    let selection_range = selection_byte_range(snapshot, selection);
    let row_range = display_row.projection.visible_source_range();
    if selection_range.end <= row_range.start || selection_range.start >= row_range.end {
        return None;
    }

    let start = selection_range.start.max(row_range.start);
    let end = selection_range.end.min(row_range.end);
    let start = display_row.projection.source_to_display(start);
    let end = display_row.projection.source_to_display(end);

    Some(start.min(end)..end.max(start))
}

fn active_source_range_for_selection(
    snapshot: &BufferSnapshot,
    selection: &Selection<Point>,
) -> Option<Range<usize>> {
    let selection = clip_selection(snapshot, selection);
    if !selection.is_empty() {
        return Some(selection_byte_range(snapshot, &selection));
    }

    let text_snapshot = snapshot.as_text_snapshot();
    if text_snapshot.len() == 0 {
        return None;
    }

    let offset = text_snapshot.point_to_offset(selection.head());
    if offset < text_snapshot.len() {
        let end = text_snapshot
            .as_rope()
            .ceil_char_boundary(offset.saturating_add(1));
        Some(offset..end)
    } else {
        let start = text_snapshot
            .as_rope()
            .floor_char_boundary(offset.saturating_sub(1));
        Some(start..offset)
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
mod tests {
    use super::*;

    #[test]
    fn display_rows_preserve_empty_lines_and_final_empty_row() {
        let mut buffer = Buffer::local("alpha\n\nbeta\n");
        let snapshot = buffer.snapshot();

        assert_eq!(
            display_rows(&snapshot, 0..snapshot.row_count() as usize)
                .into_iter()
                .map(|row| (row.row, row.text))
                .collect::<Vec<_>>(),
            vec![
                (0, "alpha".to_string()),
                (1, String::new()),
                (2, "beta".to_string()),
                (3, String::new()),
            ]
        );
    }

    #[test]
    fn display_rows_clips_requested_range_to_buffer_rows() {
        let mut buffer = Buffer::local("one\ntwo");
        let snapshot = buffer.snapshot();

        assert_eq!(
            display_rows(&snapshot, 1..10)
                .into_iter()
                .map(|row| (row.row, row.text))
                .collect::<Vec<_>>(),
            vec![(1, "two".to_string())]
        );
    }

    #[test]
    fn row_text_returns_empty_string_for_out_of_bounds_rows() {
        let mut buffer = Buffer::local("one");
        let snapshot = buffer.snapshot();

        assert_eq!(row_text(&snapshot, 1), "");
    }

    #[test]
    fn rendered_display_rows_hide_inactive_heading_markers() {
        let mut buffer = Buffer::local("# Title\nBody\n");
        let snapshot = buffer.snapshot();

        let rows = display_rows_in_mode(
            &snapshot,
            0..2,
            Some(&collapsed_selection(Point::new(1, 0))),
            MarkdownEditorMode::Rendered,
        );

        assert_eq!(rows[0].text, "Title");
        assert_eq!(rows[1].text, "Body");
    }

    #[test]
    fn rendered_display_rows_reveal_active_heading_markers() {
        let mut buffer = Buffer::local("# Title\n## Other\n");
        let snapshot = buffer.snapshot();

        let rows = display_rows_in_mode(
            &snapshot,
            0..2,
            Some(&collapsed_selection(Point::new(0, 0))),
            MarkdownEditorMode::Rendered,
        );

        assert_eq!(rows[0].text, "# Title");
        assert_eq!(rows[1].text, "Other");
    }

    #[test]
    fn rendered_display_rows_hide_inactive_inline_markers() {
        let mut buffer = Buffer::local("Before **bold** after\n");
        let snapshot = buffer.snapshot();

        let rows = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&collapsed_selection(Point::new(0, 0))),
            MarkdownEditorMode::Rendered,
        );

        assert_eq!(rows[0].text, "Before bold after");
    }

    #[test]
    fn rendered_display_rows_reveal_active_inline_markers() {
        let mut buffer = Buffer::local("Before **bold** after\n");
        let snapshot = buffer.snapshot();

        let rows = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&collapsed_selection(Point::new(0, 10))),
            MarkdownEditorMode::Rendered,
        );

        assert_eq!(rows[0].text, "Before **bold** after");
    }

    #[test]
    fn rendered_styled_segments_apply_heading_semantics() {
        let mut buffer = Buffer::local("# Title\nBody\n");
        let snapshot = buffer.snapshot();
        let row = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&collapsed_selection(Point::new(1, 0))),
            MarkdownEditorMode::Rendered,
        )
        .remove(0);

        let row_style = row_display_style(&snapshot, row.row, MarkdownEditorMode::Rendered);
        let fragments =
            display_inline_fragments(&snapshot, &row, MarkdownEditorMode::Rendered, row_style);
        let segments = text_segments_for_fragments(&fragments);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Title");
        assert_eq!(segments[0].style.font_weight, Some(FontWeight::BLACK));
        assert_eq!(
            segments[0].style.color,
            Some(md_theme::editor_palette().heading_primary)
        );
    }

    #[test]
    fn rendered_styled_segments_apply_inline_semantics() {
        let mut buffer = Buffer::local("Before **bold** and `code`\n");
        let snapshot = buffer.snapshot();
        let row = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&collapsed_selection(Point::new(0, 0))),
            MarkdownEditorMode::Rendered,
        )
        .remove(0);

        let row_style = row_display_style(&snapshot, row.row, MarkdownEditorMode::Rendered);
        let fragments =
            display_inline_fragments(&snapshot, &row, MarkdownEditorMode::Rendered, row_style);
        let segments = text_segments_for_fragments(&fragments);

        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Before ", "bold", " and ", "code"]
        );
        assert_eq!(segments[1].style.font_weight, Some(FontWeight::BOLD));
        assert!(segments[3].style.text_background.is_some());
        assert_eq!(
            segments[3].style.color,
            Some(md_theme::editor_palette().inline_code_text)
        );
    }

    #[test]
    fn rendered_inline_fragments_create_inline_math_atom() {
        let mut buffer = Buffer::local("Before $x + y$ after\n");
        let snapshot = buffer.snapshot();
        let row = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&collapsed_selection(Point::new(0, 0))),
            MarkdownEditorMode::Rendered,
        )
        .remove(0);

        assert_eq!(row.text, "Before x + y after");

        let row_style = row_display_style(&snapshot, row.row, MarkdownEditorMode::Rendered);
        let fragments =
            display_inline_fragments(&snapshot, &row, MarkdownEditorMode::Rendered, row_style);
        let atom = fragments
            .iter()
            .find_map(|fragment| match fragment {
                DisplayInlineFragment::Atom(atom) => Some(atom),
                DisplayInlineFragment::Text(_) => None,
            })
            .expect("expected inline math atom fragment");

        assert_eq!(atom.kind, DisplayInlineAtomKind::InlineMath);
        assert_eq!(atom.fallback_text, "x + y");
        assert_eq!(atom.display_range, 7..12);
        assert_eq!(
            atom.height,
            row_style.line_height + INLINE_MATH_ATOM_EXTRA_HEIGHT
        );
        assert_eq!(
            text_segments_for_fragments(&fragments)
                .iter()
                .map(|segment| segment.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Before ", "x + y", " after"]
        );
    }

    #[test]
    fn inline_atom_height_expands_visual_row_height() {
        let row_style: RowDisplayStyle = md_theme::default_row_metrics().into();
        let atom_height = row_style.line_height + INLINE_MATH_ATOM_EXTRA_HEIGHT;
        let fragments = vec![
            DisplayInlineFragment::Text(StyledDisplaySegment {
                display_range: 0..7,
                text: "Before ".to_string(),
                style: DisplayTextStyle::default(),
            }),
            DisplayInlineFragment::Atom(DisplayInlineAtom {
                kind: DisplayInlineAtomKind::InlineMath,
                source_range: 8..15,
                display_range: 7..12,
                fallback_text: "x + y".to_string(),
                style: inline_style(MarkdownInlineKind::InlineMath),
                height: atom_height,
            }),
            DisplayInlineFragment::Text(StyledDisplaySegment {
                display_range: 12..18,
                text: " after".to_string(),
                style: DisplayTextStyle::default(),
            }),
        ];

        assert_eq!(
            visual_row_height_for_range(&fragments, &(0..7), row_style),
            row_style.line_height
        );
        assert_eq!(
            visual_row_height_for_range(&fragments, &(7..12), row_style),
            atom_height
        );
        assert_eq!(
            visual_row_height_for_range(&fragments, &(12..18), row_style),
            row_style.line_height
        );
    }

    #[test]
    fn atomic_wrap_boundary_keeps_inline_atom_on_one_visual_row() {
        let fragments = vec![DisplayInlineFragment::Atom(DisplayInlineAtom {
            kind: DisplayInlineAtomKind::InlineMath,
            source_range: 8..15,
            display_range: 7..12,
            fallback_text: "x + y".to_string(),
            style: inline_style(MarkdownInlineKind::InlineMath),
            height: px(24.),
        })];

        assert_eq!(atom_range_containing_display_index(&fragments, 7), None);
        assert_eq!(
            atom_range_containing_display_index(&fragments, 9),
            Some(7..12)
        );
        assert_eq!(atom_range_containing_display_index(&fragments, 12), None);
        assert_eq!(atomic_wrap_boundary_index(&fragments, 9, 0), 7);
        assert_eq!(atomic_wrap_boundary_index(&fragments, 9, 7), 12);
        assert_eq!(atomic_wrap_boundary_index(&fragments, 15, 12), 15);
    }

    #[test]
    fn fragment_text_for_visual_row_clips_to_visible_range() {
        let visual_row = VisualDisplayRow {
            display_range: 7..12,
            line_start_x: px(48.),
            top: px(20.),
            height: px(24.),
        };

        assert_eq!(
            fragment_text_for_visual_row(&(0..18), "Before x + y after", &visual_row),
            Some("x + y".to_string())
        );
        assert_eq!(
            fragment_text_for_visual_row(&(0..7), "Before ", &visual_row),
            None
        );
    }

    #[test]
    fn rendered_row_display_style_scales_headings() {
        let mut buffer = Buffer::local("# Title\n## Subtitle\nBody\n");
        let snapshot = buffer.snapshot();

        assert_eq!(
            row_display_style(&snapshot, 0, MarkdownEditorMode::Rendered),
            md_theme::heading_row_metrics(1).into()
        );
        assert_eq!(
            row_display_style(&snapshot, 1, MarkdownEditorMode::Rendered),
            md_theme::heading_row_metrics(2).into()
        );
        assert_eq!(
            row_display_style(&snapshot, 2, MarkdownEditorMode::Rendered),
            md_theme::default_row_metrics().into()
        );
        assert_eq!(
            row_display_style(&snapshot, 0, MarkdownEditorMode::Source),
            md_theme::default_row_metrics().into()
        );
    }

    #[test]
    fn rendered_image_block_detects_inactive_remote_image_row() {
        let mut buffer = Buffer::local("![alt](https://example.com/cat.png)\nnext\n");
        let snapshot = buffer.snapshot();
        let selection = collapsed_selection(Point::new(1, 0));
        let row = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&selection),
            MarkdownEditorMode::Rendered,
        )
        .remove(0);

        assert_eq!(row.text, "alt");
        assert_eq!(
            rendered_image_block_for_row(&snapshot, &row, &selection, MarkdownEditorMode::Rendered),
            Some(RenderedImageBlock {
                url: "https://example.com/cat.png".to_string(),
                alt_text: "alt".to_string(),
                source_range: 0..35,
            })
        );
    }

    #[test]
    fn image_block_mouse_x_maps_to_source_range_edges() {
        let mut buffer = Buffer::local("![alt](https://example.com/cat.png)\n");
        let snapshot = buffer.snapshot();
        let image_block = RenderedImageBlock {
            url: "https://example.com/cat.png".to_string(),
            alt_text: "alt".to_string(),
            source_range: 0..35,
        };

        assert_eq!(
            point_for_image_block_mouse_x(&snapshot, &image_block, px(200.), gutter_width()),
            Point::new(0, 0)
        );
        assert_eq!(
            point_for_image_block_mouse_x(
                &snapshot,
                &image_block,
                px(200.),
                gutter_width() + px(160.)
            ),
            Point::new(0, 35)
        );
    }

    #[test]
    fn image_block_local_x_maps_to_source_range_edges() {
        let image_block = RenderedImageBlock {
            url: "https://example.com/cat.png".to_string(),
            alt_text: "alt".to_string(),
            source_range: 4..39,
        };

        assert_eq!(
            image_block_source_offset_for_x(&image_block, px(200.), px(0.)),
            4
        );
        assert_eq!(
            image_block_source_offset_for_x(&image_block, px(200.), px(99.)),
            4
        );
        assert_eq!(
            image_block_source_offset_for_x(&image_block, px(200.), px(100.)),
            39
        );
        assert_eq!(
            image_block_source_offset_for_x(&image_block, px(200.), px(250.)),
            39
        );
    }

    #[test]
    fn image_block_source_offset_maps_to_visible_x() {
        let image_layout = RenderedImageBlockLayout {
            image_block: RenderedImageBlock {
                url: "https://example.com/cat.png".to_string(),
                alt_text: "alt".to_string(),
                source_range: 4..39,
            },
            width: px(200.),
            height: px(120.),
        };

        assert_eq!(image_block_x_for_source_offset(&image_layout, 4), px(0.));
        assert_eq!(image_block_x_for_source_offset(&image_layout, 20), px(100.));
        assert_eq!(image_block_x_for_source_offset(&image_layout, 39), px(200.));
    }

    #[test]
    fn image_block_height_preserves_aspect_ratio() {
        assert_eq!(
            image_block_height_for_size(px(600.), 1200, 800),
            Some(px(400.))
        );
        assert_eq!(
            image_block_height_for_size(px(300.), 800, 1200),
            Some(px(450.))
        );
    }

    #[test]
    fn image_block_height_returns_none_for_empty_image_size() {
        assert_eq!(image_block_height_for_size(px(600.), 0, 800), None);
        assert_eq!(image_block_height_for_size(px(600.), 1200, 0), None);
        assert_eq!(image_block_height_for_size(px(600.), 0, 0), None);
    }

    #[test]
    fn rendered_image_block_reveals_active_image_source() {
        let mut buffer = Buffer::local("![alt](https://example.com/cat.png)\n");
        let snapshot = buffer.snapshot();
        let selection = collapsed_selection(Point::new(0, 2));
        let row = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&selection),
            MarkdownEditorMode::Rendered,
        )
        .remove(0);

        assert_eq!(row.text, "![alt](https://example.com/cat.png)");
        assert_eq!(
            rendered_image_block_for_row(&snapshot, &row, &selection, MarkdownEditorMode::Rendered),
            None
        );
    }

    #[test]
    fn rendered_image_block_skips_inline_images_with_surrounding_text() {
        let mut buffer = Buffer::local("before ![alt](https://example.com/cat.png) after\n");
        let snapshot = buffer.snapshot();
        let selection = collapsed_selection(Point::new(0, 0));
        let row = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&selection),
            MarkdownEditorMode::Rendered,
        )
        .remove(0);

        assert_eq!(
            rendered_image_block_for_row(&snapshot, &row, &selection, MarkdownEditorMode::Rendered),
            None
        );
    }

    #[test]
    fn horizontal_movement_crosses_lines_and_respects_utf8_boundaries() {
        let mut buffer = Buffer::local("a\nβ");
        let snapshot = buffer.snapshot();

        let cursor = move_right(&snapshot, Point::zero());
        assert_eq!(cursor, Point::new(0, 1));
        let cursor = move_right(&snapshot, cursor);
        assert_eq!(cursor, Point::new(1, 0));
        let cursor = move_right(&snapshot, cursor);
        assert_eq!(cursor, Point::new(1, "β".len() as u32));
        assert_eq!(move_right(&snapshot, cursor), cursor);

        let cursor = move_left(&snapshot, cursor);
        assert_eq!(cursor, Point::new(1, 0));
        let cursor = move_left(&snapshot, cursor);
        assert_eq!(cursor, Point::new(0, 1));
        let cursor = move_left(&snapshot, cursor);
        assert_eq!(cursor, Point::zero());
        assert_eq!(move_left(&snapshot, cursor), Point::zero());
    }

    #[test]
    fn vertical_movement_clips_to_target_line_end() {
        let mut buffer = Buffer::local("abcd\nx\nβγ");
        let snapshot = buffer.snapshot();

        assert_eq!(
            move_vertical(&snapshot, Point::new(0, 3), 1),
            Point::new(1, 1)
        );
        assert_eq!(
            move_vertical(&snapshot, Point::new(0, 3), 2),
            Point::new(2, "β".len() as u32)
        );
        assert_eq!(
            move_vertical(&snapshot, Point::new(2, 2), -1),
            Point::new(1, 1)
        );
    }

    #[test]
    fn line_boundary_movement_uses_current_row() {
        let mut buffer = Buffer::local("abc\nβ");
        let snapshot = buffer.snapshot();

        assert_eq!(
            move_to_beginning_of_line(&snapshot, Point::new(1, 1)),
            Point::new(1, 0)
        );
        assert_eq!(
            move_to_end_of_line(&snapshot, Point::new(1, 0)),
            Point::new(1, "β".len() as u32)
        );
    }

    #[test]
    fn moving_left_collapses_non_empty_selection_to_start() {
        let mut buffer = Buffer::local("abcd");
        let snapshot = buffer.snapshot();
        let selection = Selection {
            id: 1,
            start: Point::new(0, 1),
            end: Point::new(0, 3),
            reversed: false,
            goal: SelectionGoal::None,
        };

        assert_eq!(
            move_selection_left(&snapshot, &selection),
            collapsed_selection(Point::new(0, 1))
        );
    }

    #[test]
    fn select_left_moves_head_and_preserves_tail() {
        let mut buffer = Buffer::local("abcd");
        let snapshot = buffer.snapshot();
        let selection = collapsed_selection(Point::new(0, 3));

        assert_eq!(
            select_left(&snapshot, &selection),
            Selection {
                id: 0,
                start: Point::new(0, 2),
                end: Point::new(0, 3),
                reversed: true,
                goal: SelectionGoal::None,
            }
        );
    }

    #[test]
    fn selected_range_for_row_handles_multiline_selection() {
        let mut buffer = Buffer::local("abcd\nxy\npq");
        let snapshot = buffer.snapshot();
        let display_rows = display_rows(&snapshot, 0..3);
        let selection = Selection {
            id: 1,
            start: Point::new(0, 2),
            end: Point::new(2, 1),
            reversed: false,
            goal: SelectionGoal::None,
        };

        assert_eq!(
            selected_range_for_row(&snapshot, &display_rows[0], &selection),
            Some(2..4)
        );
        assert_eq!(
            selected_range_for_row(&snapshot, &display_rows[1], &selection),
            Some(0..2)
        );
        assert_eq!(
            selected_range_for_row(&snapshot, &display_rows[2], &selection),
            Some(0..1)
        );
    }

    #[test]
    fn visual_row_contains_caret_assigns_wrap_boundary_to_next_row() {
        let first_visual_row = VisualDisplayRow {
            display_range: 0..5,
            line_start_x: px(0.),
            top: px(0.),
            height: px(20.),
        };
        let second_visual_row = VisualDisplayRow {
            display_range: 5..10,
            line_start_x: px(48.),
            top: px(20.),
            height: px(20.),
        };

        assert!(visual_row_contains_caret(&first_visual_row, 4, 10));
        assert!(!visual_row_contains_caret(&first_visual_row, 5, 10));
        assert!(visual_row_contains_caret(&second_visual_row, 5, 10));
        assert!(visual_row_contains_caret(&second_visual_row, 10, 10));
    }

    #[test]
    fn visual_row_contains_caret_handles_empty_text_row() {
        let empty_visual_row = VisualDisplayRow {
            display_range: 0..0,
            line_start_x: px(0.),
            top: px(0.),
            height: px(20.),
        };

        assert!(visual_row_contains_caret(&empty_visual_row, 0, 0));
    }

    #[test]
    fn visual_row_index_containing_caret_finds_boundary_row() {
        let visual_rows = vec![
            VisualDisplayRow {
                display_range: 0..5,
                line_start_x: px(0.),
                top: px(0.),
                height: px(20.),
            },
            VisualDisplayRow {
                display_range: 5..10,
                line_start_x: px(48.),
                top: px(20.),
                height: px(20.),
            },
        ];

        assert_eq!(
            visual_row_index_containing_caret(&visual_rows, 0, 10),
            Some(0)
        );
        assert_eq!(
            visual_row_index_containing_caret(&visual_rows, 5, 10),
            Some(1)
        );
        assert_eq!(
            visual_row_index_containing_caret(&visual_rows, 10, 10),
            Some(1)
        );
        assert_eq!(
            visual_row_index_containing_caret(&visual_rows, 11, 10),
            None
        );
    }

    #[test]
    fn desired_visual_x_reuses_vertical_movement_goal() {
        assert_eq!(desired_visual_x(SelectionGoal::None, px(12.)), px(12.));
        assert_eq!(
            desired_visual_x(SelectionGoal::HorizontalPosition(42.), px(12.)),
            px(42.)
        );
        assert_eq!(
            desired_visual_x(SelectionGoal::WrappedHorizontalPosition((3, 64.)), px(12.)),
            px(64.)
        );
    }

    #[test]
    fn replace_selection_inserts_text_and_collapses_after_inserted_text() {
        let mut buffer = Buffer::local("abef");
        let selection = Selection {
            id: 1,
            start: Point::new(0, 2),
            end: Point::new(0, 2),
            reversed: false,
            goal: SelectionGoal::None,
        };

        let (selection, transaction_id) = replace_selection(&mut buffer, &selection, "cd");

        assert_eq!(buffer.text(), "abcdef");
        assert_eq!(selection, collapsed_selection(Point::new(0, 4)));
        assert!(transaction_id.is_some());
    }

    #[test]
    fn replace_selection_replaces_active_selection() {
        let mut buffer = Buffer::local("abcdef");
        let selection = Selection {
            id: 1,
            start: Point::new(0, 2),
            end: Point::new(0, 4),
            reversed: false,
            goal: SelectionGoal::None,
        };

        let (selection, transaction_id) = replace_selection(&mut buffer, &selection, "ZZ");

        assert_eq!(buffer.text(), "abZZef");
        assert_eq!(selection, collapsed_selection(Point::new(0, 4)));
        assert!(transaction_id.is_some());
    }

    #[test]
    fn backspace_selection_deletes_previous_utf8_character() {
        let mut buffer = Buffer::local("aβ");
        let selection = collapsed_selection(Point::new(0, "aβ".len() as u32));

        let (selection, transaction_id) = backspace_selection(&mut buffer, &selection);

        assert_eq!(buffer.text(), "a");
        assert_eq!(selection, collapsed_selection(Point::new(0, 1)));
        assert!(transaction_id.is_some());
    }

    #[test]
    fn delete_selection_deletes_selected_range() {
        let mut buffer = Buffer::local("abcdef");
        let selection = Selection {
            id: 1,
            start: Point::new(0, 1),
            end: Point::new(0, 4),
            reversed: false,
            goal: SelectionGoal::None,
        };

        let (selection, transaction_id) = delete_selection(&mut buffer, &selection);

        assert_eq!(buffer.text(), "aef");
        assert_eq!(selection, collapsed_selection(Point::new(0, 1)));
        assert!(transaction_id.is_some());
    }

    #[test]
    fn tab_inserts_soft_tab_spaces() {
        let mut buffer = Buffer::local("ab");
        let selection = collapsed_selection(Point::new(0, 1));

        // Soft tabs: tab_size=4 → insert 4 spaces
        let tab_text = "    "; // 4 spaces
        let (selection, transaction_id) = replace_selection(&mut buffer, &selection, tab_text);

        assert_eq!(buffer.text(), "a    b");
        assert_eq!(selection, collapsed_selection(Point::new(0, 5)));
        assert!(transaction_id.is_some());
    }

    #[test]
    fn tab_inserts_hard_tab_character() {
        let mut buffer = Buffer::local("ab");
        let selection = collapsed_selection(Point::new(0, 1));

        let (_selection, transaction_id) = replace_selection(&mut buffer, &selection, "\t");

        assert_eq!(buffer.text(), "a\tb");
        assert!(transaction_id.is_some());
    }

    #[test]
    fn auto_indent_preserves_current_line_indent_on_newline() {
        let mut buffer = Buffer::local("    hello");
        let cursor = Point::new(0, 7); // after 'l' in 'hello'
        let indent = current_line_indent(&buffer.snapshot(), cursor);

        assert_eq!(indent, "    "); // 4 spaces preserved

        // Simulating InsertNewline: "\n" + indent
        let selection = collapsed_selection(cursor);
        let insert_text = format!("\n{indent}");
        let (selection, _) = replace_selection(&mut buffer, &selection, &insert_text);

        assert_eq!(buffer.text(), "    hel\n    lo");
        assert_eq!(selection, collapsed_selection(Point::new(1, 4)));
    }

    #[test]
    fn auto_indent_no_indent_for_unindented_line() {
        let mut buffer = Buffer::local("hello");
        let cursor = Point::new(0, 3);
        let indent = current_line_indent(&buffer.snapshot(), cursor);

        assert_eq!(indent, "");

        let selection = collapsed_selection(cursor);
        let (_selection, _) = replace_selection(&mut buffer, &selection, "\n");

        assert_eq!(buffer.text(), "hel\nlo");
    }
}
