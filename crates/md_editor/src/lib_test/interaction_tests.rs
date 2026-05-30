use super::test_support::*;

#[gpui::test]
fn source_display_rows_uses_text_snapshot_without_refreshing_markdown_syntax(
    cx: &mut gpui::TestAppContext,
) {
    let editor = cx.update(|cx| cx.new(|cx| MarkdownEditor::for_text("# Heading\nBody\n", cx)));

    editor.update(cx, |editor, _| {
        let _snapshot = editor.buffer.snapshot();
        let cached_syntax_version = editor.buffer.cached_syntax_version_for_tests();
        assert_eq!(
            &cached_syntax_version,
            editor.buffer.as_text_snapshot().version()
        );

        assert!(editor.buffer.edit([(0..0, "Plain text\n")]).is_some());
        let edited_text_version = editor.buffer.as_text_snapshot().version().clone();
        assert_ne!(cached_syntax_version, edited_text_version);

        let display_rows = editor.display_rows(0..2);
        assert_eq!(
            display_rows
                .into_iter()
                .map(|row| row.text)
                .collect::<Vec<_>>(),
            vec!["Plain text".to_string(), "# Heading".to_string()]
        );
        assert_eq!(
            editor.buffer.cached_syntax_version_for_tests(),
            cached_syntax_version
        );
    });
}

#[gpui::test]
fn source_mode_actions_follow_wrapped_visual_rows(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(90.), px(200.)));
    let editor = cx.new(|cx| MarkdownEditor::for_text("abcdefghijklmnopqrst\n", cx));

    editor.update_in(cx, |editor, window, cx| {
        let source_line_end = editor.buffer.as_text_snapshot().line_len(0);
        editor.set_cursor(Point::new(0, 0));

        editor.move_down(&MoveDown, window, cx);
        let wrapped_row_start = editor.cursor();
        assert_eq!(wrapped_row_start.row, 0);
        assert!(wrapped_row_start.column > 0);
        assert!(wrapped_row_start.column < source_line_end);

        editor.move_to_end_of_line(&MoveToEndOfLine, window, cx);
        let wrapped_row_end = editor.cursor();
        assert_eq!(wrapped_row_end.row, 0);
        assert!(wrapped_row_end.column > wrapped_row_start.column);
        assert!(wrapped_row_end.column < source_line_end);

        editor.move_to_beginning_of_line(&MoveToBeginningOfLine, window, cx);
        assert_eq!(editor.cursor(), wrapped_row_start);

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(editor.cursor(), Point::new(0, 0));
    });
}

#[gpui::test]
fn source_interaction_layouts_cache_wrapped_text_rows(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(90.), px(200.)));
    let editor = cx.new(|cx| MarkdownEditor::for_text("abcdefghijklmnopqrst\n", cx));

    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(editor.row_layout_cache.len(), 0);

        editor.set_cursor(Point::new(0, 0));
        editor.move_down(&MoveDown, window, cx);
        assert_eq!(editor.row_layout_cache.len(), 1);

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(editor.row_layout_cache.len(), 1);
    });
}

#[gpui::test]
fn source_layout_input_cache_reuses_width_independent_inputs_without_refreshing_syntax(
    cx: &mut gpui::TestAppContext,
) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| MarkdownEditor::for_text("# Heading and plain wrapped text\n", cx));

    editor.update_in(cx, |editor, window, cx| {
        let _snapshot = editor.buffer.snapshot();
        let cached_syntax_version = editor.buffer.cached_syntax_version_for_tests();
        assert!(editor.buffer.edit([(0..0, "Plain ")]).is_some());
        assert_ne!(
            editor.buffer.as_text_snapshot().version(),
            &cached_syntax_version
        );

        let text_snapshot = editor.buffer.text_snapshot();
        let display_row = editor
            .cached_source_display_row(&text_snapshot, 0)
            .expect("source row should exist");
        let row_style = default_row_metrics().into();
        let _ =
            editor.cached_source_text_layout(&display_row, row_style, px(120.), false, window, cx);
        assert_eq!(editor.row_layout_input_cache.len(), 1);
        assert_eq!(editor.row_layout_cache.len(), 1);

        let _ =
            editor.cached_source_text_layout(&display_row, row_style, px(220.), false, window, cx);
        assert_eq!(editor.row_layout_input_cache.len(), 1);
        assert_eq!(editor.row_layout_cache.len(), 2);
        assert_eq!(
            editor.buffer.cached_syntax_version_for_tests(),
            cached_syntax_version
        );
    });
}

#[gpui::test]
fn source_render_prewarms_display_rows_and_layout_inputs(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let text = (0..200)
        .map(|row| format!("source prewarm row {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    let editor = cx.new(|cx| MarkdownEditor::for_text(text, cx));

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(240.), px(160.)),
        |_, _| editor.clone().into_any_element(),
    );
    editor.update_in(cx, |editor, window, cx| {
        editor.flush_source_cache_prewarm(window, cx);
    });

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
        assert!(
            editor.display_row_cache.len() >= 32,
            "source prewarm should populate display row cache beyond visible rows"
        );
        assert!(
            editor.row_layout_input_cache.len() >= 32,
            "source prewarm should populate layout input cache beyond visible rows"
        );
        assert!(
            editor.row_layout_cache.len() >= 32,
            "source prewarm should populate current-width row layout cache beyond visible rows"
        );
    });
}

#[gpui::test]
fn rendered_render_prewarms_display_rows_and_layout_inputs(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let text = (0..200)
        .map(|row| format!("rendered prewarm row {row}\n\n"))
        .collect::<String>();
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(text, cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(240.), px(160.)),
        |_, _| editor.clone().into_any_element(),
    );
    editor.update_in(cx, |editor, window, cx| {
        editor.flush_rendered_cache_prewarm(window, cx);
    });

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
        assert!(
            editor.display_row_cache.len() >= 16,
            "rendered prewarm should populate display row cache beyond visible rows"
        );
        assert!(
            editor.row_layout_input_cache.len() >= 16,
            "rendered prewarm should populate layout input cache beyond visible rows"
        );
        assert!(
            editor.row_layout_cache.len() >= 16,
            "rendered prewarm should populate current-width row layout cache beyond visible rows"
        );
    });
}

#[gpui::test]
fn source_prewarm_large_file_stays_near_scroll_anchor(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let text = (0..20_000)
        .map(|row| format!("large source prewarm row {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.len() > 300 * 1024);
    let editor = cx.new(|cx| MarkdownEditor::for_text(text, cx));

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(240.), px(160.)),
        |_, _| editor.clone().into_any_element(),
    );

    editor.read_with(cx, |editor, _| {
        let row_count = editor.buffer.as_text_snapshot().row_count() as usize;
        let state = editor
            .source_prewarm
            .as_ref()
            .expect("source prewarm should be scheduled");
        assert_eq!(state.anchor_row, 0);
        assert!(
            state.rows.len() < row_count / 10,
            "large files should not queue whole-document prewarm"
        );
    });
}

#[gpui::test]
fn source_prewarm_reanchors_after_large_scroll_jump(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let text = (0..2_000)
        .map(|row| format!("source prewarm jump row {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    let editor = cx.new(|cx| MarkdownEditor::for_text(text, cx));

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(240.), px(160.)),
        |_, _| editor.clone().into_any_element(),
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.display_list_state.scroll_to(ListOffset {
            item_ix: 200,
            offset_in_item: px(0.),
        });
        editor.schedule_source_cache_prewarm(
            default_row_metrics().min_height,
            default_row_metrics().into(),
            window,
            cx,
        );
    });

    editor.read_with(cx, |editor, _| {
        let state = editor
            .source_prewarm
            .as_ref()
            .expect("source prewarm should be scheduled");
        assert_eq!(state.anchor_row, 200);
        assert_eq!(state.rows.front().copied(), Some(200));
    });
}

#[gpui::test]
fn rendered_prewarm_large_file_stays_near_scroll_anchor(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let text = (0..20_000)
        .map(|row| format!("large rendered prewarm row {row}"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.len() > 300 * 1024);
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(text, cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(240.), px(160.)),
        |_, _| editor.clone().into_any_element(),
    );

    editor.read_with(cx, |editor, _| {
        let row_count = editor.buffer.as_text_snapshot().row_count() as usize;
        let state = editor
            .rendered_prewarm
            .as_ref()
            .expect("rendered prewarm should be scheduled");
        assert_eq!(state.anchor_row, 0);
        assert!(
            state.rows.len() < row_count / 10,
            "large files should not queue whole-document prewarm"
        );
    });
}

