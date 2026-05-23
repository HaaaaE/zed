# Markdown Editor — Extraction Log

This file tracks every batch of source files ported from Zed crates into the
`md_*` product crates.  One entry per batch; append chronologically.

## Format

```md
### YYYY-MM-DD — <batch name>

- **Source files:**
  - crates/<source>/src/...
- **Destination files:**
  - crates/<md_*>/src/...
- **Source baseline:** <commit SHA or `fork-HEAD` + git-diff summary>
- **Retained capabilities:** ...
- **Removed capabilities:** project / workspace / lsp / ...
- **Hand-written replacements:** ... (reason why port was not sufficient)
- **Verification:**
  - `cargo check -p <crate>`
  - `cargo test -p <crate>`
```

---

### 2026-05-22 — R0: Scaffold

- **Source files:** (none — empty shells only)
- **Destination files:**
  - `crates/md_rope/src/lib.rs` (placeholder)
  - `crates/md_sum_tree/src/lib.rs` (placeholder)
  - `crates/md_text/src/lib.rs` (placeholder)
  - `crates/md_buffer/src/lib.rs` (placeholder)
  - `crates/md_editor/src/lib.rs` (placeholder)
  - `crates/md_theme/src/lib.rs` (placeholder)
  - `crates/md_settings/src/lib.rs` (placeholder)
  - `crates/md_assets/src/lib.rs` (placeholder)
- **Source baseline:** fork-HEAD (no code copied)
- **Retained capabilities:** N/A
- **Removed capabilities:** N/A
- **Hand-written replacements:** N/A
- **Verification:**
  - `cargo check -p md_text` ✓
  - `cargo check -p markdown_editor --features legacy-editor` ✓
  - `cargo check -p markdown_editor --features md-editor --no-default-features` ✓
- **Notes:** `crates/markdown_editor/src/main.rs` refactored into a
  feature-dispatching entry point; legacy code extracted verbatim to
  `crates/markdown_editor/src/legacy_editor.rs` (gated on `legacy-editor`
  feature, default = true).

### 2026-05-22 — R1: sum_tree / rope / text port

