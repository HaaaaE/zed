# Refactor Status

## Goal

Meaningfully split the editor code so implementation files trend below 1000 lines where practical, while preserving the current Rendered-mode source-row foundation.

## Current Slice

- Extracted display-row construction and rendered projection text building from `crates/md_editor/src/lib.rs` into `crates/md_editor/src/display_row_builder.rs`.
- Kept source-row truth intact: source rows still drive buffer coordinates, rendered index mapping, cache invalidation, and edit ranges.
- Updated sibling modules to depend on the new display-row builder module instead of reaching through `lib.rs`.

## Validation

- `cargo check -p md_editor`
- `cargo test -p md_editor`
- Latest full result: 250 tests passed.

## File Size Snapshot

- `lib.rs`: 2771 lines, still too large.
- `layout.rs`: 1503 lines, still too large.
- `virtual_list.rs`: 1555 lines, still too large.
- `block.rs`: 1068 lines, slightly above target.
- `display_row_builder.rs`: 539 lines.

## Next Slices

- Extract rendered/source visual movement from `lib.rs` into a dedicated movement module.
- Split `layout.rs` into layout inputs, text wrapping, and item layout helpers.
- Split `virtual_list.rs` only after identifying stable internal boundaries; it may need special handling because it is lower-level infrastructure.
- Trim `block.rs` by separating image block, formula block, and source block presentation if the split stays coherent.