#[gpui::test]
fn rendered_prewarm_reanchors_after_large_scroll_jump(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let text = (0..2_000)
        .map(|row| format!("rendered prewarm jump row {row}\n\n"))
        .collect::<String>();
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(text, cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(240.), px(160.)),
        |_, _| editor.clone().into_any_element(),
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.display_list_state.scroll_to(ListOffset {
            item_ix: 20,
            offset_in_item: px(0.),
        });
        editor.schedule_rendered_cache_prewarm(px(240.), editor.selection.clone(), window, cx);
    });

    editor.read_with(cx, |editor, _| {
        let state = editor
            .rendered_prewarm
            .as_ref()
            .expect("rendered prewarm should be scheduled");
        assert_eq!(state.anchor_row, 20);
        assert_eq!(state.rows.front().copied(), Some(20));
    });
}

#[gpui::test]
fn rendered_interaction_layouts_cache_plain_text_rows(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(90.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text("abcdefghijklmnopqrst\nsecond", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        assert_eq!(editor.row_layout_cache.len(), 0);

        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let wrap_width = text_wrap_width_for_mode(window, editor.mode);
        let selection = editor.selection.clone();

        let _ = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );
        assert_eq!(editor.row_layout_cache.len(), 1);

        let _ = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );
        assert_eq!(editor.row_layout_cache.len(), 1);
    });
}

#[gpui::test]
fn rendered_table_rows_use_structured_layout_when_inactive(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("| **a** | [b](url) |\n| :- | -: |\n| 1 | 2 |\nafter\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(3, 0));
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let wrap_width = text_wrap_width_for_mode(window, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );

        let DisplayRowLayout::TableRow(table_layout) = row_layout else {
            panic!("expected structured table row layout");
        };
        assert!(table_layout.is_header);
        assert!(!table_layout.is_delimiter);
        assert_eq!(
            table_layout
                .cells
                .iter()
                .map(|cell| cell.text.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        assert!(
            table_layout.cells[0]
                .segments
                .iter()
                .any(|segment| segment.style.font_weight == Some(FontWeight::BOLD))
        );
        assert!(
            table_layout.cells[1]
                .segments
                .iter()
                .any(|segment| segment.style.underline)
        );
        assert_eq!(
            table_layout.cells[0].alignment,
            MarkdownTableAlignment::Left
        );
        assert_eq!(
            table_layout.cells[1].alignment,
            MarkdownTableAlignment::Right
        );
        assert_eq!(editor.table_layout_cache.len(), 1);

        let body_row = editor
            .cached_display_row(&snapshot, 2, editor.mode, &display_row_state)
            .expect("body row should exist");
        let body_style = row_display_style_for_display_row(&snapshot, &body_row, editor.mode);
        let body_layout = editor.cached_row_layout(
            &snapshot,
            &body_row,
            &selection,
            editor.mode,
            body_style,
            wrap_width,
            false,
            window,
            cx,
        );
        assert!(matches!(body_layout, DisplayRowLayout::TableRow(_)));
        assert_eq!(
            editor.table_layout_cache.len(),
            1,
            "table metrics should be reused across visible rows"
        );
    });
}

#[gpui::test]
fn rendered_table_cells_wrap_to_available_width(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(120.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("| longwordlongwordlongword |\n| - |\nafter\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(2, 0));
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let wrap_width = text_wrap_width_for_mode(window, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );

        let DisplayRowLayout::TableRow(table_layout) = row_layout else {
            panic!("expected structured table row layout");
        };
        assert!(table_layout.width <= wrap_width);
        assert!(table_layout.cells[0].visual_lines.len() > 1);
        assert!(table_layout.height() > row_style.line_height);
    });
}

#[gpui::test]
fn rendered_table_header_cells_fit_preferred_width(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(
            "| Column A | Column B |\n|----------|----------|\n| Cell 1   | Cell 2   |\n",
            cx,
        );
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(2, 0));
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            text_wrap_width(window),
            false,
            window,
            cx,
        );

        let DisplayRowLayout::TableRow(table_layout) = row_layout else {
            panic!("expected structured table row layout");
        };
        assert_eq!(table_layout.cells[0].text, "Column A");
        assert_eq!(table_layout.cells[1].text, "Column B");
        assert_eq!(
            table_layout.cells[0].visual_lines,
            vec![0.."Column A".len()]
        );
        assert_eq!(
            table_layout.cells[1].visual_lines,
            vec![0.."Column B".len()]
        );
    });
}

#[gpui::test]
fn rendered_table_shrinks_long_columns_before_short_columns(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(620.), px(300.)));
    let source = format!(
        "| Name | Age | City |\n| --- | --- | --- |\n| Alice | 30 | {} |\n",
        "Cit".repeat(36)
    );
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(&source, cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(3, 0));
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 2, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let wrap_width = text_wrap_width_for_mode(window, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );

        let DisplayRowLayout::TableRow(table_layout) = row_layout else {
            panic!("expected structured table row layout");
        };
        assert!(table_layout.width <= wrap_width);
        assert_eq!(table_layout.cells[0].text, "Alice");
        assert_eq!(table_layout.cells[1].text, "30");
        assert_eq!(table_layout.cells[0].visual_lines, vec![0.."Alice".len()]);
        assert_eq!(table_layout.cells[1].visual_lines, vec![0.."30".len()]);
        assert!(
            table_layout.cells[2].visual_lines.len() > 1,
            "long city column should absorb wrapping, got {:?}",
            table_layout.cells[2]
        );
    });
}

#[gpui::test]
fn rendered_table_delimiter_row_uses_structured_separator_layout(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("| Name | Age |\n| --- | --- |\n| Alice | 30 |\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(2, 0));
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 1, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            text_wrap_width(window),
            false,
            window,
            cx,
        );

        let DisplayRowLayout::TableRow(table_layout) = &row_layout else {
            panic!("expected structured table row layout");
        };
        assert!(table_layout.is_delimiter);
        assert!(table_layout.height() <= row_style.line_height);
    });
}

#[gpui::test]
fn rendered_table_wrapping_uses_text_measurement_for_words(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(720.), px(300.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(
            "| Column A | Column B Column B Column B Column B Column B Column B |\n| - | - |\nafter\n",
            cx,
        );
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(2, 0));
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            text_wrap_width(window),
            false,
            window,
            cx,
        );

        let DisplayRowLayout::TableRow(table_layout) = row_layout else {
            panic!("expected structured table row layout");
        };
        assert!(
            table_layout.cells[1].visual_lines.len() <= 3,
            "word wrapping should pack multiple words per visual line, got {:?}",
            table_layout.cells[1]
        );
    });
}

#[gpui::test]
fn rendered_table_mouse_target_maps_to_cell_source(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text("| a | b |\n| - | - |\nafter\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(2, 0));
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            text_wrap_width(window),
            false,
            window,
            cx,
        );
        let DisplayRowLayout::TableRow(table_layout) = row_layout else {
            panic!("expected structured table row layout");
        };

        let second_cell_x = gutter_width() + table_layout.cells[1].x + px(10.);
        let (point, _) = table_layout.mouse_target_for_x(&snapshot, second_cell_x);
        assert_eq!(point.row, 0);
        assert!(
            point.column >= 6 && point.column <= 7,
            "expected target near second cell content, got {point:?}"
        );
    });
}

#[gpui::test]
fn rendered_table_mouse_target_accounts_for_rendered_indent(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("> | a | b |\n> | - | - |\n> | 1 | 2 |\nafter\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(3, 0));
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
            .expect("display row should exist");
        assert_eq!(display_row.rendered_indent_width(), px(24.));

        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            text_wrap_width(window),
            false,
            window,
            cx,
        );
        let DisplayRowLayout::TableRow(table_layout) = row_layout else {
            panic!("expected structured table row layout");
        };

        let second_cell_x = left_rail_width(MarkdownEditorMode::Rendered)
            + display_row.rendered_indent_width()
            + table_layout.cells[1].x
            + px(10.);
        let (point, _) = table_layout.mouse_target_for_x_with_indent(
            &snapshot,
            second_cell_x,
            display_row.rendered_indent_width(),
        );
        let source_offset = snapshot.as_text_snapshot().point_to_offset(point);
        assert!(
            (table_layout.cells[1].content_range.start..=table_layout.cells[1].content_range.end)
                .contains(&source_offset),
            "expected target inside indented second cell, got {point:?}"
        );
    });
}

