# Rendered Projection 下沉重构计划

## Summary

- 将 rendered source-row topology、caret normalization、display item source range 规则从 `md_editor` 下沉到 `md_projection`。
- 保持产品行为不变：不改 GPUI 渲染、不改编辑交互语义、不引入 Zed IDE crate。
- 本轮不做 `cache.rs` 拆分；它是下一轮候选。

## Key Changes

- 在 `md_projection` 新增并导出：
  - `RenderedCaretAffinity { Before, After }`
  - `RenderedProjectionState { active_source_range, inactive_source_ranges, active_cursor }`
  - `RenderedDisplaySourceRange { row, source_range, source_row_range }`
  - `RenderedTopology::new(snapshot, index)`, `normalize_caret(...)`, `item_display_source_range(...)`
- 把 `md_editor/src/rendered_topology.rs` 的纯逻辑迁入 `md_projection`，并让 `RenderedTopology` 内部判断 active cursor 是否映射到当前 item。
- 移除 `md_editor` 的 `rendered_index` re-export shim，调用方直接从 `md_projection` 引入 `RenderedDisplayIndex`、`RenderedDisplayItemKind`、`BlankRowRole`、`DisplayItemId` 等类型。
- 从 `layout.rs` 移除 `DisplayRowProjectionState`；在 `md_editor` 中保留一个构造 helper，基于当前 selection 生成 `md_projection::RenderedProjectionState`。
- 更新 `cache.rs`、`display_row_builder.rs`、`movement.rs`、`editor_render.rs` 和相关测试导入，使 `md_editor` 只负责 UI/layout/render/edit glue。

## Public Interfaces

- `md_projection` 的 workspace-internal API 会扩大，成为 rendered display topology 的来源。
- `md_editor` 的外部应用 API 不变：`MarkdownEditor`、mode、actions、公开编辑函数保持兼容。
- 不改 Cargo workspace 成员、不改 license metadata。

## Test Plan

- 新增/迁移 `md_projection` unit tests：
  - blank row caret normalization：separator、ignored extra、empty paragraph。
  - before/after affinity 选择 nearest caret stop。
  - merged paragraph trailing blank cursor 扩展 `source_range/source_row_range`。
- 保留 `md_editor` 行为测试覆盖：
  - rendered row projection 文本。
  - rendered empty paragraph enter/delete/backspace。
  - movement、selection、block/table rendering integration。
- 验证命令：
  - `cargo test -p md_projection`
  - `cargo test -p md_editor`
  - `cargo check -p updraft_editor`

## Assumptions

- 本轮只做投影层边界重构，不顺手拆缓存、block rendering 或测试大文件。
- 如果迁移后出现重复测试，优先让 `md_projection` 承担纯 topology/index 单测，`md_editor` 只保留跨 UI/layout 的集成行为测试。

## Progress

- [x] `md_projection` 新增 rendered topology/state/source-range API。
- [x] `md_editor/src/rendered_topology.rs` 纯逻辑迁入 `md_projection`，并由 `RenderedTopology` 内部判断 active cursor item 映射。
- [x] 移除 `md_editor/src/rendered_index.rs` shim，调用方直接使用 `md_projection` 类型。
- [x] 从 `layout.rs` 移除 `DisplayRowProjectionState`，在 `md_editor` 保留 `rendered_projection_state` 构造 helper。
- [x] 更新 cache/display row/movement/render/test 导入和调用点。
- [x] 验证通过：`cargo test -p md_projection`、`cargo test -p md_editor`、`cargo check -p updraft_editor`。
