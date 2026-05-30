use super::test_support::*;

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
fn display_rows_cache_source_text_and_range() {
    let mut buffer = Buffer::local("first\nsecond\nthird");
    let snapshot = buffer.snapshot();

    let row = display_rows(&snapshot, 1..2).remove(0);

    assert_eq!(row.text, "second");
    assert_eq!(row.source_text, "second");
    assert_eq!(row.source_range, "first\n".len().."first\nsecond".len());
}

#[test]
fn rendered_display_index_groups_paragraphs_and_keeps_structured_rows_addressable() {
    let mut buffer = Buffer::local(
        "first paragraph\ncontinued\n\n| a | b |\n| - | - |\n| 1 | 2 |\n\n```rust\nlet x = 1;\n```\n[label]: https://example.com\n<div>raw</div>\n",
    );
    let snapshot = buffer.snapshot();
    let index = rendered_display_index_for_tests(&snapshot);

    let items = (0..index.item_count())
        .map(|ix| index.item(ix).expect("item should exist"))
        .map(|item| (item.index, item.row_range.clone(), item.kind))
        .collect::<Vec<_>>();

    assert_eq!(
        items,
        vec![
            (0, 0..2, rendered_index::RenderedDisplayItemKind::Paragraph),
            (1, 2..3, rendered_index::RenderedDisplayItemKind::SourceRow),
            (
                2,
                3..4,
                rendered_index::RenderedDisplayItemKind::PipeTableRow
            ),
            (
                3,
                4..5,
                rendered_index::RenderedDisplayItemKind::PipeTableRow
            ),
            (
                4,
                5..6,
                rendered_index::RenderedDisplayItemKind::PipeTableRow
            ),
            (5, 6..7, rendered_index::RenderedDisplayItemKind::SourceRow),
            (
                6,
                7..8,
                rendered_index::RenderedDisplayItemKind::FencedCodeBlock
            ),
            (
                7,
                8..9,
                rendered_index::RenderedDisplayItemKind::FencedCodeBlock
            ),
            (
                8,
                9..10,
                rendered_index::RenderedDisplayItemKind::FencedCodeBlock
            ),
            (
                9,
                10..11,
                rendered_index::RenderedDisplayItemKind::LinkReferenceDefinition
            ),
            (
                10,
                11..12,
                rendered_index::RenderedDisplayItemKind::HtmlBlock
            ),
            (
                11,
                12..13,
                rendered_index::RenderedDisplayItemKind::SourceRow
            ),
        ]
    );

    assert_eq!(index.item_index_for_source_row(0), Some(0));
    assert_eq!(index.item_index_for_source_row(1), Some(0));
    assert_eq!(index.item_index_for_source_row(4), Some(3));
    assert_eq!(index.item_index_for_source_row(8), Some(7));
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
fn rendered_display_rows_hide_inactive_setext_heading_marker() {
    let mut buffer = Buffer::local("Title\n=====\nBody\n");
    let snapshot = buffer.snapshot();

    let rows = display_rows_in_mode(
        &snapshot,
        0..3,
        Some(&collapsed_selection(Point::new(2, 0))),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(rows[0].text, "Title");
    assert_eq!(rows[0].heading_level, Some(1));
    assert_eq!(rows[1].text, "");
    assert_eq!(rows[2].text, "Body");
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
fn rendered_display_rows_replace_inactive_escapes_and_entities() {
    let source = "Escape \\* &amp; end\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();
    let escaped = source.find("\\*").expect("expected escape");
    let entity = source.find("&amp;").expect("expected entity");

    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, 0))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "Escape * & end");
    assert_eq!(row.source_to_display(escaped), "Escape ".len());
    assert_eq!(row.display_to_source("Escape ".len()), escaped);
    assert_eq!(row.source_to_display(entity), "Escape * ".len());
    assert_eq!(row.display_to_source("Escape * ".len()), entity);
}

#[test]
fn rendered_display_rows_replace_full_html5_named_entities() {
    let source = "Entities &CounterClockwiseContourIntegral; &Aopf; &NotEqualTilde;\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();

    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, 0))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "Entities \u{2233} \u{1D538} \u{2242}\u{0338}");
}

#[test]
fn rendered_display_rows_keep_tagfilter_disallowed_raw_html_as_text() {
    let source = "<script>alert(1)</script>\nInline <iframe src=\"x\"></iframe>\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();

    let rows = display_rows_in_mode(&snapshot, 0..2, None, MarkdownEditorMode::Rendered);

    assert_eq!(rows[0].text, "<script>alert(1)</script>");
    assert_eq!(rows[1].text, "Inline <iframe src=\"x\"></iframe>");
}

#[test]
fn rendered_display_rows_reveal_active_escape_and_entity_source() {
    let source = "Escape \\* &amp; end\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();
    let escaped = source.find("\\*").expect("expected escape");
    let entity = source.find("&amp;").expect("expected entity");

    let escape_row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, escaped as u32))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);
    let entity_row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, entity as u32))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(escape_row.text, "Escape \\* & end");
    assert_eq!(entity_row.text, "Escape * &amp; end");
}

#[test]
fn rendered_display_rows_replace_inactive_task_list_markers() {
    let source = "- [ ] todo\n- [x] done\n\nbody\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();

    let rows = display_rows_in_mode(
        &snapshot,
        0..2,
        Some(&collapsed_selection(Point::new(3, 0))),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(rows[0].text, "\u{2610} todo");
    assert_eq!(rows[1].text, "\u{2611} done");
}

#[test]
fn rendered_display_rows_reveal_active_task_list_marker_source() {
    let source = "- [ ] todo\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();
    let marker = source.find("[ ]").expect("expected task marker");

    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, marker as u32))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "- [ ] todo");
    assert_eq!(row.source_to_display(marker), marker);
    assert_eq!(row.display_to_source(marker), marker);
}