#[gpui::test]
fn rendered_table_vertical_movement_enters_neighboring_table_rows(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("| alpha | beta |\n| - | - |\n| one | two |\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(0, 4));
        editor.move_down(&MoveDown, window, cx);
        assert_eq!(editor.cursor().row, 1);

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(editor.cursor().row, 2);
    });
}

#[gpui::test]
fn rendered_table_click_reveals_only_clicked_source_row(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("| **a** | b |\n| - | - |\n| 1 | 2 |\nafter\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(3, 0));
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(320.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    let click = gpui::point(
        gutter_width() + px(18.),
        default_row_metrics().line_height * 0.5,
    );
    cx.simulate_mouse_down(click, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(click, MouseButton::Left, gpui::Modifiers::none());

    editor.update(cx, |editor, _| {
        assert_eq!(editor.cursor().row, 0);
        let snapshot = editor.buffer.snapshot();
        let rows = display_rows_in_mode(
            &snapshot,
            0..3,
            Some(&editor.selection),
            MarkdownEditorMode::Rendered,
        );

        assert_eq!(rows[0].text, "| **a** | b |");
        assert_ne!(rows[1].text, "| - | - |");
        assert_ne!(rows[2].text, "| 1 | 2 |");
    });
}

#[gpui::test]
fn rendered_table_shift_down_selects_across_source_rows(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("| alpha | beta |\n| - | - |\n| one | two |\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(0, 4));
        editor.select_down(&SelectDown, window, cx);

        assert_eq!(editor.selection.start.row, 0);
        assert_eq!(editor.selection.end.row, 1);
        assert!(!editor.selection.is_empty());
    });
}

#[cfg(perf_enabled)]
#[gpui::test]
fn rendered_display_row_cache_hit_skips_syntax_queries(cx: &mut gpui::TestAppContext) {
    let editor = cx.update(|cx| {
        cx.new(|cx| {
            let mut editor = MarkdownEditor::for_text("# Heading\nBefore **bold** after\n", cx);
            editor.set_mode(MarkdownEditorMode::Rendered, cx);
            editor
        })
    });

    editor.update(cx, |editor, _| {
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let row = editor
            .cached_display_row(&snapshot, 1, editor.mode, &display_row_state)
            .expect("row should exist");

        editor.reset_layout_computation_counts();
        let cached_row = editor
            .cached_display_row(&snapshot, 1, editor.mode, &display_row_state)
            .expect("row should still exist");
        let counts = editor.layout_computation_counts();

        assert!(Arc::ptr_eq(&row, &cached_row));
        assert_eq!(counts.display_rows_created, 0);
        assert_eq!(counts.rendered_block_queries, 0);
        assert_eq!(counts.rendered_inline_span_queries, 0);
    });
}

#[gpui::test]
fn rendered_mode_actions_follow_wrapped_visual_rows_with_inline_image(
    cx: &mut gpui::TestAppContext,
) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(110.), px(240.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(
            "Before ![alt](https://example.com/cat.png) after more words here\n",
            cx,
        );
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        let source_line_end = editor.buffer.as_text_snapshot().line_len(0);
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let wrap_width = text_wrap_width_for_mode(window, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );

        let DisplayRowLayout::Text(text_layout) = row_layout else {
            panic!("expected wrapped text layout");
        };
        assert!(
            text_layout.visual_rows.len() >= 3,
            "expected at least three visual rows, got {:?}",
            text_layout.visual_rows
        );

        let second_visual_row = &text_layout.visual_rows[1];
        let wrapped_row_start = point_for_visual_row_x(
            &snapshot,
            &display_row,
            &text_layout,
            second_visual_row,
            px(0.),
        )
        .expect("second visual row should map to a point");
        let wrapped_row_end = point_for_display_offset(
            &snapshot,
            &display_row,
            &text_layout,
            second_visual_row.display_range.end,
        );

        assert_eq!(wrapped_row_start.row, 0);
        assert!(wrapped_row_start.column > 0);
        assert!(wrapped_row_start.column < source_line_end);
        assert_eq!(wrapped_row_end.row, 0);
        assert!(wrapped_row_end.column > wrapped_row_start.column);
        assert!(wrapped_row_end.column < source_line_end);

        editor.set_cursor(Point::new(0, 0));

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(editor.cursor(), wrapped_row_start);

        editor.move_to_end_of_line(&MoveToEndOfLine, window, cx);
        assert_eq!(editor.cursor(), wrapped_row_end);

        editor.move_to_beginning_of_line(&MoveToBeginningOfLine, window, cx);
        assert_eq!(editor.cursor(), wrapped_row_start);

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(editor.cursor(), Point::new(0, 0));
    });
}

#[gpui::test]
fn rendered_mode_select_actions_follow_wrapped_visual_rows_with_inline_image(
    cx: &mut gpui::TestAppContext,
) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(110.), px(240.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(
            "Before ![alt](https://example.com/cat.png) after more words here\n",
            cx,
        );
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        let source_line_end = editor.buffer.as_text_snapshot().line_len(0);
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
            .expect("display row should exist");
        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let wrap_width = text_wrap_width_for_mode(window, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            wrap_width,
            false,
            window,
            cx,
        );

        let DisplayRowLayout::Text(text_layout) = row_layout else {
            panic!("expected wrapped text layout");
        };
        assert!(
            text_layout.visual_rows.len() >= 3,
            "expected at least three visual rows, got {:?}",
            text_layout.visual_rows
        );

        let second_visual_row = &text_layout.visual_rows[1];
        let wrapped_row_start = point_for_visual_row_x(
            &snapshot,
            &display_row,
            &text_layout,
            second_visual_row,
            px(0.),
        )
        .expect("second visual row should map to a point");
        let wrapped_row_end = point_for_display_offset(
            &snapshot,
            &display_row,
            &text_layout,
            second_visual_row.display_range.end,
        );

        assert_eq!(wrapped_row_start.row, 0);
        assert!(wrapped_row_start.column > 0);
        assert!(wrapped_row_start.column < source_line_end);
        assert_eq!(wrapped_row_end.row, 0);
        assert!(wrapped_row_end.column > wrapped_row_start.column);
        assert!(wrapped_row_end.column < source_line_end);

        editor.set_cursor(Point::new(0, 0));

        editor.select_down(&SelectDown, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, wrapped_row_start);
        assert!(!editor.selection.reversed);

        editor.select_to_end_of_line(&SelectToEndOfLine, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, wrapped_row_end);
        assert!(!editor.selection.reversed);

        editor.select_to_beginning_of_line(&SelectToBeginningOfLine, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, wrapped_row_start);
        assert!(!editor.selection.reversed);

        editor.select_up(&SelectUp, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, Point::new(0, 0));
        assert!(!editor.selection.reversed);
    });
}

#[gpui::test]
fn rendered_mode_actions_follow_image_block_boundaries(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(500.), px(240.)));
    let image_source = "![alt](https://example.com/cat.png)";
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(&format!("Intro\n{image_source}\nAfter\n"), cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(0, 0));

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(editor.cursor(), Point::new(1, 0));

        editor.move_to_end_of_line(&MoveToEndOfLine, window, cx);
        assert_eq!(editor.cursor(), Point::new(1, image_source.len() as u32));

        editor.move_to_beginning_of_line(&MoveToBeginningOfLine, window, cx);
        assert_eq!(editor.cursor(), Point::new(1, 0));

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(editor.cursor(), Point::new(2, 0));

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(editor.cursor(), Point::new(1, 0));
    });
}

#[gpui::test]
fn rendered_mode_actions_follow_formula_block_boundaries(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(500.), px(240.)));
    let formula_source = "$$x + y$$";
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(&format!("Intro\n{formula_source}\nAfter\n"), cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(0, 0));

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(editor.cursor(), Point::new(1, 0));

        editor.move_to_end_of_line(&MoveToEndOfLine, window, cx);
        assert_eq!(editor.cursor(), Point::new(1, formula_source.len() as u32));

        editor.move_to_beginning_of_line(&MoveToBeginningOfLine, window, cx);
        assert_eq!(editor.cursor(), Point::new(1, 0));

        editor.move_down(&MoveDown, window, cx);
        assert_eq!(editor.cursor(), Point::new(2, 0));

        editor.move_up(&MoveUp, window, cx);
        assert_eq!(editor.cursor(), Point::new(1, 0));
    });
}

