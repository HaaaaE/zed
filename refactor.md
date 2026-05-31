# Refactor Status

## Goal

Meaningfully split the editor code so implementation files trend below 1000 lines where practical, while preserving the current Rendered-mode source-row foundation.

## Current Slice

- Extracted editor action handling from `crates/md_editor/src/lib.rs` into `crates/md_editor/src/editor_actions.rs`.
- Extracted mouse selection/hit-routing from `crates/md_editor/src/lib.rs` into `crates/md_editor/src/editor_mouse.rs`.
- Extracted the GPUI `Render` implementation and editor row shell from `crates/md_editor/src/lib.rs` into `crates/md_editor/src/editor_render.rs`.
- `lib.rs` now holds editor state, constructors, mode/index helpers, selection synchronization, and shared invalidation utilities.
- Kept the rendered foundation source-row based: rendered item lookup, caret reveal, active projection invalidation, and row cache invalidation still use source rows/ranges as the underlying truth.

## Validation

- `cargo check -p md_editor`
- `cargo test -p md_editor`
- Latest full result: 250 tests passed.

## File Size Snapshot

- `lib.rs`: 881 lines.
- `virtual_list.rs`: 1375 lines, still too large.
- `movement.rs`: 923 lines.
- `layout.rs`: 849 lines.
- `block.rs`: 986 lines.
- `cache.rs`: 864 lines.
- `table.rs`: 644 lines.
- `inline_layout.rs`: 543 lines.
- `display_row_builder.rs`: 436 lines.
- `editor_actions.rs`: 337 lines.
- `editor_mouse.rs`: 266 lines.
- `editor_render.rs`: 218 lines.
- `rendered_topology.rs`: 164 lines.

## Next Slices

- Split `virtual_list.rs` only after identifying stable internal boundaries; it may need special handling because it is lower-level infrastructure.
- Review whether `virtual_list.rs` has a meaningful split between list state/range math and GPUI element rendering, or document why it should be exempted as lower-level infrastructure.