#[test]
fn rendered_display_rows_hide_inactive_blockquote_and_list_markers() {
    let source = "> quote\n- item\n1) ordered\n\nbody\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();

    let rows = display_rows_in_mode(
        &snapshot,
        0..3,
        Some(&collapsed_selection(Point::new(4, 0))),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(rows[0].text, "quote");
    assert_eq!(rows[1].text, "item");
    assert_eq!(rows[2].text, "ordered");
    assert_eq!(rows[0].rendered_indent_level, 1);
    assert_eq!(rows[1].rendered_indent_level, 1);
    assert_eq!(rows[2].rendered_indent_level, 1);
}

#[test]
fn rendered_display_rows_reveal_active_blockquote_and_list_markers() {
    let source = "> quote\n- item\n\nbody\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();

    let quote_rows = display_rows_in_mode(
        &snapshot,
        0..2,
        Some(&collapsed_selection(Point::new(0, 3))),
        MarkdownEditorMode::Rendered,
    );
    let list_rows = display_rows_in_mode(
        &snapshot,
        0..2,
        Some(&collapsed_selection(Point::new(1, 2))),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(quote_rows[0].text, "> quote");
    assert_eq!(quote_rows[1].text, "item");
    assert_eq!(list_rows[0].text, "quote");
    assert_eq!(list_rows[1].text, "- item");
}

#[test]
fn rendered_display_rows_track_nested_blockquote_indent() {
    let source = "> quote\n> > nested\n\nbody\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();

    let rows = display_rows_in_mode(
        &snapshot,
        0..4,
        Some(&collapsed_selection(Point::new(3, 0))),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
        vec!["quote", "nested", "", "body"]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.rendered_indent_level)
            .collect::<Vec<_>>(),
        vec![1, 2, 0, 0]
    );
    assert_eq!(rows[0].rendered_indent_width(), px(24.));
    assert_eq!(rows[1].rendered_indent_width(), px(48.));
    assert_eq!(rows[2].rendered_indent_width(), px(0.));
    assert_eq!(rows[3].rendered_indent_width(), px(0.));
}

#[test]
fn rendered_display_rows_track_nested_list_indent() {
    let source = "- outer\n  - nested\n\nbody\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();

    let rows = display_rows_in_mode(
        &snapshot,
        0..4,
        Some(&collapsed_selection(Point::new(3, 0))),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
        vec!["outer", "nested", "", "body"]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.rendered_indent_level)
            .collect::<Vec<_>>(),
        vec![1, 2, 1, 0]
    );
    assert_eq!(rows[0].rendered_indent_width(), px(24.));
    assert_eq!(rows[1].rendered_indent_width(), px(48.));
    assert_eq!(rows[2].rendered_indent_width(), px(24.));
    assert_eq!(rows[3].rendered_indent_width(), px(0.));
}

#[test]
fn rendered_display_rows_track_task_list_indent() {
    let source = "body\n> - [ ] quoted\n- outer\n  - [x] nested\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();

    let rows = display_rows_in_mode(
        &snapshot,
        1..4,
        Some(&collapsed_selection(Point::new(0, 0))),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(
        rows.iter().map(|row| row.text.as_str()).collect::<Vec<_>>(),
        vec!["\u{2610} quoted", "outer", "\u{2611} nested"]
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.rendered_indent_level)
            .collect::<Vec<_>>(),
        vec![2, 1, 2]
    );
    assert_eq!(rows[0].rendered_indent_width(), px(48.));
    assert_eq!(rows[1].rendered_indent_width(), px(24.));
    assert_eq!(rows[2].rendered_indent_width(), px(48.));
}

#[test]
fn rendered_display_rows_keep_inline_atom_boundaries_inactive() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let snapshot = buffer.snapshot();
    let atom_start = "Before ".len();
    let atom_end = "Before $x + y$".len();

    for cursor in [atom_start, atom_end] {
        let rows = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&collapsed_selection(Point::new(0, cursor as u32))),
            MarkdownEditorMode::Rendered,
        );

        assert_eq!(rows[0].text, "Before x + y after");
    }
}

#[test]
fn rendered_display_rows_reveal_inline_atom_when_cursor_enters_content() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let snapshot = buffer.snapshot();
    let content_start = "Before $".len();

    let rows = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, content_start as u32))),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(rows[0].text, "Before $x + y$ after");
}

#[test]
fn rendered_display_rows_keep_whole_inline_atom_inactive_when_selected() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let snapshot = buffer.snapshot();
    let atom_start = "Before ".len();
    let atom_end = "Before $x + y$".len();
    let selection = Selection {
        id: 0,
        start: Point::new(0, atom_start as u32),
        end: Point::new(0, atom_end as u32),
        reversed: false,
        goal: SelectionGoal::None,
    };

    let rows = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(rows[0].text, "Before x + y after");
    assert_eq!(
        selected_range_for_row_in_text_snapshot(snapshot.as_text_snapshot(), &rows[0], &selection),
        Some(7..12)
    );
}

#[test]
fn rendered_display_rows_keep_contained_inline_atom_inactive_when_selected() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let snapshot = buffer.snapshot();
    let selection_end = "Before $x + y$ after".len();
    let selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(0, selection_end as u32),
        reversed: false,
        goal: SelectionGoal::None,
    };

    let rows = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(rows[0].text, "Before x + y after");
    assert_eq!(
        selected_range_for_row_in_text_snapshot(snapshot.as_text_snapshot(), &rows[0], &selection),
        Some(0.."Before x + y after".len())
    );

    let row_style =
        row_display_style_for_display_row(&snapshot, &rows[0], MarkdownEditorMode::Rendered);
    let fragments = display_inline_fragments(
        &snapshot,
        &rows[0],
        MarkdownEditorMode::Rendered,
        row_style,
        None,
    );
    let atom = fragments.iter().find_map(|fragment| match fragment {
        DisplayInlineFragment::Atom(atom) => Some(atom),
        DisplayInlineFragment::Text(_) => None,
    });
    let selected_range = 0.."Before x + y after".len();

    assert!(atom.is_some_and(|atom| {
        atom.display_range == (7..12) && atom.is_selected(Some(&selected_range))
    }));
}

#[test]
fn rendered_display_rows_reveal_partially_selected_inline_atom() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
    let snapshot = buffer.snapshot();
    let content_start = "Before $".len();
    let selection = Selection {
        id: 0,
        start: Point::new(0, content_start as u32),
        end: Point::new(0, content_start as u32 + 1),
        reversed: false,
        goal: SelectionGoal::None,
    };

    let rows = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    );

    assert_eq!(rows[0].text, "Before $x + y$ after");
}

#[test]
fn rendered_inline_row_inputs_collect_styles_and_atoms_together() {
    let source = "Before **bold** and $x$ after\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, source.len() as u32))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    let row_style =
        row_display_style_for_display_row(&snapshot, &row, MarkdownEditorMode::Rendered);
    let inputs = display_inline_row_inputs(&snapshot, &row, row_style, None);
    let bold_start = source.find("bold").expect("expected bold content");
    let bold_range = bold_start..bold_start + "bold".len();
    let math_start = source.find("$x$").expect("expected inline math");
    let math_source_range = math_start..math_start + "$x$".len();

    assert!(inputs.style_ranges.iter().any(|(range, style)| {
        range == &bold_range && style.font_weight == Some(FontWeight::BOLD)
    }));
    assert!(inputs.atom_ranges.iter().any(|atom| {
        atom.kind() == DisplayInlineAtomKind::InlineMath
            && atom.source_range == math_source_range
            && atom.fallback_text == "x"
    }));
}

