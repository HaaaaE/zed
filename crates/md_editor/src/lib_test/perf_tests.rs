use super::test_support::*;
use gpui::{px, size};
use util_macros::perf;

const LARGE_MARKDOWN_TARGET_BYTES: usize = 300 * 1024;
const SHORT_MARKDOWN_TARGET_BYTES: usize = 5 * 1024;
const PERF_WINDOW_WIDTH: f32 = 900.;
const PERF_WINDOW_HEIGHT: f32 = 700.;
const PERF_NARROW_WINDOW_WIDTH: f32 = 560.;
const SCROLL_STEP_PIXELS: f32 = 168.;
const SCROLL_STEPS: usize = 12;

fn plain_markdown_fixture(target_bytes: usize) -> String {
    let mut text = String::with_capacity(target_bytes + 1024);
    let paragraph = "Before **bold** text and regular wrapped prose repeated for source-row layout profiling.\n";
    let block = "## Heading\n\n";
    let list = "- first item in a long wrapped list entry for layout profiling\n";

    while text.len() < target_bytes {
        text.push_str(block);
        text.push_str(paragraph);
        text.push_str(paragraph);
        text.push_str(list);
        text.push('\n');
    }

    text
}

fn short_plain_markdown_fixture() -> String {
    plain_markdown_fixture(SHORT_MARKDOWN_TARGET_BYTES)
}

fn large_plain_markdown_fixture() -> String {
    plain_markdown_fixture(LARGE_MARKDOWN_TARGET_BYTES)
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

fn open_source_perf_window<'a>(
    cx: &'a mut gpui::TestAppContext,
    text: &str,
) -> (
    gpui::Entity<MarkdownEditor>,
    &'a mut gpui::VisualTestContext,
) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(size(px(PERF_WINDOW_WIDTH), px(PERF_WINDOW_HEIGHT)));
    let text = text.to_string();
    let editor = cx.new(|cx| MarkdownEditor::for_text(text, cx));
    (editor, cx)
}

