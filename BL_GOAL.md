# Markdown Editor Layout Refactor Progress

## Objective

Refactor the current project's `markdown-editor` path so Source and Rendered modes support width-aware automatic wrapping, and so Rendered mode can eventually lay out inline and block GPUI elements as first-class content. The final state must keep text, Markdown styling, images, previews, custom GPUI elements, cursor, selections, hit testing, keyboard movement, scrolling, and mode switching consistent with the visible layout without unnecessary whole-document reflow.

## Architecture Constraints

- Keep the outer editor/list virtualization unit as a Markdown source row. Do not change the goal into a visual-row or chunk-level virtualizer unless the goal is explicitly reset.
- Optimize large-document behavior within that source-row model: cache row projection/layout work, narrow invalidation, avoid immediate whole-document remeasure, and keep non-visible rows represented by estimates.
- Accept that extremely long single source rows remain a row-local worst case. Mitigate those rows with source-row-local caching and measurement improvements rather than redesigning the outer architecture.
- Improve maintainability while completing the goal: keep new behavior behind explicit row/layout/cache concepts, avoid adding more hidden coupling between Rendered projection, GPUI list measurement, and input handling, and split responsibilities when a local extraction materially reduces risk.

## Current Progress

### Layout and Wrapping

- Source and Rendered text rows now use a shared display-row layout path with width-aware GPUI text shaping, soft-wrapped `VisualDisplayRow` ranges, visual-row heights, caret geometry, selection bounds, and mouse hit testing tied to the visible layout.
- Row rendering distinguishes text rows from block rows through `DisplayRowLayout` / `DisplayBlockLayout`, preserving source-row virtualization while giving block content its own measured height and render path.
- Wrapped Home/End, vertical movement, mouse hit testing, selection bounds, empty selected rows, mode switching, resize reflow, and undo/redo now account for visual-row-local geometry and clear stale layout-specific selection goals when the visible projection can change.
- Text rows without inline atoms skip the second wrap-shaping pass when the unwrapped shaped line already fits the current width.

### Inline Atoms and Inline Images

- Rendered text rows now build `DisplayInlineFragment`s and `DisplayInlineAtom`s, with inactive inline math and inline images represented as atomic layout content instead of ordinary editable text.
- Inline atoms participate in wrapping, row height, explicit width/height measurement, selection styling, cursor snapping, mouse hit testing, horizontal/vertical keyboard movement, Home/End, and boundary Backspace/Delete behavior.
- Inline atom width can be measured from the rendered GPUI element tree, and inline images now use loaded image dimensions via `ImgResourceLoader` when available, preserving aspect ratio within bounded inline atom geometry.
- Empty-alt inline images insert an internal object-replacement placeholder so they can use the same atom path and source/display mapping as visible-alt inline images.
- Rows with still-loading inline image atoms render with fallback sizes but avoid caching those fallback text layouts as final measured layouts.

### Block Layout and Rendered Elements

- Standalone remote images use the block layout path with loaded asset dimensions, aspect-ratio-preserving sizing, block padding, full-row hit targets, boundary carets, whole-block selection styling, and atomic boundary movement/deletion.
- Inactive non-image block content now also uses a generic block layout path through `DisplayBlockKind::Generic` and `DisplayBlockLayout::Generic`, with measured block height, block-row mouse targets, boundary carets, and cacheable row layouts.
- Rendered element boundaries and whole/contained rendered-element selections stay inactive when appropriate, so inline atoms and standalone image blocks remain rendered while surrounding selected Markdown can still reveal source syntax.
- Block geometry now routes through the shared block interface for source ranges, visible x positions, mouse targets, line-boundary movement, caret positions, and selection state.
- Remote image block layouts now expose explicit cacheability: loaded image dimensions can be cached, while placeholder layouts for still-loading or invalid image assets remain uncached.
- Render-time list remeasure was removed after reproducing a GPUI `ListState` re-entry panic; block rows are measured through the normal list layout pass.

### Caching and Large-Document Performance

- Row layouts are cached by source row, mode, wrap width, and relevant Rendered-mode active source ranges, and are reused by rendering, mouse hit testing, Home/End, and visual-row movement.
- Display rows are cached across render and interaction paths by buffer version, row, mode, and marker-visibility dependencies, with projection state built once per pass and reused for row layout cache keys.
- Rendered row inline span queries are range-local and backed by an indexed span-start structure, avoiding full-document inline span scans for each visible row.
- Source rows use text-snapshot fast paths for display-row lookup/APIs, rendering, row-local text layout, selection/caret drawing, mouse targeting, and plain-fragment projection. These paths avoid Rendered-only style/atom/hidden-range work and avoid refreshing Markdown syntax when Source callers only need text.
- Display rows carry their original source text/range so row layout paths can avoid re-reading the buffer for row-local Markdown checks.
- Rendered render frames snapshot the buffer and clip the selection once per frame, and buffer snapshots share the cached Markdown syntax tree through `Arc`.
- Text-only editor paths such as row counts, row text reads, cursor clipping, Source-mode horizontal and visual movement/selection, wrapped Home/End, ordinary replace/backspace/delete edits, cursor reveal, auto-indent, and Source edit cache invalidation now use the buffer's text snapshot directly instead of refreshing the Markdown syntax tree through a full buffer snapshot.
- Source-mode interaction paths now cache cacheable plain-text row layouts even when they do not need inline atom measurement, improving reuse for wrapped keyboard movement and wrapped Home/End.
- Rendered interaction paths now also cache row layouts whenever the computed layout is intrinsically cacheable, so plain-text Rendered rows can reuse layouts across non-render interaction calls while rows with unmeasured inline atom fallbacks stay uncached.
- Loaded remote image block layouts can participate in row-layout cache reuse, but loading placeholder block layouts stay uncached so final image dimensions can replace them.
- `ListState::with_default_size_hint` gives long variable-height lists a default unmeasured-row height, reducing scrollbar collapse and scroll-position churn before rows are measured.
- Width changes clear editor row layout state and stale selection goals without forcing an additional full-list remeasure beyond GPUI list width invalidation.
- Ordinary Source-mode single-row edits now clear and remeasure only the edited row's cached layout, and rekey reusable Source display-row cache entries to the new buffer version. Length-changing edits retain only rows before the edit because later source ranges can shift; length-preserving edits also retain later rows. Undo/redo can use the same local invalidation when editor selection history proves the transaction stayed on one Source row; Rendered edits, cross-row edits, row-count changes, and transactions without selection history remain conservative.