#[test]
fn source_fragments_use_plain_text_fast_path() {
    let mut buffer = Buffer::local("Before **bold** and $x$\n");
    let snapshot = buffer.snapshot();
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, 0))),
        MarkdownEditorMode::Source,
    )
    .remove(0);

    let row_style = row_display_style_for_display_row(&snapshot, &row, MarkdownEditorMode::Source);
    let fragments = display_fragments_for_text_layout(
        &snapshot,
        &row,
        MarkdownEditorMode::Source,
        row_style,
        None,
    );

    assert_eq!(
        fragments,
        vec![DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..row.text.len(),
            text: String::new(),
            style: DisplayTextStyle::default(),
        })]
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

    let row_style =
        row_display_style_for_display_row(&snapshot, &row, MarkdownEditorMode::Rendered);
    let fragments = display_inline_fragments(
        &snapshot,
        &row,
        MarkdownEditorMode::Rendered,
        row_style,
        None,
    );
    let atom = fragments
        .iter()
        .find_map(|fragment| match fragment {
            DisplayInlineFragment::Atom(atom) => Some(atom),
            DisplayInlineFragment::Text(_) => None,
        })
        .expect("expected inline math atom fragment");

    assert_eq!(atom.kind(), DisplayInlineAtomKind::InlineMath);
    assert_eq!(atom.fallback_text, "x + y");
    assert_eq!(atom.display_range, 7..12);
    assert_eq!(
        atom.height,
        row_style.line_height + INLINE_MATH_ATOM_EXTRA_HEIGHT
    );
    assert_eq!(
        text_segments_for_fragments(&row.text, &fragments)
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Before ", "x + y", " after"]
    );
}

#[test]
fn rendered_inline_fragments_create_inline_image_atom() {
    let mut buffer = Buffer::local("Before ![alt](https://example.com/cat.png) after\n");
    let snapshot = buffer.snapshot();
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, 0))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "Before alt after");

    let row_style =
        row_display_style_for_display_row(&snapshot, &row, MarkdownEditorMode::Rendered);
    let fragments = display_inline_fragments(
        &snapshot,
        &row,
        MarkdownEditorMode::Rendered,
        row_style,
        None,
    );
    let atom = fragments
        .iter()
        .find_map(|fragment| match fragment {
            DisplayInlineFragment::Atom(atom) => Some(atom),
            DisplayInlineFragment::Text(_) => None,
        })
        .expect("expected inline image atom fragment");

    assert_eq!(atom.kind(), DisplayInlineAtomKind::InlineImage);
    assert_eq!(atom.fallback_text, "alt");
    assert_eq!(
        atom.image_source
            .as_ref()
            .map(|source| source.raw_destination()),
        Some("https://example.com/cat.png")
    );
    assert_eq!(atom.display_range, 7..10);
    assert_eq!(atom.height, INLINE_IMAGE_ATOM_SIZE);
    assert_eq!(
        text_segments_for_fragments(&row.text, &fragments)
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>(),
        vec!["Before ", "alt", " after"]
    );
}

#[test]
fn rendered_inline_fragments_create_empty_alt_inline_image_atom() {
    let mut buffer = Buffer::local("Before ![](https://example.com/cat.png) after\n");
    let snapshot = buffer.snapshot();
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&collapsed_selection(Point::new(0, 0))),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);
    let expected_text = format!("Before {INLINE_IMAGE_PLACEHOLDER} after");

    assert_eq!(row.text, expected_text);

    let row_style =
        row_display_style_for_display_row(&snapshot, &row, MarkdownEditorMode::Rendered);
    let fragments = display_inline_fragments(
        &snapshot,
        &row,
        MarkdownEditorMode::Rendered,
        row_style,
        None,
    );
    let atom = fragments
        .iter()
        .find_map(|fragment| match fragment {
            DisplayInlineFragment::Atom(atom) => Some(atom),
            DisplayInlineFragment::Text(_) => None,
        })
        .expect("expected empty-alt inline image atom fragment");

    assert_eq!(atom.kind(), DisplayInlineAtomKind::InlineImage);
    assert_eq!(atom.fallback_text, INLINE_IMAGE_PLACEHOLDER);
    assert_eq!(
        atom.image_source
            .as_ref()
            .map(|source| source.raw_destination()),
        Some("https://example.com/cat.png")
    );
    assert_eq!(atom.display_range, 7..7 + INLINE_IMAGE_PLACEHOLDER.len());
    assert_eq!(atom.height, INLINE_IMAGE_ATOM_SIZE);
    assert_eq!(
        text_segments_for_fragments(&row.text, &fragments)
            .iter()
            .map(|segment| segment.text.clone())
            .collect::<Vec<_>>(),
        vec![
            "Before ".to_string(),
            INLINE_IMAGE_PLACEHOLDER.to_string(),
            " after".to_string(),
        ]
    );
}

#[test]
fn inline_atom_measurement_key_tracks_descriptor_content_and_row_style() {
    let row_style: RowDisplayStyle = md_theme::default_row_metrics().into();
    let heading_style: RowDisplayStyle = md_theme::heading_row_metrics(1).into();
    let image = DisplayInlineAtom {
        descriptor: image_descriptor(
            0..30,
            "https://example.com/cat.png",
            "cat",
            RenderedElementPlacement::Inline,
        ),
        source_range: 0..30,
        display_range: 0..3,
        fallback_text: "cat".to_string(),
        image_source: Some(markdown_image_source("https://example.com/cat.png", "")),
        style: inline_style(MarkdownInlineKind::Image),
        height: INLINE_IMAGE_ATOM_SIZE,
        width: px(0.),
    };
    let same_image = image.measurement_key(row_style, Some(px(300.)));
    let different_url = DisplayInlineAtom {
        descriptor: image_descriptor(
            0..30,
            "https://example.com/dog.png",
            "cat",
            RenderedElementPlacement::Inline,
        ),
        image_source: Some(markdown_image_source("https://example.com/dog.png", "")),
        ..image.clone()
    }
    .measurement_key(row_style, Some(px(300.)));
    let different_style = image.measurement_key(heading_style, Some(px(300.)));
    let different_image_width = image.measurement_key(row_style, Some(px(200.)));
    let math = DisplayInlineAtom {
        descriptor: math_descriptor(0..3, "x"),
        source_range: 0..3,
        display_range: 0..1,
        fallback_text: "x".to_string(),
        image_source: None,
        style: inline_style(MarkdownInlineKind::InlineMath),
        height: row_style.line_height + INLINE_MATH_ATOM_EXTRA_HEIGHT,
        width: px(0.),
    };
    let different_math = DisplayInlineAtom {
        descriptor: math_descriptor(0..3, "y"),
        fallback_text: "y".to_string(),
        ..math.clone()
    }
    .measurement_key(row_style, None);

    assert_eq!(same_image, image.measurement_key(row_style, Some(px(300.))));
    assert_ne!(same_image, different_url);
    assert_ne!(same_image, different_style);
    assert_ne!(same_image, different_image_width);
    assert_ne!(math.measurement_key(row_style, None), different_math);
    assert_ne!(
        math.measurement_key_with_scale(row_style, 1., None),
        math.measurement_key_with_scale(row_style, 2., None)
    );
}