fn open_rendered_perf_window<'a>(
    cx: &'a mut gpui::TestAppContext,
    text: &str,
) -> (
    gpui::Entity<MarkdownEditor>,
    &'a mut gpui::VisualTestContext,
) {
    let cx = cx.add_empty_window();
    cx.simulate_resize(size(px(PERF_WINDOW_WIDTH), px(PERF_WINDOW_HEIGHT)));
    let text = text.to_string();
    let editor = cx.new(|cx| {
        let mut editor = MarkdownEditor::for_text(text, cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });
    (editor, cx)
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

fn report_layout_computation_counts(
    label: &str,
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
) {
    let counts = editor.read_with(cx, |editor, _| editor.layout_computation_counts());
    eprintln!("{label}: {counts:?}");
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

const PERF_DRAW_ITERATIONS: usize = 64;
const PERF_CACHED_DRAW_ITERATIONS: usize = 128;
const PERF_SCROLL_ITERATIONS: usize = 8;
const PERF_EDIT_ITERATIONS: usize = 64;
const PERF_RESIZE_ITERATIONS: usize = 32;
const PERF_SELF_TIMED_LINE_PREFIX: &str = "ZED_PERF_SELF_TIMED_NS";

fn perf_iter_count() -> usize {
    std::env::var("ZED_PERF_ITER")
        .expect("perf harness should set ZED_PERF_ITER")
        .parse::<usize>()
        .expect("ZED_PERF_ITER should be a usize")
}

fn report_self_timed_duration(duration: std::time::Duration) {
    println!("{PERF_SELF_TIMED_LINE_PREFIX} {}", duration.as_nanos());
}

fn measure_self_timed_iterations(mut measure_iteration: impl FnMut() -> std::time::Duration) {
    let mut duration = std::time::Duration::ZERO;
    for _ in 0..perf_iter_count() {
        duration += measure_iteration();
    }
    report_self_timed_duration(duration);
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
) -> std::time::Duration {
    scroll_to(editor, cx, initial_offset);
    clear_editor_layout_caches(editor, cx);
    warm_draw(editor, cx);
    reset_layout_computation_counts(editor, cx);

    let start = std::time::Instant::now();
    scroll_and_draw(editor, cx, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    start.elapsed()
}

fn measure_cached_region_scroll_iteration(
    editor: &gpui::Entity<MarkdownEditor>,
    cx: &mut gpui::VisualTestContext,
    initial_offset: ListOffset,
) -> std::time::Duration {
    scroll_to(editor, cx, initial_offset);
    warm_draw(editor, cx);
    scroll_and_draw(editor, cx, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    scroll_to(editor, cx, initial_offset);
    warm_draw(editor, cx);
    reset_layout_computation_counts(editor, cx);

    let start = std::time::Instant::now();
    scroll_and_draw(editor, cx, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    start.elapsed()
}

#[perf(important, iterations = PERF_DRAW_ITERATIONS, self_timed)]
fn source_mode_draw_large_markdown() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let (editor, cx) = open_source_perf_window(&mut cx, &text);

    measure_self_timed_iterations(|| {
        clear_editor_layout_caches(&editor, cx);
        reset_layout_computation_counts(&editor, cx);
        let start = std::time::Instant::now();
        warm_draw(&editor, cx);
        start.elapsed()
    });
    report_layout_computation_counts("source large draw", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_DRAW_ITERATIONS, self_timed)]
fn rendered_mode_draw_large_markdown() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let (editor, cx) = open_rendered_perf_window(&mut cx, &text);

    measure_self_timed_iterations(|| {
        clear_editor_layout_caches(&editor, cx);
        reset_layout_computation_counts(&editor, cx);
        let start = std::time::Instant::now();
        warm_draw(&editor, cx);
        start.elapsed()
    });
    report_layout_computation_counts("rendered large draw", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_CACHED_DRAW_ITERATIONS, self_timed)]
fn source_mode_redraw_large_markdown_cached() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let (editor, cx) = open_source_perf_window(&mut cx, &text);

    warm_draw(&editor, cx);
    measure_self_timed_iterations(|| {
        reset_layout_computation_counts(&editor, cx);
        let start = std::time::Instant::now();
        warm_draw(&editor, cx);
        start.elapsed()
    });
    report_layout_computation_counts("source large cached redraw", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_CACHED_DRAW_ITERATIONS, self_timed)]
fn rendered_mode_redraw_large_markdown_cached() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let (editor, cx) = open_rendered_perf_window(&mut cx, &text);

    warm_draw(&editor, cx);
    measure_self_timed_iterations(|| {
        reset_layout_computation_counts(&editor, cx);
        let start = std::time::Instant::now();
        warm_draw(&editor, cx);
        start.elapsed()
    });
    report_layout_computation_counts("rendered large cached redraw", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_SCROLL_ITERATIONS, self_timed)]
fn source_mode_scroll_short_markdown() {
    let mut cx = gpui::TestAppContext::single();
    let text = short_plain_markdown_fixture();
    let (editor, cx) = open_source_perf_window(&mut cx, &text);
    let initial_offset = editor.update(cx, |editor, _| {
        editor.display_list_state.logical_scroll_top()
    });

    measure_self_timed_iterations(|| {
        measure_uncached_scroll_iteration(&editor, cx, initial_offset)
    });
    report_layout_computation_counts("source short first scroll", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_SCROLL_ITERATIONS, self_timed)]
fn source_mode_scroll_large_markdown() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let (editor, cx) = open_source_perf_window(&mut cx, &text);
    let initial_offset = editor.update(cx, |editor, _| {
        editor.display_list_state.logical_scroll_top()
    });

    measure_self_timed_iterations(|| {
        measure_uncached_scroll_iteration(&editor, cx, initial_offset)
    });
    report_layout_computation_counts("source large first scroll", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_SCROLL_ITERATIONS, self_timed)]
fn source_mode_scroll_short_markdown_cached_region() {
    let mut cx = gpui::TestAppContext::single();
    let text = short_plain_markdown_fixture();
    let (editor, cx) = open_source_perf_window(&mut cx, &text);
    let initial_offset = editor.update(cx, |editor, _| {
        editor.display_list_state.logical_scroll_top()
    });

    measure_self_timed_iterations(|| {
        measure_cached_region_scroll_iteration(&editor, cx, initial_offset)
    });
    report_layout_computation_counts("source short cached-region scroll", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_SCROLL_ITERATIONS, self_timed)]
fn source_mode_scroll_large_markdown_cached_region() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let (editor, cx) = open_source_perf_window(&mut cx, &text);
    let initial_offset = editor.update(cx, |editor, _| {
        editor.display_list_state.logical_scroll_top()
    });

    measure_self_timed_iterations(|| {
        measure_cached_region_scroll_iteration(&editor, cx, initial_offset)
    });
    report_layout_computation_counts("source large cached-region scroll", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_SCROLL_ITERATIONS, self_timed)]
fn rendered_mode_scroll_short_markdown() {
    let mut cx = gpui::TestAppContext::single();
    let text = short_plain_markdown_fixture();
    let (editor, cx) = open_rendered_perf_window(&mut cx, &text);
    let initial_offset = editor.update(cx, |editor, _| {
        editor.display_list_state.logical_scroll_top()
    });

    measure_self_timed_iterations(|| {
        measure_uncached_scroll_iteration(&editor, cx, initial_offset)
    });
    report_layout_computation_counts("rendered short first scroll", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_SCROLL_ITERATIONS, self_timed)]
