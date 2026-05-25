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
- This keeps inactive inline math atoms from accepting cursor positions in their interior for mouse/vertical movement. Richer atom selection visuals still remain open.

### 2026-05-25 - Inline atoms are skipped by rendered horizontal movement

- Scope: `crates/md_editor/src/lib.rs`.
- `MoveLeft`, `MoveRight`, `SelectLeft`, and `SelectRight` now route through mode-aware helpers.
- Rendered-mode horizontal movement now treats inactive inline math atoms as atomic:
  - moving right from the atom source start jumps to the atom source end,
  - moving left from the atom source end jumps to the atom source start,
  - shift-selection variants extend selection across the same source range.
- Source mode still uses normal character movement, and cursors already inside inline math content continue to move character-by-character so active source editing remains available.
- Added focused tests for rendered atom skipping, rendered atom range selection, active inline math character movement, and Source-mode character movement.
- This closes the immediate keyboard-horizontal gap for inactive inline atoms, but atom selection is still rendered with the existing source-range highlight rather than a richer atom-specific visual.

### 2026-05-25 - Inline atoms are deleted atomically at rendered boundaries

- Scope: `crates/md_editor/src/lib.rs`.
- `Backspace` and `Delete` now route through mode-aware helpers.
- Rendered-mode deletion now treats inactive inline math atoms as atomic at their boundaries:
  - Backspace from the atom source end deletes the whole atom source range,
  - Delete from the atom source start deletes the whole atom source range.
- Non-empty selections, Source mode, and cursors already inside inline math content continue to use the existing source-level deletion behavior.
- Added focused tests for rendered Backspace/Delete atom deletion, active inline math character deletion, and Source-mode character deletion.
- This keeps keyboard deletion consistent with Rendered-mode horizontal atom navigation, while richer atom-specific selection visuals remain open.

### 2026-05-25 - Whole-atom selection keeps rendered atom display

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered-mode projection now treats a non-empty selection that exactly matches an inline math atom's source range as an atom selection rather than an active editing range.
- Selecting a whole inline math atom now keeps marker ranges hidden, so the row continues to display the atom fallback text and the selected display range maps to the rendered atom content.
- Partial selections inside inline math still reveal the source markers, preserving the existing editing path for active math content.
- Added focused tests for whole-atom selection staying inactive and partial atom selection still revealing source syntax.
- This keeps keyboard selection display consistent with atom-aware horizontal movement, while richer atom-specific selection visuals remain open.

### 2026-05-25 - Inline atom boundary cursors keep rendered atom display

- Scope: `crates/md_editor/src/lib.rs`.
- Collapsed Rendered-mode cursors at an inactive inline math atom's source start now keep the atom inactive instead of immediately revealing `$...$` markers.
- Cursors at the atom source end already remain inactive through the existing non-overlap projection behavior; this is now covered alongside the source-start case.
- Moving the cursor into inline math content still reveals markers, preserving the source editing path once the cursor is actually inside the atom.
- Added focused tests for atom-boundary cursor display and content cursor marker reveal.
- This keeps the display state consistent with atom-aware horizontal movement at atom boundaries.

### 2026-05-25 - Visual line boundaries drive Home/End on wrapped rows

- Scope: `crates/md_editor/src/lib.rs`.
- `MoveToBeginningOfLine`, `MoveToEndOfLine`, `SelectToBeginningOfLine`, and `SelectToEndOfLine` now use the current visual row when wrapped layout is available, falling back to the source-line helpers when layout cannot be resolved.
- Text rows map the current visual row's start or end display offset back through the Markdown projection, so Source and Rendered modes keep hidden marker handling consistent with the visible row.
- Remote image block rows map Home/End to the rendered image source range edges, matching the existing block mouse and vertical movement behavior.
- `SelectionGoal::WrappedHorizontalPosition` now disambiguates caret ownership at soft-wrap boundaries, so a caret at the end of a wrapped visual row can still render and continue moving from that visual row instead of defaulting to the next row's start.
- Added focused pure helper tests for wrapped-boundary caret ownership and visual-row Home/End targets.