#[test]
fn inline_atom_display_boundaries_map_to_source_boundaries() {
    let mut buffer = Buffer::local("Before $x + y$ after\n");
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

    assert_eq!(source_offset_for_display_offset(&row, &fragments, 7), 7);
    assert_eq!(source_offset_for_display_offset(&row, &fragments, 12), 14);
    assert_eq!(source_offset_for_display_offset(&row, &fragments, 0), 0);
}

#[test]
fn empty_alt_inline_image_display_boundaries_map_to_source_boundaries() {
    let mut buffer = Buffer::local("Before ![](https://example.com/cat.png) after\n");
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
    let display_start = "Before ".len();
    let display_end = display_start + INLINE_IMAGE_PLACEHOLDER.len();
    let source_start = "Before ".len();
    let source_end = "Before ![](https://example.com/cat.png)".len();

    assert_eq!(
        source_offset_for_display_offset(&row, &fragments, display_start),
        source_start
    );
    assert_eq!(
        source_offset_for_display_offset(&row, &fragments, display_end),
        source_end
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
            descriptor: math_descriptor(8..15, "x + y"),
            source_range: 8..15,
            display_range: 7..12,
            fallback_text: "x + y".to_string(),
            image_source: None,
            style: inline_style(MarkdownInlineKind::InlineMath),
            height: atom_height,
            width: px(50.),
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
fn inline_atom_selected_state_requires_full_display_range() {
    let row_style: RowDisplayStyle = md_theme::default_row_metrics().into();
    let atom = DisplayInlineAtom {
        descriptor: math_descriptor(8..15, "x + y"),
        source_range: 8..15,
        display_range: 7..12,
        fallback_text: "x + y".to_string(),
        image_source: None,
        style: inline_style(MarkdownInlineKind::InlineMath),
        height: row_style.line_height + INLINE_MATH_ATOM_EXTRA_HEIGHT,
        width: px(50.),
    };

    assert!(atom.is_selected(Some(&(7..12))));
    assert!(atom.is_selected(Some(&(0..18))));
    assert!(!atom.is_selected(Some(&(7..11))));
    assert!(!atom.is_selected(Some(&(8..12))));
    assert!(!atom.is_selected(None));
}

#[test]
fn inline_atom_width_includes_horizontal_padding() {
    assert_eq!(
        DisplayInlineAtomKind::InlineMath.width_for_content(px(30.)),
        px(30.) + INLINE_MATH_ATOM_HORIZONTAL_PADDING * 2.
    );
    assert_eq!(
        DisplayInlineAtomKind::InlineImage.width_for_content(px(30.)),
        INLINE_IMAGE_ATOM_SIZE
    );
}

#[test]
fn inline_image_atom_size_uses_natural_size_when_it_fits() {
    assert_eq!(
        inline_image_atom_size_for_size(80, 40, px(300.)),
        Some(gpui::size(px(80.), px(40.)))
    );
}

#[test]
fn inline_image_atom_size_scales_wide_image_to_max_width() {
    assert_eq!(
        inline_image_atom_size_for_size(1200, 800, px(600.)),
        Some(gpui::size(px(600.), px(400.)))
    );
}

#[test]
fn inline_image_atom_size_preserves_tall_image_aspect_ratio() {
    assert_eq!(
        inline_image_atom_size_for_size(800, 1200, px(300.)),
        Some(gpui::size(px(300.), px(450.)))
    );
    assert_eq!(
        inline_image_atom_size_for_size(40, 80, px(300.)),
        Some(gpui::size(px(40.), px(80.)))
    );
}

#[test]
fn inline_image_atom_size_rejects_invalid_image_dimensions() {
    assert_eq!(inline_image_atom_size_for_size(0, 40, px(300.)), None);
    assert_eq!(inline_image_atom_size_for_size(80, 0, px(300.)), None);
    assert_eq!(inline_image_atom_size_for_size(0, 0, px(300.)), None);
}

#[test]
fn line_fragments_for_wrapping_uses_inline_atom_element_width() {
    let fragments = vec![
        DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..7,
            text: "Before ".to_string(),
            style: DisplayTextStyle::default(),
        }),
        DisplayInlineFragment::Atom(DisplayInlineAtom {
            descriptor: math_descriptor(8..15, "x + y"),
            source_range: 8..15,
            display_range: 7..12,
            fallback_text: "x + y".to_string(),
            image_source: None,
            style: inline_style(MarkdownInlineKind::InlineMath),
            height: px(24.),
            width: px(42.),
        }),
        DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 12..18,
            text: " after".to_string(),
            style: DisplayTextStyle::default(),
        }),
    ];
    let Some(line_fragments) = line_fragments_for_wrapping("Before x + y after", &fragments) else {
        panic!("expected valid line fragments");
    };

    assert_eq!(line_fragments.len(), 3);
    assert!(matches!(
        &line_fragments[0],
        LineFragment::Text { text } if *text == "Before "
    ));
    assert!(matches!(
        &line_fragments[1],
        LineFragment::Element { width, len_utf8 }
            if *width == px(42.) && *len_utf8 == "x + y".len()
    ));
    assert!(matches!(
        &line_fragments[2],
        LineFragment::Text { text } if *text == " after"
    ));
}

#[test]
fn line_fragments_for_wrapping_uses_inline_image_atom_size() {
    let fragments = vec![
        DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..7,
            text: "Before ".to_string(),
            style: DisplayTextStyle::default(),
        }),
        DisplayInlineFragment::Atom(DisplayInlineAtom {
            descriptor: image_descriptor(
                7..42,
                "https://example.com/cat.png",
                "alt",
                RenderedElementPlacement::Inline,
            ),
            source_range: 7..42,
            display_range: 7..10,
            fallback_text: "alt".to_string(),
            image_source: Some(markdown_image_source("https://example.com/cat.png", "")),
            style: inline_style(MarkdownInlineKind::Image),
            height: INLINE_IMAGE_ATOM_SIZE,
            width: INLINE_IMAGE_ATOM_SIZE,
        }),
        DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 10..16,
            text: " after".to_string(),
            style: DisplayTextStyle::default(),
        }),
    ];
    let Some(line_fragments) = line_fragments_for_wrapping("Before alt after", &fragments) else {
        panic!("expected valid line fragments");
    };

    assert_eq!(line_fragments.len(), 3);
    assert!(matches!(
        &line_fragments[1],
        LineFragment::Element { width, len_utf8 }
            if *width == INLINE_IMAGE_ATOM_SIZE && *len_utf8 == "alt".len()
    ));
    assert_eq!(
        visual_row_height_for_range(&fragments, &(0..16), md_theme::default_row_metrics().into()),
        INLINE_IMAGE_ATOM_SIZE
    );
}