fn rendered_mode_scroll_large_markdown() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let (editor, cx) = open_rendered_perf_window(&mut cx, &text);
    let initial_offset = editor.update(cx, |editor, _| {
        editor.display_list_state.logical_scroll_top()
    });

    measure_self_timed_iterations(|| {
        measure_uncached_scroll_iteration(&editor, cx, initial_offset)
    });
    report_layout_computation_counts("rendered large first scroll", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_SCROLL_ITERATIONS, self_timed)]
fn rendered_mode_scroll_short_markdown_cached_region() {
    let mut cx = gpui::TestAppContext::single();
    let text = short_plain_markdown_fixture();
    let (editor, cx) = open_rendered_perf_window(&mut cx, &text);
    let initial_offset = editor.update(cx, |editor, _| {
        editor.display_list_state.logical_scroll_top()
    });

    measure_self_timed_iterations(|| {
        measure_cached_region_scroll_iteration(&editor, cx, initial_offset)
    });
    report_layout_computation_counts("rendered short cached-region scroll", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_SCROLL_ITERATIONS, self_timed)]
fn rendered_mode_scroll_large_markdown_cached_region() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let (editor, cx) = open_rendered_perf_window(&mut cx, &text);
    let initial_offset = editor.update(cx, |editor, _| {
        editor.display_list_state.logical_scroll_top()
    });

    measure_self_timed_iterations(|| {
        measure_cached_region_scroll_iteration(&editor, cx, initial_offset)
    });
    report_layout_computation_counts("rendered large cached-region scroll", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_EDIT_ITERATIONS, self_timed)]
fn source_mode_single_row_edit_large_markdown() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let target_row = middle_row_containing(&text, "source-row");
    let (editor, cx) = open_source_perf_window(&mut cx, &text);
    let mut using_underscore = false;

    warm_draw(&editor, cx);
    measure_self_timed_iterations(|| {
        let (from, to) = if using_underscore {
            ("source_rows", "source-row")
        } else {
            ("source-row", "source_rows")
        };
        reset_layout_computation_counts(&editor, cx);
        let start = std::time::Instant::now();
        replace_middle_row_word(&editor, cx, target_row, from, to);
        warm_draw(&editor, cx);
        let elapsed = start.elapsed();
        using_underscore = !using_underscore;
        elapsed
    });

    editor.update(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
        assert!(editor.row_count() > 0);
    });
}

#[perf(important, iterations = PERF_EDIT_ITERATIONS, self_timed)]
fn source_mode_single_row_edit_large_markdown_length_change() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let target_row = middle_row_containing(&text, "source-row");
    let (editor, cx) = open_source_perf_window(&mut cx, &text);
    let mut shortened = false;

    warm_draw(&editor, cx);
    measure_self_timed_iterations(|| {
        let (from, to) = if shortened {
            ("row", "source-row")
        } else {
            ("source-row", "row")
        };
        reset_layout_computation_counts(&editor, cx);
        let start = std::time::Instant::now();
        replace_middle_row_word(&editor, cx, target_row, from, to);
        warm_draw(&editor, cx);
        let elapsed = start.elapsed();
        shortened = !shortened;
        elapsed
    });

    editor.update(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
        assert!(editor.row_count() > 0);
    });
}

#[perf(important, iterations = PERF_RESIZE_ITERATIONS, self_timed)]
fn rendered_mode_resize_large_markdown() {
    let mut cx = gpui::TestAppContext::single();
    let text = large_plain_markdown_fixture();
    let (editor, cx) = open_rendered_perf_window(&mut cx, &text);
    let mut target_is_narrow = true;

    warm_draw(&editor, cx);
    measure_self_timed_iterations(|| {
        let (baseline_width, target_width) = if target_is_narrow {
            (PERF_WINDOW_WIDTH, PERF_NARROW_WINDOW_WIDTH)
        } else {
            (PERF_NARROW_WINDOW_WIDTH, PERF_WINDOW_WIDTH)
        };
        cx.simulate_resize(size(px(baseline_width), px(PERF_WINDOW_HEIGHT)));
        clear_editor_layout_caches(&editor, cx);
        warm_draw(&editor, cx);
        reset_layout_computation_counts(&editor, cx);

        let start = std::time::Instant::now();
        cx.simulate_resize(size(px(target_width), px(PERF_WINDOW_HEIGHT)));
        warm_draw(&editor, cx);
        let elapsed = start.elapsed();
        target_is_narrow = !target_is_narrow;
        elapsed
    });
    report_layout_computation_counts("rendered large resize", &editor, cx);

    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

// Deferred: mixed-content perf cases for inline image / inline math / image
// block / formula block remain out of scope for the current source-row perf
// pass. This includes draw, cached redraw, and scroll coverage with atoms.
