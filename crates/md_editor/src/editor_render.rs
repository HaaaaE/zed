use super::*;

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
                    self.current_rendered_projection_state(&snapshot, &selection);
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
            .on_action(cx.listener(Self::shift_tab))
            .on_action(cx.listener(Self::toggle_bold))
            .on_action(cx.listener(Self::toggle_italic))
            .on_action(cx.listener(Self::toggle_inline_code))
            .on_action(cx.listener(Self::toggle_strikethrough))
            .on_action(cx.listener(Self::insert_link))
            .on_action(cx.listener(Self::edit_link))
            .on_action(cx.listener(Self::toggle_source_reveal_current_block))
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
