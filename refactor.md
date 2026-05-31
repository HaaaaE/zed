# Refactor Status

## Goal

Meaningfully split the editor code so implementation files trend below 1000 lines where practical, while preserving the current Rendered-mode source-row foundation.

## Current Slice

- Split `crates/md_editor/src/virtual_list.rs` by moving `MdListState` and `StateInner` behavior into `crates/md_editor/src/virtual_list/state.rs`.
- `virtual_list.rs` now owns the public element type, list item model, GPUI `Element` implementation, and SumTree summary/dimension adapters.
- `virtual_list/state.rs` owns list state mutation, scrolling, remeasurement, follow-tail behavior, item layout, prepaint layout, and scrollbar offset math.
- Product implementation files are now below 1000 lines. Test files under `lib_test/` are intentionally excluded from this threshold per the current refactor scope.
- This split is infrastructure-only and does not change the rendered editor source-row model.

## Validation

- `cargo check -p md_editor`
- `cargo test -p md_editor --no-run`
- `cargo test -p md_editor`
- Latest full run before this slice: 250 tests passed.

## File Size Snapshot

- `lib.rs`: 881 lines.
- `movement.rs`: 923 lines.
- `virtual_list/state.rs`: 848 lines.
- `layout.rs`: 849 lines.
- `block.rs`: 986 lines.
- `cache.rs`: 864 lines.
- `table.rs`: 644 lines.
- `virtual_list.rs`: 529 lines.
- `inline_layout.rs`: 543 lines.
- `display_row_builder.rs`: 436 lines.
- `editor_actions.rs`: 337 lines.
- `editor_mouse.rs`: 266 lines.
- `editor_render.rs`: 218 lines.
- `rendered_topology.rs`: 164 lines.

## Next Slices

- Do a final implementation-file audit for any newly introduced files over 1000 lines.
- If continuing beyond the current scope, consider smaller test-file organization separately; tests were explicitly excluded from this pass.
