use gpui::{Context, Window};

use md_buffer::BufferSnapshot;
use md_text::{BufferSnapshot as TextBufferSnapshot, Point, Selection, SelectionGoal};
use md_theme::default_row_metrics;

use crate::{
    DisplayRowLayout, MarkdownEditor, MarkdownEditorMode, MoveDown, MoveLeft, MoveRight,
    MoveToBeginningOfLine, MoveToEndOfLine, MoveUp, SelectDown, SelectLeft, SelectRight,
    SelectToBeginningOfLine, SelectToEndOfLine, SelectUp, clip_cursor, clip_selection,
    layout::{VisualDisplayRow, row_display_style_for_display_row, text_wrap_width_for_mode},
    rendered_projection_state,
    selection::{
        HorizontalDirection, clip_cursor_in_text_snapshot, clip_selection_in_text_snapshot,
        collapsed_selection, move_horizontal_in_mode, move_left_in_text_snapshot,
        move_right_in_text_snapshot, move_selection_left_in_mode,
        move_selection_left_in_text_snapshot, move_selection_right_in_mode,
        move_selection_right_in_text_snapshot, move_selection_to_beginning_of_line,
        move_selection_to_beginning_of_line_in_text_snapshot, move_selection_to_end_of_line,
        move_selection_to_end_of_line_in_text_snapshot, move_selection_vertical,
        move_selection_vertical_in_text_snapshot, select_left_in_mode,
        select_left_in_text_snapshot, select_right_in_mode, select_right_in_text_snapshot,
        select_to_beginning_of_line, select_to_beginning_of_line_in_text_snapshot,
        select_to_end_of_line, select_to_end_of_line_in_text_snapshot, select_vertical,
        select_vertical_in_text_snapshot,
    },
    visual_row::{
        VisualLineBoundary, desired_visual_x, display_x_for_offset, point_for_display_offset,
        point_for_display_offset_in_text_snapshot, point_for_visual_row_x,
        point_for_visual_row_x_in_text_snapshot, visual_horizontal_goal,
        visual_line_boundary_for_caret, visual_row_index_for_caret,
    },
};
use md_projection::{RenderedCaretAffinity, RenderedTopology};

impl MarkdownEditor {
    pub(crate) fn normalize_rendered_caret(
        &mut self,
        cursor: Point,
        affinity: RenderedCaretAffinity,
    ) -> Point {
        if self.mode != MarkdownEditorMode::Rendered {
            return cursor;
        }

        let snapshot = self.buffer.snapshot();
        let index = self.rendered_display_index(&snapshot);
        RenderedTopology::new(&snapshot, index).normalize_caret(cursor, affinity)
    }

    fn normalize_rendered_selection_head(
        &mut self,
        snapshot: &BufferSnapshot,
        mut selection: Selection<Point>,
        affinity: RenderedCaretAffinity,
    ) -> Selection<Point> {
        if self.mode != MarkdownEditorMode::Rendered {
            return selection;
        }

        let index = self.rendered_display_index(snapshot);
        let head =
            RenderedTopology::new(snapshot, index).normalize_caret(selection.head(), affinity);
        if selection.is_empty() {
            selection.collapse_to(head, selection.goal);
        } else {
            selection.set_head(head, selection.goal);
        }
        selection
    }

