# Render 模式大文档编辑性能真实修复计划

## Summary

当前干净 HEAD 的 perf 与手工卡顿一致：大文档 `rendered_edit_equal_length` 约 5.28s，`rendered_edit_length_change` 约 4.30s，`rendered_enter_delete` 约 10.44s，而 cached redraw 只有几十毫秒。真正热点在编辑动作同步刷新链路，不在绘制。

修复目标：普通字符输入、长度变化输入、回车/删除在 Render 模式下不触发全文 syntax refresh、不 rebuild 全量 rendered index、不全量重测列表。禁止引入 Zed IDE crates，不做“老代码兼容”保留分叉，直接迁移调用链。

## Key Changes

- 先补真实分段 perf：
  - 把 Render 编辑黑盒段拆成 `edit_apply`、`rendered_plan`、`notify_after_edit`、`syntax_snapshot_refresh`、`rendered_index_update`、`list_state_sync`、`draw_after_edit`。
  - 给 `md_buffer`、`markdown_wysiwyg`、`md_projection` 加 `cfg(perf_enabled)` 计数/耗时：syntax refresh 次数、全文 source copy 次数、derived syntax 全量收集耗时、rendered index full build/update 耗时、扫描 item/row 数。
  - perf 输出必须能解释每个 4-10 秒编辑段具体花在哪。

- 重做 Render 普通文本编辑 fast path：
  - `notify_after_edit` 不再在普通文本编辑路径里调用 `buffer.snapshot()`。
  - 基于 `BufferEditSummary` + 当前 cached `RenderedDisplayIndex` + `TextBufferSnapshot` 判断是否为结构无关编辑。
  - 覆盖真实常见场景：段落、标题、列表内容行、blockquote 内容行、表格 cell 内普通字符编辑，只要编辑没有触碰 Markdown marker、行边界、inline marker、table delimiter、code fence、HTML/link reference 结构字符。
  - 命中 fast path 时只更新当前 rendered item 的 source range/id 和后续 byte offset，不刷新 syntax，不 rebuild full index，只清当前 item 的 display/layout/table 局部缓存。

- 回车/删除走结构窗口增量：
  - rendered edit plan 只允许读取一次已有 index/topology；规划阶段不得额外 build index。
  - 对回车、backspace、delete，用 edit summary 计算 dirty source row window，扩展到相邻空行 run、当前 block/list/table/code fence 边界。
  - 只 rebuild dirty window 覆盖的 rendered items，并对后续 item 做 row/byte offset 平移。
  - `MdListState` 使用 item splice/局部 remeasure，不允许因 row/item count 变化而全量 `remeasure()`。

- 真正削掉 syntax 全文成本：
  - `refresh_syntax_tree` 不再先 `self.text.snapshot().text()` 复制整篇新文档。
  - `MarkdownSyntaxTree` 增加基于 rope/chunk callback 的 incremental parse 输入。
  - 对普通 fast path，允许 syntax cache 延迟刷新；只有后续真正需要 syntax 语义时才按 dirty window 增量更新。
  - 对复杂编辑才 full parse fallback，并计数；普通输入/回车/删除 perf 中 fallback 必须为 0。

- 清理无效优化和断言：
  - 删除只覆盖“空行包围孤立段落”的过窄 fast path 判断，替换为结构字符/结构窗口判定。
  - perf 断言不再只看 “full index build = 0”，必须同时断言 no full syntax refresh、no full source copy、no full list remeasure。

## Test Plan

- 单元/交互测试：
  - Render 模式段落、标题、列表项、blockquote、表格 cell 普通字符输入只失效当前 item。
  - 长度变化输入正确平移后续 item source range，row mapping 不变。
  - 回车/删除在段落 split/merge、blank run、列表项、表格行、code fence 边界正确 splice item count。
  - undo/redo 使用同一 summary invalidation 管线，行为与正向编辑一致。
  - 复杂结构编辑触发 fallback 时结果正确且计数可见。

- 等价性测试：
  - incremental rendered index 更新后，与 full build 的 items、row mappings、blank roles 完全一致。
  - syntax dirty-window 更新后，与 full parse 的 blocks、tables、inline spans、projection 完全一致。
  - 随机局部编辑覆盖段落、标题、列表、表格、代码块、HTML、link reference、inline markdown marker。

- Perf 验收：
  - `cargo perf-test -p md_editor -- --important` 必须输出编辑子阶段计时。
  - 大文档 Render 普通等长/变长输入不得触发 full syntax refresh、full source copy、full rendered index build、full list remeasure。
  - 大文档 `rendered_edit_equal_length`、`rendered_edit_length_change` 目标降到 source 编辑同量级；`rendered_enter_delete` 不再是多秒级。
  - 最终跑：`cargo test -p markdown_wysiwyg`、`cargo test -p md_projection`、`cargo test -p md_editor`、`cargo check -p updraft_editor`、`cargo perf-test -p md_editor -- --important`。

