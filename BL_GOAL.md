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

### 2026-05-25 - Remote image block mouse hit testing

- Scope: `crates/md_editor/src/lib.rs`.
- Remote image block rows now retain the source range for the Markdown image syntax.
- The rendered image element now handles left mouse down and drag events:
  - clicking or dragging on the left half maps to the image source range start,
  - clicking or dragging on the right half maps to the image source range end,
  - shift-click and drag selection use the same selection update paths as text rows.
- Added a pure helper test for image block mouse x-position mapping.
- This improves block row cursor/selection consistency, but does not replace the need for a general fragment hit-test model for arbitrary GPUI inline and block elements.

### 2026-05-25 - Remote image block keyboard vertical movement

- Scope: `crates/md_editor/src/lib.rs`.
- Keyboard vertical movement now handles remote image block rows instead of falling back to source-row-only movement whenever the current or target row is a block.
- Moving from text into a rendered image block maps the preserved visible x position onto the image source range:
  - left half maps to the Markdown image source range start,
  - right half maps to the Markdown image source range end.
- Moving out of a rendered image block derives the desired visible x position from the cursor's location within that image source range, then reuses the text-row visual movement path.
- Added pure helper tests for image block local x-to-source mapping and source offset-to-visible x mapping.
- This keeps remote-image block cursor movement consistent with the mouse hit-test path, but still does not implement arbitrary GPUI fragment hit testing or inline element layout.

### 2026-05-25 - Inline fragment foundation for rendered text rows

- Scope: `crates/md_editor/src/lib.rs` and `crates/markdown_wysiwyg/src/markdown_wysiwyg.rs`.
- Replaced the text-row render path's direct styled-segment storage with a `DisplayInlineFragment` model:
  - text fragments still render through the existing styled text path,
  - inline atom fragments can carry source/display ranges, fallback text, and styling for future GPUI element measurement.
- Added an `InlineMath` atom fragment for inactive Rendered-mode inline math. It currently renders via fallback text, preserving the existing visual output while giving the layout cache an explicit atom boundary to replace with measured GPUI inline content later.
- Rendered text rows now render from fragments directly and derive text runs from flattened fragments for the current GPUI text shaping path.
- Fixed inline math projection in `markdown_wysiwyg` so math operators such as `+` are kept as content instead of being hidden as unnamed marker nodes.
- Added tests for inline math projection and for rendered inline math fragment creation/flattening.
- This is a structural step toward inline GPUI elements participating in text flow; actual inline element measurement, line-height maxing, hit testing around inline atoms, and GPUI element rendering are still open.

### 2026-05-25 - Inline atom height participates in visual-row layout

- Scope: `crates/md_editor/src/lib.rs`.
- Inline atom fragments now carry a layout height, currently used by inactive Rendered-mode inline math atoms.
- `VisualDisplayRow` now stores its own height instead of assuming every wrapped row has the text line height.
- Text row layout height now sums per-visual-row heights, and row rendering plus selection backgrounds use the visual row height.
- Wrapped and fallback visual-row construction now maxes each row's text line height with any inline atom height overlapping that row.
- Added a focused pure helper test that verifies only the visual row containing an inline atom expands.
- This is still using fallback text for atom width and wrapping, so true inline GPUI element measurement, atomic line breaking, and atom-specific hit testing remain open.

### 2026-05-25 - Inline atoms render through a distinct element path

- Scope: `crates/md_editor/src/lib.rs`.
- Split visual-row fragment rendering by fragment type:
  - text fragments still render through the existing styled text helper,
  - inline atom fragments now render through a dedicated atom helper.
- Added `fragment_text_for_visual_row` so both text and atom fragments use the same visible-range clipping logic.
- Inactive Rendered-mode inline math atoms now render inside their own GPUI container with atom height and a subtle math background while keeping fallback text width unchanged.
- Added a focused pure helper test for clipping fragment text to the active visual row.
- This creates a real replacement point for future measured inline GPUI elements, but atom width is still fallback-text-shaped and atom-aware line breaking/hit testing remain open.

### 2026-05-25 - Inline atoms stay atomic across soft wraps

- Scope: `crates/md_editor/src/lib.rs`.
- Visual-row construction now adjusts GPUI wrap boundaries that land inside an inline atom:
  - if the atom can fit after previous content, the boundary moves before the atom,
  - if the atom starts the current visual row, the boundary moves after the atom instead of splitting it.
- The next visual row's `line_start_x` is recomputed from the adjusted display boundary, keeping caret and selection geometry aligned with the adjusted row ranges.
- Added a focused pure helper test for atom range detection and atomic wrap-boundary adjustment.
- This prevents inline atom render fragments from being split across two visual rows, but cursor and mouse mapping inside atom bounds still need an atom-aware snap policy.

### 2026-05-25 - Inline atom hit targets snap to atom boundaries

- Scope: `crates/md_editor/src/lib.rs`.
- Mouse hit testing and visual-row keyboard movement now share `display_offset_for_visual_row_x`, so both paths use the same atom-aware x-to-display-offset mapping.
- When a target x position falls inside an inline atom's rendered fallback bounds, the target display offset now snaps to the atom start or end based on the atom midpoint.
- The soft-wrap end-of-row fallback now preserves a row-end offset when that row ends at an atom boundary, avoiding a snap back into the atom text.
- Added focused pure helper tests for inline atom midpoint snapping and atom end-boundary detection.
- This keeps inactive inline math atoms from accepting cursor positions in their interior for mouse/vertical movement. Horizontal movement through inactive atom source content and richer atom selection visuals still remain open.

## Verification

- `cargo fmt -p markdown_wysiwyg -p md_editor -p markdown_editor` passed.
- `cargo test -p markdown_wysiwyg` passed: 12/12 tests.
- `cargo test -p md_editor` passed: 42/42 tests.
- `cargo check -p markdown_editor` passed.

## Known Remaining Work

- Implement a real Rendered-mode fragment model for inline GPUI elements embedded in text flow, including measuring element width/height, line breaking with text, max-height visual row computation, and hit-test/caret mapping around element spans.
- Implement general block-level GPUI elements as measured list items or subitems. Remote image blocks now update from loaded image dimensions, but arbitrary GPUI block measurement is not solved.
- Add stronger runtime or visual tests for visual-row keyboard movement, especially Rendered-mode marker reveal/hide transitions.
- Add stronger runtime or visual tests for resize reflow, wrapped hit testing, selection across visual rows, mode switching at wrapped positions, and Rendered image/block behavior.
- Add performance-oriented caching/invalidation so large-document wrapping reuses per-row layout and avoids whole-document reflow except when global width changes.
