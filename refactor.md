# Refactor Status

## Goal

Meaningfully split the editor code so implementation files trend below 1000 lines where practical, while preserving the current Rendered-mode source-row foundation.

## Current Slice

- Extracted inline fragment/style/text-run generation from `crates/md_editor/src/layout.rs` into `crates/md_editor/src/inline_layout.rs`.
- `layout.rs` now focuses on layout inputs, text shaping/wrapping, visual rows, and display item layout types.
- `inline_layout.rs` owns rendered/source inline fragments, row style selection, markdown style mapping, atom wrapping boundaries, and text-run normalization.
- Kept the rendered foundation source-row based: inline fragments still derive from `DisplayRow.source_range`, projection operations, hidden ranges, and source-to-display mapping.

## Validation

- `cargo check -p md_editor`
- `cargo test -p md_editor`
- Latest full result: 250 tests passed.

## File Size Snapshot

- `lib.rs`: 1682 lines, still too large.
- `virtual_list.rs`: 1375 lines, still too large.
- `movement.rs`: 923 lines.
- `layout.rs`: 849 lines.
- `block.rs`: 986 lines.
- `cache.rs`: 864 lines.
- `table.rs`: 644 lines.
- `inline_layout.rs`: 543 lines.
- `display_row_builder.rs`: 436 lines.
- `rendered_topology.rs`: 164 lines.

## Next Slices

- Split `virtual_list.rs` only after identifying stable internal boundaries; it may need special handling because it is lower-level infrastructure.
- Continue reducing `lib.rs` by moving editor state/cache/render orchestration into focused modules without changing the source-row rendered foundation.
