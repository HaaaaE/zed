use super::test_support::*;

fn test_text_run(len: usize) -> TextRun {
    TextRun {
        len,
        font: font(EDITOR_FONT_FAMILY),
        color: md_theme::editor_palette().text,
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

fn shape_test_line(cx: &mut gpui::TestAppContext, text: &str) -> gpui::ShapedLine {
    let cx = cx.add_empty_window();
    cx.update(|window, _| {
        window.text_system().shape_line(
            SharedString::from(text.to_string()),
            px(10.),
            &[test_text_run(text.len())],
            None,
        )
    })
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

    let row_style =
        row_display_style_for_display_row(&snapshot, &row, MarkdownEditorMode::Rendered);
    let fragments = display_inline_fragments(
        &snapshot,
        &row,
        MarkdownEditorMode::Rendered,
        row_style,
        None,
    );
    let segments = text_segments_for_fragments(&row.text, &fragments);

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

    let row_style =
        row_display_style_for_display_row(&snapshot, &row, MarkdownEditorMode::Rendered);
    let fragments = display_inline_fragments(
        &snapshot,
        &row,
        MarkdownEditorMode::Rendered,
        row_style,
        None,
    );
    let segments = text_segments_for_fragments(&row.text, &fragments);

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
fn rendered_styled_segments_apply_raw_html_semantics() {
    let mut buffer = Buffer::local("<div data-kind=\"raw\">html</div>\nInline <br> raw\n");
    let snapshot = buffer.snapshot();

    let html_block = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(1, 0))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);
    let inline_html = display_rows_in_mode(
        &snapshot,
        1..2,
        Some(&collapsed_selection(Point::new(0, 0))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    let block_row_style =
        row_display_style_for_display_row(&snapshot, &html_block, MarkdownEditorMode::Rendered);
    let block_segments = text_segments_for_fragments(
        &html_block.text,
        &display_inline_fragments(
            &snapshot,
            &html_block,
            MarkdownEditorMode::Rendered,
            block_row_style,
            None,
        ),
    );
    let inline_row_style =
        row_display_style_for_display_row(&snapshot, &inline_html, MarkdownEditorMode::Rendered);
    let inline_segments = text_segments_for_fragments(
        &inline_html.text,
        &display_inline_fragments(
            &snapshot,
            &inline_html,
            MarkdownEditorMode::Rendered,
            inline_row_style,
            None,
        ),
    );
    let muted = Some(md_theme::editor_palette().muted_text);

    assert_eq!(html_block.text, "<div data-kind=\"raw\">html</div>");
    assert_eq!(block_segments.len(), 1);
    assert_eq!(block_segments[0].style.color, muted);
    assert_eq!(inline_html.text, "Inline <br> raw");
    assert!(
        inline_segments
            .iter()
            .any(|segment| segment.text.contains("<br>") && segment.style.color == muted),
        "inline segments: {inline_segments:?}"
    );
    assert_eq!(inline_style(MarkdownInlineKind::InlineHtml).color, muted);
}

#[gpui::test]
fn mouse_target_for_wrapped_row_end_keeps_clicked_visual_row_goal(cx: &mut gpui::TestAppContext) {
    let mut buffer = Buffer::local("abcdefghij\n");
    let snapshot = buffer.snapshot();
    let Some(display_row) = display_rows(&snapshot, 0..1).into_iter().next() else {
        panic!("expected display row");
    };
    let text = display_row.text.clone();
    let shaped_line = shape_test_line(cx, &text);
    let first_visual_row = VisualDisplayRow {
        display_range: 0..5,
        line_start_x: px(0.),
        top: px(0.),
        height: px(20.),
    };
    let second_visual_row = VisualDisplayRow {
        display_range: 5..text.len(),
        line_start_x: shaped_line.x_for_index(5),
        top: px(20.),
        height: px(20.),
    };
    let text_layout = DisplayRowTextLayout {
        fragments: vec![DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..text.len(),
            text,
            style: DisplayTextStyle::default(),
        })],
        visual_rows: vec![first_visual_row.clone(), second_visual_row],
        shaped_line,
        text_len: display_row.text.len(),
        cacheable: true,
    };

    let row_end_x = display_x_for_offset(
        &text_layout.fragments,
        &text_layout.shaped_line,
        first_visual_row.display_range.end,
    ) - first_visual_row.line_start_x;
    let (point, goal) = mouse_target_for_text_layout(
        &snapshot,
        &display_row,
        0,
        &first_visual_row,
        gutter_width() + row_end_x,
        gutter_width(),
        &text_layout,
    );

    assert_eq!(point, Point::new(0, 5));
    assert_eq!(goal, visual_horizontal_goal(0, row_end_x));
    assert_eq!(
        visual_row_index_for_caret(
            &text_layout.visual_rows,
            first_visual_row.display_range.end,
            text_layout.text_len,
            goal
        ),
        Some(0)
    );
}

#[gpui::test]
fn mouse_target_for_wrapped_row_uses_visual_row_local_x(cx: &mut gpui::TestAppContext) {
    let mut buffer = Buffer::local("abcdefghij\n");
    let snapshot = buffer.snapshot();
    let Some(display_row) = display_rows(&snapshot, 0..1).into_iter().next() else {
        panic!("expected display row");
    };
    let text = display_row.text.clone();
    let shaped_line = shape_test_line(cx, &text);
    let second_visual_row = VisualDisplayRow {
        display_range: 5..text.len(),
        line_start_x: shaped_line.x_for_index(5),
        top: px(20.),
        height: px(20.),
    };
    let text_layout = DisplayRowTextLayout {
        fragments: vec![DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..text.len(),
            text,
            style: DisplayTextStyle::default(),
        })],
        visual_rows: vec![second_visual_row.clone()],
        shaped_line,
        text_len: display_row.text.len(),
        cacheable: true,
    };
    let local_x = display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, 7)
        - second_visual_row.line_start_x;

    let (row_start_point, row_start_goal) = mouse_target_for_text_layout(
        &snapshot,
        &display_row,
        1,
        &second_visual_row,
        gutter_width(),
        gutter_width(),
        &text_layout,
    );
    let (middle_point, middle_goal) = mouse_target_for_text_layout(
        &snapshot,
        &display_row,
        1,
        &second_visual_row,
        gutter_width() + local_x,
        gutter_width(),
        &text_layout,
    );

    assert_eq!(row_start_point, Point::new(0, 5));
    assert_eq!(row_start_goal, visual_horizontal_goal(1, px(0.)));
    assert_eq!(middle_point, Point::new(0, 7));
    assert_eq!(middle_goal, visual_horizontal_goal(1, local_x));
}