#[gpui::test]
fn rendered_mode_select_actions_extend_across_image_block(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(500.), px(240.)));
    let image_source = "![alt](https://example.com/cat.png)";
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(&format!("Intro\n{image_source}\nAfter\n"), cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(0, 0));

        editor.select_down(&SelectDown, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, Point::new(1, 0));
        assert!(!editor.selection.reversed);

        editor.select_to_end_of_line(&SelectToEndOfLine, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(
            editor.selection.end,
            Point::new(1, image_source.len() as u32)
        );
        assert!(!editor.selection.reversed);

        editor.select_to_beginning_of_line(&SelectToBeginningOfLine, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, Point::new(1, 0));
        assert!(!editor.selection.reversed);

        editor.select_up(&SelectUp, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, Point::new(0, 0));
        assert!(!editor.selection.reversed);
    });
}

#[gpui::test]
fn rendered_mode_select_actions_extend_across_formula_block(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(500.), px(240.)));
    let formula_source = "$$x + y$$";
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(&format!("Intro\n{formula_source}\nAfter\n"), cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(0, 0));

        editor.select_down(&SelectDown, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, Point::new(1, 0));
        assert!(!editor.selection.reversed);

        editor.select_to_end_of_line(&SelectToEndOfLine, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(
            editor.selection.end,
            Point::new(1, formula_source.len() as u32)
        );
        assert!(!editor.selection.reversed);

        editor.select_to_beginning_of_line(&SelectToBeginningOfLine, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, Point::new(1, 0));
        assert!(!editor.selection.reversed);

        editor.select_up(&SelectUp, window, cx);
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(editor.selection.end, Point::new(0, 0));
        assert!(!editor.selection.reversed);
    });
}

#[gpui::test]
fn source_render_uses_text_snapshot_without_refreshing_markdown_syntax(
    cx: &mut gpui::TestAppContext,
) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(240.), px(200.)));
    let editor = cx.new(|cx| MarkdownEditor::for_text("# Heading\nBody\n", cx));

    let (cached_syntax_version, edited_text_version) = editor.update(cx, |editor, _| {
        let _snapshot = editor.buffer.snapshot();
        let cached_syntax_version = editor.buffer.cached_syntax_version_for_tests();
        assert_eq!(
            &cached_syntax_version,
            editor.buffer.as_text_snapshot().version()
        );

        assert!(editor.buffer.edit([(0..0, "Plain text\n")]).is_some());
        let edited_text_version = editor.buffer.as_text_snapshot().version().clone();
        assert_ne!(cached_syntax_version, edited_text_version);
        assert_eq!(
            editor.buffer.cached_syntax_version_for_tests(),
            cached_syntax_version
        );

        (cached_syntax_version, edited_text_version)
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(240.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.buffer.cached_syntax_version_for_tests(),
            cached_syntax_version
        );
        assert_eq!(
            editor.buffer.as_text_snapshot().version(),
            &edited_text_version
        );
    });
}

#[gpui::test]
fn source_mouse_events_hit_and_select_wrapped_visual_rows(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(90.), px(200.)));
    let editor = cx.new(|cx| MarkdownEditor::for_text("abcdefghijklmnopqrst\n", cx));

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(90.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    let line_height = default_row_metrics().line_height;
    let second_visual_row = gpui::point(gutter_width() + px(2.), line_height * 1.5);
    cx.simulate_mouse_move(second_visual_row, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(
        second_visual_row,
        MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_up(
        second_visual_row,
        MouseButton::Left,
        gpui::Modifiers::none(),
    );

    editor.read_with(cx, |editor, _| {
        assert!(editor.selection.is_empty());
        assert_eq!(editor.cursor().row, 0);
        assert!(editor.cursor().column > 0);
        assert!(editor.cursor().column < editor.buffer.as_text_snapshot().line_len(0));
        assert!(matches!(
            editor.selection.goal,
            SelectionGoal::WrappedHorizontalPosition((1, _))
        ));
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(90.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    let third_visual_row = gpui::point(gutter_width() + px(30.), line_height * 2.5);
    cx.simulate_mouse_down(
        third_visual_row,
        MouseButton::Left,
        gpui::Modifiers::shift(),
    );
    cx.simulate_mouse_up(
        third_visual_row,
        MouseButton::Left,
        gpui::Modifiers::shift(),
    );

    editor.read_with(cx, |editor, _| {
        assert!(
            !editor.selection.is_empty(),
            "expected extended selection, got {:?}",
            editor.selection
        );
        assert_eq!(editor.selection.start.row, 0);
        assert_eq!(editor.selection.end.row, 0);
        assert!(editor.selection.end.column > editor.selection.start.column);
        assert!(matches!(
            editor.selection.goal,
            SelectionGoal::WrappedHorizontalPosition((2, _))
        ));
    });
}

#[gpui::test]
fn source_mouse_down_hits_blank_area_after_text(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(240.), px(120.)));
    let editor = cx.new(|cx| MarkdownEditor::for_text("short\nnext\n", cx));

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(240.), px(120.)),
        |_, _| editor.clone().into_any_element(),
    );

    let blank_area_after_text = gpui::point(gutter_width() + px(180.), px(10.));
    cx.simulate_mouse_move(blank_area_after_text, None, gpui::Modifiers::none());
    cx.simulate_mouse_down(
        blank_area_after_text,
        MouseButton::Left,
        gpui::Modifiers::none(),
    );
    cx.simulate_mouse_up(
        blank_area_after_text,
        MouseButton::Left,
        gpui::Modifiers::none(),
    );

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.cursor(), Point::new(0, 5));
    });
}

#[gpui::test]
fn source_clipboard_keybindings_copy_paste_and_cut(cx: &mut gpui::TestAppContext) {
    cx.update(init_standalone);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::for_text("alpha beta", cx);
        editor.selection = Selection {
            id: 0,
            start: Point::new(0, 0),
            end: Point::new(0, 5),
            reversed: false,
            goal: SelectionGoal::None,
        };
        window.focus(&editor.focus_handle(cx));
        window.activate_window();
        editor
    });

    cx.simulate_keystrokes("ctrl-c");
    assert_eq!(
        cx.cx.read_from_clipboard().and_then(|item| item.text()),
        Some("alpha".to_string())
    );

    editor.update(cx, |editor, _| {
        editor.set_cursor(Point::new(0, 10));
    });
    cx.cx
        .write_to_clipboard(ClipboardItem::new_string(" gamma".to_string()));
    cx.simulate_keystrokes("ctrl-v");
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.serialized_text(), "alpha beta gamma");
    });

    editor.update(cx, |editor, _| {
        editor.selection = Selection {
            id: 0,
            start: Point::new(0, 6),
            end: Point::new(0, 10),
            reversed: false,
            goal: SelectionGoal::None,
        };
    });
    cx.simulate_keystrokes("ctrl-x");
    assert_eq!(
        cx.cx.read_from_clipboard().and_then(|item| item.text()),
        Some("beta".to_string())
    );
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.serialized_text(), "alpha  gamma");
    });
}

#[gpui::test]
fn rendered_mode_actions_update_marker_visibility(cx: &mut gpui::TestAppContext) {
    cx.update(init_standalone);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        let mut editor = MarkdownEditor::for_text("# Title\nBody\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(1, 0));
        window.focus(&editor.focus_handle(cx));
        window.activate_window();
        editor
    });

    editor.update(cx, |editor, _| {
        assert_eq!(cached_row_text_for_current_selection(editor, 0), "Title");
    });

    cx.simulate_keystrokes("up");

    editor.update(cx, |editor, _| {
        assert_eq!(editor.cursor().row, 0);
        assert_eq!(cached_row_text_for_current_selection(editor, 0), "# Title");
    });

    cx.simulate_keystrokes("down");

    editor.update(cx, |editor, _| {
        assert_eq!(editor.cursor().row, 1);
        assert_eq!(cached_row_text_for_current_selection(editor, 0), "Title");
    });
}

