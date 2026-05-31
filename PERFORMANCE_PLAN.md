  # Render 模式大文档编辑性能彻底修复方案

  ## Summary

  目标是让 render 模式编辑从“每次按键全量重建”改成“按编辑影响范围增量更新”，并用 perf/test 锁住回归。

  当前根因是 render 模式单行编辑被强制走 conservative invalidation：清空 display/layout/rendered index，随后 `buffer.snapshot()` 触发全文 Markdown syntax 派生结构重建，再重建
  `RenderedDisplayIndex`。Enter/backspace/delete 还会在 edit plan 阶段额外多次全量 build index。

  默认修复策略：分两层落地。先做同步局部失效和 index 复用，立刻消除可见卡顿；再把 Markdown syntax 和 rendered index 改成基于 edit summary 的增量更新，消除根本的 O(document) 编辑成本。

  ## Status

  - [x] 第一阶段局部失效落地：
    - Source 单行编辑保持局部 invalidation。
    - Rendered 同一 item 内等长编辑只失效 affected item，并 rekey 可安全复用的 rendered display/layout input cache。
    - Rendered 编辑不再一律 conservative 清空 display/layout cache。
  - [x] 第一阶段重复 index build 收敛：
    - `rendered_edit.rs` 新增 `RenderedEditContext`，一次 top-level rendered edit plan 只在 planner 内构建一次 `RenderedDisplayIndex`。
  - [x] 第一阶段测试：
    - 新增 rendered 等长局部编辑缓存复用回归测试。
    - 已通过 `cargo test -p md_editor` 和 `cargo check -p updraft_editor`。
  - [x] 第二阶段 edit summary API：
    - `md_text::Buffer::edit` 直接返回 operation patch；undo/redo 同样暴露 patch。
    - `md_buffer::edit`/`edit_non_coalesce`/`undo`/`redo` 直接返回 `BufferEditSummary`。
    - editor 普通输入、删除、换行、undo/redo 改用 summary 的 transaction id 和 byte delta，不再手算 buffer 长度差。
    - 已通过 `cargo test -p md_text`、`cargo test -p md_buffer`、`cargo test -p md_editor`。
  - [ ] 待完成：Markdown syntax tree 基于 edit summary 的增量 reparse，移除 old/new 全文复制热路径。
  - [ ] 待完成：`RenderedDisplayIndex::update_after_edit` 真增量更新及等价性测试。
  - [ ] 待完成：md_editor perf suite 的 rendered edit 回归场景与 important perf gate。

  ## Key Changes

  - 新增编辑影响摘要 API：
    - 在 `md_text` 让 `edit/undo/redo` 返回 `Vec<Edit<usize>>` 样式的 old/new byte range patch。
    - 在 `md_buffer` 新增 `BufferEditSummary`，包含 old range、new range、byte delta、row delta、old/new affected rows、transaction id，并作为主编辑返回值。
    - 所有 editor edit action 改用 summary；undo/redo 也走同一条 invalidation 管线。

  - 修复 render 模式局部失效：
    - 替换 `local_source_edit_invalidation_rows` 为 mode-agnostic 的 `local_edit_invalidation`。
    - Source 模式保持现有单行优化。
    - Rendered 模式对同一 rendered item 内的单行/局部编辑只清 affected item 的 display row、layout input、layout、table layout，并只 remeasure affected item。
    - row count 或 rendered item count 改变时，对 `MdListState` 使用 item-level splice，而不是清全列表。
    - 删除“rendered mode 一律 conservative”的测试断言，改成验证 rendered 单行编辑只失效 affected item。

  - 避免重复全量 rendered index build：
    - `rendered_edit.rs` 的 newline/delete plan 不再直接调用 `RenderedDisplayIndex::build(snapshot)`。
    - 新增 `RenderedEditContext { snapshot, index, topology }`，由 `MarkdownEditor` 传入当前缓存 index。
    - 同一次编辑规划内只允许构建/读取一次 index。
    - `schedule_rendered_cache_prewarm` 不得因为 prewarm 重新触发 index build；只能复用当前 version 的 cached index。

  - Markdown syntax 增量化：
    - `MarkdownSyntaxTree::reparse_after_edit_range` 不再复制整篇 old/new source。
    - 用旧 `line_starts` 和 edit summary 计算 `InputEdit` positions；新文本通过 rope/chunk callback 喂给 tree-sitter。
    - `line_starts` 用 edit summary splice 增量维护。
    - block tree 用 tree-sitter incremental parse；根据 changed ranges 扩展到相邻 block/blank-run/list/table/code-fence 边界，只重收集 dirty range 的 blocks、tables、inline spans、projection
  replacements/dependencies。
    - 对无法安全局部化的批量多点编辑或异常结构编辑，保留 full reparse fallback，但加 perf counter，确保普通输入不会走 fallback。

  - `RenderedDisplayIndex` 增量化：
    - 新增 `RenderedDisplayIndex::update_after_edit(old, snapshot, syntax_delta, edit_summary)`。
    - dirty source rows 来自 syntax dirty rows 加 blank-run 邻居。
    - 只移除/重建 dirty rows 覆盖的 rendered items；后续 item 的 row/source offsets 通过 edit delta 调整。
    - `row_to_item` 和 blank row roles 只更新 dirty row window；item count delta 反馈给 editor 用于 `MdListState::splice`。
    - 保留 `build(snapshot)` 作为 cold start 和 fallback，测试中对比 incremental result 与 full build 等价。

  ## Test Plan

  - 单元测试：
    - Source 单行编辑仍只失效该 source row。
    - Rendered 单行等长编辑、长度变化编辑只失效当前 rendered item，不清全量 display cache/index。
    - Rendered paragraph merge/split、blank run、list item、blockquote、table row、code fence 边界编辑能正确 splice item count。
    - Enter/backspace/delete plan 在一次 action 内不重复 build index。
    - undo/redo 使用 edit summary 后 invalidation 与正向编辑一致。

  - 等价性测试：
    - `MarkdownSyntaxTree` incremental reparse 后的 blocks/tables/inline spans/projection 与 full parse 完全一致。
    - `RenderedDisplayIndex::update_after_edit` 后的 items、row mappings、blank roles 与 `RenderedDisplayIndex::build` 完全一致。
    - 用随机局部编辑覆盖普通段落、标题、列表、表格、代码块、HTML、link reference、inline math/image。

  - Perf 回归测试：
    - 在 `md_editor` perf suite 增加 `rendered_edit_equal_length`、`rendered_edit_length_change`、`rendered_enter_delete`。
    - 断言大文档 render 模式普通输入不触发 full syntax fallback、不触发 full rendered index build、不清全量 list measurement。
    - 保留现有 source/rendered scroll、resize、cached redraw 指标。

  - 验证命令：
    - `cargo test -p markdown_wysiwyg`
    - `cargo test -p md_projection`
    - `cargo test -p md_editor`
    - `cargo check -p updraft_editor`
    - `cargo perf-test -p md_editor -- --important`

  ## Assumptions

  - 实时编辑主路径优先：相关调用方直接迁移到 summary/incremental API，不维护并行旧路径。
  - 正确性优先于激进局部化：不能证明安全的复杂编辑走 full fallback，但 fallback 必须可计数并被 perf 测试限制。
  - 第一批优化目标是用户实时输入路径：普通字符输入、删除、换行、undo/redo；批量多点编辑可以先走 fallback。
  - 不引入 Zed IDE crate；只使用现有 `md_*`、`markdown_wysiwyg`、`md_sum_tree`、`gpui` 边界内能力。
