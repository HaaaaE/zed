# Table Structured Layout Plan

## Summary

Implement table-aware structured layout for pipe tables in Rendered mode. Inactive
tables should render as real grid tables. When the cursor or selection enters a
table, only the corresponding Markdown source row is revealed and edited through
the existing text buffer, selection, undo/redo, and dirty-state paths.

Keep the outer editor virtualization unit as Markdown source rows. Do not replace
it with visual-row virtualization or a whole-table block virtualizer.

The first version targets GitHub-style Markdown tables: header row, delimiter row,
body rows, left/center/right alignment, cell padding/borders, basic inline text
styles inside cells, and width-based cell wrapping. Performance should prioritize
large documents with a small number of tables: visible source rows do the row
work, while table structure and column widths are cached at table level.

## Key Changes

### Markdown Syntax Model

- Add table structure types in `markdown_wysiwyg`:
  - `MarkdownTable`
  - `MarkdownTableRow`
  - `MarkdownTableCell`
  - `MarkdownTableAlignment`
- Parse each `pipe_table` into:
  - table source range and source row range
  - header row, delimiter row, and body rows
  - cell source ranges and trimmed content ranges
  - pipe marker ranges and delimiter marker ranges
  - per-column alignment from the delimiter row
- Keep `MarkdownBlockKind::PipeTable` for compatibility with the current block
  model.
- Add read-only query helpers to find a table or table row by source row/source
  range.

### Editor Layout

- Add an internal `table` module in `md_editor` for table layout, rendering, and
  hit testing.
- Extend `DisplayRowLayout` with a `TableRow` variant. Do not put tables into
  `DisplayBlockLayout`; table layout is structured across multiple source rows,
  while image/formula block layout is single-row block content.
- Render inactive table rows as grid rows.
- Render the delimiter source row as a thin separator row rather than as source
  text.
- Reveal only the active source row as ordinary Markdown text, including pipes
  and spacing. The revealed row uses the existing text layout and editing paths.
- Keep non-active rows of the same table in structured table rendering.

### Interaction

- Mouse hit testing on inactive table rows maps x/y geometry to the matching cell
  and then to the nearest source offset in that cell.
- After a click places the selection in a table row, the next render reveals that
  source row for normal Markdown editing.
- Add `TableRow` handling for visual movement:
  - Home/End target the current table row source boundaries.
  - Up/Down preserve visual x and map into the neighboring table/text row.
  - Shift-selection across table rows selects the corresponding source ranges.
- Inside a revealed row, all typing, deletion, clipboard, undo/redo, and dirty
  tracking continue through the existing text editing code.

### Rendering

- Compute table column widths once per table layout:
  - preferred width from cell content
  - minimum width from wrapped content constraints
  - shrink columns to fit the available wrap width
- Apply delimiter-row alignment:
  - `:---` => left
  - `:---:` => center
  - `---:` => right
  - default => left
- Render header rows with distinct header styling.
- Render cell borders, padding, backgrounds, and aligned content.
- Support basic inline text styles inside cells for the first version.
- Render inline image/formula atoms inside table cells as text/style fallback in
  the first version; do not introduce async image/formula measurement inside
  tables yet.

### Caching And Invalidation

- Add a table layout cache to `MarkdownEditor`.
- Key table layout cache entries by:
  - buffer version
  - table source range or stable table id
  - wrap width
  - row style
- Reuse one table layout across all visible rows of that table.
- On Rendered-mode edits, initially use conservative invalidation for rendered
  display rows, row layouts, and table layouts.
- Keep Source-mode row-local cache reuse unchanged.
- On width changes, clear table layout cache along with row layout cache.
- Do not optimize for a single huge table in the first version. If future
  profiling shows very large tables are important, add table-internal row/column
  measurement optimization as a separate project.

## Test Plan

- Add `markdown_wysiwyg` parser tests for:
  - header/body/delimiter rows
  - left/center/right alignment
  - missing leading/trailing pipes
  - empty cells
  - inline Markdown inside cells
  - table, row, and cell source ranges
- Add `md_editor` layout tests for:
  - inactive table rows using `TableRow`
  - active source rows revealing original Markdown source text
  - delimiter rows rendering as separators
  - column width calculation, alignment, and wrapping
- Add interaction tests for:
  - mouse click mapping to the correct table cell source offset
  - click-to-reveal of only the active source row
  - Up/Down preserving visual x across table rows
  - Home/End targeting table row boundaries
  - Shift-selection across table rows selecting correct source ranges
- Add cache tests for:
  - table layout reuse across multiple visible rows
  - selection changes only affecting active row layout/display state
  - wrap width changes rebuilding table layout
  - Source mode avoiding Markdown syntax refresh for source-only fast paths
- Update the existing pipe table rendered-mode test so it no longer asserts that
  pipe tables always remain plain text layout.