### 2026-05-25 - Inline atom widths feed fragment-aware wrapping

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered inline atom fragments now carry an explicit width and rows containing atoms wrap through GPUI `LineFragment` data instead of only shaping the fallback text string.
- Inline atom fragments are converted to `LineFragment::element(width, len_utf8)` so the GPUI line wrapper can keep their measured width atomic while text fragments still wrap as text.
- Caret, selection, mouse hit testing, and visual-row keyboard x calculations now use shared helpers that translate between display offsets and x positions with atom width deltas applied.
- Inline atom render containers use the same explicit width used by wrapping and hit testing, keeping geometry ready for future real GPUI inline element measurement.
- Current atom width is still derived from the fallback shaped text width; replacing that with actual GPUI element measurement remains open.
- Added focused helper tests for inline atom `LineFragment::element` conversion and invalid fragment range fallback.

### 2026-05-25 - Inline atom box width includes padding

- Scope: `crates/md_editor/src/lib.rs`.
- Inline math atom width now represents the whole rendered element box instead of only the fallback text content width.
- The width helper adds the atom's horizontal padding to the shaped fallback content width, and the render path applies the same padding inside the fixed-width atom container.
- Wrapping, caret x mapping, selection bounds, hit testing, and rendered atom width now all consume the same padded atom geometry.
- Added a focused helper test for the padded inline atom width formula.
- The content width is still derived from fallback text shaping; measuring real GPUI inline element content remains open.

### 2026-05-25 - Inline atom width measured from rendered element

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered text rows now measure inline atom width from the same GPUI element tree used for rendering, including atom padding, font family, text size, line height, and nowrap behavior.
- The measured width is used before fragment-aware wrapping, so inline atom line breaks, caret x mapping, selection bounds, and mouse/keyboard hit targets consume the rendered element box width instead of only the shaped fallback text width.
- Measurement is only performed from the render/prepaint layout path because `layout_as_root` is phase-sensitive in GPUI.
- Event paths reuse a cached measured layout when available. If a row has not yet been measured by rendering, they compute a non-cached fallback layout from shaped text width so mouse and keyboard handling do not call `layout_as_root` outside the allowed phase.

### 2026-05-25 - Inline atom measurement returns full element size

- Scope: `crates/md_editor/src/lib.rs`.
- The inline atom measurement path now returns a full GPUI element size instead of only a width.
- Rendered inline atom fragments update both `width` and `height` from that measured size before visual-row construction, so visual-row height can follow the rendered atom element as richer inline atoms are introduced.
- Fallback event-path layout still assigns width and height from shaped text plus the existing inline math minimum height, and measured layout keeps those fallback dimensions as lower bounds.
- The render path now fixes both width and height from the cached atom size, while the measurement path leaves dimensions unconstrained except for the atom's minimum height.

### 2026-05-25 - Image block hit target covers full block row

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered remote image block mouse handlers now live on the full block-row container instead of only the inner image rectangle.
- Clicking or dragging in the image block's right-side empty area now maps to the image source range end, while the left side still maps to the source range start.
- Added coverage for mouse x positions beyond the rendered image width so block-row hit testing remains consistent with the displayed block footprint.

### 2026-05-25 - Rendered element boundaries stay inactive

- Scope: `crates/md_editor/src/lib.rs`.
- Collapsed Rendered-mode cursors at the source start or source end of rendered elements now keep those elements inactive instead of immediately revealing Markdown source markers.
- This generalizes the previous inline math boundary behavior to remote image block elements, so clicking the left or right edge of a rendered image block keeps the block rendered while still placing the cursor at the source range edge.
- Added coverage for remote image block source-boundary cursors and for inline images with surrounding text staying source-editable at their boundaries.

### 2026-05-25 - Image block layout height includes padding

- Scope: `crates/md_editor/src/lib.rs`.
- Remote image block layout now distinguishes the inner rendered image height from the full block height.
- The block height now includes the same vertical padding used by the rendered element, keeping list measurement, scroll geometry, and block hit area aligned with the actual GPUI element.
- Added coverage for the padded block-height calculation.

### 2026-05-25 - Whole image block selection stays rendered

- Scope: `crates/md_editor/src/lib.rs`.
- Generalized the whole-rendered-element selection check so an exact non-empty selection of a standalone remote image block behaves like an exact inline math atom selection.
- Selecting the full Markdown image source range for a standalone remote image block now keeps the row projected as the rendered image block instead of revealing the image syntax.
- Inline images with surrounding text still reveal source when selected, so only true standalone block images use the rendered block selection behavior.
- Added coverage for whole image-block selection and for the inline-image whole-selection guard.

