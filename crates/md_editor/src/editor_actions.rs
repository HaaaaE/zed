use super::*;

impl MarkdownEditor {
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

    pub(crate) fn key_down(
        &mut self,
        event: &KeyDownEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(crate) fn toggle_task_checkbox_source_range(
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

    pub(crate) fn record_selection_history(
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

    pub(crate) fn notify_after_edit(
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
}
