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