#[test]
fn line_fragments_for_wrapping_uses_empty_alt_inline_image_atom_size() {
    let placeholder_end = 7 + INLINE_IMAGE_PLACEHOLDER.len();
    let display_text = format!("Before {INLINE_IMAGE_PLACEHOLDER} after");
    let fragments = vec![
        DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: 0..7,
            text: "Before ".to_string(),
            style: DisplayTextStyle::default(),
        }),
        DisplayInlineFragment::Atom(DisplayInlineAtom {
            descriptor: image_descriptor(
                7..39,
                "https://example.com/cat.png",
                "",
                RenderedElementPlacement::Inline,
            ),
            source_range: 7..39,
            display_range: 7..placeholder_end,
            fallback_text: INLINE_IMAGE_PLACEHOLDER.to_string(),
            image_source: Some(markdown_image_source("https://example.com/cat.png", "")),
            style: inline_style(MarkdownInlineKind::Image),
            height: INLINE_IMAGE_ATOM_SIZE,
            width: INLINE_IMAGE_ATOM_SIZE,
        }),
        DisplayInlineFragment::Text(StyledDisplaySegment {
            display_range: placeholder_end..placeholder_end + " after".len(),
            text: " after".to_string(),
            style: DisplayTextStyle::default(),
        }),
    ];
    let Some(line_fragments) = line_fragments_for_wrapping(&display_text, &fragments) else {
        panic!("expected valid line fragments");
    };

    assert_eq!(line_fragments.len(), 3);
    assert!(matches!(
        &line_fragments[1],
        LineFragment::Element { width, len_utf8 }
            if *width == INLINE_IMAGE_ATOM_SIZE
                && *len_utf8 == INLINE_IMAGE_PLACEHOLDER.len()
    ));
}

#[test]
fn line_fragments_for_wrapping_rejects_invalid_text_range() {
    let fragments = vec![DisplayInlineFragment::Text(StyledDisplaySegment {
        display_range: 0..10,
        text: "short".to_string(),
        style: DisplayTextStyle::default(),
    })];

    assert!(line_fragments_for_wrapping("short", &fragments).is_none());
}

#[test]
fn unwrapped_visual_rows_if_fits_skips_wrap_shaping_for_fitting_text() {
    let row_style: RowDisplayStyle = md_theme::default_row_metrics().into();
    let fragments = vec![DisplayInlineFragment::Text(StyledDisplaySegment {
        display_range: 0..5,
        text: "short".to_string(),
        style: DisplayTextStyle::default(),
    })];

    assert_eq!(
        unwrapped_visual_rows_if_fits(5, &fragments, row_style, px(80.), px(80.)),
        Some(vec![VisualDisplayRow {
            display_range: 0..5,
            line_start_x: px(0.),
            top: px(0.),
            height: row_style.line_height,
        }])
    );
    assert_eq!(
        unwrapped_visual_rows_if_fits(5, &fragments, row_style, px(81.), px(80.)),
        None
    );
}

#[test]
fn atomic_wrap_boundary_keeps_inline_atom_on_one_visual_row() {
    let fragments = vec![DisplayInlineFragment::Atom(DisplayInlineAtom {
        descriptor: math_descriptor(8..15, "x + y"),
        source_range: 8..15,
        display_range: 7..12,
        fallback_text: "x + y".to_string(),
        image_source: None,
        style: inline_style(MarkdownInlineKind::InlineMath),
        height: px(24.),
        width: px(50.),
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
fn inline_atom_x_position_snaps_to_nearest_boundary() {
    let atom = DisplayInlineAtom {
        descriptor: math_descriptor(8..15, "x + y"),
        source_range: 8..15,
        display_range: 7..12,
        fallback_text: "x + y".to_string(),
        image_source: None,
        style: inline_style(MarkdownInlineKind::InlineMath),
        height: px(24.),
        width: px(50.),
    };

    assert_eq!(atom.boundary_for_x(px(70.), px(120.), px(80.)), 7);
    assert_eq!(atom.boundary_for_x(px(70.), px(120.), px(95.)), 12);
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
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        ),
        Some(rendered_image_block(0..35, "alt"))
    );
}

#[test]
fn rendered_image_block_detects_inactive_relative_local_image_row_with_document_path() {
    let document_path = std::env::current_dir().unwrap().join("docs/readme.md");
    let mut buffer = Buffer::local("![alt](./cat.png)\nnext\n");
    let snapshot = buffer.snapshot();
    let selection = collapsed_selection(Point::new(1, 0));
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    let image_block = rendered_image_block_for_row(
        &snapshot,
        &row,
        &selection,
        MarkdownEditorMode::Rendered,
        Some(&document_path),
    )
    .expect("relative local image should become a block with a document path");

    assert_eq!(row.text, "alt");
    assert!(image_block.image_source.is_renderable());
}

#[test]
fn rendered_relative_local_image_stays_inline_fallback_without_document_path() {
    let mut buffer = Buffer::local("Before ![alt](./cat.png) after\n");
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
    let atom = fragments
        .iter()
        .find_map(|fragment| match fragment {
            DisplayInlineFragment::Atom(atom) => Some(atom),
            DisplayInlineFragment::Text(_) => None,
        })
        .expect("expected inline local image atom");

    assert!(!atom.image_source.as_ref().unwrap().is_renderable());
}

#[test]
fn rendered_formula_block_detects_inactive_formula_row() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\nnext\n"));
    let snapshot = buffer.snapshot();
    let selection = collapsed_selection(Point::new(1, 0));
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "x + y");
    assert_eq!(
        rendered_formula_block_for_row(&snapshot, &row, &selection, MarkdownEditorMode::Rendered),
        Some(rendered_formula_block(0..formula_source.len(), "x + y"))
    );
    assert!(
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        )
        .is_none()
    );
}

#[test]
fn rendered_formula_block_reveals_active_formula_source() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\n"));
    let snapshot = buffer.snapshot();
    let selection = collapsed_selection(Point::new(0, 2));
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, formula_source);
    assert_eq!(
        rendered_formula_block_for_row(&snapshot, &row, &selection, MarkdownEditorMode::Rendered),
        None
    );
}

#[test]
fn rendered_image_block_keeps_source_boundaries_inactive() {
    let mut buffer = Buffer::local("![alt](https://example.com/cat.png)\nnext\n");
    let snapshot = buffer.snapshot();
    let image_source_end = "![alt](https://example.com/cat.png)".len();

    for cursor in [0, image_source_end] {
        let selection = collapsed_selection(Point::new(0, cursor as u32));
        let row = display_rows_in_mode(
            &snapshot,
            0..1,
            Some(&selection),
            MarkdownEditorMode::Rendered,
        )
        .remove(0);

        assert_eq!(row.text, "alt");
        assert_eq!(
            rendered_image_block_for_row(
                &snapshot,
                &row,
                &selection,
                MarkdownEditorMode::Rendered,
                None
            ),
            Some(rendered_image_block(0..image_source_end, "alt"))
        );
    }
}