    pub fn move_left(&mut self, _: &MoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection =
            self.move_selection_visual_horizontal(window, cx, HorizontalDirection::Left, false);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn move_right(&mut self, _: &MoveRight, window: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection =
            self.move_selection_visual_horizontal(window, cx, HorizontalDirection::Right, false);
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_selection = self.selection.clone();
        self.selection =
            self.move_selection_visual_line_boundary(window, cx, VisualLineBoundary::Start, false);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn move_to_end_of_line(
        &mut self,
        _: &MoveToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_selection = self.selection.clone();
        self.selection =
            self.move_selection_visual_line_boundary(window, cx, VisualLineBoundary::End, false);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection =
            self.move_selection_visual_horizontal(window, cx, HorizontalDirection::Left, true);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        let previous_selection = self.selection.clone();
        self.selection =
            self.move_selection_visual_horizontal(window, cx, HorizontalDirection::Right, true);
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

    fn move_selection_visual_horizontal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        direction: HorizontalDirection,
        extend_selection: bool,
    ) -> Selection<Point> {
        if self.mode == MarkdownEditorMode::Source {
            return self.move_source_selection_visual_horizontal(
                window,
                cx,
                direction,
                extend_selection,
            );
        }

        let snapshot = self.buffer.snapshot();
        let selection = clip_selection(&snapshot, &self.selection);
        let fallback = if extend_selection {
            match direction {
                HorizontalDirection::Left => select_left_in_mode(&snapshot, &selection, self.mode),
                HorizontalDirection::Right => {
                    select_right_in_mode(&snapshot, &selection, self.mode)
                }
            }
        } else {
            match direction {
                HorizontalDirection::Left => {
                    move_selection_left_in_mode(&snapshot, &selection, self.mode)
                }
                HorizontalDirection::Right => {
                    move_selection_right_in_mode(&snapshot, &selection, self.mode)
                }
            }
        };
        if !extend_selection && !selection.is_empty() {
            return self.normalize_rendered_selection_head(
                &snapshot,
                fallback,
                rendered_caret_affinity_for_horizontal_direction(direction),
            );
        }

        let Some((target, goal)) =
            self.visual_horizontal_target_point(&snapshot, &selection, direction, window, cx)
        else {
            return self.normalize_rendered_selection_head(
                &snapshot,
                fallback,
                rendered_caret_affinity_for_horizontal_direction(direction),
            );
        };

        let updated = if extend_selection {
            let mut updated = selection.clone();
            updated.set_head(target, goal);
            updated
        } else {
            let mut updated = selection.clone();
            updated.collapse_to(target, goal);
            updated
        };
        self.normalize_rendered_selection_head(
            &snapshot,
            updated,
            rendered_caret_affinity_for_horizontal_direction(direction),
        )
    }

    fn move_source_selection_visual_horizontal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        direction: HorizontalDirection,
        extend_selection: bool,
    ) -> Selection<Point> {
        let snapshot = self.buffer.text_snapshot();
        let selection = clip_selection_in_text_snapshot(&snapshot, &self.selection);
        let fallback = if extend_selection {
            match direction {
                HorizontalDirection::Left => select_left_in_text_snapshot(&snapshot, &selection),
                HorizontalDirection::Right => select_right_in_text_snapshot(&snapshot, &selection),
            }
        } else {
            match direction {
                HorizontalDirection::Left => {
                    move_selection_left_in_text_snapshot(&snapshot, &selection)
                }
                HorizontalDirection::Right => {
                    move_selection_right_in_text_snapshot(&snapshot, &selection)
                }
            }
        };
        if !extend_selection && !selection.is_empty() {
            return fallback;
        }

        let Some((target, goal)) = self
            .source_visual_horizontal_target_point(&snapshot, &selection, direction, window, cx)
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

    fn move_selection_visual_vertical(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        delta_visual_rows: i32,
        extend_selection: bool,
    ) -> Selection<Point> {
        if self.mode == MarkdownEditorMode::Source {
            return self.move_source_selection_visual_vertical(
                window,
                cx,
                delta_visual_rows,
                extend_selection,
            );
        }

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
            return self.normalize_rendered_selection_head(
                &snapshot,
                fallback,
                rendered_caret_affinity_for_vertical_delta(delta_visual_rows),
            );
        };

        let updated = if extend_selection {
            let mut updated = selection.clone();
            updated.set_head(target, goal);
            updated
        } else {
            let mut updated = selection.clone();
            updated.collapse_to(target, goal);
            updated
        };
        self.normalize_rendered_selection_head(
            &snapshot,
            updated,
            rendered_caret_affinity_for_vertical_delta(delta_visual_rows),
        )
    }

    fn move_selection_visual_line_boundary(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        boundary: VisualLineBoundary,
        extend_selection: bool,
    ) -> Selection<Point> {
        if self.mode == MarkdownEditorMode::Source {
            return self.move_source_selection_visual_line_boundary(
                window,
                cx,
                boundary,
                extend_selection,
            );
        }

        let snapshot = self.buffer.snapshot();
        let selection = clip_selection(&snapshot, &self.selection);
        let fallback = if extend_selection {
            match boundary {
                VisualLineBoundary::Start => select_to_beginning_of_line(&snapshot, &selection),
                VisualLineBoundary::End => select_to_end_of_line(&snapshot, &selection),
            }
        } else {
            match boundary {
                VisualLineBoundary::Start => {
                    move_selection_to_beginning_of_line(&snapshot, &selection)
                }
                VisualLineBoundary::End => move_selection_to_end_of_line(&snapshot, &selection),
            }
        };

        let Some((target, goal)) =
            self.visual_line_boundary_target_point(&snapshot, &selection, boundary, window, cx)
        else {
            return fallback;
        };

        let updated = if extend_selection {
            let mut updated = selection.clone();
            updated.set_head(target, goal);
            updated
        } else {
            let mut updated = selection.clone();
            updated.collapse_to(target, goal);
            updated
        };
        self.normalize_rendered_selection_head(
            &snapshot,
            updated,
            rendered_caret_affinity_for_line_boundary(boundary),
        )
    }

    fn move_source_selection_visual_vertical(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        delta_visual_rows: i32,
        extend_selection: bool,
    ) -> Selection<Point> {
        let snapshot = self.buffer.text_snapshot();
        let selection = clip_selection_in_text_snapshot(&snapshot, &self.selection);
        let fallback = if extend_selection {
            select_vertical_in_text_snapshot(&snapshot, &selection, delta_visual_rows)
        } else {
            move_selection_vertical_in_text_snapshot(&snapshot, &selection, delta_visual_rows)
        };

        let Some((target, goal)) = self.source_visual_vertical_target_point(
            &snapshot,
            &selection,
            delta_visual_rows,
            window,
            cx,
        ) else {
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

    fn move_source_selection_visual_line_boundary(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        boundary: VisualLineBoundary,
        extend_selection: bool,
    ) -> Selection<Point> {
        let snapshot = self.buffer.text_snapshot();
        let selection = clip_selection_in_text_snapshot(&snapshot, &self.selection);
        let fallback = if extend_selection {
            match boundary {
                VisualLineBoundary::Start => {
                    select_to_beginning_of_line_in_text_snapshot(&snapshot, &selection)
                }
                VisualLineBoundary::End => {
                    select_to_end_of_line_in_text_snapshot(&snapshot, &selection)
                }
            }
        } else {
            match boundary {
                VisualLineBoundary::Start => {
                    move_selection_to_beginning_of_line_in_text_snapshot(&snapshot, &selection)
                }
                VisualLineBoundary::End => {
                    move_selection_to_end_of_line_in_text_snapshot(&snapshot, &selection)
                }
            }
        };

        let Some((target, goal)) = self
            .source_visual_line_boundary_target_point(&snapshot, &selection, boundary, window, cx)
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

    fn source_visual_horizontal_target_point(
        &mut self,
        snapshot: &TextBufferSnapshot,
        selection: &Selection<Point>,
        direction: HorizontalDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Point, SelectionGoal)> {
        let cursor = clip_cursor_in_text_snapshot(snapshot, selection.head());
        let target = match direction {
            HorizontalDirection::Left => move_left_in_text_snapshot(snapshot, cursor),
            HorizontalDirection::Right => move_right_in_text_snapshot(snapshot, cursor),
        };
        let display_row = self.cached_source_display_row(snapshot, target.row as usize)?;
        let row_style = default_row_metrics().into();
        let wrap_width = text_wrap_width_for_mode(window, self.mode);
        let text_layout =
            self.cached_source_text_layout(&display_row, row_style, wrap_width, false, window, cx);
        let source_offset = snapshot.point_to_offset(target);
        let display_offset = display_row
            .source_to_display(source_offset)
            .min(text_layout.text_len);
        let visual_row_index = visual_row_index_for_horizontal_movement(
            &text_layout.visual_rows,
            &display_row.text,
            display_offset,
            text_layout.text_len,
            direction,
        )?;
        let visual_row = &text_layout.visual_rows[visual_row_index];
        let target_x = display_x_for_offset(
            &text_layout.fragments,
            &text_layout.shaped_line,
            display_offset,
        ) - visual_row.line_start_x;

        Some((target, visual_horizontal_goal(visual_row_index, target_x)))
    }

    fn source_visual_line_boundary_target_point(
        &mut self,
        snapshot: &TextBufferSnapshot,
        selection: &Selection<Point>,
        boundary: VisualLineBoundary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Point, SelectionGoal)> {
        let cursor = clip_cursor_in_text_snapshot(snapshot, selection.head());
        let display_row = self.cached_source_display_row(snapshot, cursor.row as usize)?;
        let row_style = default_row_metrics().into();
        let wrap_width = text_wrap_width_for_mode(window, self.mode);
        let text_layout =
            self.cached_source_text_layout(&display_row, row_style, wrap_width, false, window, cx);

        let source_offset = snapshot.point_to_offset(cursor);
        let display_offset = display_row
            .source_to_display(source_offset)
            .min(text_layout.text_len);
        let (visual_row_index, target_display_offset) = visual_line_boundary_for_caret(
            &text_layout.visual_rows,
            display_offset,
            text_layout.text_len,
            selection.goal,
            boundary,
        )?;
        let visual_row = &text_layout.visual_rows[visual_row_index];
        let point = point_for_display_offset_in_text_snapshot(
            snapshot,
            &display_row,
            &text_layout,
            target_display_offset,
        );
        let target_x = display_x_for_offset(
            &text_layout.fragments,
            &text_layout.shaped_line,
            target_display_offset,
        ) - visual_row.line_start_x;
        Some((point, visual_horizontal_goal(visual_row_index, target_x)))
    }

    fn source_visual_vertical_target_point(
        &mut self,
        snapshot: &TextBufferSnapshot,
        selection: &Selection<Point>,
        delta_visual_rows: i32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Point, SelectionGoal)> {
        if delta_visual_rows == 0 {
            return Some((selection.head(), selection.goal));
        }

        let cursor = clip_cursor_in_text_snapshot(snapshot, selection.head());
        let display_row = self.cached_source_display_row(snapshot, cursor.row as usize)?;
        let row_style = default_row_metrics().into();
        let wrap_width = text_wrap_width_for_mode(window, self.mode);
        let current_layout =
            self.cached_source_text_layout(&display_row, row_style, wrap_width, false, window, cx);

        let source_offset = snapshot.point_to_offset(cursor);
        let display_offset = display_row
            .source_to_display(source_offset)
            .min(current_layout.text_len);
        let visual_row_index = visual_row_index_for_caret(
            &current_layout.visual_rows,
            display_offset,
            current_layout.text_len,
            selection.goal,
        )?;
        let visual_row = &current_layout.visual_rows[visual_row_index];
        let cursor_x = display_x_for_offset(
            &current_layout.fragments,
            &current_layout.shaped_line,
            display_offset,
        ) - visual_row.line_start_x;
        let desired_x = desired_visual_x(selection.goal, cursor_x);

        let target_visual_row_index = visual_row_index as i32 + delta_visual_rows;
        if target_visual_row_index >= 0
            && (target_visual_row_index as usize) < current_layout.visual_rows.len()
        {
            let target_visual_row_index = target_visual_row_index as usize;
            return point_for_visual_row_x_in_text_snapshot(
                snapshot,
                &display_row,
                &current_layout,
                &current_layout.visual_rows[target_visual_row_index],
                desired_x,
            )
            .map(|point| {
                (
                    point,
                    visual_horizontal_goal(target_visual_row_index, desired_x),
                )
            });
        }

        let target_row = if delta_visual_rows.is_negative() {
            display_row.row.checked_sub(1)?
        } else {
            let next_row = display_row.row.saturating_add(1);
            if next_row >= snapshot.row_count() {
                return None;
            }
            next_row
        };
        let target_display_row = self.cached_source_display_row(snapshot, target_row as usize)?;
        let target_layout = self.cached_source_text_layout(
            &target_display_row,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );
        let target_visual_row = if delta_visual_rows.is_negative() {
            target_layout.visual_rows.last()?
        } else {
            target_layout.visual_rows.first()?
        };
        let target_visual_row_index = if delta_visual_rows.is_negative() {
            target_layout.visual_rows.len().saturating_sub(1)
        } else {
            0
        };
        point_for_visual_row_x_in_text_snapshot(
            snapshot,
            &target_display_row,
            &target_layout,
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

    fn visual_horizontal_target_point(
        &mut self,
        snapshot: &BufferSnapshot,
        selection: &Selection<Point>,
        direction: HorizontalDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Point, SelectionGoal)> {
        let cursor = clip_cursor(snapshot, selection.head());
        let target = move_horizontal_in_mode(snapshot, cursor, self.mode, direction);
        let source_offset = snapshot.as_text_snapshot().point_to_offset(target);
        let target_selection = collapsed_selection(target);
        let display_row_state =
            rendered_projection_state(snapshot, Some(&target_selection), self.mode);
        let item_index = self.display_item_index_for_cursor(snapshot, target, self.mode)?;
        let display_row =
            self.cached_display_row(snapshot, item_index, self.mode, &display_row_state)?;
        let row_style = row_display_style_for_display_row(snapshot, &display_row, self.mode);
        let wrap_width = text_wrap_width_for_mode(window, self.mode);
        let layout = self.cached_row_layout(
            snapshot,
            &display_row,
            &target_selection,
            self.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );

        match layout {
            DisplayRowLayout::Text(text_layout) => {
                let display_offset = display_row
                    .source_to_display(source_offset)
                    .min(text_layout.text_len);
                let visual_row_index = visual_row_index_for_horizontal_movement(
                    &text_layout.visual_rows,
                    &display_row.text,
                    display_offset,
                    text_layout.text_len,
                    direction,
                )?;
                let visual_row = &text_layout.visual_rows[visual_row_index];
                let target_x = display_x_for_offset(
                    &text_layout.fragments,
                    &text_layout.shaped_line,
                    display_offset,
                ) - visual_row.line_start_x;
                Some((target, visual_horizontal_goal(visual_row_index, target_x)))
            }
            DisplayRowLayout::Block(block_layout) => Some((
                target,
                visual_horizontal_goal(0, block_layout.visible_x_for_source_offset(source_offset)),
            )),
            DisplayRowLayout::TableRow(table_layout) => Some((
                target,
                visual_horizontal_goal(0, table_layout.visible_x_for_source_offset(source_offset)),
            )),
        }
    }

    fn visual_line_boundary_target_point(
        &mut self,
        snapshot: &BufferSnapshot,
        selection: &Selection<Point>,
        boundary: VisualLineBoundary,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<(Point, SelectionGoal)> {
        let cursor = clip_cursor(snapshot, selection.head());
        let display_row_state = rendered_projection_state(snapshot, Some(selection), self.mode);
        let item_index = self.display_item_index_for_cursor(snapshot, cursor, self.mode)?;
        let display_row =
            self.cached_display_row(snapshot, item_index, self.mode, &display_row_state)?;
        let row_style = row_display_style_for_display_row(snapshot, &display_row, self.mode);
        let wrap_width = text_wrap_width_for_mode(window, self.mode);
        let layout = self.cached_row_layout(
            snapshot,
            &display_row,
            selection,
            self.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );

        match layout {
            DisplayRowLayout::Text(text_layout) => {
                let source_offset = snapshot.as_text_snapshot().point_to_offset(cursor);
                let display_offset = display_row
                    .source_to_display(source_offset)
                    .min(text_layout.text_len);
                let (visual_row_index, target_display_offset) = visual_line_boundary_for_caret(
                    &text_layout.visual_rows,
                    display_offset,
                    text_layout.text_len,
                    selection.goal,
                    boundary,
                )?;
                let visual_row = &text_layout.visual_rows[visual_row_index];
                let point = point_for_display_offset(
                    snapshot,
                    &display_row,
                    &text_layout,
                    target_display_offset,
                );
                let target_x = display_x_for_offset(
                    &text_layout.fragments,
                    &text_layout.shaped_line,
                    target_display_offset,
                ) - visual_row.line_start_x;
                Some((point, visual_horizontal_goal(visual_row_index, target_x)))
            }
            DisplayRowLayout::Block(block_layout) => {
                Some(block_layout.line_boundary_target(snapshot, boundary))
            }
            DisplayRowLayout::TableRow(table_layout) => {
                Some(table_layout.line_boundary_target(snapshot, boundary))
            }
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
        let display_row_state = rendered_projection_state(snapshot, Some(selection), self.mode);
        let item_index = self.display_item_index_for_cursor(snapshot, cursor, self.mode)?;
        let display_row =
            self.cached_display_row(snapshot, item_index, self.mode, &display_row_state)?;
        let row_style = row_display_style_for_display_row(snapshot, &display_row, self.mode);
        let wrap_width = text_wrap_width_for_mode(window, self.mode);
        let current_layout = self.cached_row_layout(
            snapshot,
            &display_row,
            selection,
            self.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );

        let source_offset = snapshot.as_text_snapshot().point_to_offset(cursor);
        let desired_x = match current_layout {
            DisplayRowLayout::Text(text_layout) => {
                let display_offset = display_row
                    .source_to_display(source_offset)
                    .min(text_layout.text_len);
                let visual_row_index = visual_row_index_for_caret(
                    &text_layout.visual_rows,
                    display_offset,
                    text_layout.text_len,
                    selection.goal,
                )?;
                let visual_row = &text_layout.visual_rows[visual_row_index];
                let cursor_x = display_x_for_offset(
                    &text_layout.fragments,
                    &text_layout.shaped_line,
                    display_offset,
                ) - visual_row.line_start_x;
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
            DisplayRowLayout::Block(block_layout) => desired_visual_x(
                selection.goal,
                block_layout.visible_x_for_source_offset(source_offset),
            ),
            DisplayRowLayout::TableRow(table_layout) => desired_visual_x(
                selection.goal,
                table_layout.visible_x_for_source_offset(source_offset),
            ),
        };

        let target_item = if delta_visual_rows.is_negative() {
            display_row.item_index.checked_sub(1)?
        } else {
            let next_item = display_row.item_index.saturating_add(1);
            if next_item as usize >= self.display_item_count_for_mode(self.mode) {
                return None;
            }
            next_item
        };
        let target_display_row = self.cached_display_row(
            snapshot,
            target_item as usize,
            self.mode,
            &display_row_state,
        )?;
        let target_row_style =
            row_display_style_for_display_row(snapshot, &target_display_row, self.mode);
        let target_layout = self.cached_row_layout(
            snapshot,
            &target_display_row,
            selection,
            self.mode,
            target_row_style,
            wrap_width,
            false,
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
            DisplayRowLayout::Block(block_layout) => Some((
                block_layout.point_for_x(snapshot, desired_x),
                visual_horizontal_goal(0, desired_x),
            )),
            DisplayRowLayout::TableRow(table_layout) => Some((
                table_layout.point_for_x(snapshot, desired_x),
                visual_horizontal_goal(0, desired_x),
            )),
        }
    }

    pub fn select_to_beginning_of_line(
        &mut self,
        _: &SelectToBeginningOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_selection = self.selection.clone();
        self.selection =
            self.move_selection_visual_line_boundary(window, cx, VisualLineBoundary::Start, true);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub fn select_to_end_of_line(
        &mut self,
        _: &SelectToEndOfLine,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let previous_selection = self.selection.clone();
        self.selection =
            self.move_selection_visual_line_boundary(window, cx, VisualLineBoundary::End, true);
        self.notify_after_selection_change(&previous_selection, cx);
    }
}

fn visual_row_index_for_horizontal_movement(
    visual_rows: &[VisualDisplayRow],
    display_text: &str,
    display_offset: usize,
    text_len: usize,
    direction: HorizontalDirection,
) -> Option<usize> {
    match direction {
        HorizontalDirection::Right
            if !display_text[..display_offset.min(display_text.len())].ends_with('\n') =>
        {
            if let Some(index) = visual_rows.iter().position(|visual_row| {
                visual_row.display_range.end == display_offset
                    && display_offset < text_len
                    && !visual_row.display_range.is_empty()
            }) {
                return Some(index);
            }
        }
        HorizontalDirection::Right | HorizontalDirection::Left => {}
    }

    if direction == HorizontalDirection::Left
        && let Some(index) = visual_rows
            .iter()
            .position(|visual_row| visual_row.display_range.start == display_offset)
    {
        return Some(index);
    }

    visual_row_index_for_caret(visual_rows, display_offset, text_len, SelectionGoal::None)
}

fn rendered_caret_affinity_for_horizontal_direction(
    direction: HorizontalDirection,
) -> RenderedCaretAffinity {
    match direction {
        HorizontalDirection::Left => RenderedCaretAffinity::Before,
        HorizontalDirection::Right => RenderedCaretAffinity::After,
    }
}

fn rendered_caret_affinity_for_vertical_delta(delta_visual_rows: i32) -> RenderedCaretAffinity {
    if delta_visual_rows.is_negative() {
        RenderedCaretAffinity::Before
    } else {
        RenderedCaretAffinity::After
    }
}

fn rendered_caret_affinity_for_line_boundary(
    boundary: VisualLineBoundary,
) -> RenderedCaretAffinity {
    match boundary {
        VisualLineBoundary::Start => RenderedCaretAffinity::Before,
        VisualLineBoundary::End => RenderedCaretAffinity::After,
    }
}