fn rendered_task_checkbox_click_positions(
    editor: &mut MarkdownEditor,
    window: &mut Window,
    cx: &mut Context<MarkdownEditor>,
    source: &str,
    marker: &str,
    glyph: &str,
) -> (gpui::Pixels, gpui::Pixels) {
    let snapshot = editor.buffer.snapshot();
    let display_row_state =
        DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
    let display_row = editor
        .cached_display_row(&snapshot, 0, editor.mode, &display_row_state)
        .expect("display row should exist");
    assert!(
        display_row.text.contains(glyph),
        "expected inactive task marker glyph in {:?}",
        display_row.text
    );
    let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
    let selection = editor.selection.clone();
    let row_layout = editor.cached_row_layout(
        &snapshot,
        &display_row,
        &selection,
        editor.mode,
        row_style,
        text_wrap_width(window),
        false,
        window,
        cx,
    );
    let DisplayRowLayout::Text(text_layout) = row_layout else {
        panic!("expected text layout");
    };
    let marker_start = source.find(marker).expect("expected task marker");
    let display_start = display_row.source_to_display(marker_start);
    let display_end = display_start + glyph.len();
    let indent_width = display_row.rendered_indent_width();
    let x_start = display_x_for_offset(
        &text_layout.fragments,
        &text_layout.shaped_line,
        display_start,
    );
    let x_end = display_x_for_offset(
        &text_layout.fragments,
        &text_layout.shaped_line,
        display_end,
    );

    (
        left_rail_width(MarkdownEditorMode::Rendered)
            + indent_width
            + x_start
            + (x_end - x_start) * 0.5,
        left_rail_width(MarkdownEditorMode::Rendered) + indent_width + x_end + px(6.),
    )
}

#[gpui::test]
fn rendered_task_checkbox_click_toggles_unchecked_marker(cx: &mut gpui::TestAppContext) {
    let source = "- [ ] todo\nnext\n";
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(source, cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(1, 0));
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(320.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    let (click_x, _) = editor.update_in(cx, |editor, window, cx| {
        rendered_task_checkbox_click_positions(editor, window, cx, source, "[ ]", "\u{2610}")
    });
    let click = gpui::point(click_x, default_row_metrics().line_height * 0.5);
    cx.simulate_mouse_down(click, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(click, MouseButton::Left, gpui::Modifiers::none());

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.serialized_text(), "- [x] todo\nnext\n");
        assert_eq!(editor.cursor(), Point::new(1, 0));
    });
}

#[gpui::test]
fn rendered_task_checkbox_click_toggles_checked_marker(cx: &mut gpui::TestAppContext) {
    let source = "- [X] todo\nnext\n";
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(source, cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(1, 0));
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(320.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    let (click_x, _) = editor.update_in(cx, |editor, window, cx| {
        rendered_task_checkbox_click_positions(editor, window, cx, source, "[X]", "\u{2611}")
    });
    let click = gpui::point(click_x, default_row_metrics().line_height * 0.5);
    cx.simulate_mouse_down(click, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(click, MouseButton::Left, gpui::Modifiers::none());

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.serialized_text(), "- [ ] todo\nnext\n");
        assert_eq!(editor.cursor(), Point::new(1, 0));
    });
}

#[gpui::test]
fn rendered_task_checkbox_click_toggles_quoted_marker(cx: &mut gpui::TestAppContext) {
    let source = "> - [ ] todo\nnext\n";
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(source, cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(1, 0));
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(320.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    let (click_x, _) = editor.update_in(cx, |editor, window, cx| {
        rendered_task_checkbox_click_positions(editor, window, cx, source, "[ ]", "\u{2610}")
    });
    let click = gpui::point(click_x, default_row_metrics().line_height * 0.5);
    cx.simulate_mouse_down(click, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(click, MouseButton::Left, gpui::Modifiers::none());

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.serialized_text(), "> - [x] todo\nnext\n");
        assert_eq!(editor.cursor(), Point::new(1, 0));
    });
}

#[gpui::test]
fn rendered_task_checkbox_click_outside_glyph_keeps_source_text(cx: &mut gpui::TestAppContext) {
    let source = "- [ ] todo\nnext\n";
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(source, cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(1, 0));
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(320.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    let (_, outside_x) = editor.update_in(cx, |editor, window, cx| {
        rendered_task_checkbox_click_positions(editor, window, cx, source, "[ ]", "\u{2610}")
    });
    let click = gpui::point(outside_x, default_row_metrics().line_height * 0.5);
    cx.simulate_mouse_down(click, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(click, MouseButton::Left, gpui::Modifiers::none());

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.serialized_text(), source);
    });
}

#[gpui::test]
fn resize_reflow_clears_wrapped_action_goal(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(90.), px(200.)));
    let editor = cx.new(|cx| MarkdownEditor::for_text("abcdefghijklmnopqrst\n", cx));

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(90.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(0, 0));
        editor.move_down(&MoveDown, window, cx);
        assert!(matches!(
            editor.selection.goal,
            SelectionGoal::WrappedHorizontalPosition(_)
        ));
    });

    cx.simulate_resize(gpui::size(px(180.), px(200.)));
    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(180.), px(200.)),
        |_, _| editor.clone().into_any_element(),
    );

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.selection.goal, SelectionGoal::None);
    });
}

#[gpui::test]
fn mode_switch_clears_wrapped_visual_goal(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(90.), px(200.)));
    let editor = cx.new(|cx| MarkdownEditor::for_text("abcdefghijklmnopqrst\n", cx));

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(0, 0));
        editor.move_down(&MoveDown, window, cx);
        let wrapped_cursor = editor.cursor();
        assert!(matches!(
            editor.selection.goal,
            SelectionGoal::WrappedHorizontalPosition(_)
        ));

        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        assert_eq!(editor.cursor(), wrapped_cursor);
        assert_eq!(editor.selection.goal, SelectionGoal::None);

        editor.set_mode(MarkdownEditorMode::Source, cx);
        assert_eq!(editor.cursor(), wrapped_cursor);
        assert_eq!(editor.selection.goal, SelectionGoal::None);
    });
}

#[gpui::test]
fn rendered_mode_draws_image_block_without_reentering_list_state(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(
            "# Heading\n\n![Test Image](https://example.com/cat.png)\n\nAfter image",
            cx,
        );
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(400.)),
        |_, _| editor.clone().into_any_element(),
    );
}

#[gpui::test]
fn rendered_mode_does_not_cache_loading_image_block_layout(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("![alt](https://example.com/cat.png)\nnext\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(1, 0));
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(120.)),
        |_, _| editor.clone().into_any_element(),
    );

    editor.read_with(cx, |editor, _| {
        assert!(
            !editor
                .row_layout_cache
                .keys()
                .any(|key| key.item_index == 0)
        );
    });
}

#[gpui::test]
fn rendered_image_block_mouse_events_select_source_boundaries(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(500.), px(400.)));
    let image_source = "![alt](https://example.com/cat.png)";
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(&format!("{image_source}\nnext\n"), cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(1, 0));
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(400.)),
        |_, _| editor.clone().into_any_element(),
    );

    let block_middle_y = (RENDERED_IMAGE_BLOCK_PLACEHOLDER_HEIGHT
        + RENDERED_IMAGE_BLOCK_VERTICAL_PADDING * 2.)
        * 0.5;
    let left_half = gpui::point(gutter_width() + px(20.), block_middle_y);
    cx.simulate_mouse_down(left_half, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(left_half, MouseButton::Left, gpui::Modifiers::none());

    editor.read_with(cx, |editor, _| {
        assert!(editor.selection.is_empty());
        assert_eq!(editor.cursor(), Point::new(0, 0));
        assert_eq!(editor.selection.goal, SelectionGoal::HorizontalPosition(0.));
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(400.)),
        |_, _| editor.clone().into_any_element(),
    );

    let right_half = gpui::point(gutter_width() + px(360.), block_middle_y);
    cx.simulate_mouse_down(right_half, MouseButton::Left, gpui::Modifiers::shift());
    cx.simulate_mouse_up(right_half, MouseButton::Left, gpui::Modifiers::shift());

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(
            editor.selection.end,
            Point::new(0, image_source.len() as u32)
        );
        assert!(!editor.selection.reversed);
        assert!(matches!(
            editor.selection.goal,
            SelectionGoal::WrappedHorizontalPosition((0, x)) if x > 0.
        ));
    });
}