#[test]
fn rendered_image_block_keeps_whole_selection_inactive() {
    let mut buffer = Buffer::local("![alt](https://example.com/cat.png)\nnext\n");
    let snapshot = buffer.snapshot();
    let image_source_end = "![alt](https://example.com/cat.png)".len();
    let selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(0, image_source_end as u32),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "alt");
    assert_eq!(
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        ),
        Some(rendered_image_block(0..image_source_end, "alt"))
    );
}

#[test]
fn rendered_image_block_keeps_contained_selection_inactive() {
    let image_source = "![alt](https://example.com/cat.png)";
    let mut buffer = Buffer::local(&format!("intro\n{image_source}\noutro\n"));
    let snapshot = buffer.snapshot();
    let selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(2, "outro".len() as u32),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let row = display_rows_in_mode(
        &snapshot,
        1..2,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    let image_source_start = "intro\n".len();
    assert_eq!(row.text, "alt");
    assert_eq!(
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        ),
        Some(rendered_image_block(
            image_source_start..image_source_start + image_source.len(),
            "alt",
        ))
    );
}

#[test]
fn fenced_code_stays_text_layout_in_rendered_mode() {
    let source = "```rust\nlet x = 1;\n```\nnext\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();
    let selection = collapsed_selection(Point::new(3, 0));
    let row = display_rows_in_mode(
        &snapshot,
        1..2,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "let x = 1;");
    assert!(
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        )
        .is_none()
    );

    let row_style =
        row_display_style_for_display_row(&snapshot, &row, MarkdownEditorMode::Rendered);
    let fragments = display_fragments_for_text_layout(
        &snapshot,
        &row,
        MarkdownEditorMode::Rendered,
        row_style,
        None,
    );
    assert!(
        fragments
            .iter()
            .all(|fragment| matches!(fragment, DisplayInlineFragment::Text(_)))
    );
}

#[test]
fn active_pipe_table_row_stays_text_layout_in_rendered_mode() {
    let source = "| a | b |\n| - | - |\n| 1 | 2 |\n";
    let mut buffer = Buffer::local(source);
    let snapshot = buffer.snapshot();
    let selection = collapsed_selection(Point::new(2, 0));
    let row = display_rows_in_mode(
        &snapshot,
        2..3,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "| 1 | 2 |");
    assert!(
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        )
        .is_none()
    );

    let row_style =
        row_display_style_for_display_row(&snapshot, &row, MarkdownEditorMode::Rendered);
    let fragments = display_fragments_for_text_layout(
        &snapshot,
        &row,
        MarkdownEditorMode::Rendered,
        row_style,
        None,
    );
    assert!(
        fragments
            .iter()
            .all(|fragment| matches!(fragment, DisplayInlineFragment::Text(_)))
    );
}

#[test]
fn image_block_whole_selection_is_selected_state() {
    let image_source = "![alt](https://example.com/cat.png)";
    let mut buffer = Buffer::local(&format!("{image_source}\nnext\n"));
    let snapshot = buffer.snapshot();
    let block_layout = image_block_layout(0..image_source.len(), px(200.));
    let whole_selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(0, image_source.len() as u32),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let containing_selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(1, 0),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let partial_selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(0, 1),
        reversed: false,
        goal: SelectionGoal::None,
    };

    assert!(block_layout.is_whole_selected(&snapshot, &whole_selection));
    assert!(block_layout.is_whole_selected(&snapshot, &containing_selection));
    assert!(!block_layout.is_whole_selected(&snapshot, &partial_selection));
    assert!(!block_layout.is_whole_selected(&snapshot, &collapsed_selection(Point::new(0, 0))));
}

#[test]
fn formula_block_whole_selection_is_selected_state() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\nnext\n"));
    let snapshot = buffer.snapshot();
    let block_layout = formula_block_layout(0..formula_source.len(), px(200.));
    let whole_selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(0, formula_source.len() as u32),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let containing_selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(1, 0),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let partial_selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(0, 1),
        reversed: false,
        goal: SelectionGoal::None,
    };

    assert!(block_layout.is_whole_selected(&snapshot, &whole_selection));
    assert!(block_layout.is_whole_selected(&snapshot, &containing_selection));
    assert!(!block_layout.is_whole_selected(&snapshot, &partial_selection));
    assert!(!block_layout.is_whole_selected(&snapshot, &collapsed_selection(Point::new(0, 0))));
}

#[test]
fn image_block_mouse_x_maps_to_source_range_edges() {
    let mut buffer = Buffer::local("![alt](https://example.com/cat.png)\n");
    let snapshot = buffer.snapshot();
    let block_layout = image_block_layout(0..35, px(200.));

    assert_eq!(
        block_layout.point_for_mouse_x(&snapshot, gutter_width()),
        Point::new(0, 0)
    );
    assert_eq!(
        block_layout.point_for_mouse_x(&snapshot, gutter_width() + px(160.)),
        Point::new(0, 35)
    );
    assert_eq!(
        block_layout.point_for_mouse_x(&snapshot, gutter_width() + px(260.)),
        Point::new(0, 35)
    );
}

#[test]
fn formula_block_mouse_x_maps_to_source_range_edges() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\n"));
    let snapshot = buffer.snapshot();
    let block_layout = formula_block_layout(0..formula_source.len(), px(200.));

    assert_eq!(
        block_layout.point_for_mouse_x(&snapshot, gutter_width()),
        Point::new(0, 0)
    );
    assert_eq!(
        block_layout.point_for_mouse_x(&snapshot, gutter_width() + px(160.)),
        Point::new(0, formula_source.len() as u32)
    );
    assert_eq!(
        block_layout.point_for_mouse_x(&snapshot, gutter_width() + px(260.)),
        Point::new(0, formula_source.len() as u32)
    );
}

#[test]
fn image_block_mouse_target_tracks_visible_caret_goal() {
    let mut buffer = Buffer::local("![alt](https://example.com/cat.png)\n");
    let snapshot = buffer.snapshot();
    let block_layout = image_block_layout(0..35, px(200.));

    assert_eq!(
        block_layout.mouse_target_for_x(&snapshot, gutter_width()),
        (Point::new(0, 0), visual_horizontal_goal(0, px(0.)))
    );
    assert_eq!(
        block_layout.mouse_target_for_x(&snapshot, gutter_width() + px(160.)),
        (Point::new(0, 35), visual_horizontal_goal(0, px(200.)))
    );
    assert_eq!(
        block_layout.mouse_target_for_x(&snapshot, gutter_width() + px(260.)),
        (Point::new(0, 35), visual_horizontal_goal(0, px(200.)))
    );
}