## Progress

### 2026-05-28

- Added the `markdown_wysiwyg` table syntax model:
  - `MarkdownTable`
  - `MarkdownTableRow`
  - `MarkdownTableCell`
  - `MarkdownTableAlignment`
- Added structured extraction for existing `MarkdownBlockKind::PipeTable`
  blocks, including table/source row ranges, header/delimiter/body rows, cell
  source/content ranges, pipe marker ranges, delimiter marker ranges, and
  delimiter-derived left/center/right alignment.
- Added read-only table lookup helpers by source row and source range.
- Added parser tests for structured ranges, alignments, missing
  leading/trailing pipes, empty cells, and inline Markdown text inside cells.
- Formatted the touched Rust file with `rustfmt`.
- Cargo validation has not been run because the current plan still carries the
  local "no cargo" constraint.

### 2026-05-28, editor layout skeleton

- Added an internal `md_editor::table` module with `DisplayTableRowLayout` and
  per-cell layout data.
- Added `DisplayRowLayout::TableRow` and wired it through rendered row layout
  caching, row height calculation, rendering, mouse hit testing, Home/End, and
  vertical movement target mapping.
- Inactive rendered table source rows now choose structured table row layout;
  the active table source row still falls back to existing text layout/editing.
- Adjusted table projection so entering one table source row reveals only that
  row while other rows in the same table remain inactive; inline Markdown
  markers in the active table row are also revealed with the row source.
- Added a rendered-mode interaction test for inactive structured table row
  layout and updated the old pipe-table text-layout test to cover the active
  row case.
- Cargo validation has not been run because the current plan still carries the
  local "no cargo" constraint.

### 2026-05-28, table layout cache

- Added a table-level layout cache keyed by buffer version, table source range,
  wrap width, and row style.
- Reused cached table column metrics across multiple rendered table source rows.
- Cleared table layout cache with row-layout/display-row cache clears and on
  rendered-mode edits.
- Extended the rendered table row test to assert table metric reuse across
  header/body rows.
- Cargo validation has not been run because the current plan still carries the
  local "no cargo" constraint.

### 2026-05-28, table cell inline fallback

- Table cell display text now applies the existing Markdown inline span
  projection rules inside cells: inline markers are hidden for inactive table
  rendering while cell text remains visible.
- Basic inline styles inside cells now flow through `StyledDisplaySegment` and
  the existing text-piece renderer for bold, links, code, emphasis,
  strikethrough, images, and math fallback text.
- Column preferred widths now use projected cell display text rather than raw
  Markdown marker text.
- Extended the rendered table row test to assert bold/link inline styling inside
  inactive structured table cells.
- Cargo validation has not been run because the current plan still carries the
  local "no cargo" constraint.

### 2026-05-28, table cell wrapping

- Added width-based wrapped-line estimation for table cells after column shrink.
- Table row height now grows with the maximum wrapped line count among cells in
  the source row.
- Table cell rendering now allows normal text wrapping instead of forcing a
  single nowrap flex line.
- Adjusted cell width estimation conservatively and clipped cell contents to
  avoid single-character overflow artifacts when rendered text is wider than the
  estimate.
- Added a rendered-mode test covering column shrink to wrap width and row height
  growth for a long table cell.
- Cargo validation has not been run because the current plan still carries the
  local "no cargo" constraint.

### 2026-05-28, table interaction coverage

- Added rendered-mode interaction coverage for inactive table-row mouse target
  mapping into the matching cell source range.
- Added vertical movement coverage for moving from an active revealed table row
  into neighboring table rows while preserving the rendered-mode path.
- Added rendered mouse-click coverage showing that clicking an inactive table row
  reveals only the clicked source row on the next display pass.
- Added Shift+Down coverage for source-range selection across table rows.
- Cargo validation has not been run because the current plan still carries the
  local "no cargo" constraint.

### 2026-05-28, robustness fixes

- Malformed pipe-table candidates now fail closed: unsafe row ranges are ignored
  and rows without a valid delimiter no longer build a structured
  `MarkdownTable`.
- Added parser coverage for malformed pipe-table input staying out of the
  structured table model.
- Fixed a general virtual-list jump on row-count-changing edits by splicing only
  the edited row range instead of rebuilding the entire list.
- Added unit coverage for the row-count splice range used by newline/join-line
  edits.
- Cargo validation has not been run because the current plan still carries the
  local "no cargo" constraint.

## Assumptions

- First version is "structured rendering plus source editing", not full
  WYSIWYG cell editing.
- Reveal granularity is active source row, not whole table and not cell-only.
- First version does not add insert/delete column actions or automatic Markdown
  table source formatting.
- Source-row virtualization remains the outer architecture.
- The delimiter source row may render as a thin separator while preserving source
  row mapping.
- Do not run cargo validation while the local "no cargo" constraint is in force.