- **Source files:**
  - crates/sum_tree/src/*.rs  (sum_tree.rs + cursor.rs + property_test.rs + tree_map.rs)
  - crates/rope/src/*.rs      (rope.rs + chunk.rs + offset_utf16.rs + point.rs + point_utf16.rs + unclipped.rs)
  - crates/text/src/*.rs      (text.rs + anchor.rs + locator.rs + network.rs + operation_queue.rs + patch.rs + selection.rs + subscription.rs + tests.rs + undo_map.rs)
- **Destination files:**
  - crates/md_sum_tree/src/*  (verbatim copy)
  - crates/md_rope/src/*      (verbatim copy)
  - crates/md_text/src/*      (verbatim copy)
- **Source baseline:** fork-HEAD, no modifications to source
- **Retained capabilities:** full rope/sum_tree/text APIs and tests
- **Removed capabilities:** bench harness removed from md_rope (to be wired in later stage)
- **Hand-written replacements:**
  - md_rope/Cargo.toml: aliases md_sum_tree as 'sum_tree' via package rename
  - md_text/Cargo.toml: aliases md_rope as 'rope' and md_sum_tree as 'sum_tree' via package rename
- **Verification:**
  - cargo check -p md_sum_tree            OK
  - cargo check -p md_rope                OK
  - cargo check -p md_text                OK
  - cargo test -p md_sum_tree             10/10 passed
  - cargo test -p md_rope                 24/24 passed
  - cargo test -p md_text                 36/36 passed
  - cargo tree -p md_text (no language/editor/project/workspace)  OK
### 2026-05-22 — R2: md_buffer initial single-file buffer

- **Source files:**
  - crates/language/src/buffer.rs  (single-file Buffer API reference only)
  - crates/multi_buffer/src/*.rs   (singleton semantics reference only)
- **Destination files:**
  - crates/md_buffer/Cargo.toml
  - crates/md_buffer/src/md_buffer.rs
- **Source baseline:** fork-HEAD, hand-written minimal standalone buffer atop md_text + markdown_wysiwyg
- **Retained capabilities:** local single-file buffer construction, text snapshot export, Markdown syntax snapshot, dirty/saved tracking, edit, undo/redo, transaction wrappers
- **Removed capabilities:** file handles, diagnostics, language registry, LSP, project/workspace integration, multi-buffer excerpts, remote state
- **Hand-written replacements:**
  - standalone buffer id allocator in md_buffer
  - Markdown syntax cache keyed by md_text version
  - current syntax refresh reparses the full document on version change; incremental edit-aware reparse is deferred to a later R2/R3 batch
- **Verification:**
  - cargo test -p md_buffer                              3/3 passed
  - cargo tree -p md_buffer --edges normal -q           no language / multi_buffer / project / workspace matches


### 2026-05-22 — R2: md_buffer boundary tightening

- **Source files:**
  - crates/md_text/src/text.rs
  - crates/md_buffer/src/md_buffer.rs
  - crates/md_buffer/Cargo.toml
- **Destination files:**
  - crates/md_text/src/text.rs
  - crates/md_buffer/src/md_buffer.rs
  - crates/md_buffer/Cargo.toml
- **Source baseline:** fork-HEAD, refine initial R2 port to match target dependency direction
- **Retained capabilities:** unchanged standalone buffer API, syntax snapshot, dirty/saved, undo/redo behavior
- **Removed capabilities:** direct md_buffer dependency on workspace clock crate
- **Hand-written replacements:**
  - md_text re-exports `Global`, `Lamport`, and `ReplicaId` so md_buffer consumes versioning types through md_text instead of depending on clock directly
- **Verification:**
  - cargo test -p md_buffer
  - cargo tree -p md_buffer --edges normal -q


### 2026-05-22 — R2: md_buffer API alignment follow-up

- **Source files:**
  - crates/language/src/buffer.rs  (standalone editing API semantics reference)
  - crates/md_text/src/text.rs     (has_edits_since, line ending, transaction helpers)
- **Destination files:**
  - crates/md_text/src/text.rs
  - crates/md_text/src/tests.rs
  - crates/md_buffer/src/md_buffer.rs
- **Source baseline:** fork-HEAD, refine the standalone md_buffer surface toward language::Buffer single-file behavior
- **Retained capabilities:** local buffer creation, syntax snapshotting, undo/redo, saved-version tracking
- **Removed capabilities:** none
- **Hand-written replacements:**
  - md_buffer dirty tracking now uses `md_text::Buffer::has_edits_since(saved_version)` so undo back to saved content clears the dirty flag
  - md_buffer now forwards `edit_non_coalesce`, `undo_transaction`, `undo_to_transaction`, `redo_to_transaction`, `transaction_group_interval`, `set_group_interval`, and `set_line_ending`
  - md_buffer now tracks `preview_version` and exposes `refresh_preview` / `preserve_preview` with the same content-based semantics as `language::Buffer`'s single-file path
  - md_buffer now exposes `remote_id`, `replica_id`, `base_text`, `deferred_ops_len`, and `has_deferred_ops` for standalone callers that need low-level buffer identity/history inspection
  - md_buffer now exposes `as_text_snapshot` and `has_edits_since` so callers can inspect the raw text snapshot by reference and query content changes against arbitrary versions without cloning or re-deriving dirty state
  - md_buffer now forwards `wait_for_version` and `give_up_waiting`; to support that standalone contract, md_text now resolves version waiters after local edit / undo paths in addition to applied remote operations
  - added standalone tests for undo-to-saved-state, transaction-specific undo/redo, reversed edit ranges, group interval forwarding, line-ending metadata updates, preview preservation behavior, base text stability, deferred-op inspection, content-change tracking across undo, immediate version waits, successful version waits after local edits, and forced waiter cancellation
- **Verification:**
  - cargo test -p md_text                                37/37 passed
  - cargo test -p md_buffer                              14/14 passed
  - cargo tree -p md_buffer --depth 1 -q                direct deps limited to markdown_wysiwyg + md_text


### 2026-05-22 — R2: md_buffer file I/O closeout

- **Source files:**
  - crates/markdown_editor/src/legacy_editor.rs  (legacy application shell file open/save path)
  - crates/text/src/text.rs                      (line-ending-aware text serialization helper reference)
- **Destination files:**
  - crates/md_buffer/src/md_buffer.rs
  - crates/markdown_editor/src/legacy_editor.rs
  - crates/markdown_editor/Cargo.toml
- **Source baseline:** fork-HEAD, finish the optional R2 application-side file I/O wiring without crossing into R3 md_editor work
- **Retained capabilities:** legacy editor UI path unchanged; md_buffer remains the standalone single-file document model; markdown_editor legacy shell can still open/save files normally
- **Removed capabilities:** none
- **Hand-written replacements:**
  - md_buffer now exposes `serialized_text`, which emits the normalized buffer text using the buffer's tracked line ending convention so standalone saves preserve LF/CRLF semantics
  - markdown_editor `legacy-editor` now constructs an `md_buffer::Buffer` when opening documents and uses that standalone buffer as the file I/O source of truth during save, while the editing UI continues to use the legacy `editor` / `language` stack until R3
- **Verification:**
  - cargo test -p md_buffer                              15/15 passed
  - cargo check -p markdown_editor --features legacy-editor ✓
  - cargo check -p markdown_editor --features md-editor --no-default-features ✓


### 2026-05-22 — R3: md_editor read-only display slice

- **Source files:**
  - crates/gpui/examples/uniform_list.rs  (virtualized list rendering pattern)
  - crates/gpui/examples/input.rs         (focus handle / focused text surface pattern)
  - crates/editor/src/editor.rs           (`Editor::for_buffer` goal reference only)
- **Destination files:**
  - crates/md_editor/Cargo.toml
  - crates/md_editor/src/lib.rs
  - crates/markdown_editor/src/main.rs
  - crates/markdown_editor/src/md_editor_app.rs
- **Source baseline:** fork-HEAD, hand-written first R3 slice atop md_buffer + GPUI
- **Retained capabilities:** standalone md_editor entity, GPUI focus handle, virtualized row rendering, line-number gutter, md-editor feature path opens a Markdown file into an md_buffer-backed read-only surface
- **Removed capabilities:** editing transactions, selections, soft wrap, syntax highlighting, WYSIWYG decorations, project/workspace/lsp/editor crate integration
- **Hand-written replacements:**
  - md_editor renders md_buffer text rows directly via `gpui::uniform_list`; this is a temporary read-only R3 bootstrap before porting Zed display_map / element pipeline
  - markdown_editor `md-editor` path now opens a real window instead of printing the R0 placeholder; legacy-editor remains the default feature path
- **Verification:**
  - cargo test -p md_editor                              3/3 passed
  - cargo check -p markdown_editor --features md-editor --no-default-features ✓
  - cargo check -p markdown_editor --features legacy-editor ✓


### 2026-05-22 — R3: md_editor basic cursor movement

- **Source files:**
  - crates/gpui/examples/input.rs         (action binding and focus/caret reference)
  - crates/editor/src/movement.rs         (movement semantics reference only)
  - crates/editor/src/editor.rs           (keyboard action wiring reference only)
- **Destination files:**
  - crates/md_editor/src/lib.rs
  - crates/markdown_editor/src/md_editor_app.rs
  - REFACTOR_GOAL.md
  - docs/md-extraction.md
- **Source baseline:** fork-HEAD, hand-written minimal cursor model using md_buffer snapshots
- **Retained capabilities:** focused editor surface, current-row highlight, visible caret, left/right/up/down/home/end actions, UTF-8-safe cursor movement across Markdown buffer rows
- **Removed capabilities:** multi-cursor selection, mouse hit-testing, soft wrap aware movement, edit transactions, IME/text input, project/workspace/editor crate integration
- **Hand-written replacements:**
  - movement is implemented on top of `md_text::Point` and `BufferSnapshot` while the full Zed movement/display map stack is still pending migration
  - markdown_editor `md-editor` path binds basic movement keys directly to the standalone `MarkdownEditor` key context
- **Verification:**
  - cargo test -p md_editor                              6/6 passed
  - cargo check -p markdown_editor --features md-editor --no-default-features ✓
  - cargo check -p markdown_editor --features legacy-editor ✓


### 2026-05-23 — R3: md_editor save and dirty-state shell sync

- **Source files:**
  - crates/markdown_editor/src/legacy_editor.rs  (save/save-as shell flow reference)
  - crates/gpui/examples/input.rs                (entity focus + action wiring reference)
- **Destination files:**
  - crates/md_editor/src/lib.rs
  - crates/markdown_editor/src/md_editor_app.rs
  - docs/md-extraction.md
- **Source baseline:** fork-HEAD, hand-written md-editor shell/document status follow-up on top of the earlier R3 bootstrap
- **Retained capabilities:** standalone md_editor editing surface, single-buffer editing, keyboard/mouse selection, undo/redo, md-editor feature-gated app shell
- **Removed capabilities:** open dialog, preview toggle, source/rendered mode toggle, WYSIWYG decorations, project/workspace/editor crate integration
- **Hand-written replacements:**
  - md_editor now emits a minimal `DirtyChanged` event so the app shell can observe edit/save transitions without depending on Zed editor events
  - markdown_editor `md-editor` path now implements `Save` / `SaveAs`, writes via `md_editor::serialized_text()`, and reflects saved/unsaved state in the title bar while keeping the shell GPUI-only
- **Verification:**
  - not run in this batch


### 2026-05-23 — R3: md_editor minimal rendered block projection

- **Source files:**
  - crates/markdown_wysiwyg/src/markdown_wysiwyg.rs  (block marker projection and active-range reveal)
  - crates/gpui/examples/input.rs                    (single-surface hit-testing / focus flow reference)
- **Destination files:**
  - crates/md_editor/src/lib.rs
  - crates/markdown_editor/src/md_editor_app.rs
  - docs/md-extraction.md
- **Source baseline:** fork-HEAD, hand-written minimal Rendered mode follow-up on top of the earlier R3 editing shell
- **Retained capabilities:** source-coordinate selection/caret model, keyboard editing, mouse hit-testing, undo/redo, save/save-as, new/open document shell flow
- **Removed capabilities:** inline WYSIWYG decorations, block widgets, soft wrap-aware position mapping, IME/composition handling, multi-selection, project/workspace/editor crate integration
- **Hand-written replacements:**
  - `md_editor` now carries `MarkdownEditorMode::{Source, Rendered}` and attaches a per-row `MarkdownProjectionMap` so Rendered mode can hide block-level marker ranges while continuing to store caret/selection in source coordinates
  - Rendered rows reveal marker text for the active block by threading the current selection/caret source range into `markdown_wysiwyg` projection generation, avoiding a fully markerless editing surface
  - caret painting, selection highlighting, and mouse hit-testing now convert through the row projection instead of assuming source offsets and display offsets are identical
  - `markdown_editor` shell now exposes `ToggleMode` and shows the active mode in the title bar, while save/open paths continue to serialize the unchanged source text from `md_buffer`
- **Verification:**
  - cargo test -p md_editor                              15/15 passed
  - cargo check -p markdown_editor --features md-editor --no-default-features ✓


### 2026-05-23 — R3: md_editor new/open document shell workflow

- **Source files:**
  - crates/markdown_editor/src/legacy_editor.rs  (new/open document shell flow reference)
  - crates/gpui/examples/input.rs                (entity replacement + focus flow reference)
- **Destination files:**
  - crates/markdown_editor/src/md_editor_app.rs
  - docs/md-extraction.md
- **Source baseline:** fork-HEAD, hand-written md-editor shell workflow follow-up on top of the save/dirty batch
- **Retained capabilities:** standalone md-editor window, dirty-state subscription, save/save-as, keyboard-driven editing and navigation
- **Removed capabilities:** unsaved-change confirmation, command palette, preview toggle, source/rendered mode toggle, WYSIWYG decorations, project/workspace/editor crate integration
- **Hand-written replacements:**
  - md-editor shell now rebuilds the active `MarkdownEditor` entity when creating or opening a document, replacing the editor-local subscription in place instead of depending on workspace item infrastructure
  - `OpenDocument` uses GPUI `prompt_for_paths` directly and keeps the workflow confined to the standalone markdown-editor shell
- **Verification:**
  - not run in this batch

### 2026-05-23 — R3: md_editor inline marker projection

- **Source files:**
  - crates/markdown_editor/src/legacy_editor.rs  (rendered-mode marker hiding behavior reference)
  - crates/markdown_wysiwyg/src/markdown_wysiwyg.rs  (projection and inline span model)
- **Destination files:**
  - crates/markdown_wysiwyg/src/markdown_wysiwyg.rs
  - crates/md_editor/src/lib.rs
  - docs/md-extraction.md
  - REFACTOR_GOAL.md
- **Source baseline:** fork-HEAD, hand-written rendered-mode follow-up on top of the earlier md_editor projection slice
- **Retained capabilities:** source/display coordinate mapping, active-block reveal, keyboard editing, mouse hit-testing, undo/redo, save/new/open shell flow
- **Removed capabilities:** none
- **Hand-written replacements:**
  - `MarkdownSyntaxTree::projection_for_source_range` now folds inline span marker ranges into the same `MarkdownProjectionMap` that already hid inactive block markers, so `md_editor` does not need a separate rendered-only text transform for `**`, `*`, backticks, or similar inline control syntax
  - `md_editor` keeps the existing selection/caret and hit-test code unchanged because it already routes through the projection map; only the projection inputs widened from block markers to block + inline markers
- **Verification:**
  - cargo test -p markdown_wysiwyg
  - cargo test -p md_editor
  - cargo check -p markdown_editor --features md-editor --no-default-features
  - cargo check -p markdown_editor --features legacy-editor

### 2026-05-23 — R3: md_editor rendered semantic styling

- **Source files:**
  - crates/markdown_editor/src/legacy_editor.rs  (Rendered mode highlight palette and semantic styling reference)
  - crates/markdown_wysiwyg/src/markdown_wysiwyg.rs  (block + inline semantic ranges)
- **Destination files:**
  - crates/md_editor/src/lib.rs
  - docs/md-extraction.md
  - REFACTOR_GOAL.md
- **Source baseline:** fork-HEAD, hand-written style-aware rendering follow-up on top of the md_editor projection batches
- **Retained capabilities:** source/display coordinate mapping, keyboard editing, mouse hit-testing, save/new/open shell flow, inline/block marker projection, single-selection rendering
- **Removed capabilities:** none
- **Hand-written replacements:**
  - `md_editor` now converts `markdown_wysiwyg` block and inline semantic ranges into per-row styled text segments, rather than depending on Zed editor custom highlights
  - caret and selection painting continue to operate on display offsets by splitting those styled segments at caret/selection boundaries, so the standalone renderer keeps one text path instead of forking separate “styled” and “interactive” renderers
  - the current standalone palette is hard-coded inside `md_editor`; this is intentionally temporary until `md_theme` exists and can own Markdown-specific colors/typography
- **Verification:**
  - cargo test -p md_editor
  - cargo test -p markdown_wysiwyg
  - cargo check -p markdown_editor --features md-editor --no-default-features
  - cargo check -p markdown_editor --features legacy-editor

### 2026-05-23 — R3: md_editor shell state consistency follow-up

- **Source files:**
  - crates/markdown_editor/src/md_editor_app.rs  (standalone shell state hand-off follow-up)
- **Destination files:**
  - crates/markdown_editor/src/md_editor_app.rs
  - docs/md-extraction.md
  - REFACTOR_GOAL.md
- **Source baseline:** fork-HEAD, hand-written shell consistency follow-up on top of the rendered-mode and document workflow batches
- **Retained capabilities:** Source/Rendered toggle, new/open document flow, save/save-as, dirty-state subscription, standalone GPUI shell
- **Removed capabilities:** none
- **Hand-written replacements:**
  - replacing the active `MarkdownEditor` on new/open now preserves the current `MarkdownEditorMode`, so Rendered-mode verification does not silently fall back to Source after document switches
  - startup file-open failures now clear the shell `path` instead of retaining an invalid current-document path, keeping save state and title-bar path display aligned with the actually loaded buffer
- **Verification:**
  - not run in this batch

### 2026-05-23 — R3: md_editor rendered heading row geometry

- **Source files:**
  - crates/markdown_editor/src/legacy_editor.rs  (Rendered heading typography / row metrics reference)
  - crates/markdown_wysiwyg/src/markdown_wysiwyg.rs  (heading block metadata reference)
- **Destination files:**
  - crates/md_editor/src/lib.rs
  - docs/md-extraction.md
  - REFACTOR_GOAL.md
- **Source baseline:** fork-HEAD, hand-written row-metrics follow-up on top of the rendered semantic styling batch
- **Retained capabilities:** source/display projection, semantic styling, keyboard editing, mouse hit-testing, save/new/open shell flow
- **Removed capabilities:** none
- **Hand-written replacements:**
  - `md_editor` now resolves a per-row `RowDisplayStyle` in Rendered mode so heading rows can increase text size, line height, minimum row height, and caret height without reintroducing Zed editor row-metrics machinery
  - mouse hit-testing now uses the active row display style when shaping text, keeping rendered heading clicks aligned with the larger standalone typography instead of assuming the source-mode default size
- **Verification:**
  - cargo test -p md_editor
  - cargo test -p markdown_wysiwyg
  - cargo check -p markdown_editor --features md-editor --no-default-features
  - cargo check -p markdown_editor --features legacy-editor

### 2026-05-23 — R3: md_editor image block + standalone init closeout

- **Source files:**
  - crates/markdown_editor/src/legacy_editor.rs  (Rendered remote image block replacement reference)
  - crates/markdown_editor/src/md_editor_app.rs  (standalone keybinding owner before extraction)
- **Destination files:**
  - crates/md_editor/src/lib.rs
  - crates/markdown_editor/src/md_editor_app.rs
  - docs/md-extraction.md
  - REFACTOR_GOAL.md
- **Source baseline:** fork-HEAD, hand-written R3 closeout batch on top of the rendered-mode and shell follow-up slices
- **Retained capabilities:** standalone md-editor open/edit/save flow, source/display projection, marker reveal, rendered semantic styling, heading row geometry
- **Removed capabilities:** shell-owned md_editor keybinding registration in `markdown_editor`
- **Hand-written replacements:**
  - `md_editor::init_standalone` now owns the standalone editor keybindings for movement, selection, text input, and undo/redo, so the app shell only keeps document-level actions
  - Rendered mode now recognizes inactive image-only rows backed by remote URLs and renders them as image blocks, while still revealing the raw Markdown source whenever the caret or selection enters the image span
  - added md_editor tests for inactive image block detection, active-image reveal, and skipping inline images with surrounding paragraph text
- **Verification:**
  - cargo test -p md_editor                              23/23 passed
  - cargo test -p markdown_wysiwyg                       11/11 passed
  - cargo check -p markdown_editor --features md-editor --no-default-features ✓
  - cargo check -p markdown_editor --features legacy-editor ✓
  - cargo tree -p md_editor --edges normal -q           no editor / language / multi_buffer / project / workspace matches

### 2026-05-23 — R4: md_theme minimal palette extraction

- **Source files:**
  - crates/markdown_editor/src/legacy_editor.rs  (standalone shell/editor palette reference)
  - crates/markdown_editor/src/md_editor_app.rs  (shell chrome colors and sizing)
  - crates/md_editor/src/lib.rs                  (rendered palette + row metrics owner)
- **Destination files:**
  - crates/md_theme/Cargo.toml
  - crates/md_theme/src/lib.rs
  - crates/md_editor/Cargo.toml
  - crates/md_editor/src/lib.rs
  - crates/markdown_editor/src/md_editor_app.rs
  - docs/md-extraction.md
  - REFACTOR_GOAL.md
- **Source baseline:** fork-HEAD, hand-written R4 bootstrap that extracts standalone theme constants without reintroducing Zed theme/settings crates
- **Retained capabilities:** standalone md-editor shell, rendered semantic styling, heading row metrics, source/rendered mode toggle, save/new/open flow
- **Removed capabilities:** hard-coded palette ownership inside `md_editor`
- **Hand-written replacements:**
  - `md_theme` now owns the minimal standalone palette, gutter/title-bar sizing, and row metric presets needed by the current md-editor path
  - `md_editor` now consumes `md_theme` for text colors, selection/caret colors, gutter styling, font family, and rendered heading geometry instead of carrying those constants inline
  - `markdown_editor` now consumes the same `md_theme` shell palette for title-bar, background, dirty-state, and error styling so the standalone shell and editor stay visually aligned
- **Verification:**
  - cargo test -p md_theme
  - cargo test -p md_editor
  - cargo check -p markdown_editor --features md-editor --no-default-features