#[test]
fn image_block_mouse_target_accounts_for_rendered_indent() {
    let mut buffer = Buffer::local("![alt](https://example.com/cat.png)\n");
    let snapshot = buffer.snapshot();
    let block_layout = image_block_layout(0..35, px(200.));
    let indent_width = px(48.);

    assert_eq!(
        block_layout.mouse_target_for_x_with_indent(
            &snapshot,
            gutter_width() + indent_width,
            indent_width
        ),
        (Point::new(0, 0), visual_horizontal_goal(0, px(0.)))
    );
    assert_eq!(
        block_layout.mouse_target_for_x_with_indent(
            &snapshot,
            gutter_width() + indent_width + px(260.),
            indent_width
        ),
        (Point::new(0, 35), visual_horizontal_goal(0, px(200.)))
    );
}

#[test]
fn formula_block_mouse_target_tracks_visible_caret_goal() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\n"));
    let snapshot = buffer.snapshot();
    let block_layout = formula_block_layout(0..formula_source.len(), px(200.));

    assert_eq!(
        block_layout.mouse_target_for_x(&snapshot, gutter_width()),
        (Point::new(0, 0), visual_horizontal_goal(0, px(0.)))
    );
    assert_eq!(
        block_layout.mouse_target_for_x(&snapshot, gutter_width() + px(160.)),
        (
            Point::new(0, formula_source.len() as u32),
            visual_horizontal_goal(0, px(200.))
        )
    );
    assert_eq!(
        block_layout.mouse_target_for_x(&snapshot, gutter_width() + px(260.)),
        (
            Point::new(0, formula_source.len() as u32),
            visual_horizontal_goal(0, px(200.))
        )
    );
}

#[test]
fn image_block_local_x_maps_to_source_range_edges() {
    let image_block = rendered_image_block(4..39, "alt");

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
    let block_layout = image_block_layout(4..39, px(200.));

    assert_eq!(block_layout.visible_x_for_source_offset(4), px(0.));
    assert_eq!(block_layout.visible_x_for_source_offset(20), px(100.));
    assert_eq!(block_layout.visible_x_for_source_offset(39), px(200.));
}

#[test]
fn formula_block_source_offset_maps_to_visible_x() {
    let block_layout = formula_block_layout(4..13, px(200.));

    assert_eq!(block_layout.visible_x_for_source_offset(4), px(0.));
    assert_eq!(block_layout.visible_x_for_source_offset(8), px(100.));
    assert_eq!(block_layout.visible_x_for_source_offset(13), px(200.));
}

#[test]
fn image_block_line_boundary_targets_source_edges() {
    let mut buffer = Buffer::local("    ![alt](https://example.com/cat.png)\n");
    let snapshot = buffer.snapshot();
    let block_layout = image_block_layout(4..39, px(200.));

    assert_eq!(
        block_layout.line_boundary_target(&snapshot, VisualLineBoundary::Start),
        (Point::new(0, 4), visual_horizontal_goal(0, px(0.)))
    );
    assert_eq!(
        block_layout.line_boundary_target(&snapshot, VisualLineBoundary::End),
        (Point::new(0, 39), visual_horizontal_goal(0, px(200.)))
    );
}

#[test]
fn formula_block_line_boundary_targets_source_edges() {
    let mut buffer = Buffer::local("    $$x + y$$\n");
    let snapshot = buffer.snapshot();
    let block_layout = formula_block_layout(4..13, px(200.));

    assert_eq!(
        block_layout.line_boundary_target(&snapshot, VisualLineBoundary::Start),
        (Point::new(0, 4), visual_horizontal_goal(0, px(0.)))
    );
    assert_eq!(
        block_layout.line_boundary_target(&snapshot, VisualLineBoundary::End),
        (Point::new(0, 13), visual_horizontal_goal(0, px(200.)))
    );
}

#[test]
fn image_block_caret_x_tracks_collapsed_source_boundaries() {
    let image_source = "![alt](https://example.com/cat.png)";
    let mut buffer = Buffer::local(&format!("{image_source}\n"));
    let snapshot = buffer.snapshot();
    let block_layout = image_block_layout(0..image_source.len(), px(200.));
    let selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(0, image_source.len() as u32),
        reversed: false,
        goal: SelectionGoal::None,
    };

    assert_eq!(
        block_layout.caret_x(&snapshot, &collapsed_selection(Point::new(0, 0))),
        Some(px(0.))
    );
    assert_eq!(
        block_layout.caret_x(
            &snapshot,
            &collapsed_selection(Point::new(0, image_source.len() as u32))
        ),
        Some(px(200.))
    );
    assert_eq!(
        block_layout.caret_x(&snapshot, &collapsed_selection(Point::new(0, 1))),
        None
    );
    assert_eq!(block_layout.caret_x(&snapshot, &selection), None);
}

#[test]
fn formula_block_caret_x_tracks_collapsed_source_boundaries() {
    let formula_source = "$$x + y$$";
    let mut buffer = Buffer::local(&format!("{formula_source}\n"));
    let snapshot = buffer.snapshot();
    let block_layout = formula_block_layout(0..formula_source.len(), px(200.));
    let selection = Selection {
        id: 0,
        start: Point::new(0, 0),
        end: Point::new(0, formula_source.len() as u32),
        reversed: false,
        goal: SelectionGoal::None,
    };

    assert_eq!(
        block_layout.caret_x(&snapshot, &collapsed_selection(Point::new(0, 0))),
        Some(px(0.))
    );
    assert_eq!(
        block_layout.caret_x(
            &snapshot,
            &collapsed_selection(Point::new(0, formula_source.len() as u32))
        ),
        Some(px(200.))
    );
    assert_eq!(
        block_layout.caret_x(&snapshot, &collapsed_selection(Point::new(0, 1))),
        None
    );
    assert_eq!(block_layout.caret_x(&snapshot, &selection), None);
}

#[test]
fn image_block_layout_height_includes_vertical_padding() {
    let image_layout = RenderedImageBlockLayout {
        image_block: rendered_image_block(4..39, "alt"),
        width: px(200.),
        image_height: px(120.),
        cacheable: true,
    };

    assert_eq!(image_layout.image_height(), px(120.));
    assert_eq!(
        image_layout.height(),
        px(120.) + RENDERED_IMAGE_BLOCK_VERTICAL_PADDING * 2.
    );
}

#[test]
fn formula_block_layout_is_cacheable_and_uses_measured_height() {
    let formula_layout = RenderedFormulaBlockLayout {
        formula_block: rendered_formula_block(4..13, "x + y"),
        width: px(200.),
        height: px(36.),
        rendered_formula: None,
        cacheable: true,
    };
    let block_layout =
        DisplayRowLayout::Block(DisplayBlockLayout::Formula(formula_layout.clone()).into());

    assert!(block_layout.cacheable());
    assert_eq!(formula_layout.height(), px(36.));
}