### 2026-05-25 - Image block boundaries delete atomically

- Scope: `crates/md_editor/src/lib.rs`.
- Reused a single rendered-element source-range helper for whole-element selection, source-boundary inactivity, and Rendered-mode boundary deletion.
- Backspace at the source end of a standalone rendered remote image block now deletes the whole image source range.
- Delete at the source start of a standalone rendered remote image block now deletes the whole image source range.
- Inline images with surrounding text still use source character deletion, preserving the source-editing path for non-block images.
- Added coverage for image-block Backspace/Delete and for inline-image character deletion.

### 2026-05-25 - Image block boundaries move atomically

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered-mode horizontal movement now reuses the same rendered-element boundary helper used by selection display and deletion.
- MoveRight from the source start of a standalone rendered remote image block jumps to the image source range end.
- MoveLeft from the source end of a standalone rendered remote image block jumps to the image source range start.
- Shift+Left and Shift+Right extend selection across the same full image source range.
- Inline images with surrounding text still use source character movement, preserving non-block image editing behavior.
- Added coverage for image-block horizontal movement, image-block horizontal selection, and inline-image character movement.

### 2026-05-25 - Whole image block selection has visible state

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered image block rendering now receives a selected state derived from the selection source range.
- When the full standalone image source range is selected, the rendered block keeps its image layout and applies a selection-tinted row background plus selected border color.
- Partial selections and collapsed cursors do not trigger the selected block styling.
- Added coverage for the image-block whole-selection state helper.

### 2026-05-25 - Image block boundary cursors are visible

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered image block rendering now receives an optional caret x position derived from the collapsed selection.
- Collapsed cursors at the source start or source end of a standalone rendered remote image block now draw the existing caret element at the block's left or right edge.
- Non-empty selections and cursors inside the image source range do not draw a block caret.
- Added coverage for image-block boundary caret x calculation.

### 2026-05-25 - Whole inline atom selection has visible state

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered inline atom rendering now receives the current selected display range for visual-row rendering.
- When a selection fully covers an inline math atom's rendered display range, the atom keeps its rendered layout and applies the editor selection background and selection text color.
- Partial selections and empty selections do not trigger the atom selected styling, preserving the active source-editing path for partial math selections.
- Measurement rendering still uses the unselected atom style, so selected styling does not affect cached atom dimensions.
- Added coverage for the inline-atom selected-state helper.

### 2026-05-25 - Contained rendered element selections stay rendered

- Scope: `crates/markdown_wysiwyg/src/markdown_wysiwyg.rs` and `crates/md_editor/src/lib.rs`.
- Added a projection path that accepts inactive source ranges inside an otherwise active selection range.
- Rendered-mode selections that fully contain inline math atoms or standalone remote image blocks now keep those elements rendered instead of revealing their source markers.
- Other Markdown covered by the same larger selection still uses the active projection path, so normal source-editing behavior is preserved outside the fully contained rendered elements.
- Image block selected styling now treats any selection containing the full standalone image source range as a selected block, not only an exact image-only selection.
- Added projection coverage for inactive override ranges and md_editor coverage for contained inline atom/image block selections.

### 2026-05-25 - Inline atom display boundaries map to source boundaries

- Scope: `crates/md_editor/src/lib.rs`.
- Mouse hit testing, visual-row vertical movement, and visual-row Home/End now map inline atom display boundaries through an atom-aware source-offset helper.
- The rendered inline math atom's display start maps to the full `$...$` source range start, and its display end maps to the source range end.
- This keeps clicks and visual movement on the atom's left side from accidentally placing the cursor inside the math content and revealing source markers.
- Added coverage for inline atom display-boundary to source-boundary mapping.

### 2026-05-25 - Contained inline atom selection state is covered

- Scope: `crates/md_editor/src/lib.rs`.
- Strengthened the contained inline math selection coverage so a larger Rendered-mode selection still maps to the rendered display range while the fully contained atom reports selected state.
- This locks the interaction between inactive rendered-element override ranges, row selection geometry, and inline atom selected styling without changing runtime behavior.

### 2026-05-25 - Wrapped row-end mouse hits keep visual-row ownership