## Assumptions

- 这是本地 GPUI Markdown editor，不涉及远程协作，不引入 `project`、`workspace`、`editor`、`language`、`multi_buffer` 等 Zed IDE crate。
- 不做旧 API 兼容层；直接把编辑、undo、redo、render invalidation 迁到 summary/incremental 路径。
- 正确性优先：无法证明安全的结构编辑可以 full fallback，但普通输入、回车、删除不能 fallback。

## Progress

- 2026-05-31:
  - 扩展 `RenderedDisplayIndex::update_after_plain_text_edit`，不再只接受空行包围的单行 paragraph；现在覆盖普通 paragraph 内容、heading 内容、list/blockquote fallback 行内容、table cell 内容的单行普通文本编辑。
  - fast path 继续禁止结构字符、Markdown marker 前缀、table delimiter 行/分隔符编辑；命中时只平移当前/后续 item source range，不请求 `Buffer::snapshot()`，保留 deferred syntax refresh。
  - 新增等价测试：common rendered rows 的 plain-text 增量 index 与 full build 完全一致；marker/table delimiter 编辑拒绝 fast path。
  - 新增编辑层回归：Render 模式 heading/list/blockquote/table 普通文本编辑 `full_builds = 0`、`incremental_updates = 1`，且 `cached_syntax_version` 不变。
  - 已验证：`cargo test -p md_projection`、`cargo test -p md_editor rendered_plain_text_edits_in_common_rows_skip_syntax_and_full_index_build`、`cargo test -p md_editor rendered_length_preserving_single_item_edit_keeps_other_display_rows`。
  - 给 `md_buffer` 增加 `BufferSyntaxStats`，开始计数 full parse、full syntax refresh、incremental syntax refresh、full source copy；普通 rendered text edit 回归现在同时断言 syntax stats 全为 0。
  - 已验证：`cargo test -p md_buffer single_edit_defers_incremental_reparse_until_syntax_is_requested`、`cargo test -p md_editor rendered_plain_text_edits_in_common_rows_skip_syntax_and_full_index_build`、`cargo check -p updraft_editor`。

- 2026-06-01:
  - 给 `MdListState` 增加 remeasure stats，开始计数 full remeasure、局部 remeasure 调用次数和 item 数。
  - 普通 Render 文本编辑回归现在同时断言 `full_remeasures = 0`，且只局部 remeasure 当前 item。
  - 已验证：`cargo fmt --check`、`cargo test -p md_editor rendered_plain_text_edits_in_common_rows_skip_syntax_and_full_index_build`、`cargo check -p updraft_editor`。
  - `md_editor` perf helper 不再只断言 rendered index full build；Render 普通等长/变长编辑 perf 路径现在同时断言 syntax stats 全 0、list full remeasure 为 0、仅局部 remeasure 当前 item。
  - Render 编辑 perf 输出拆出 `rendered_edit_equal_length_apply` / `rendered_edit_equal_length_draw_after_edit`、`rendered_edit_length_change_apply` / `rendered_edit_length_change_draw_after_edit`、`rendered_enter_delete_apply` / `rendered_enter_delete_draw_after_edit`。
  - 已验证：`cargo test -p md_editor --profile release-fast --lib --no-run --config 'target."cfg(true)".rustflags=["--cfg","perf_enabled"]'`。
  - `RenderedDisplayIndexStats` 增加 full build row/block 扫描数和 incremental item 扫描数，普通 Render 编辑测试/perf 断言现在确认走了 incremental scan。
  - `RenderedDisplayIndex::update_after_edit` 增加 row-count dirty window splice：对插入换行、paragraph break、删除 blank boundary 等行数变化编辑，与 full build 的 items、row mappings、blank roles 保持一致，避免直接 fallback 到 full rendered index build。
  - 已验证：`cargo test -p md_projection`、`cargo test -p md_editor rendered_plain_text_edits_in_common_rows_skip_syntax_and_full_index_build`、`cargo check -p updraft_editor`。
  - rendered edit planning 增加带 `RenderedDisplayIndex` 的路径；编辑器动作在 Render 模式下把当前 cached index 传入 newline/backspace/delete planning，避免 planning 阶段额外 `RenderedDisplayIndex::build`。
  - `rendered_enter_delete` perf helper 现在断言 apply 阶段 `full_builds = 0` 且发生 incremental update。
  - 已验证：`cargo fmt --check`、`cargo test -p md_editor rendered_enter_continues_unordered_task_ordered_and_blockquote_lines`、`cargo test -p md_editor --profile release-fast --lib --no-run --config 'target."cfg(true)".rustflags=["--cfg","perf_enabled"]'`。