#[test]
fn image_block_layout_cacheability_tracks_loaded_size() {
    let image_block = rendered_image_block(4..39, "alt");
    let loaded_layout = DisplayRowLayout::Block(
        DisplayBlockLayout::RemoteImage(RenderedImageBlockLayout {
            image_block: image_block.clone(),
            width: px(200.),
            image_height: px(120.),
            cacheable: true,
        })
        .into(),
    );
    let placeholder_layout = DisplayRowLayout::Block(
        DisplayBlockLayout::RemoteImage(RenderedImageBlockLayout {
            image_block,
            width: px(200.),
            image_height: RENDERED_IMAGE_BLOCK_PLACEHOLDER_HEIGHT,
            cacheable: false,
        })
        .into(),
    );

    assert!(loaded_layout.cacheable());
    assert!(!placeholder_layout.cacheable());
}

#[test]
fn image_block_size_uses_natural_size_when_it_fits() {
    assert_eq!(
        image_block_size_for_size(300, 200, px(600.)),
        Some(gpui::size(px(300.), px(200.)))
    );
}

#[test]
fn image_block_size_scales_wide_image_to_max_width() {
    assert_eq!(
        image_block_size_for_size(1200, 800, px(600.)),
        Some(gpui::size(px(600.), px(400.)))
    );
}

#[test]
fn image_block_size_preserves_tall_image_aspect_ratio() {
    assert_eq!(
        image_block_size_for_size(800, 1200, px(300.)),
        Some(gpui::size(px(300.), px(450.)))
    );
    assert_eq!(
        image_block_size_for_size(40, 80, px(300.)),
        Some(gpui::size(px(40.), px(80.)))
    );
}

#[test]
fn image_block_size_returns_none_for_empty_image_size() {
    assert_eq!(image_block_size_for_size(0, 800, px(600.)), None);
    assert_eq!(image_block_size_for_size(1200, 0, px(600.)), None);
    assert_eq!(image_block_size_for_size(0, 0, px(600.)), None);
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
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        ),
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
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        ),
        None
    );
}

#[test]
fn rendered_inline_image_boundary_stays_inactive() {
    let mut buffer = Buffer::local("before ![alt](https://example.com/cat.png) after\n");
    let snapshot = buffer.snapshot();
    let image_start = "before ".len();
    let selection = collapsed_selection(Point::new(0, image_start as u32));
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "before alt after");
    assert_eq!(
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        ),
        None
    );
}

#[test]
fn rendered_inline_image_whole_selection_stays_inactive() {
    let mut buffer = Buffer::local("before ![alt](https://example.com/cat.png) after\n");
    let snapshot = buffer.snapshot();
    let image_start = "before ".len();
    let image_end = "before ![alt](https://example.com/cat.png)".len();
    let selection = Selection {
        id: 0,
        start: Point::new(0, image_start as u32),
        end: Point::new(0, image_end as u32),
        reversed: false,
        goal: SelectionGoal::None,
    };
    let row = display_rows_in_mode(
        &snapshot,
        0..1,
        Some(&selection),
        MarkdownEditorMode::Rendered,
    )
    .remove(0);

    assert_eq!(row.text, "before alt after");
    assert_eq!(
        rendered_image_block_for_row(
            &snapshot,
            &row,
            &selection,
            MarkdownEditorMode::Rendered,
            None
        ),
        None
    );
}

#[test]
fn source_rows_for_active_range_change_returns_touched_rows() {
    let mut buffer = Buffer::local("first\nsecond\nthird\n");
    let snapshot = buffer.snapshot();

    assert_eq!(
        source_rows_for_active_range_change(&snapshot, Some(&(1..3)), Some(&(14..16))),
        vec![0..1, 2..3]
    );
    assert_eq!(
        source_rows_for_active_range_change(&snapshot, Some(&(1..3)), Some(&(7..10))),
        vec![0..2]
    );
}

#[test]
fn active_projection_source_ranges_tracks_marker_visibility_dependencies() {
    let mut buffer = Buffer::local("# Title\nplain\n```rust\ncode\n```\n**bold**\n");
    let snapshot = buffer.snapshot();
    let heading_range = snapshot
        .syntax_tree()
        .blocks()
        .iter()
        .find_map(|block| match block.kind {
            MarkdownBlockKind::AtxHeading { .. } => Some(block.source_range.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let fenced_range = snapshot
        .syntax_tree()
        .blocks()
        .iter()
        .find_map(|block| match block.kind {
            MarkdownBlockKind::FencedCodeBlock => Some(block.source_range.clone()),
            _ => None,
        })
        .unwrap_or_default();
    let strong_range = snapshot
        .syntax_tree()
        .inline_spans()
        .iter()
        .find_map(|span| match span.kind {
            MarkdownInlineKind::Strong => Some(span.source_range.clone()),
            _ => None,
        })
        .unwrap_or_default();
    assert!(!heading_range.is_empty());
    assert!(!fenced_range.is_empty());
    assert!(!strong_range.is_empty());

    let heading_state = DisplayRowProjectionState::new(
        &snapshot,
        Some(&collapsed_selection(Point::new(0, 3))),
        MarkdownEditorMode::Rendered,
    );
    let fenced_state = DisplayRowProjectionState::new(
        &snapshot,
        Some(&collapsed_selection(Point::new(3, 1))),
        MarkdownEditorMode::Rendered,
    );
    let strong_state = DisplayRowProjectionState::new(
        &snapshot,
        Some(&collapsed_selection(Point::new(5, 3))),
        MarkdownEditorMode::Rendered,
    );
    let active_ranges_for_row = |row, state: &DisplayRowProjectionState, mode| {
        if mode != MarkdownEditorMode::Rendered {
            return Vec::new();
        }

        let row_source_range = row_source_range(&snapshot, row);
        snapshot
            .syntax_tree()
            .range_semantics_for_source_range(
                row_source_range,
                state.active_source_range.clone(),
                &state.inactive_source_ranges,
            )
            .active_projection_source_ranges
    };

    assert_eq!(
        active_ranges_for_row(0, &heading_state, MarkdownEditorMode::Rendered),
        vec![heading_range]
    );
    assert_eq!(
        active_ranges_for_row(1, &heading_state, MarkdownEditorMode::Rendered),
        Vec::<Range<usize>>::new()
    );
    assert_eq!(
        active_ranges_for_row(2, &fenced_state, MarkdownEditorMode::Rendered),
        vec![fenced_range]
    );
    assert_eq!(
        active_ranges_for_row(3, &fenced_state, MarkdownEditorMode::Rendered),
        Vec::<Range<usize>>::new()
    );
    assert_eq!(
        active_ranges_for_row(5, &strong_state, MarkdownEditorMode::Rendered),
        vec![strong_range]
    );
    assert_eq!(
        active_ranges_for_row(0, &heading_state, MarkdownEditorMode::Source),
        Vec::<Range<usize>>::new()
    );
}
