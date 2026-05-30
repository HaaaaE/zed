use super::test_support::*;
use gpui::{px, size};
use std::time::{Duration, Instant};
use util_macros::perf;

const LARGE_MARKDOWN_TARGET_BYTES: usize = 300 * 1024;
const SHORT_MARKDOWN_TARGET_BYTES: usize = 5 * 1024;
const PERF_WINDOW_WIDTH: f32 = 900.;
const PERF_WINDOW_HEIGHT: f32 = 700.;
const PERF_NARROW_WINDOW_WIDTH: f32 = 560.;
const SCROLL_STEP_PIXELS: f32 = 168.;
const SCROLL_STEPS: usize = 12;

fn mixed_gfm_markdown_fixture(target_bytes: usize) -> String {
    let mut text = String::with_capacity(target_bytes + 1024);
    let block = [
        "## Mixed GFM section",
        "",
        "Before **bold** text, _emphasis_, ~~strike~~, `code`, [link](https://example.com), <https://example.com>, &amp; entity, escaped \\* marker, and source-row layout profiling.",
        "CJK mixed content: 中文段落用于覆盖宽字符 wrapping 和 source-row edit 定位。",
        "",
        "> quoted paragraph with source-row target",
        "> - [ ] quoted unchecked task",
        "> - [x] quoted checked task",
        ">   1. nested ordered item",
        "",
        "- [ ] unchecked task item with a long wrapped source-row entry for layout profiling",
        "- [X] uppercase checked task item",
        "- plain unordered item",
        "  - nested unordered item",
        "1. ordered item",
        "2) ordered paren item",
        "",
        "| Feature | State | Notes |",
        "| :--- | :---: | ---: |",
        "| table | ok | source-row |",
        "| task | mixed | 42 |",
        "",
        "```rust",
        "fn main() {",
        "    println!(\"source-row fenced code\");",
        "}",
        "```",
        "",
        "    indented code block source-row",
        "",
        "<div data-kind=\"safe\">raw html source-row</div>",
        "<script>blocked_by_tagfilter()</script>",
        "",
        "Hard break follows two spaces  ",
        "after hard break and soft",
        "break continuation source-row.",
        "",
    ]
    .join("\n");

    while text.len() < target_bytes {
        text.push_str(&block);
        text.push('\n');
    }

    text
}

fn middle_row_containing(text: &str, needle: &str) -> u32 {
    let rows: Vec<&str> = text.lines().collect();
    let middle = rows.len() / 2;

    if let Some((row, _)) = rows
        .iter()
        .enumerate()
        .skip(middle)
        .find(|(_, line)| line.contains(needle))
    {
        return u32::try_from(row).expect("fixture row should fit into u32");
    }

    if let Some((row, _)) = rows
        .iter()
        .enumerate()
        .take(middle)
        .find(|(_, line)| line.contains(needle))
    {
        return u32::try_from(row).expect("fixture row should fit into u32");
    }

    panic!("fixture should contain target text");
}

fn warm_draw(editor: &gpui::Entity<MarkdownEditor>, cx: &mut gpui::VisualTestContext) {
    let size = cx.update(|window, _| window.bounds().size);
    cx.draw(gpui::point(px(0.), px(0.)), size, |_, _| {
        editor.clone().into_any_element()
    });
    cx.run_until_parked();
}

fn reset_layout_computation_counts(
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
) {
    editor.update(cx, |editor, _| editor.reset_layout_computation_counts());
}

fn scroll_and_draw(
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
    steps: usize,
    step_pixels: f32,
) {
    for _ in 0..steps {
        editor.update(cx, |editor, _| {
            let current = editor.display_list_state.logical_scroll_top();
            editor.display_list_state.scroll_to(ListOffset {
                item_ix: current.item_ix,
                offset_in_item: current.offset_in_item + px(step_pixels),
            });
        });
        warm_draw(editor, cx);
    }
}

fn replace_middle_row_word(
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
    target_row: u32,
    from: &str,
    to: &str,
) {
    editor.update(cx, |editor, cx| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);

        let row_text = editor.row_text(target_row);
        let column = row_text
            .find(from)
            .expect("target word should exist in fixture row");
        let start_column = u32::try_from(column).expect("fixture column should fit into u32");
        let end_column =
            start_column + u32::try_from(from.len()).expect("word length should fit into u32");
        let previous_selection = Selection {
            id: 1,
            start: Point::new(target_row, start_column),
            end: Point::new(target_row, end_column),
            reversed: false,
            goal: SelectionGoal::None,
        };

        editor.selection = previous_selection.clone();
        let row_count_before = editor.display_list_state.item_count();
        let buffer_len_before = editor.buffer.len();
        let (selection, transaction_id) =
            replace_selection(&mut editor.buffer, &editor.selection, to);
        let changed = transaction_id.is_some();
        let byte_delta = buffer_byte_delta(buffer_len_before, editor.buffer.len());

        editor.selection = selection;
        editor.record_selection_history(
            transaction_id,
            previous_selection.clone(),
            editor.selection.clone(),
        );
        editor.notify_after_edit(
            changed,
            row_count_before,
            &previous_selection,
            EditLayoutInvalidation::LocalSourceSelection { byte_delta },
            cx,
        );

        assert!(!editor.serialized_text().is_empty());
    });
}

