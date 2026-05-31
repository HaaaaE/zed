use super::*;

impl MarkdownEditor {
    pub(crate) fn mouse_left_down_on_row(
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
                    display_row.content_origin_x(),
                ),
        };
        self.freeze_rendered_drag_projection(&snapshot);
        let previous_selection = self.selection.clone();
        self.selection = if event.modifiers.shift {
            select_to_point_with_goal(&snapshot, &self.selection, point, goal)
        } else {
            collapsed_selection_with_goal(point, goal)
        };
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub(crate) fn mouse_move_on_row(
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
                    display_row.content_origin_x(),
                ),
        };
        let previous_selection = self.selection.clone();
        self.selection = select_to_point_with_goal(&snapshot, &self.selection, point, goal);
        self.notify_after_selection_change(&previous_selection, cx);
    }

    pub(crate) fn mouse_left_down_on_block(
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
        self.freeze_rendered_drag_projection(&snapshot);
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

    pub(crate) fn mouse_move_on_block(
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

    pub(crate) fn mouse_left_down_on_table_row(
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
        self.freeze_rendered_drag_projection(&snapshot);
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

    pub(crate) fn mouse_move_on_table_row(
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

    pub(crate) fn mouse_left_up(
        &mut self,
        _: &MouseUpEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting_with_mouse = false;
        if self.rendered_drag_projection_state.take().is_some() {
            self.clear_display_row_cache();
            self.clear_row_layout_cache();
            cx.notify();
        }
    }
}