#[gpui::test]
fn rendered_formula_block_mouse_events_select_source_boundaries(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(500.), px(400.)));
    let formula_source = "$$x + y$$";
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(&format!("{formula_source}\nnext\n"), cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(1, 0));
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(400.)),
        |_, _| editor.clone().into_any_element(),
    );

    let block_middle_y =
        (default_row_metrics().line_height + RENDERED_FORMULA_BLOCK_VERTICAL_PADDING * 2.) * 0.5;
    let left_half = gpui::point(gutter_width() + px(20.), block_middle_y);
    cx.simulate_mouse_down(left_half, MouseButton::Left, gpui::Modifiers::none());
    cx.simulate_mouse_up(left_half, MouseButton::Left, gpui::Modifiers::none());

    editor.read_with(cx, |editor, _| {
        assert!(editor.selection.is_empty());
        assert_eq!(editor.cursor(), Point::new(0, 0));
        assert_eq!(editor.selection.goal, SelectionGoal::HorizontalPosition(0.));
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(400.)),
        |_, _| editor.clone().into_any_element(),
    );

    let right_half = gpui::point(gutter_width() + px(360.), block_middle_y);
    cx.simulate_mouse_down(right_half, MouseButton::Left, gpui::Modifiers::shift());
    cx.simulate_mouse_up(right_half, MouseButton::Left, gpui::Modifiers::shift());

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.selection.start, Point::new(0, 0));
        assert_eq!(
            editor.selection.end,
            Point::new(0, formula_source.len() as u32)
        );
        assert!(!editor.selection.reversed);
        assert!(matches!(
            editor.selection.goal,
            SelectionGoal::WrappedHorizontalPosition((0, x)) if x > 0.
        ));
    });
}

#[gpui::test]
fn rendered_mode_draws_inline_image_atom(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("Before ![alt](https://example.com/cat.png) after", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(120.)),
        |_, _| editor.clone().into_any_element(),
    );
}

#[gpui::test]
fn rendered_inline_math_ready_measurement_makes_row_layout_cacheable(
    cx: &mut gpui::TestAppContext,
) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text("Before $x + y$ after", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(120.)),
        |_, _| editor.clone().into_any_element(),
    );

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.row_layout_input_cache.len(), 1);
        assert_eq!(editor.inline_atom_measurement_cache.len(), 1);
        assert!(
            editor
                .inline_atom_measurement_cache
                .values()
                .all(|state| { matches!(state, InlineAtomMeasurementState::Ready(_)) })
        );
        assert_eq!(editor.row_layout_cache.len(), 1);
    });
}

#[gpui::test]
fn rendered_mode_does_not_cache_loading_inline_image_layout(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("Before ![alt](https://example.com/cat.png) after", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(120.)),
        |_, _| editor.clone().into_any_element(),
    );

    assert_eq!(
        editor.read_with(cx, |editor, _| editor.row_layout_cache.len()),
        0
    );
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.inline_atom_measurement_cache.len(), 0);
        assert_eq!(editor.pending_inline_atom_rows.len(), 1);
    });
}

#[gpui::test]
fn rendered_mode_draws_empty_alt_inline_image_atom(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| {
        let mut editor =
            MarkdownEditor::for_text("Before ![](https://example.com/cat.png) after", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    cx.draw(
        gpui::point(px(0.), px(0.)),
        gpui::size(px(500.), px(120.)),
        |_, _| editor.clone().into_any_element(),
    );
}

#[gpui::test]
fn inline_atom_deferred_remeasure_only_clears_affected_row(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text("first cached row\nsecond cached row\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        editor.set_cursor(Point::new(2, 0));
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let selection = editor.selection.clone();
        let wrap_width = text_wrap_width(window);
        for row in 0..2 {
            let display_row = editor
                .cached_display_row(&snapshot, row, editor.mode, &display_row_state)
                .expect("display row should exist");
            let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
            let _ = editor.cached_row_layout(
                &snapshot,
                &display_row,
                &selection,
                editor.mode,
                row_style,
                wrap_width,
                false,
                window,
                cx,
            );
        }
        assert_eq!(editor.row_layout_cache.len(), 2);

        let row_style = default_row_metrics().into();
        let key = InlineAtomMeasurementKey {
            kind: DisplayInlineAtomKind::InlineMath,
            source_range: 0..3,
            descriptor: math_descriptor(0..3, "x"),
            fallback_text: "x".to_string(),
            row_style,
            resource_id: None,
            image_max_width: None,
            formula_scale_factor_bits: Some(window.scale_factor().to_bits()),
        };
        editor
            .pending_inline_atom_rows
            .entry(key.clone())
            .or_default()
            .insert(0);
        editor.update_inline_atom_measurement_cache(
            0,
            key,
            InlineAtomMeasurementState::Ready(gpui::size(px(40.), row_style.line_height)),
            window,
            cx,
        );
        assert!(editor.inline_atom_remeasure_scheduled);
        assert_eq!(editor.row_layout_cache.len(), 2);

        editor.flush_inline_atom_row_remeasures(cx);
        assert!(
            !editor
                .row_layout_cache
                .keys()
                .any(|key| key.item_index == 0)
        );
        assert!(
            editor
                .row_layout_cache
                .keys()
                .any(|key| key.item_index == 1)
        );
    });
}

#[test]
fn selection_without_goal_preserves_selection_shape() {
    let selection = Selection {
        id: 7,
        start: Point::new(0, 2),
        end: Point::new(3, 1),
        reversed: true,
        goal: SelectionGoal::WrappedHorizontalPosition((2, 48.)),
    };

    assert_eq!(
        selection_without_goal(&selection),
        Selection {
            id: 7,
            start: Point::new(0, 2),
            end: Point::new(3, 1),
            reversed: true,
            goal: SelectionGoal::None,
        }
    );
}

#[test]
fn transaction_selection_history_drops_layout_goals() {
    let before = Selection {
        id: 7,
        start: Point::new(0, 2),
        end: Point::new(3, 1),
        reversed: true,
        goal: SelectionGoal::WrappedHorizontalPosition((2, 48.)),
    };
    let after = Selection {
        id: 8,
        start: Point::new(1, 0),
        end: Point::new(1, 4),
        reversed: false,
        goal: SelectionGoal::WrappedHorizontalPosition((1, 24.)),
    };

    assert_eq!(
        transaction_selection_state_without_goals(before, after),
        TransactionSelectionState {
            before: Selection {
                id: 7,
                start: Point::new(0, 2),
                end: Point::new(3, 1),
                reversed: true,
                goal: SelectionGoal::None,
            },
            after: Selection {
                id: 8,
                start: Point::new(1, 0),
                end: Point::new(1, 4),
                reversed: false,
                goal: SelectionGoal::None,
            },
        }
    );
}

#[test]
fn reveal_selection_head_row_scrolls_to_clipped_cursor_row() {
    let buffer = Buffer::local("zero\none\ntwo\n");
    let list_state = MdListState::new(2, ListAlignment::Top, px(1000.));
    list_state.scroll_to(ListOffset {
        item_ix: 1,
        offset_in_item: px(5.),
    });
    let selection = collapsed_selection(Point::new(2, 0));

    reveal_selection_head_row_in_text_snapshot(&list_state, buffer.as_text_snapshot(), &selection);

    let scroll_top = list_state.logical_scroll_top();
    assert_eq!(scroll_top.item_ix, 1);
    assert_eq!(scroll_top.offset_in_item, px(0.));
}

#[test]
fn text_wrap_width_change_clears_stale_selection_goal_once() {
    let mut last_text_wrap_width = Some(px(120.));
    let mut selection = Selection {
        id: 7,
        start: Point::new(0, 2),
        end: Point::new(3, 1),
        reversed: true,
        goal: SelectionGoal::WrappedHorizontalPosition((2, 48.)),
    };

    assert!(apply_text_wrap_width_change(
        &mut last_text_wrap_width,
        &mut selection,
        px(80.)
    ));
    assert_eq!(last_text_wrap_width, Some(px(80.)));
    assert_eq!(selection.goal, SelectionGoal::None);
    assert_eq!(selection.start, Point::new(0, 2));
    assert_eq!(selection.end, Point::new(3, 1));
    assert!(selection.reversed);

    selection.goal = SelectionGoal::WrappedHorizontalPosition((1, 24.));

    assert!(!apply_text_wrap_width_change(
        &mut last_text_wrap_width,
        &mut selection,
        px(80.)
    ));
    assert_eq!(
        selection.goal,
        SelectionGoal::WrappedHorizontalPosition((1, 24.))
    );
}

#[test]
fn source_single_row_edit_invalidates_only_that_row() {
    let previous_selection = Selection {
        id: 7,
        start: Point::new(4, 2),
        end: Point::new(4, 5),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let current_selection = collapsed_selection(Point::new(4, 8));

    assert_eq!(
        local_source_edit_invalidation_rows(
            MarkdownEditorMode::Source,
            12,
            12,
            &previous_selection,
            &current_selection,
        ),
        Some(4..5)
    );
}

#[gpui::test]
fn source_cached_display_row_reuses_text_snapshot_fast_path(cx: &mut gpui::TestAppContext) {
    let editor = cx.update(|cx| cx.new(|cx| MarkdownEditor::for_text("one\n**two**", cx)));

    editor.update(cx, |editor, _| {
        let snapshot = editor.buffer.snapshot();
        let display_row_state = DisplayRowProjectionState::new(
            &snapshot,
            Some(&editor.selection),
            MarkdownEditorMode::Source,
        );

        let display_row = editor
            .cached_display_row(&snapshot, 1, MarkdownEditorMode::Source, &display_row_state)
            .expect("source row should exist");
        let source_display_row = editor
            .cached_source_display_row(snapshot.as_text_snapshot(), 1)
            .expect("source row should exist");

        assert!(Arc::ptr_eq(&display_row, &source_display_row));
        assert_eq!(display_row.text, "**two**");
        assert_eq!(display_row.source_text, "**two**");
    });
}

#[gpui::test]
fn source_single_row_edit_rekeys_display_row_cache_before_edited_row(
    cx: &mut gpui::TestAppContext,
) {
    let editor = cx.update(|cx| cx.new(|cx| MarkdownEditor::for_text("one\ntwo\nthree", cx)));

    editor.update(cx, |editor, cx| {
        let mode = editor.mode;
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), mode);
        let row_0 = editor
            .cached_display_row(&snapshot, 0, mode, &display_row_state)
            .expect("row 0 should exist");
        let row_1 = editor
            .cached_display_row(&snapshot, 1, mode, &display_row_state)
            .expect("row 1 should exist");
        let row_2 = editor
            .cached_display_row(&snapshot, 2, mode, &display_row_state)
            .expect("row 2 should exist");

        let previous_selection = collapsed_selection(Point::new(1, 1));
        editor.selection = previous_selection.clone();
        let row_count_before = editor.display_list_state.item_count();
        let (selection, transaction_id) =
            replace_selection(&mut editor.buffer, &editor.selection, "XX");
        assert!(transaction_id.is_some());
        editor.selection = selection;

        editor.notify_after_edit(
            true,
            row_count_before,
            &previous_selection,
            EditLayoutInvalidation::LocalSourceSelection {
                byte_delta: Some("XX".len() as isize),
            },
            cx,
        );

        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), mode);
        let cached_row_0 = editor
            .cached_display_row(&snapshot, 0, mode, &display_row_state)
            .expect("row 0 should exist after edit");
        let cached_row_1 = editor
            .cached_display_row(&snapshot, 1, mode, &display_row_state)
            .expect("row 1 should exist after edit");
        let cached_row_2 = editor
            .cached_display_row(&snapshot, 2, mode, &display_row_state)
            .expect("row 2 should exist after edit");

        assert!(Arc::ptr_eq(&row_0, &cached_row_0));
        assert!(!Arc::ptr_eq(&row_1, &cached_row_1));
        assert!(!Arc::ptr_eq(&row_2, &cached_row_2));
        assert_eq!(cached_row_1.text, "tXXwo");
        assert_eq!(
            cached_row_2.source_range.start,
            row_2.source_range.start + "XX".len()
        );
    });
}

