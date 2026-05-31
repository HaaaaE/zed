use super::test_support::*;

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
fn rendered_horizontal_movement_skips_inactive_inline_atom() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let snapshot = buffer.snapshot();
    let atom_start = "Before ".len();
    let atom_end = "Before $x + y$".len();

    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, atom_start as u32),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Right,
        ),
        Point::new(0, atom_end as u32)
    );
    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, atom_end as u32),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Left,
        ),
        Point::new(0, atom_start as u32)
    );
}

#[test]
fn rendered_element_boundary_queries_stay_local_to_cursor() {
    let mut buffer =
        Buffer::local("Before $x$ after\nplain text\n![alt](https://example.com/cat.png)\n");
    let snapshot = buffer.snapshot();
    let inline_atom_start = "Before ".len();
    let inline_atom_end = "Before $x$".len();
    let image_source_len = "![alt](https://example.com/cat.png)".len();

    assert_eq!(
        rendered_element_range_at_cursor(
            &snapshot,
            Point::new(0, inline_atom_start as u32),
            HorizontalDirection::Right,
        ),
        Some(inline_atom_start..inline_atom_end)
    );
    assert_eq!(
        rendered_element_range_at_cursor(
            &snapshot,
            Point::new(0, inline_atom_end as u32),
            HorizontalDirection::Left,
        ),
        Some(inline_atom_start..inline_atom_end)
    );
    assert_eq!(
        rendered_element_range_at_cursor(&snapshot, Point::new(1, 0), HorizontalDirection::Right,),
        None
    );
    assert!(source_offset_is_rendered_element_boundary(
        &snapshot,
        snapshot
            .as_text_snapshot()
            .point_to_offset(Point::new(2, image_source_len as u32))
    ));
}

#[test]
fn rendered_select_horizontal_extends_across_inactive_inline_atom() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let snapshot = buffer.snapshot();
    let atom_start = "Before ".len();
    let atom_end = "Before $x + y$".len();

    assert_eq!(
        select_right_in_mode(
            &snapshot,
            &collapsed_selection(Point::new(0, atom_start as u32)),
            MarkdownEditorMode::Rendered,
        ),
        Selection {
            id: 0,
            start: Point::new(0, atom_start as u32),
            end: Point::new(0, atom_end as u32),
            reversed: false,
            goal: SelectionGoal::None,
        }
    );
    assert_eq!(
        select_left_in_mode(
            &snapshot,
            &collapsed_selection(Point::new(0, atom_end as u32)),
            MarkdownEditorMode::Rendered,
        ),
        Selection {
            id: 0,
            start: Point::new(0, atom_start as u32),
            end: Point::new(0, atom_end as u32),
            reversed: true,
            goal: SelectionGoal::None,
        }
    );
}

#[test]
fn rendered_horizontal_movement_skips_inactive_replacement_at_right_boundary() {
    let source = "Escape \\* &amp;\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();
    let escape_start = source.find("\\*").expect("expected escaped marker");
    let escape_end = escape_start + "\\*".len();
    let entity_start = source.find("&amp;").expect("expected entity");
    let entity_end = entity_start + "&amp;".len();

    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, escape_end as u32),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Left,
        ),
        Point::new(0, escape_start as u32)
    );
    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, entity_end as u32),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Left,
        ),
        Point::new(0, entity_start as u32)
    );
}

#[test]
fn rendered_select_horizontal_extends_across_inactive_replacement_at_right_boundary() {
    let source = "Escape \\* &amp;\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();
    let escape_start = source.find("\\*").expect("expected escaped marker");
    let escape_end = escape_start + "\\*".len();

    assert_eq!(
        select_left_in_mode(
            &snapshot,
            &collapsed_selection(Point::new(0, escape_end as u32)),
            MarkdownEditorMode::Rendered,
        ),
        Selection {
            id: 0,
            start: Point::new(0, escape_start as u32),
            end: Point::new(0, escape_end as u32),
            reversed: true,
            goal: SelectionGoal::None,
        }
    );
}

#[test]
fn rendered_horizontal_movement_skips_inactive_image_block() {
    let image_source = "![alt](https://example.com/cat.png)";
    let mut buffer = Buffer::local(&format!("{image_source}\nnext\n"));
    let snapshot = buffer.snapshot();
    let image_end = image_source.len();

    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, 0),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Right,
        ),
        Point::new(0, image_end as u32)
    );
    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, image_end as u32),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Left,
        ),
        Point::new(0, 0)
    );
}

#[test]
fn rendered_horizontal_movement_skips_inactive_formula_block() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\nnext\n"));
    let snapshot = buffer.snapshot();
    let formula_end = formula_source.len();

    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, 0),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Right,
        ),
        Point::new(0, formula_end as u32)
    );
    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, formula_end as u32),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Left,
        ),
        Point::new(0, 0)
    );
}