const SMALL_SESSION_ITERATIONS: usize = 16;
const LARGE_SESSION_ITERATIONS: usize = 8;
const PERF_SELF_TIMED_LINE_PREFIX: &str = "MD_PERF_SELF_TIMED_NS";
const PERF_SEGMENT_LINE_PREFIX: &str = "MD_PERF_SEGMENT_NS";

fn perf_iter_count() -> usize {
    std::env::var("MD_PERF_ITER")
        .expect("perf harness should set MD_PERF_ITER")
        .parse::<usize>()
        .expect("MD_PERF_ITER should be a usize")
}

fn report_self_timed_duration(duration: Duration) {
    println!("{PERF_SELF_TIMED_LINE_PREFIX} {}", duration.as_nanos());
}

fn report_segment_duration(name: &str, duration: Duration) {
    println!("{PERF_SEGMENT_LINE_PREFIX} {name} {}", duration.as_nanos());
}

fn record_segment<T>(
    segments: &mut Vec<(&'static str, Duration)>,
    name: &'static str,
    run: impl FnOnce() -> T,
) -> T {
    let start = Instant::now();
    let value = run();
    segments.push((name, start.elapsed()));
    value
}

fn record_scroll_segments(
    segments: &mut Vec<(&'static str, Duration)>,
    prepare_name: &'static str,
    scroll_name: &'static str,
    run: impl FnOnce() -> (Duration, Duration),
) {
    let (prepare_duration, scroll_duration) = run();
    segments.push((prepare_name, prepare_duration));
    segments.push((scroll_name, scroll_duration));
}

fn clear_editor_layout_caches(
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
) {
    editor.update(cx, |editor, _| {
        editor.clear_display_row_cache();
        editor.clear_row_layout_cache();
        editor.display_list_state.remeasure();
    });
}

fn scroll_to(
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
    offset: ListOffset,
) {
    editor.update(cx, |editor, _| {
        editor.display_list_state.scroll_to(offset);
    });
}

fn measure_uncached_scroll_iteration(
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
    initial_offset: ListOffset,
) -> (Duration, Duration) {
    let prepare_start = Instant::now();
    scroll_to(editor, cx, initial_offset);
    clear_editor_layout_caches(editor, cx);
    warm_draw(editor, cx);
    reset_layout_computation_counts(editor, cx);
    let prepare_duration = prepare_start.elapsed();

    let start = Instant::now();
    scroll_and_draw(editor, cx, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    (prepare_duration, start.elapsed())
}

fn measure_cached_region_scroll_iteration(
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
    initial_offset: ListOffset,
) -> (Duration, Duration) {
    let prepare_start = Instant::now();
    scroll_to(editor, cx, initial_offset);
    warm_draw(editor, cx);
    scroll_and_draw(editor, cx, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    scroll_to(editor, cx, initial_offset);
    warm_draw(editor, cx);
    reset_layout_computation_counts(editor, cx);
    let prepare_duration = prepare_start.elapsed();

    let start = Instant::now();
    scroll_and_draw(editor, cx, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    (prepare_duration, start.elapsed())
}

fn run_editor_session(target_bytes: usize) {
    let mut totals: Vec<(&'static str, Duration)> = Vec::new();

    for _ in 0..perf_iter_count() {
        let mut segments = Vec::new();
        let (text, target_row) = record_segment(&mut segments, "fixture_prepare", || {
            let text = mixed_gfm_markdown_fixture(target_bytes);
            let target_row = middle_row_containing(&text, "source-row");
            (text, target_row)
        });
        let mut app = record_segment(
            &mut segments,
            "context_create",
            gpui::TestAppContext::single,
        );
        let cx = record_segment(&mut segments, "window_create", || {
            let cx = app.add_empty_window();
            cx.simulate_resize(size(px(PERF_WINDOW_WIDTH), px(PERF_WINDOW_HEIGHT)));
            cx
        });
        let editor = record_segment(&mut segments, "source_editor_create", || {
            cx.new(|cx| MarkdownEditor::for_text(text.clone(), cx))
        });
        let initial_offset = editor.update(cx, |editor, _| {
            editor.display_list_state.logical_scroll_top()
        });

        record_segment(&mut segments, "source_first_draw", || {
            clear_editor_layout_caches(&editor, cx);
            reset_layout_computation_counts(&editor, cx);
            warm_draw(&editor, cx);
        });
        record_segment(&mut segments, "source_cached_redraw", || {
            reset_layout_computation_counts(&editor, cx);
            warm_draw(&editor, cx);
        });
        record_scroll_segments(
            &mut segments,
            "source_scroll_cold_prepare",
            "source_scroll_cold",
            || measure_uncached_scroll_iteration(&editor, cx, initial_offset),
        );
        record_scroll_segments(
            &mut segments,
            "source_scroll_cached_region_prepare",
            "source_scroll_cached_region",
            || measure_cached_region_scroll_iteration(&editor, cx, initial_offset),
        );
        record_segment(&mut segments, "source_edit_equal_length", || {
            replace_middle_row_word(&editor, cx, target_row, "source-row", "source-raw");
            warm_draw(&editor, cx);
        });
        record_segment(&mut segments, "source_edit_length_change", || {
            replace_middle_row_word(&editor, cx, target_row, "source-raw", "row");
            warm_draw(&editor, cx);
        });
        record_segment(&mut segments, "source_cached_redraw_after_edit", || {
            reset_layout_computation_counts(&editor, cx);
            warm_draw(&editor, cx);
        });
        record_segment(&mut segments, "switch_to_rendered", || {
            editor.update(cx, |editor, cx| {
                editor.set_mode(MarkdownEditorMode::Rendered, cx)
            });
        });
        record_segment(&mut segments, "rendered_first_draw", || {
            clear_editor_layout_caches(&editor, cx);
            reset_layout_computation_counts(&editor, cx);
            warm_draw(&editor, cx);
        });

        for _ in 0..2 {
            record_segment(&mut segments, "rendered_cached_redraw", || {
                reset_layout_computation_counts(&editor, cx);
                warm_draw(&editor, cx);
            });
            record_scroll_segments(
                &mut segments,
                "rendered_scroll_cold_prepare",
                "rendered_scroll_cold",
                || measure_uncached_scroll_iteration(&editor, cx, initial_offset),
            );
            record_scroll_segments(
                &mut segments,
                "rendered_scroll_cached_region_prepare",
                "rendered_scroll_cached_region",
                || measure_cached_region_scroll_iteration(&editor, cx, initial_offset),
            );
            record_segment(&mut segments, "rendered_resize", || {
                cx.simulate_resize(size(px(PERF_NARROW_WINDOW_WIDTH), px(PERF_WINDOW_HEIGHT)));
                warm_draw(&editor, cx);
                cx.simulate_resize(size(px(PERF_WINDOW_WIDTH), px(PERF_WINDOW_HEIGHT)));
                warm_draw(&editor, cx);
            });
        }

        record_segment(&mut segments, "switch_to_source", || {
            editor.update(cx, |editor, cx| {
                editor.set_mode(MarkdownEditorMode::Source, cx)
            });
        });
        record_scroll_segments(
            &mut segments,
            "source_scroll_cached_region_after_switch_prepare",
            "source_scroll_cached_region_after_switch",
            || measure_cached_region_scroll_iteration(&editor, cx, initial_offset),
        );
        record_segment(
            &mut segments,
            "source_edit_equal_length_after_switch",
            || {
                replace_middle_row_word(&editor, cx, target_row, "row", "raw");
                warm_draw(&editor, cx);
            },
        );
        record_segment(
            &mut segments,
            "source_edit_length_change_after_switch",
            || {
                replace_middle_row_word(&editor, cx, target_row, "raw", "source-row");
                warm_draw(&editor, cx);
            },
        );
        record_segment(&mut segments, "switch_to_rendered_again", || {
            editor.update(cx, |editor, cx| {
                editor.set_mode(MarkdownEditorMode::Rendered, cx)
            });
        });
        record_segment(
            &mut segments,
            "rendered_cached_redraw_after_second_switch",
            || {
                reset_layout_computation_counts(&editor, cx);
                warm_draw(&editor, cx);
            },
        );

        editor.update(cx, |editor, _| {
            assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
            assert!(!editor.serialized_text().is_empty());
            assert!(editor.row_count() > 0);
        });
        if totals.is_empty() {
            totals = segments;
        } else {
            assert_eq!(totals.len(), segments.len());
            for ((total_name, total_duration), (name, duration)) in
                totals.iter_mut().zip(segments.into_iter())
            {
                assert_eq!(*total_name, name);
                *total_duration += duration;
            }
        }
    }

    let total_duration = totals
        .iter()
        .map(|(_, duration)| *duration)
        .sum::<Duration>();
    report_self_timed_duration(total_duration);
    for (name, duration) in totals {
        report_segment_duration(name, duration);
    }
}

#[perf(important, iterations = SMALL_SESSION_ITERATIONS)]
fn small_document_session() {
    run_editor_session(SHORT_MARKDOWN_TARGET_BYTES);
}

#[perf(important, iterations = LARGE_SESSION_ITERATIONS)]
fn large_document_session() {
    run_editor_session(LARGE_MARKDOWN_TARGET_BYTES);
}

// Deferred: mixed-content perf cases for inline image / inline math / image
// block / formula block remain out of scope for the current source-row perf
// pass. This includes draw, cached redraw, and scroll coverage with atoms.