#[gpui::test]
fn source_length_preserving_single_row_edit_keeps_later_display_rows(
    cx: &mut gpui::TestAppContext,
) {
    let editor = cx.update(|cx| cx.new(|cx| MarkdownEditor::for_text("one\ntwo\nthree", cx)));

    editor.update(cx, |editor, cx| {
        let mode = editor.mode;
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), mode);
        let row_0 = editor
            .cached_display_row(&snapshot, 0, mode, &display_row_state)
            .expect("row 0 should exist");
        let row_1 = editor
            .cached_display_row(&snapshot, 1, mode, &display_row_state)
            .expect("row 1 should exist");
        let row_2 = editor
            .cached_display_row(&snapshot, 2, mode, &display_row_state)
            .expect("row 2 should exist");

        let previous_selection = Selection {
            id: 7,
            start: Point::new(1, 1),
            end: Point::new(1, 2),
            reversed: false,
            goal: SelectionGoal::None,
        };
        editor.selection = previous_selection.clone();
        let row_count_before = editor.display_list_state.item_count();
        let buffer_len_before = editor.buffer.len();
        let (selection, transaction_id) =
            replace_selection(&mut editor.buffer, &editor.selection, "X");
        assert!(transaction_id.is_some());
        let byte_delta = buffer_byte_delta(buffer_len_before, editor.buffer.len());
        editor.selection = selection;

        editor.notify_after_edit(
            true,
            row_count_before,
            &previous_selection,
            EditLayoutInvalidation::LocalSourceSelection { byte_delta },
            cx,
        );

        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), mode);
        let cached_row_0 = editor
            .cached_display_row(&snapshot, 0, mode, &display_row_state)
            .expect("row 0 should exist after edit");
        let cached_row_1 = editor
            .cached_display_row(&snapshot, 1, mode, &display_row_state)
            .expect("row 1 should exist after edit");
        let cached_row_2 = editor
            .cached_display_row(&snapshot, 2, mode, &display_row_state)
            .expect("row 2 should exist after edit");

        assert!(Arc::ptr_eq(&row_0, &cached_row_0));
        assert!(!Arc::ptr_eq(&row_1, &cached_row_1));
        assert!(Arc::ptr_eq(&row_2, &cached_row_2));
        assert_eq!(cached_row_1.text, "tXo");
        assert_eq!(cached_row_2.source_range, row_2.source_range);
    });
}

#[gpui::test]
fn source_undo_redo_single_row_edit_keeps_later_display_rows(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    let editor = cx.new(|cx| MarkdownEditor::for_text("one\ntwo\nthree", cx));

    editor.update_in(cx, |editor, window, cx| {
        let mode = editor.mode;
        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), mode);
        let row_0 = editor
            .cached_display_row(&snapshot, 0, mode, &display_row_state)
            .expect("row 0 should exist");
        let row_1 = editor
            .cached_display_row(&snapshot, 1, mode, &display_row_state)
            .expect("row 1 should exist");
        let row_2 = editor
            .cached_display_row(&snapshot, 2, mode, &display_row_state)
            .expect("row 2 should exist");

        let previous_selection = Selection {
            id: 7,
            start: Point::new(1, 1),
            end: Point::new(1, 2),
            reversed: false,
            goal: SelectionGoal::None,
        };
        editor.selection = previous_selection.clone();
        let row_count_before = editor.display_list_state.item_count();
        let buffer_len_before = editor.buffer.len();
        let (selection, transaction_id) =
            replace_selection(&mut editor.buffer, &editor.selection, "X");
        assert!(transaction_id.is_some());
        let byte_delta = buffer_byte_delta(buffer_len_before, editor.buffer.len());
        editor.selection = selection;
        editor.record_selection_history(
            transaction_id,
            previous_selection.clone(),
            editor.selection.clone(),
        );

        editor.notify_after_edit(
            true,
            row_count_before,
            &previous_selection,
            EditLayoutInvalidation::LocalSourceSelection { byte_delta },
            cx,
        );

        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), mode);
        let edited_row_0 = editor
            .cached_display_row(&snapshot, 0, mode, &display_row_state)
            .expect("row 0 should exist after edit");
        let edited_row_1 = editor
            .cached_display_row(&snapshot, 1, mode, &display_row_state)
            .expect("row 1 should exist after edit");
        let edited_row_2 = editor
            .cached_display_row(&snapshot, 2, mode, &display_row_state)
            .expect("row 2 should exist after edit");

        assert!(Arc::ptr_eq(&row_0, &edited_row_0));
        assert!(!Arc::ptr_eq(&row_1, &edited_row_1));
        assert!(Arc::ptr_eq(&row_2, &edited_row_2));
        assert_eq!(edited_row_1.text, "tXo");

        editor.undo(&Undo, window, cx);

        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), mode);
        let undo_row_0 = editor
            .cached_display_row(&snapshot, 0, mode, &display_row_state)
            .expect("row 0 should exist after undo");
        let undo_row_1 = editor
            .cached_display_row(&snapshot, 1, mode, &display_row_state)
            .expect("row 1 should exist after undo");
        let undo_row_2 = editor
            .cached_display_row(&snapshot, 2, mode, &display_row_state)
            .expect("row 2 should exist after undo");

        assert!(Arc::ptr_eq(&edited_row_0, &undo_row_0));
        assert!(!Arc::ptr_eq(&edited_row_1, &undo_row_1));
        assert!(Arc::ptr_eq(&edited_row_2, &undo_row_2));
        assert_eq!(undo_row_1.text, "two");
        assert_eq!(undo_row_2.source_range, edited_row_2.source_range);

        editor.redo(&Redo, window, cx);

        let snapshot = editor.buffer.snapshot();
        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), mode);
        let redo_row_0 = editor
            .cached_display_row(&snapshot, 0, mode, &display_row_state)
            .expect("row 0 should exist after redo");
        let redo_row_1 = editor
            .cached_display_row(&snapshot, 1, mode, &display_row_state)
            .expect("row 1 should exist after redo");
        let redo_row_2 = editor
            .cached_display_row(&snapshot, 2, mode, &display_row_state)
            .expect("row 2 should exist after redo");

        assert!(Arc::ptr_eq(&undo_row_0, &redo_row_0));
        assert!(!Arc::ptr_eq(&undo_row_1, &redo_row_1));
        assert!(Arc::ptr_eq(&undo_row_2, &redo_row_2));
        assert_eq!(redo_row_1.text, "tXo");
        assert_eq!(redo_row_2.source_range, undo_row_2.source_range);
    });
}