#[test]
fn rendered_select_horizontal_extends_across_inactive_image_block() {
    let image_source = "![alt](https://example.com/cat.png)";
    let mut buffer = Buffer::local(&format!("{image_source}\nnext\n"));
    let snapshot = buffer.snapshot();
    let image_end = image_source.len();

    assert_eq!(
        select_right_in_mode(
            &snapshot,
            &collapsed_selection(Point::new(0, 0)),
            MarkdownEditorMode::Rendered,
        ),
        Selection {
            id: 0,
            start: Point::new(0, 0),
            end: Point::new(0, image_end as u32),
            reversed: false,
            goal: SelectionGoal::None,
        }
    );
    assert_eq!(
        select_left_in_mode(
            &snapshot,
            &collapsed_selection(Point::new(0, image_end as u32)),
            MarkdownEditorMode::Rendered,
        ),
        Selection {
            id: 0,
            start: Point::new(0, 0),
            end: Point::new(0, image_end as u32),
            reversed: true,
            goal: SelectionGoal::None,
        }
    );
}

#[test]
fn rendered_select_horizontal_extends_across_inactive_formula_block() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\nnext\n"));
    let snapshot = buffer.snapshot();
    let formula_end = formula_source.len();

    assert_eq!(
        select_right_in_mode(
            &snapshot,
            &collapsed_selection(Point::new(0, 0)),
            MarkdownEditorMode::Rendered,
        ),
        Selection {
            id: 0,
            start: Point::new(0, 0),
            end: Point::new(0, formula_end as u32),
            reversed: false,
            goal: SelectionGoal::None,
        }
    );
    assert_eq!(
        select_left_in_mode(
            &snapshot,
            &collapsed_selection(Point::new(0, formula_end as u32)),
            MarkdownEditorMode::Rendered,
        ),
        Selection {
            id: 0,
            start: Point::new(0, 0),
            end: Point::new(0, formula_end as u32),
            reversed: true,
            goal: SelectionGoal::None,
        }
    );
}

#[test]
fn rendered_horizontal_movement_keeps_active_inline_atom_character_movement() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let snapshot = buffer.snapshot();
    let atom_content_start = "Before $".len();

    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, atom_content_start as u32),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Right,
        ),
        Point::new(0, atom_content_start as u32 + 1)
    );
    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, atom_content_start as u32 + 1),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Left,
        ),
        Point::new(0, atom_content_start as u32)
    );
}

#[test]
fn rendered_horizontal_movement_skips_inactive_inline_image() {
    let mut buffer = Buffer::local("before ![alt](https://example.com/cat.png) after\n");
    let snapshot = buffer.snapshot();
    let image_start = "before ".len();
    let image_end = "before ![alt](https://example.com/cat.png)".len();

    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, image_start as u32),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Right,
        ),
        Point::new(0, image_end as u32)
    );
    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, image_end as u32),
            MarkdownEditorMode::Rendered,
            HorizontalDirection::Left,
        ),
        Point::new(0, image_start as u32)
    );
}

#[test]
fn source_horizontal_movement_keeps_inline_atom_source_editable() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let snapshot = buffer.snapshot();
    let atom_start = "Before ".len();

    assert_eq!(
        move_horizontal_in_mode(
            &snapshot,
            Point::new(0, atom_start as u32),
            MarkdownEditorMode::Source,
            HorizontalDirection::Right,
        ),
        Point::new(0, atom_start as u32 + 1)
    );
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
fn visual_row_index_containing_caret_prefers_empty_trailing_break_row() {
    let visual_rows = vec![
        VisualDisplayRow {
            display_range: 0..2,
            line_start_x: px(0.),
            top: px(0.),
            height: px(20.),
        },
        VisualDisplayRow {
            display_range: 2..2,
            line_start_x: px(24.),
            top: px(20.),
            height: px(20.),
        },
    ];

    assert_eq!(
        visual_row_index_containing_caret(&visual_rows, 2, 2),
        Some(1)
    );
}

#[test]
fn visual_row_index_for_caret_uses_wrapped_goal_at_boundary() {
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
        visual_row_index_for_caret(&visual_rows, 5, 10, SelectionGoal::None),
        Some(1)
    );
    assert_eq!(
        visual_row_index_for_caret(
            &visual_rows,
            5,
            10,
            SelectionGoal::WrappedHorizontalPosition((0, 48.))
        ),
        Some(0)
    );
    assert_eq!(
        visual_row_index_for_caret(
            &visual_rows,
            5,
            10,
            SelectionGoal::WrappedHorizontalPosition((1, 0.))
        ),
        Some(1)
    );
}

#[test]
fn visual_line_boundary_for_caret_uses_current_visual_row() {
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
        visual_line_boundary_for_caret(
            &visual_rows,
            2,
            10,
            SelectionGoal::None,
            VisualLineBoundary::Start
        ),
        Some((0, 0))
    );
    assert_eq!(
        visual_line_boundary_for_caret(
            &visual_rows,
            2,
            10,
            SelectionGoal::None,
            VisualLineBoundary::End
        ),
        Some((0, 5))
    );
    assert_eq!(
        visual_line_boundary_for_caret(
            &visual_rows,
            5,
            10,
            SelectionGoal::WrappedHorizontalPosition((0, 48.)),
            VisualLineBoundary::End
        ),
        Some((0, 5))
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
