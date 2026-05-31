# Refactor Status

## Goal

Meaningfully split the editor code so implementation files trend below 1000 lines where practical, while preserving the current Rendered-mode source-row foundation.

## Current Slice

- Extracted movement and selection-action routing from `crates/md_editor/src/lib.rs` into `crates/md_editor/src/movement.rs`.
- Replaced the narrower rendered caret helper with `crates/md_editor/src/rendered_topology.rs`.
- `RenderedTopology` now owns the rendered display index view, blank-row role semantics, caret-stop normalization, and trailing break ownership for active paragraph/heading rows.
- Kept source-row truth intact: source rows still drive buffer coordinates, rendered index mapping, cache invalidation, edit ranges, and rendered caret normalization.

## Validation

- `cargo check -p md_editor`
- `cargo test -p md_editor`
- Latest full result: 250 tests passed.

## File Size Snapshot

- `lib.rs`: 1679 lines, still too large.
- `layout.rs`: 1377 lines, still too large.
- `virtual_list.rs`: 1375 lines, still too large.
- `movement.rs`: 923 lines.
- `block.rs`: 986 lines.
- `cache.rs`: 864 lines.
- `table.rs`: 644 lines.
- `display_row_builder.rs`: 436 lines.
- `rendered_topology.rs`: 164 lines.

## Next Slices

- Split `layout.rs` into layout inputs, text wrapping, and item layout helpers.
- Split `virtual_list.rs` only after identifying stable internal boundaries; it may need special handling because it is lower-level infrastructure.
- Continue reducing `lib.rs` by moving editor state/cache/render orchestration into focused modules without changing the source-row rendered foundation.
