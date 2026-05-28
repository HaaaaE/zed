# 性能护栏下的 GFM 重构总计划

## 总结

先把 Markdown 引擎重构为数据导向的语义索引，再在这个基础上实现正式 GitHub Flavored Markdown 的编辑器支持。目标是代码更好，性能也尽量更好；性能是硬护栏，所以关键阶段都要跑 perf，并设置很长的超时时间。

语法目标以正式 GFM 规范为准：<https://github.github.com/gfm/>。本计划不实现独立 HTML 导出器。现有数学和图片能力作为产品扩展保留。

## 进度记录

### 2026-05-28：合并语义查询与懒增量解析接入

已完成：

- 新增 `MarkdownRangeSemantics`，一次返回可见 source range 所需的 blocks、inline spans、projection、active projection source ranges、rendered element candidates。
- `markdown_wysiwyg` 内部的 projection 构建改为可复用已收集的 block/inline 语义，兼容 `projection_for_source_range_with_inactive_ranges` 等旧 API。
- 新增 `MarkdownSyntaxTree::reparse_after_edit_range`，保留 `reparse_after_edit` 作为兼容 wrapper。
- `md_editor` rendered display row 改为通过 `MarkdownRangeSemantics` 构建，移除编辑器侧分散 block/inline/projection 汇总逻辑。
- Source mode 快路径保持不刷新 markdown syntax tree；相关 source cache/render 测试继续覆盖。
- `md_buffer` 对缓存语法树有效的单一连续 edit 记录 pending incremental reparse，并在真正请求 syntax tree 时懒执行；多 edit、缓存过期、undo/redo 等路径回退全量 parse。
- 新增语义聚合和懒增量解析测试。

验证：

- 修改前基线：
  - `cargo test -p markdown_wysiwyg`：23 passed。
  - `cargo test -p md_buffer`：16 passed。
  - `cargo test -p md_editor`：184 passed。
- 修改后：
  - `cargo check -p updraft_editor`：passed，保留既有 dead_code warnings。
  - `cargo test -p markdown_wysiwyg`：25 passed。
  - `cargo test -p md_buffer`：17 passed。
  - `cargo test -p md_editor`：184 passed，保留既有 `move_selection_right` dead_code warning。
  - `cargo test -p md_sum_tree`：10 passed。
  - `cargo test -p md_rope`：24 passed，doc-tests 0 passed。
  - `cargo test -p md_text`：37 passed。
  - `cargo perf-test -p md_editor -- --quiet`：passed。当前 mean：rendered draw large 1908.80ms，rendered cached redraw 1916.50ms，rendered resize 2011.90ms，rendered scroll large 2152.50ms，source draw large 1921.50ms，source cached redraw 1888.10ms，source single-row edit large 1881.60ms，source single-row edit length-change 1932.60ms。

后续仍未完成：

- 将 `crates/markdown_wysiwyg` 文件级实现拆成 parser、block collection、inline collection、table extraction、projection、indexing 模块。
- 将 `MarkdownProjectionMap` 泛化为 projection operations，覆盖 hide/replace/source-display offset 映射。
- 正式 GFM blocks/inline/extensions 的完整覆盖与 rendered/editor 行为测试。
- 300KB mixed GFM perf fixture。
- 每个主要 GFM 批次后的 perf 对比与回归判定。

## 关键改动

- 重构 `crates/markdown_wysiwyg`：
  - 保持扁平、可按 row/range 查询的语义模型，因为 `md_editor` 以源文件行为渲染单位。
  - 内部拆分 parser、block collection、inline collection、table extraction、projection、indexing。
  - 统一管理 block ranges、inline span ranges、marker dependencies、row mappings、table lookups。
  - 初期保留现有公开 API，但内部全部走新索引。
- 增加性能友好的查询接口：
  - 新增 `MarkdownRangeSemantics`，一次返回可见范围需要的 blocks、inline spans、projection、active projection ranges、rendered-element candidates。
  - `md_editor` 不再分别查询 blocks、inline spans、projection。
  - Source mode 快路径保持不变。
- 在完整 GFM 前先泛化 projection：
  - 将 `MarkdownProjectionMap` 从“只隐藏 range”扩展为有序 projection operations。
  - 支持 hide、replace、source/display offset 映射。
  - 覆盖 escape、entity、task checkbox、markers 和未来 inline replacement。
  - 尽量保留 `hidden_ranges()` 兼容现有测试。
- 接入增量解析：
  - 新增 `MarkdownSyntaxTree::reparse_after_edit_range`。
  - `md_buffer` 在单一连续 edit 且缓存语法树有效时使用增量 reparse。
  - 多 edit、undo/redo、缓存过期或不确定场景回退全量 parse。
  - 正确性优先，不做过度复杂的增量策略。
- 实现正式 GFM 覆盖：
  - Blocks：setext heading、thematic break、blockquote、ordered/unordered/nested list、task list item、indented code、fenced code、HTML block、link reference definition、现有 pipe table。
  - Inline：escape、entity、hard/soft break、inline HTML、code/link/image variants、reference link/image、autolink、现有 emphasis/strong/strike/code。
  - GFM extensions：table、task list item、strikethrough、autolink extension、tagfilter/disallowed raw HTML。
  - Rendered mode 继续使用 active source reveal：非活动内容富文本显示，活动 block/span 显示可编辑源码。

## 公开接口

- 扩展 `MarkdownBlockKind` 和 `MarkdownInlineKind`，覆盖正式 GFM 节点。
- 新增 `MarkdownRangeSemantics`，作为编辑器侧首选查询结果。
- 新增 `MarkdownProjectionOperation` 或等价类型。
- 新增 `MarkdownSyntaxTree::reparse_after_edit_range`。
- 保留 `blocks_in_source_range`、`inline_spans_in_source_range`、table helpers、projection helpers 作为兼容 wrapper，直到 `md_editor` 完成迁移。

## 测试与性能

- 修改前先跑并记录基线：
  - `cargo check -p updraft_editor`
  - `cargo test -p markdown_wysiwyg`
  - `cargo test -p md_buffer`
  - `cargo test -p md_editor`
  - `cargo perf-test -p md_editor -- --quiet`
- perf 命令使用很长超时，通常 2 小时：`timeout_ms = 7200000`。
- 以下节点必须跑 perf：
  - 重构前 baseline
  - semantic index 重构后
  - `md_editor` 接入合并语义查询后
  - projection operations 替换 hidden-range-only projection 后
  - 增量解析接入后
  - 每个主要 GFM block/inline 批次后
  - 最终完整验证
- 性能失败标准：
  - 现有 important perf case median 回退超过约 5%，复跑后仍成立。
  - cached redraw/scroll 的 layout computation counts 明显增加，且没有合理功能原因。
- 新增 300KB mixed GFM perf fixture，覆盖 heading、nested list、blockquote、table、task item、link、autolink、HTML、code fence、CJK。
- 每类 GFM 语法新增 parser tests，验证 kind、source range、content range、marker ranges、nesting/depth，以及 invalid Markdown 不 panic。
- 新增 rendered/editor tests，覆盖 source reveal、cursor movement、selection、deletion、task checkbox toggle、list/blockquote indentation、entity/escape projection、table regression。

## 假设

- “完整 GFM”指正式 GFM 的编辑器完整支持，不指 HTML 输出逐字节对齐。
- Raw HTML 只解析和安全表示，不执行脚本，也不引入 embedded browser 行为。
- 现有 math/image 能力继续支持，但不作为正式 GFM 验收项。
- 可以使用冗余索引和专用 hot-path API 换取性能，只要 ownership 和 invalidation 规则明确。