#[gpui::test]
fn rendered_soft_break_caret_stays_inside_paragraph_item(cx: &mut gpui::TestAppContext) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(gpui::size(px(320.), px(200.)));
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text("first\nsecond\n\nnext\n", cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor.set_cursor(Point::new(1, 0));
        editor
    });

    editor.update_in(cx, |editor, window, cx| {
        let snapshot = editor.buffer.snapshot();
        let index = rendered_display_index_for_tests(&snapshot);
        let paragraph_item = index
            .item_index_for_source_row(1)
            .expect("soft-break row should map to paragraph item");
        assert_eq!(index.item_index_for_source_row(0), Some(paragraph_item));

        let display_row_state =
            DisplayRowProjectionState::new(&snapshot, Some(&editor.selection), editor.mode);
        let display_row = editor
            .cached_display_row(&snapshot, paragraph_item, editor.mode, &display_row_state)
            .expect("paragraph display row should exist");
        assert_eq!(display_row.source_row_range, 0..2);
        assert_eq!(display_row.text, "first\nsecond");
        assert!(display_row.contains_source_point(editor.cursor()));

        let row_style = row_display_style_for_display_row(&snapshot, &display_row, editor.mode);
        let selection = editor.selection.clone();
        let row_layout = editor.cached_row_layout(
            &snapshot,
            &display_row,
            &selection,
            editor.mode,
            row_style,
            text_wrap_width_for_mode(window, editor.mode),
            false,
            window,
            cx,
        );
        let DisplayRowLayout::Text(text_layout) = row_layout else {
            panic!("expected paragraph text layout");
        };
        assert_eq!(
            text_layout
                .visual_rows
                .iter()
                .map(|visual_row| visual_row.display_range.clone())
                .collect::<Vec<_>>(),
            vec![0.."first\n".len(), "first\n".len().."first\nsecond".len()]
        );

        assert!(
            caret_position_for_visual_row(
                snapshot.as_text_snapshot(),
                &display_row,
                &selection,
                &text_layout,
                0,
                &text_layout.visual_rows[0],
            )
            .is_none()
        );
        assert!(
            caret_position_for_visual_row(
                snapshot.as_text_snapshot(),
                &display_row,
                &selection,
                &text_layout,
                1,
                &text_layout.visual_rows[1],
            )
            .is_some()
        );
    });
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
    let rendered_rows = display_rows_in_mode(
        &snapshot,
        0..3,
        Some(&collapsed_selection(Point::new(2, 0))),
        MarkdownEditorMode::Rendered,
    );
    let source_row =
        display_rows_in_mode(&snapshot, 0..1, None, MarkdownEditorMode::Source).remove(0);

    assert_eq!(
        row_display_style_for_display_row(
            &snapshot,
            &rendered_rows[0],
            MarkdownEditorMode::Rendered
        ),
        md_theme::heading_row_metrics(1).into()
    );
    assert_eq!(
        row_display_style_for_display_row(
            &snapshot,
            &rendered_rows[1],
            MarkdownEditorMode::Rendered
        ),
        md_theme::heading_row_metrics(2).into()
    );
    assert_eq!(
        row_display_style_for_display_row(
            &snapshot,
            &rendered_rows[2],
            MarkdownEditorMode::Rendered
        ),
        md_theme::default_row_metrics().into()
    );
    assert_eq!(
        row_display_style_for_display_row(&snapshot, &source_row, MarkdownEditorMode::Source),
        md_theme::default_row_metrics().into()
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
        selected_range_for_row_in_text_snapshot(
            snapshot.as_text_snapshot(),
            &display_rows[0],
            &selection
        ),
        Some(2..4)
    );
    assert_eq!(
        selected_range_for_row_in_text_snapshot(
            snapshot.as_text_snapshot(),
            &display_rows[1],
            &selection
        ),
        Some(0..2)
    );
    assert_eq!(
        selected_range_for_row_in_text_snapshot(
            snapshot.as_text_snapshot(),
            &display_rows[2],
            &selection
        ),
        Some(0..1)
    );
}

