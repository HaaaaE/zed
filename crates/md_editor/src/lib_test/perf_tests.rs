use super::test_support::*;
use gpui::{TestApp, px, size};
use util_macros::perf;

const LARGE_MARKDOWN_TARGET_BYTES: usize = 300 * 1024;
const SHORT_MARKDOWN_TARGET_BYTES: usize = 5 * 1024;
const PERF_WINDOW_WIDTH: f32 = 900.;
const PERF_WINDOW_HEIGHT: f32 = 700.;
const PERF_NARROW_WINDOW_WIDTH: f32 = 560.;
const SCROLL_STEP_PIXELS: f32 = 168.;
const SCROLL_STEPS: usize = 12;
const PERF_ITERATIONS: usize = 5;

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

fn open_source_perf_window(app: &mut TestApp, text: &str) -> gpui::TestAppWindow<MarkdownEditor> {
    let text = text.to_string();
    let mut window = app.open_window(move |_, cx| MarkdownEditor::for_text(text.clone(), cx));
    window.simulate_resize(size(px(PERF_WINDOW_WIDTH), px(PERF_WINDOW_HEIGHT)));
    window
}

fn open_rendered_perf_window(app: &mut TestApp, text: &str) -> gpui::TestAppWindow<MarkdownEditor> {
    let text = text.to_string();
    let mut window = app.open_window(move |_, cx| {
        let mut editor = MarkdownEditor::for_text(text.to_string(), cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });
    window.simulate_resize(size(px(PERF_WINDOW_WIDTH), px(PERF_WINDOW_HEIGHT)));
    window
}

fn warm_draw(window: &mut gpui::TestAppWindow<MarkdownEditor>, app: &mut TestApp) {
    window.draw();
    app.run_until_parked();
}

fn reset_layout_computation_counts(window: &mut gpui::TestAppWindow<MarkdownEditor>) {
    window.update(|editor, _, _| editor.reset_layout_computation_counts());
}

fn report_layout_computation_counts(label: &str, window: &mut gpui::TestAppWindow<MarkdownEditor>) {
    let counts = window.read(|editor, _| editor.layout_computation_counts());
    eprintln!("{label}: {counts:?}");
}

fn scroll_and_draw(
    window: &mut gpui::TestAppWindow<MarkdownEditor>,
    app: &mut TestApp,
    steps: usize,
    step_pixels: f32,
) {
    for _ in 0..steps {
        window.update(|editor, _, _| {
            let current = editor.display_list_state.logical_scroll_top();
            editor.display_list_state.scroll_to(ListOffset {
                item_ix: current.item_ix,
                offset_in_item: current.offset_in_item + px(step_pixels),
            });
        });
        window.draw();
        app.run_until_parked();
    }
}

fn scroll_same_region_twice(window: &mut gpui::TestAppWindow<MarkdownEditor>, app: &mut TestApp) {
    let initial_offset =
        window.update(|editor, _, _| editor.display_list_state.logical_scroll_top());
    reset_layout_computation_counts(window);
    scroll_and_draw(window, app, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    report_layout_computation_counts("first scroll", window);
    window.update(|editor, _, _| {
        editor.display_list_state.scroll_to(initial_offset);
    });
    warm_draw(window, app);
    reset_layout_computation_counts(window);
    scroll_and_draw(window, app, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    report_layout_computation_counts("second scroll", window);
}

fn replace_middle_row_word(
    window: &mut gpui::TestAppWindow<MarkdownEditor>,
    target_row: u32,
    from: &str,
    to: &str,
) {
    window.update(|editor, _, cx| {
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

#[perf(important, iterations = PERF_ITERATIONS)]
fn source_mode_draw_large_markdown() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let mut window = open_source_perf_window(&mut app, &text);

    reset_layout_computation_counts(&mut window);
    warm_draw(&mut window, &mut app);
    report_layout_computation_counts("source large draw", &mut window);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn rendered_mode_draw_large_markdown() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let mut window = open_rendered_perf_window(&mut app, &text);

    reset_layout_computation_counts(&mut window);
    warm_draw(&mut window, &mut app);
    report_layout_computation_counts("rendered large draw", &mut window);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn source_mode_redraw_large_markdown_cached() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let mut window = open_source_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    reset_layout_computation_counts(&mut window);
    warm_draw(&mut window, &mut app);
    report_layout_computation_counts("source large cached redraw", &mut window);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn rendered_mode_redraw_large_markdown_cached() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let mut window = open_rendered_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    reset_layout_computation_counts(&mut window);
    warm_draw(&mut window, &mut app);
    report_layout_computation_counts("rendered large cached redraw", &mut window);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn source_mode_scroll_short_markdown() {
    let mut app = TestApp::new();
    let text = short_plain_markdown_fixture();
    let mut window = open_source_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    reset_layout_computation_counts(&mut window);
    scroll_and_draw(&mut window, &mut app, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    report_layout_computation_counts("source short first scroll", &mut window);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn source_mode_scroll_large_markdown() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let mut window = open_source_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    reset_layout_computation_counts(&mut window);
    scroll_and_draw(&mut window, &mut app, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    report_layout_computation_counts("source large first scroll", &mut window);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn source_mode_scroll_short_markdown_cached_region() {
    let mut app = TestApp::new();
    let text = short_plain_markdown_fixture();
    let mut window = open_source_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    scroll_same_region_twice(&mut window, &mut app);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn source_mode_scroll_large_markdown_cached_region() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let mut window = open_source_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    scroll_same_region_twice(&mut window, &mut app);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn rendered_mode_scroll_short_markdown() {
    let mut app = TestApp::new();
    let text = short_plain_markdown_fixture();
    let mut window = open_rendered_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    reset_layout_computation_counts(&mut window);
    scroll_and_draw(&mut window, &mut app, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    report_layout_computation_counts("rendered short first scroll", &mut window);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn rendered_mode_scroll_large_markdown() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let mut window = open_rendered_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    reset_layout_computation_counts(&mut window);
    scroll_and_draw(&mut window, &mut app, SCROLL_STEPS, SCROLL_STEP_PIXELS);
    report_layout_computation_counts("rendered large first scroll", &mut window);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn rendered_mode_scroll_short_markdown_cached_region() {
    let mut app = TestApp::new();
    let text = short_plain_markdown_fixture();
    let mut window = open_rendered_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    scroll_same_region_twice(&mut window, &mut app);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn rendered_mode_scroll_large_markdown_cached_region() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let mut window = open_rendered_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    scroll_same_region_twice(&mut window, &mut app);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn source_mode_single_row_edit_large_markdown() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let target_row = middle_row_containing(&text, "source-row");
    let mut window = open_source_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    replace_middle_row_word(&mut window, target_row, "source-row", "source_rows");
    warm_draw(&mut window, &mut app);

    window.update(|editor, _, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
        assert!(editor.row_count() > 0);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn source_mode_single_row_edit_large_markdown_length_change() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let target_row = middle_row_containing(&text, "source-row");
    let mut window = open_source_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    replace_middle_row_word(&mut window, target_row, "source-row", "row");
    warm_draw(&mut window, &mut app);

    window.update(|editor, _, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
        assert!(editor.row_count() > 0);
    });
}

#[perf(important, iterations = PERF_ITERATIONS)]
fn rendered_mode_resize_large_markdown() {
    let mut app = TestApp::new();
    let text = large_plain_markdown_fixture();
    let mut window = open_rendered_perf_window(&mut app, &text);

    warm_draw(&mut window, &mut app);
    reset_layout_computation_counts(&mut window);
    window.simulate_resize(size(px(PERF_NARROW_WINDOW_WIDTH), px(PERF_WINDOW_HEIGHT)));
    warm_draw(&mut window, &mut app);
    report_layout_computation_counts("rendered large resize", &mut window);

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}

// Deferred: mixed-content perf cases for inline image / inline math / image
// block / formula block remain out of scope for the current source-row perf
// pass. This includes draw, cached redraw, and scroll coverage with atoms.
