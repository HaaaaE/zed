use std::{collections::HashMap, ops::Range};

use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, FontWeight, IntoElement, KeyDownEvent,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Render, SharedString,
    StatefulInteractiveElement, TextAlign, TextRun, Window, div, font, prelude::*, px,
    uniform_list,
};
use markdown_wysiwyg::{MarkdownBlockKind, MarkdownInlineKind, MarkdownProjectionMap};
use md_buffer::{Buffer, BufferSnapshot};
use md_text::{Bias, Point, Selection, SelectionGoal};

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
        Undo,
        Redo,
    ]
);

pub struct MarkdownEditor {
    buffer: Buffer,
    focus_handle: FocusHandle,
    mode: MarkdownEditorMode,
    selection: Selection<Point>,
    is_selecting_with_mouse: bool,
    selection_history: HashMap<md_text::TransactionId, TransactionSelectionState>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MarkdownEditorEvent {
    DirtyChanged(bool),
}

impl EventEmitter<MarkdownEditorEvent> for MarkdownEditor {}

impl MarkdownEditor {
    pub fn new(buffer: Buffer, cx: &mut Context<Self>) -> Self {
        Self {
            buffer,
            focus_handle: cx.focus_handle(),
            mode: MarkdownEditorMode::Source,
            selection: collapsed_selection(Point::zero()),
            is_selecting_with_mouse: false,
            selection_history: HashMap::default(),
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
        self.selection = collapsed_selection(clip_cursor(&self.buffer.snapshot(), cursor));
    }

    pub fn move_left(&mut self, _: &MoveLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = move_selection_left(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn move_right(&mut self, _: &MoveRight, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = move_selection_right(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = move_selection_vertical(&self.buffer.snapshot(), &self.selection, -1);
        cx.notify();
    }

    pub fn move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = move_selection_vertical(&self.buffer.snapshot(), &self.selection, 1);
        cx.notify();
    }

    pub fn move_to_beginning_of_line(
        &mut self,
        _: &MoveToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection =
            move_selection_to_beginning_of_line(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn move_to_end_of_line(
        &mut self,
        _: &MoveToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = move_selection_to_end_of_line(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = select_left(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = select_right(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = select_vertical(&self.buffer.snapshot(), &self.selection, -1);
        cx.notify();
    }

    pub fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        self.selection = select_vertical(&self.buffer.snapshot(), &self.selection, 1);
        cx.notify();
    }

    pub fn select_to_beginning_of_line(
        &mut self,
        _: &SelectToBeginningOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = select_to_beginning_of_line(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_to_end_of_line(
        &mut self,
        _: &SelectToEndOfLine,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selection = select_to_end_of_line(&self.buffer.snapshot(), &self.selection);
        cx.notify();
    }

    pub fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let max_point = self.buffer.snapshot().as_text_snapshot().max_point();
        self.selection = Selection {
            id: 0,
            start: Point::zero(),
            end: max_point,
            reversed: false,
            goal: SelectionGoal::None,
        };
        cx.notify();
    }

    pub fn backspace(&mut self, _: &Backspace, _: &mut Window, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let (selection, transaction_id) = backspace_selection(&mut self.buffer, &self.selection);
        let changed = transaction_id.is_some();
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(changed, cx);
    }

    pub fn delete(&mut self, _: &Delete, _: &mut Window, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let (selection, transaction_id) = delete_selection(&mut self.buffer, &self.selection);
        let changed = transaction_id.is_some();
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(changed, cx);
    }

    pub fn insert_newline(&mut self, _: &InsertNewline, _: &mut Window, cx: &mut Context<Self>) {
        let selection_before = self.selection.clone();
        let (selection, transaction_id) =
            replace_selection(&mut self.buffer, &self.selection, "\n");
        let changed = transaction_id.is_some();
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        self.notify_after_edit(changed, cx);
    }

    pub fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
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
        self.notify_after_edit(changed, cx);
    }

    pub fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
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
        self.notify_after_edit(changed, cx);
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
        let (selection, transaction_id) =
            replace_selection(&mut self.buffer, &self.selection, text);
        let changed = transaction_id.is_some();
        self.selection = selection;
        self.record_selection_history(transaction_id, selection_before, self.selection.clone());
        cx.stop_propagation();
        self.notify_after_edit(changed, cx);
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

    fn notify_after_edit(&mut self, changed: bool, cx: &mut Context<Self>) {
        if changed {
            self.emit_dirty_state(cx);
        } else {
            cx.notify();
        }
    }

    fn emit_dirty_state(&mut self, cx: &mut Context<Self>) {
        cx.emit(MarkdownEditorEvent::DirtyChanged(self.buffer.is_dirty()));
        cx.notify();
    }

    fn mouse_left_down_on_row(
        &mut self,
        display_row: &DisplayRow,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle.clone(), cx);
        self.is_selecting_with_mouse = true;

        let snapshot = self.buffer.snapshot();
        let point = point_for_mouse_x(&snapshot, display_row, event.position.x, window);
        self.selection = if event.modifiers.shift {
            select_to_point(&snapshot, &self.selection, point)
        } else {
            collapsed_selection(point)
        };
        cx.notify();
    }

    fn mouse_move_on_row(
        &mut self,
        display_row: &DisplayRow,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting_with_mouse || event.pressed_button != Some(MouseButton::Left) {
            return;
        }

        let snapshot = self.buffer.snapshot();
        let point = point_for_mouse_x(&snapshot, display_row, event.position.x, window);
        self.selection = select_to_point(&snapshot, &self.selection, point);
        cx.notify();
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
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let row_count = self.row_count() as usize;
        let mode = self.mode;
        let selection = self.selection.clone();

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
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_key_down(cx.listener(Self::key_down))
            .bg(gpui::rgb(0x181818))
            .text_color(gpui::rgb(0xd6d6d6))
            .font_family("Zed Mono")
            .text_size(px(14.))
            .line_height(px(22.))
            .overflow_y_scroll()
            .child(
                uniform_list(
                    "md-editor-display-rows",
                    row_count,
                    cx.processor(move |this, range, _window, _cx| {
                        let snapshot = this.buffer.snapshot();
                        let selection = clip_selection(&snapshot, &selection);
                        let cursor = selection.head();
                        display_rows_in_mode(&snapshot, range, Some(&selection), mode)
                            .into_iter()
                            .map(|display_row| {
                                let is_cursor_row = display_row.row == cursor.row;
                                let mouse_row = display_row.clone();
                                let mouse_move_row = display_row.clone();
                                div()
                                    .id(display_row.row as usize)
                                    .min_h(px(22.))
                                    .flex()
                                    .items_baseline()
                                    .when(is_cursor_row, |this| this.bg(gpui::rgba(0xffffff10)))
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        _cx.listener(move |this, event, window, cx| {
                                            this.mouse_left_down_on_row(
                                                &mouse_row, event, window, cx,
                                            )
                                        }),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        _cx.listener(Self::mouse_left_up),
                                    )
                                    .on_mouse_up_out(
                                        MouseButton::Left,
                                        _cx.listener(Self::mouse_left_up),
                                    )
                                    .on_mouse_move(_cx.listener(move |this, event, window, cx| {
                                        this.mouse_move_on_row(&mouse_move_row, event, window, cx)
                                    }))
                                    .child(
                                        div()
                                            .w(px(48.))
                                            .pr_2()
                                            .text_align(TextAlign::Right)
                                            .text_color(if is_cursor_row {
                                                gpui::rgba(0xffffffcc)
                                            } else {
                                                gpui::rgba(0xffffff66)
                                            })
                                            .child(SharedString::from(
                                                (display_row.row + 1).to_string(),
                                            )),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .items_baseline()
                                            .whitespace_nowrap()
                                            .children(render_row_text(
                                                &snapshot,
                                                &display_row,
                                                &selection,
                                                mode,
                                            )),
                                    )
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .h_full(),
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
                &source_text[(cursor - visible_source_range.start)..(start - visible_source_range.start)],
            );
        }
        cursor = cursor.max(end);
    }

    if cursor < visible_source_range.end {
        rendered_text.push_str(
            &source_text[(cursor - visible_source_range.start)..(visible_source_range.end - visible_source_range.start)],
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

fn point_for_mouse_x(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    x: gpui::Pixels,
    window: &mut Window,
) -> Point {
    let text_x = (x - px(48.)).max(px(0.));
    let shaped_line = window.text_system().shape_line(
        SharedString::from(display_row.text.clone()),
        px(14.),
        &[TextRun {
            len: display_row.text.len(),
            font: font("Zed Mono"),
            color: gpui::rgb(0xd6d6d6).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    let display_offset = shaped_line.closest_index_for_x(text_x);
    let source_offset = display_row.projection.display_to_source(display_offset);
    let source_offset = snapshot
        .as_text_snapshot()
        .as_rope()
        .floor_char_boundary(source_offset);

    clip_cursor(snapshot, snapshot.as_text_snapshot().offset_to_point(source_offset))
}

fn render_row_text(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    selection: &Selection<Point>,
    mode: MarkdownEditorMode,
) -> Vec<gpui::AnyElement> {
    let segments = styled_display_segments(snapshot, display_row, mode);
    let selection = selection.clone();

    if selection.is_empty() {
        let cursor = selection.head();
        let caret_column = if display_row.row == cursor.row {
            let cursor_offset = snapshot
                .as_text_snapshot()
                .point_to_offset(clip_cursor(snapshot, cursor));
            Some(display_row.projection.source_to_display(cursor_offset))
        } else {
            None
        };
        return render_styled_segments(segments, None, caret_column);
    }

    let Some(selected_range) = selected_range_for_row(snapshot, display_row, &selection) else {
        return render_styled_segments(segments, None, None);
    };
    if selected_range.is_empty() {
        return render_styled_segments(segments, None, None);
    }

    render_styled_segments(segments, Some(selected_range), None)
}

fn render_styled_segments(
    segments: Vec<StyledDisplaySegment>,
    selected_range: Option<Range<usize>>,
    caret_column: Option<usize>,
) -> Vec<gpui::AnyElement> {
    let mut elements = Vec::new();
    let mut caret_inserted = false;
    let caret_column = caret_column.unwrap_or(usize::MAX);

    for segment in segments {
        let mut split_points = Vec::new();
        if let Some(selected_range) = selected_range.as_ref() {
            if selected_range.start > segment.display_range.start
                && selected_range.start < segment.display_range.end
            {
                split_points.push(selected_range.start);
            }
            if selected_range.end > segment.display_range.start
                && selected_range.end < segment.display_range.end
            {
                split_points.push(selected_range.end);
            }
        }
        if caret_column > segment.display_range.start && caret_column < segment.display_range.end {
            split_points.push(caret_column);
        }
        split_points.sort_unstable();
        split_points.dedup();

        let mut piece_start = segment.display_range.start;
        for piece_end in split_points.into_iter().chain(std::iter::once(segment.display_range.end)) {
            if !caret_inserted && caret_column == piece_start {
                elements.push(caret_element());
                caret_inserted = true;
            }

            let local_start = piece_start - segment.display_range.start;
            let local_end = piece_end - segment.display_range.start;
            let piece_text = segment.text[local_start..local_end].to_string();
            let is_selected = selected_range.as_ref().is_some_and(|selected_range| {
                piece_start < selected_range.end && piece_end > selected_range.start
            });
            if !piece_text.is_empty() {
                elements.push(render_text_piece(piece_text, &segment.style, is_selected));
            }
            piece_start = piece_end;
        }
    }

    if !caret_inserted && caret_column != usize::MAX {
        elements.push(caret_element());
    }

    if elements.is_empty() {
        if caret_column != usize::MAX {
            return vec![caret_element()];
        }
        return vec![SharedString::from(String::new()).into_any_element()];
    }

    elements
}

fn caret_element() -> gpui::AnyElement {
    div()
        .w(px(1.))
        .h(px(17.))
        .bg(gpui::rgb(0xf0f0f0))
        .into_any_element()
}

fn render_text_piece(
    text: String,
    style: &DisplayTextStyle,
    is_selected: bool,
) -> gpui::AnyElement {
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
    if is_selected {
        element = element.bg(gpui::rgb(0x264f78)).text_color(gpui::rgb(0xf5fbff));
    }

    element.into_any_element()
}

fn styled_display_segments(
    snapshot: &BufferSnapshot,
    display_row: &DisplayRow,
    mode: MarkdownEditorMode,
) -> Vec<StyledDisplaySegment> {
    let source_range = display_row.projection.visible_source_range();
    let source_text = row_text(snapshot, display_row.row);
    if source_text.is_empty() {
        return vec![StyledDisplaySegment {
            display_range: 0..0,
            text: String::new(),
            style: DisplayTextStyle::default(),
        }];
    }

    let style_ranges = markdown_style_ranges_for_row(snapshot, source_range.clone(), mode);
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
    breakpoints.sort_unstable();
    breakpoints.dedup();

    let mut segments: Vec<StyledDisplaySegment> = Vec::new();
    for window in breakpoints.windows(2) {
        let interval = window[0]..window[1];
        if interval.start >= interval.end || hidden_ranges.iter().any(|hidden_range| range_contains(hidden_range, &interval)) {
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

        if let Some(previous) = segments.last_mut()
            && previous.style == style
            && previous.display_range.end == display_range.start
        {
            previous.text.push_str(&text);
            previous.display_range.end = display_range.end;
        } else {
            segments.push(StyledDisplaySegment {
                display_range,
                text,
                style,
            });
        }
    }

    if segments.is_empty() {
        vec![StyledDisplaySegment {
            display_range: 0..display_row.text.len(),
            text: display_row.text.clone(),
            style: DisplayTextStyle::default(),
        }]
    } else {
        segments
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
    for block in snapshot.syntax_tree().blocks_in_source_range(row_source_range.clone()) {
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
fn ranges_overlap(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn heading_style(level: u8) -> DisplayTextStyle {
    DisplayTextStyle {
        color: Some(match level {
            1 | 2 => gpui::rgb(0xf0f0f0).into(),
            3 => gpui::rgb(0x8fdcff).into(),
            _ => gpui::rgba(0xffffffb3).into(),
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

fn inline_style(kind: MarkdownInlineKind) -> DisplayTextStyle {
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
            color: Some(gpui::rgb(0x8fdcff).into()),
            text_background: Some(gpui::rgba(0xffffff14).into()),
            font_weight: Some(FontWeight::MEDIUM),
            ..Default::default()
        },
        MarkdownInlineKind::Link | MarkdownInlineKind::Image => DisplayTextStyle {
            color: Some(gpui::rgb(0x7cc7ff).into()),
            underline: true,
            ..Default::default()
        },
        MarkdownInlineKind::Strikethrough => DisplayTextStyle {
            line_through: true,
            color: Some(gpui::rgba(0xffffffb3).into()),
            ..Default::default()
        },
        MarkdownInlineKind::InlineMath => DisplayTextStyle {
            color: Some(gpui::rgb(0xd2b6ff).into()),
            italic: true,
            ..Default::default()
        },
    }
}

fn fenced_code_style() -> DisplayTextStyle {
    DisplayTextStyle {
        text_background: Some(gpui::rgba(0xffffff10).into()),
        ..Default::default()
    }
}

fn pipe_table_style() -> DisplayTextStyle {
    DisplayTextStyle {
        color: Some(gpui::rgba(0xffffff99).into()),
        text_background: Some(gpui::rgba(0xffffff0a).into()),
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
            (Some(current), Some(overlay)) => Some(if current >= overlay { current } else { overlay }),
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

        let segments = styled_display_segments(&snapshot, &row, MarkdownEditorMode::Rendered);

        assert_eq!(segments.len(), 1);
        assert_eq!(segments[0].text, "Title");
        assert_eq!(segments[0].style.font_weight, Some(FontWeight::BLACK));
        assert_eq!(segments[0].style.color, Some(gpui::rgb(0xf0f0f0).into()));
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

        let segments = styled_display_segments(&snapshot, &row, MarkdownEditorMode::Rendered);

        assert_eq!(
            segments
                .iter()
                .map(|segment| segment.text.as_str())
                .collect::<Vec<_>>(),
            vec!["Before ", "bold", " and ", "code"]
        );
        assert_eq!(segments[1].style.font_weight, Some(FontWeight::BOLD));
        assert!(segments[3].style.text_background.is_some());
        assert_eq!(segments[3].style.color, Some(gpui::rgb(0x8fdcff).into()));
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
}