#[test]
fn selected_empty_line_has_visible_selection_bounds() {
    let mut buffer = Buffer::local("abcd\n\npq");
    let snapshot = buffer.snapshot();
    let display_rows = display_rows(&snapshot, 0..3);
    let visual_row = VisualDisplayRow {
        display_range: 0..0,
        line_start_x: px(0.),
        top: px(0.),
        height: px(20.),
    };
    let text_layout = DisplayRowTextLayout {
        fragments: Vec::new(),
        visual_rows: vec![visual_row.clone()],
        shaped_line: gpui::ShapedLine::default(),
        text_len: 0,
        cacheable: true,
    };
    let crossing_selection = Selection {
        id: 1,
        start: Point::new(0, 2),
        end: Point::new(2, 1),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let ending_at_empty_line = Selection {
        id: 1,
        start: Point::new(0, 2),
        end: Point::new(1, 0),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let starting_at_empty_line = Selection {
        id: 1,
        start: Point::new(1, 0),
        end: Point::new(2, 1),
        reversed: false,
        goal: SelectionGoal::None,
    };

    for selection in [crossing_selection, starting_at_empty_line] {
        let selected_range = selected_range_for_row_in_text_snapshot(
            snapshot.as_text_snapshot(),
            &display_rows[1],
            &selection,
        );

        assert_eq!(selected_range, Some(0..0));
        assert_eq!(
            selection_bounds_for_visual_row(&text_layout, selected_range.as_ref(), &visual_row),
            Some((px(0.), px(1.)))
        );
    }

    assert_eq!(
        selected_range_for_row_in_text_snapshot(
            snapshot.as_text_snapshot(),
            &display_rows[1],
            &ending_at_empty_line
        ),
        None
    );
}

#[test]
fn selection_bounds_skip_non_empty_visual_row_boundary_touch() {
    let visual_row = VisualDisplayRow {
        display_range: 5..10,
        line_start_x: px(0.),
        top: px(0.),
        height: px(20.),
    };
    let text_layout = DisplayRowTextLayout {
        fragments: Vec::new(),
        visual_rows: vec![visual_row.clone()],
        shaped_line: gpui::ShapedLine::default(),
        text_len: 10,
        cacheable: true,
    };

    assert_eq!(
        selection_bounds_for_visual_row(&text_layout, Some(&(0..5)), &visual_row),
        None
    );
    assert_eq!(
        selection_bounds_for_visual_row(&text_layout, Some(&(10..12)), &visual_row),
        None
    );
}

#[gpui::test]
fn selection_bounds_are_relative_to_each_wrapped_visual_row(cx: &mut gpui::TestAppContext) {
    let text = "abcdefghijklmno".to_string();
    let shaped_line = shape_test_line(cx, &text);
    let fragments = vec![DisplayInlineFragment::Text(StyledDisplaySegment {
        display_range: 0..text.len(),
        text,
        style: DisplayTextStyle::default(),
    })];
    let visual_rows = vec![
        VisualDisplayRow {
            display_range: 0..5,
            line_start_x: px(0.),
            top: px(0.),
            height: px(20.),
        },
        VisualDisplayRow {
            display_range: 5..10,
            line_start_x: shaped_line.x_for_index(5),
            top: px(20.),
            height: px(20.),
        },
        VisualDisplayRow {
            display_range: 10..15,
            line_start_x: shaped_line.x_for_index(10),
            top: px(40.),
            height: px(20.),
        },
    ];
    let text_layout = DisplayRowTextLayout {
        fragments,
        visual_rows: visual_rows.clone(),
        shaped_line,
        text_len: 15,
        cacheable: true,
    };
    let selected_range = 2..13;

    assert_eq!(
        selection_bounds_for_visual_row(&text_layout, Some(&selected_range), &visual_rows[0]),
        Some((
            display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, 2),
            display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, 5)
                - display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, 2)
        ))
    );
    assert_eq!(
        selection_bounds_for_visual_row(&text_layout, Some(&selected_range), &visual_rows[1]),
        Some((
            px(0.),
            display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, 10)
                - display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, 5)
        ))
    );
    assert_eq!(
        selection_bounds_for_visual_row(&text_layout, Some(&selected_range), &visual_rows[2]),
        Some((
            px(0.),
            display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, 13)
                - display_x_for_offset(&text_layout.fragments, &text_layout.shaped_line, 10)
        ))
    );
}