- Scope: `crates/md_editor/src/lib.rs`.
- Mouse hit testing on wrapped text rows now stores a `WrappedHorizontalPosition` goal with the clicked visual-row index.
- Non-final soft-wrap row-end clicks can now return the real display-row boundary while still rendering and continuing movement from the clicked visual row instead of being reassigned to the next visual row.
- Removed the old row-end backtracking fallback and its now-unused inline atom boundary helper.
- Added regression coverage for clicking the end of a non-final wrapped visual row and preserving visual-row ownership through the selection goal.

### 2026-05-25 - Image block mouse hits preserve visible x goal

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered remote image block mouse down and drag paths now store a `WrappedHorizontalPosition` goal derived from the visible image caret edge.
- Image block hit testing still maps left/right block regions to the image source range start/end, but subsequent vertical movement can now reuse the visible x position from the mouse interaction.
- Added coverage for image block mouse targets returning both the source boundary point and the matching visible caret x goal, including clicks in the right-side block area beyond the image width.

### 2026-05-25 - Empty selected rows have visible selection bounds

- Scope: `crates/md_editor/src/lib.rs`.
- Text-row selection rendering now routes through a shared visual-row bounds helper.
- Empty visual rows that are actually covered by a multi-row selection now draw a minimal 1px selection marker instead of disappearing.
- The empty-row intersection check preserves half-open selection semantics, so a selection ending at the empty row's start does not mark that row selected.
- Added coverage for selected empty rows and for non-empty visual-row boundary touches that should not produce a zero-width selection marker.

### 2026-05-25 - Mode switches drop stale visual-row goals

- Scope: `crates/md_editor/src/lib.rs`.
- Switching between Source and Rendered modes now clears the current selection goal while preserving the selection shape.
- This prevents a `WrappedHorizontalPosition` visual-row index captured in one layout mode from influencing caret ownership, Home/End, or subsequent vertical movement after the mode's projected row layout changes.
- Added coverage that clearing the goal preserves selection id, range, and direction.

### 2026-05-25 - Resize reflow drops stale visual-row goals

- Scope: `crates/md_editor/src/lib.rs`.
- Text wrap width changes now clear the current selection goal before the row layout cache is cleared and list rows are remeasured.
- This prevents a `WrappedHorizontalPosition` visual-row index captured before a resize from influencing caret ownership, Home/End, or subsequent vertical movement after soft-wrap rows are recomputed.
- Added coverage that width changes clear the goal once while preserving selection shape, and unchanged widths leave newly established goals intact.

### 2026-05-25 - Undo/redo history drops stale visual-row goals

- Scope: `crates/md_editor/src/lib.rs`.
- Selection history stored for edit transactions now strips layout-specific goals from both the before and after selections.
- This prevents undo/redo from restoring a `WrappedHorizontalPosition` captured under an older visual-row layout while preserving selection id, range, and direction.
- Added coverage that transaction selection state keeps the selection shape but stores `SelectionGoal::None` for both sides.

### 2026-05-25 - Rendered active-range changes drop stale visual-row goals

- Scope: `crates/md_editor/src/lib.rs`.
- Rendered-mode selection changes that alter the active source range now drop only the stale visual-row index from layout-specific selection goals before row layouts are remeasured.
- This prevents a `WrappedHorizontalPosition` captured under the previous marker reveal/hide projection from influencing caret ownership or Home/End after the row projection changes, while preserving the desired x position for continued vertical movement.
- Added coverage that active-range changes downgrade wrapped goals to horizontal goals while preserving selection shape, and unchanged active ranges leave newly established wrapped goals intact.

### 2026-05-25 - Wrapped selection bounds stay visual-row-relative

- Scope: `crates/md_editor/src/lib.rs`.
- Added focused coverage for selection bounds across multiple soft-wrapped visual rows.
- The test locks down that partial first/last visual rows and fully selected middle visual rows compute their selection rectangles relative to each visual row's own `line_start_x`.
- This strengthens the wrapped-selection geometry coverage without changing runtime behavior.

### 2026-05-25 - Wrapped mouse hit testing uses visual-row-local x

- Scope: `crates/md_editor/src/lib.rs`.
- Added focused coverage for mouse hit testing on non-first soft-wrapped visual rows.
- The test locks down that clicks on a later visual row map x positions relative to that row's `line_start_x`, including row-start and mid-row clicks.
- This strengthens wrapped hit-testing coverage without changing runtime behavior.

### 2026-05-25 - Cursor row is revealed after selection changes

