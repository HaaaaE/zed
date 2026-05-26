use super::test_support::*;
use gpui::{TestApp, px, size};
use util_macros::perf;

const LARGE_MARKDOWN_TARGET_BYTES: usize = 300 * 1024;

fn large_markdown_fixture() -> String {
    let mut text = String::with_capacity(LARGE_MARKDOWN_TARGET_BYTES + 1024);
    let paragraph = "Before ![alt](https://example.com/cat.png) after **bold** text and `$x + y$` inline math repeated for wrapping.\n";
    let block = "## Heading\n\n";
    let formula = "$$x + y = z$$\n\n";

    while text.len() < LARGE_MARKDOWN_TARGET_BYTES {
        text.push_str(block);
        text.push_str(paragraph);
        text.push_str(paragraph);
        text.push_str(formula);
    }

    text
}

#[perf(important, iterations = 1)]
fn source_mode_draw_large_markdown() {
    let mut app = TestApp::new();
    let text = large_markdown_fixture();
    let mut window = app.open_window(|_, cx| MarkdownEditor::for_text(text.clone(), cx));

    window.simulate_resize(size(px(900.), px(700.)));
    window.draw();
    app.run_until_parked();

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Source);
    });
}

#[perf(important, iterations = 1)]
fn rendered_mode_draw_large_markdown() {
    let mut app = TestApp::new();
    let text = large_markdown_fixture();
    let mut window = app.open_window(|_, cx| {
        let mut editor = MarkdownEditor::for_text(text.clone(), cx);
        editor.set_mode(MarkdownEditorMode::Rendered, cx);
        editor
    });

    window.simulate_resize(size(px(900.), px(700.)));
    window.draw();
    app.run_until_parked();

    window.read(|editor, _| {
        assert_eq!(editor.mode(), MarkdownEditorMode::Rendered);
    });
}