#[test]
fn local_edit_invalidation_stays_conservative_for_cross_row_or_rendered_edits() {
    let previous_selection = Selection {
        id: 7,
        start: Point::new(4, 2),
        end: Point::new(5, 1),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let current_selection = collapsed_selection(Point::new(4, 8));

    assert_eq!(
        local_source_edit_invalidation_rows(
            MarkdownEditorMode::Source,
            12,
            12,
            &previous_selection,
            &current_selection,
        ),
        None
    );

    assert_eq!(
        local_source_edit_invalidation_rows(
            MarkdownEditorMode::Rendered,
            12,
            12,
            &collapsed_selection(Point::new(4, 2)),
            &current_selection,
        ),
        None
    );

    assert_eq!(
        local_source_edit_invalidation_rows(
            MarkdownEditorMode::Source,
            12,
            13,
            &collapsed_selection(Point::new(4, 2)),
            &current_selection,
        ),
        None
    );
}

#[test]
fn row_count_change_splice_preserves_rows_outside_edit() {
    assert_eq!(
        row_count_change_splice(
            100,
            101,
            &collapsed_selection(Point::new(50, 4)),
            &collapsed_selection(Point::new(51, 0)),
        ),
        Some((50..51, 2))
    );

    assert_eq!(
        row_count_change_splice(
            100,
            99,
            &collapsed_selection(Point::new(50, 0)),
            &collapsed_selection(Point::new(49, 8)),
        ),
        Some((49..51, 1))
    );
}

#[test]
fn rendered_active_source_range_change_drops_stale_visual_row_goal_once() {
    let previous_active = 8..12;
    let current_active = 16..24;
    let mut selection = Selection {
        id: 7,
        start: Point::new(0, 2),
        end: Point::new(3, 1),
        reversed: true,
        goal: SelectionGoal::WrappedHorizontalPosition((2, 48.)),
    };

    assert!(apply_rendered_active_source_range_change(
        &mut selection,
        Some(&previous_active),
        Some(&current_active)
    ));
    assert_eq!(
        selection,
        Selection {
            id: 7,
            start: Point::new(0, 2),
            end: Point::new(3, 1),
            reversed: true,
            goal: SelectionGoal::HorizontalPosition(48.),
        }
    );

    selection.goal = SelectionGoal::WrappedHorizontalPosition((1, 24.));

    assert!(!apply_rendered_active_source_range_change(
        &mut selection,
        Some(&current_active),
        Some(&current_active)
    ));
    assert_eq!(
        selection.goal,
        SelectionGoal::WrappedHorizontalPosition((1, 24.))
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
fn rendered_backspace_deletes_previous_inactive_inline_atom() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let atom_start = "Before ".len();
    let atom_end = "Before $x + y$".len();
    let selection = collapsed_selection(Point::new(0, atom_end as u32));

    let (selection, transaction_id) =
        backspace_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "Before  after\n");
    assert_eq!(
        selection,
        collapsed_selection(Point::new(0, atom_start as u32))
    );
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_delete_deletes_next_inactive_inline_atom() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let atom_start = "Before ".len();
    let selection = collapsed_selection(Point::new(0, atom_start as u32));

    let (selection, transaction_id) =
        delete_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "Before  after\n");
    assert_eq!(
        selection,
        collapsed_selection(Point::new(0, atom_start as u32))
    );
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_backspace_deletes_previous_image_block() {
    let image_source = "![alt](https://example.com/cat.png)";
    let mut buffer = Buffer::local(&format!("{image_source}\nnext\n"));
    let selection = collapsed_selection(Point::new(0, image_source.len() as u32));

    let (selection, transaction_id) =
        backspace_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "\nnext\n");
    assert_eq!(selection, collapsed_selection(Point::new(0, 0)));
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_delete_deletes_next_image_block() {
    let image_source = "![alt](https://example.com/cat.png)";
    let mut buffer = Buffer::local(&format!("{image_source}\nnext\n"));
    let selection = collapsed_selection(Point::new(0, 0));

    let (selection, transaction_id) =
        delete_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "\nnext\n");
    assert_eq!(selection, collapsed_selection(Point::new(0, 0)));
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_backspace_deletes_previous_formula_block() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\nnext\n"));
    let selection = collapsed_selection(Point::new(0, formula_source.len() as u32));

    let (selection, transaction_id) =
        backspace_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "\nnext\n");
    assert_eq!(selection, collapsed_selection(Point::new(0, 0)));
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_delete_deletes_next_formula_block() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\nnext\n"));
    let selection = collapsed_selection(Point::new(0, 0));

    let (selection, transaction_id) =
        delete_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "\nnext\n");
    assert_eq!(selection, collapsed_selection(Point::new(0, 0)));
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_delete_deletes_next_inactive_inline_image() {
    let mut buffer = Buffer::local("before ![alt](https://example.com/cat.png) after\n");
    let image_start = "before ".len();
    let selection = collapsed_selection(Point::new(0, image_start as u32));

    let (selection, transaction_id) =
        delete_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "before  after\n");
    assert_eq!(
        selection,
        collapsed_selection(Point::new(0, image_start as u32))
    );
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_delete_inside_inline_atom_uses_character_movement() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let atom_content_start = "Before $".len();
    let selection = collapsed_selection(Point::new(0, atom_content_start as u32));

    let (selection, transaction_id) =
        delete_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "Before $ + y$ after\n");
    assert_eq!(
        selection,
        collapsed_selection(Point::new(0, atom_content_start as u32))
    );
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_backspace_deletes_previous_inactive_replacement() {
    let source = "Escape \\* &amp;\n";
    let escape_start = source.find("\\*").expect("expected escaped marker");
    let escape_end = escape_start + "\\*".len();
    let mut buffer = Buffer::local(source);
    let selection = collapsed_selection(Point::new(0, escape_end as u32));

    let (selection, transaction_id) =
        backspace_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "Escape  &amp;\n");
    assert_eq!(
        selection,
        collapsed_selection(Point::new(0, escape_start as u32))
    );
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_backspace_deletes_previous_inactive_task_marker_replacement() {
    let source = "- [ ] todo\n";
    let marker_start = source.find("[ ]").expect("expected task marker");
    let marker_end = marker_start + "[ ]".len();
    let mut buffer = Buffer::local(source);
    let selection = collapsed_selection(Point::new(0, marker_end as u32));

    let (selection, transaction_id) =
        backspace_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "-  todo\n");
    assert_eq!(
        selection,
        collapsed_selection(Point::new(0, marker_start as u32))
    );
    assert!(transaction_id.is_some());
}

#[test]
fn rendered_delete_at_active_replacement_start_uses_source_character_movement() {
    let source = "Escape \\* &amp;\n";
    let escape_start = source.find("\\*").expect("expected escaped marker");
    let mut buffer = Buffer::local(source);
    let selection = collapsed_selection(Point::new(0, escape_start as u32));

    let (selection, transaction_id) =
        delete_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Rendered);

    assert_eq!(buffer.text(), "Escape * &amp;\n");
    assert_eq!(
        selection,
        collapsed_selection(Point::new(0, escape_start as u32))
    );
    assert!(transaction_id.is_some());
}

#[test]
fn source_delete_keeps_inline_atom_source_character_movement() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let atom_start = "Before ".len();
    let selection = collapsed_selection(Point::new(0, atom_start as u32));

    let (selection, transaction_id) =
        delete_selection_in_mode(&mut buffer, &selection, MarkdownEditorMode::Source);

    assert_eq!(buffer.text(), "Before x + y$ after\n");
    assert_eq!(
        selection,
        collapsed_selection(Point::new(0, atom_start as u32))
    );
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