- Scope: `crates/md_editor/src/lib.rs`.
- Selection changes, edits, mode switches, and explicit cursor placement now ask the list to reveal the current cursor row.
- This keeps keyboard movement, mouse selection, undo/redo, typing, and mode switching from leaving the caret on a row outside the visible list viewport.
- Added coverage that cursor-row reveal clips the target to the current list item range before scrolling.

### 2026-05-25 - Block layout geometry uses shared block interface

- Scope: `crates/md_editor/src/lib.rs`.
- Remote image block geometry now routes through `DisplayBlockLayout` for source ranges, visible x positions, x-to-point mapping, mouse hit targets, Home/End targets, caret x positions, and whole-block selected state.
- Block mouse down and drag handlers now receive the shared block layout instead of image-specific source ranges and widths.
- Removed the old high-level image-block geometry helpers, leaving the remaining image-specific x/source-offset mapping as the remote-image variant implementation detail.
- Added focused coverage for block line-boundary targets while updating existing image-block geometry tests to assert through `DisplayBlockLayout`.

### 2026-05-25 - Inline atom layout uses atom interface

- Scope: `crates/md_editor/src/lib.rs`.
- Moved inline atom height, padded width, fallback measurement, GPUI measurement rendering, normal rendering, and selected-state checks onto `DisplayInlineAtom` / `DisplayInlineAtomKind`.
- Removed the old free-function inline atom geometry and render helpers, keeping inline math as the current atom variant while giving future inline GPUI elements a shared implementation point.
- Updated existing inline atom coverage to assert through the atom interface without changing rendered behavior.

### 2026-05-25 - Inline atom hit geometry uses atom interface

- Scope: `crates/md_editor/src/lib.rs`.
- Moved inline atom line-fragment generation, display-index containment, and midpoint x-boundary snapping onto `DisplayInlineAtom`.
- Wrapping, atom-aware hit testing, and visual movement still behave the same, but future inline atom variants now have a narrower geometry surface to implement.
- Updated the focused inline atom boundary test to assert through the atom method instead of a free helper.

### 2026-05-25 - Block rendering uses shared block interface

- Scope: `crates/md_editor/src/lib.rs`.
- Moved block rendering dispatch onto `DisplayBlockLayout::render`.
- Row rendering now only distinguishes text rows from block rows; remote image rendering remains the current block variant implementation.
- This keeps selected-state and caret rendering calculation beside the rest of the block geometry interface.

### 2026-05-25 - Block row height tracking uses row layout interface

- Scope: `crates/md_editor/src/lib.rs`.
- Added a `DisplayRowLayout::block_height` accessor so the render loop no longer matches block rows directly when deciding whether a list row needs remeasurement.
- Remote image block height tracking behaves the same, but the row-level pipeline now has one shared height entry point for future block variants.

### 2026-05-25 - Block layout construction uses shared entry point

- Scope: `crates/md_editor/src/lib.rs`.
- Added `display_block_layout_for_row` as the single row-level constructor entry point for block layouts.
- Remote image blocks remain the only concrete block variant today, but `compute_display_row_layout` no longer constructs that variant inline.
- This keeps future preview/custom GPUI block variants aligned with the same row layout decision path instead of adding more ad hoc branches.

## Verification

- `cargo fmt -p markdown_wysiwyg -p md_editor -p markdown_editor` passed.
- `cargo test -p markdown_wysiwyg` passed: 13/13 tests.
- `cargo test -p md_editor` passed: 87/87 tests.
- `cargo check -p markdown_editor` passed.
- `git diff --check` passed with only the existing LF/CRLF warning for `crates/md_editor/src/lib.rs`.

## Known Remaining Work

- Generalize inline atom measurement beyond the current inactive inline math atom path, and add invalidation if future atom content can resize after the row is cached.
- Implement general block-level GPUI elements as measured list items or subitems. Remote image blocks now update from loaded image dimensions, but arbitrary GPUI block measurement is not solved.
- Add stronger runtime or visual tests for visual-row keyboard movement, especially Rendered-mode marker reveal/hide transitions.
- Add stronger runtime or visual tests for resize reflow, wrapped hit testing, selection across visual rows, mode switching at wrapped positions, and general Rendered image/block behavior.
- Add performance-oriented caching/invalidation so large-document wrapping reuses per-row layout and avoids whole-document reflow except when global width changes.