### Module Shape and Tests

- Inline atom layout, atom hit geometry, block rendering, and block layout construction have been moved onto their respective atom/block interfaces to reduce ad hoc branching in the row pipeline.
- Inline atom constants, sizing, measurement, fragment atom typing, and atom rendering helpers now live behind an internal `inline_atom` module, leaving the main editor file to focus on row layout and interaction flow.
- Inline span-to-atom dispatch and atom construction now route through shared `DisplayInlineAtomKind::for_inline_span` and `DisplayInlineAtom::from_span` entry points, reducing per-kind branching in `lib.rs` and creating a single extension seam for future inline atom kinds.
- Remote image block layout, measurement, hit geometry, rendering, and inactive-image detection now live behind an internal `block` module, leaving the main editor file to route block rows through the shared display-row layout path.
- Block row selection now routes through a `DisplayBlockKind` discovery phase before materializing `DisplayBlockLayout`, so future non-image block GPUI elements can plug into one shared kind-to-layout path instead of adding ad hoc `for_display_row` branching.
- The shared block module now exposes a generic non-image block seam (`Generic`) for kind detection, block layout construction, rendering, and row-level boundary geometry reuse, instead of adding more block-type-specific render paths.
- Rendered-element boundary and active-range helpers now live behind an internal `rendered_element` module, reducing non-layout coupling in `lib.rs` while keeping movement, selection reveal/hide, and inactive rendered-element behavior unchanged.
- `md_editor` still needs more internal module boundary cleanup; `lib.rs` now carries projection, row layout/cache, selection/movement, hit-testing, rendering, and extensive tests.
- Coverage now includes focused unit and GPUI-path tests for wrapped movement, action-level Source wrapped keyboard movement, Source display-row and render paths without Markdown syntax refresh, Source wrapped mouse hit testing and shift-selection across visual rows, keybinding-level Rendered marker reveal/hide transitions, resize reflow, mode switching at wrapped positions, visual-row bounds, rendered inline math, inline images, empty-alt inline images, remote image blocks, generic non-image block layout behavior, Rendered image block mouse hit testing and shift-selection, cache dependency keys, range-local span queries, source-row fast paths, source undo/redo local cache invalidation, Rendered interaction layout caching, remote image block cacheability, snapshot sharing, default list size hints, and the rendered image block crash path.

## Verification

- Recent touched-crate checks have passed, including `cargo fmt -p md_buffer -p md_editor`, `cargo check -p md_buffer`, `cargo check -p md_editor`, `cargo test -p md_buffer`, and `cargo test -p md_editor` (currently 127 tests). The latest module-boundary step was verified with `cargo fmt -p md_editor`, `cargo check -p md_editor`, `cargo test -p md_editor`, and `git diff --check`.
- Focused coverage now exercises wrapped movement, inline atoms/images, source display-row/cache/render fast paths, source edit and undo/redo cache invalidation, Rendered interaction layout caching, remote image block cacheability, default list size hints, Rendered image block drawing, and mouse interaction.
- `git diff --check` passes with only LF/CRLF warnings on touched files; short markdown-editor smoke runs after the Rendered image block fix did not reproduce the previous panic.

## Known Remaining Work

The bullets below are categories, not priority order or execution order.

- Profile 300KB-class Markdown files in Source and Rendered modes to identify the remaining source-row-local hot paths before making further performance changes.
- Continue optimizing within the source-row architecture: cheaper row layout, less string/fragment churn, stronger row-layout cache reuse, narrower remeasure and cache invalidation, and better behavior for large but non-extreme documents.
- Generalize inline atom measurement beyond the current inactive inline math atom path, and add invalidation if future atom content can resize after the row is cached.
- Implement broader general block-level GPUI elements as measured list items or subitems. Remote image blocks and an initial generic non-image block path now exist, but arbitrary GPUI block measurement and richer block-kind coverage are not solved.
- Continue code architecture cleanup within the existing source-row virtualization constraint. `crates/md_editor/src/lib.rs` is now large enough that display-row projection, row layout/cache, inline atoms, selection/movement, mouse hit testing, rendering, and tests should be split into clearer internal module boundaries before more general GPUI inline/block content is added. This does not imply switching away from source-row virtualization or immediately splitting `md_editor` into more crates.
- Add stronger runtime or visual tests for any remaining visual-row keyboard movement gaps found during profiling or manual use.
- Add stronger runtime or visual tests for general Rendered image/block behavior and any remaining wrapped-layout interaction gaps found during profiling or manual use.
