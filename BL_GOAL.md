# Markdown Editor Layout Refactor Progress

## Objective

Refactor the current project's `markdown-editor` path so Source and Rendered modes support width-aware automatic wrapping, and so Rendered mode can eventually lay out inline and block GPUI elements as first-class content. The final state must keep text, Markdown styling, images, previews, custom GPUI elements, cursor, selections, hit testing, keyboard movement, scrolling, and mode switching consistent with the visible layout without unnecessary whole-document reflow.

## Current Progress

### 2026-05-24 - Width-aware text row geometry foundation

- Scope: `crates/md_editor/src/lib.rs`.
- Added a text visual-row model on top of existing virtualized source rows:
  - `DisplayRowTextLayout` stores styled display segments, GPUI-shaped text, and soft-wrapped `VisualDisplayRow` ranges.
  - Source and Rendered text rows now compute wrap boundaries from `Window::text_system().shape_text(..., Some(wrap_width), None)`.
  - The list item height for normal text rows now expands to the number of visual rows instead of forcing `.whitespace_nowrap()` at the whole row level.
  - Rendered semantic text runs are reused for shaping, so heading/bold/italic/code/link styling participates in wrap measurement.
  - Caret placement, selection highlight bounds, and mouse down/drag hit testing now consume the same visual-row ranges used for rendering.
  - Window width changes are tracked with `last_text_wrap_width`; the list is remeasured only when the text wrap width actually changes.
- Existing row-level remote image replacement is preserved. Full block element measurement is not solved yet.

### 2026-05-24 - Visual-row-aware keyboard vertical movement

- Scope: `crates/md_editor/src/lib.rs`.
- Routed `MoveUp`, `MoveDown`, `SelectUp`, and `SelectDown` through layout-aware movement when the current and target rows are normal text rows.
- Keyboard vertical movement now:
  - Moves within soft-wrapped visual rows before crossing source rows.
  - Preserves the visible x position when moving between visual rows, including repeated Up/Down over shorter wrapped rows via `SelectionGoal`.
  - Maps target display offsets back through the Markdown projection map so Rendered-mode hidden markers stay consistent with cursor placement.
  - Falls back to the existing source-row movement for block image rows or unavailable layout targets.
- Added test coverage for visual-row index selection at soft-wrap boundaries and vertical movement x-goal reuse.

### 2026-05-24 - Separate rendered image block path from text layout

- Scope: `crates/md_editor/src/lib.rs`.
- Moved rendered remote-image block detection ahead of text shaping in the list item renderer.
- Image block rows now skip `text_layout_for_display_row`, keeping the text wrapping path focused on text rows and leaving a cleaner branch for future measured GPUI block elements.
- Removed the now-redundant `render_row_contents` helper.

### 2026-05-25 - Unified row layout branch for text vs block content

- Scope: `crates/md_editor/src/lib.rs`.
- Added `DisplayRowLayout` and `DisplayBlockLayout` so row rendering and keyboard movement now consume a shared per-row layout decision instead of independently special-casing rendered image rows.
- Current block support still only covers remote image rows, but the control flow is now shaped around:
  - text rows with wrapped visual-row geometry, and
  - block rows with their own height and render path.
- This keeps the next step focused on replacing placeholder block handling with measured GPUI block elements rather than reworking the row pipeline again.

### 2026-05-25 - Wrap-boundary safety fallback

- Scope: `crates/md_editor/src/lib.rs`.
- Hardened `visual_rows_for_wrapped_line` so malformed or unexpected wrap-boundary data falls back to a single visual row instead of directly indexing GPUI glyph arrays.
- This makes the current wrapping path more resilient while the richer inline/block fragment layout is still being built.

### 2026-05-25 - Conservative per-row layout cache

- Scope: `crates/md_editor/src/lib.rs`.
- Added a `row_layout_cache` keyed by source row, editor mode, wrap width, and Rendered-mode active source range.
- Render, mouse hit testing, and visual-row keyboard movement now reuse the same cached `DisplayRowLayout` instead of independently reshaping the same row text.
- Current invalidation is intentionally conservative:
  - clear on edits,
  - clear on mode changes,
  - clear on wrap-width changes,
  - clear when the Rendered-mode active source range changes because marker visibility can change.
- This does not finish the final performance story, but it removes a clear source of repeated row shaping and gives the future inline/block fragment layout a shared cache entry point.

### 2026-05-25 - Remote image block height follows loaded asset size

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered remote image block rows now compute a `RenderedImageBlockLayout` with explicit width and height instead of rendering every image into a fixed placeholder box.
- The image layout path now:
  - uses GPUI's `ImgResourceLoader` through `Window::use_asset`, so the view is notified when the remote image finishes loading,
  - preserves the image aspect ratio from `RenderImage::size(0)` once the asset is available,
  - falls back to the existing conservative placeholder height while loading or when image dimensions are unavailable,
  - avoids caching block layouts in the text row layout cache so placeholder heights are not reused after the asset loads, and
  - tracks block row height changes and remeasures only the affected list row.
- Added pure helper tests for aspect-ratio height calculation and invalid zero-size image dimensions.
- This is still only the remote-image block path. General Rendered-mode GPUI block measurement and inline GPUI element layout remain open.

## Verification

- `cargo fmt -p md_editor -p markdown_editor` passed.
- `cargo test -p md_editor` passed: 33/33 tests.
- `cargo check -p markdown_editor` passed.

## Known Remaining Work

- Implement a real Rendered-mode fragment model for inline GPUI elements embedded in text flow, including measuring element width/height, line breaking with text, max-height visual row computation, and hit-test/caret mapping around element spans.
- Implement general block-level GPUI elements as measured list items or subitems. Remote image blocks now update from loaded image dimensions, but arbitrary GPUI block measurement is not solved.
- Add stronger runtime or visual tests for visual-row keyboard movement, especially Rendered-mode marker reveal/hide transitions.
- Add stronger runtime or visual tests for resize reflow, wrapped hit testing, selection across visual rows, mode switching at wrapped positions, and Rendered image/block behavior.
- Add performance-oriented caching/invalidation so large-document wrapping reuses per-row layout and avoids whole-document reflow except when global width changes.
