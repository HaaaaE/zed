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

### 2026-05-28：Projection operations 兼容层

已完成：

- `MarkdownProjectionMap` 改为 operation-backed，新增 `MarkdownProjectionOperation::{Hide, Replace}`。
- 保留 `hidden_ranges()`，并从 operations 生成兼容 ranges，现有 marker hiding 调用不需要迁移。
- 新增 `operations()`、`with_operations()`、`project_source_text()`，`source_to_display` 和 `display_to_source` 已按 operation 语义处理 hide/replace。
- `md_editor` 的 row text projection 改为委托 `MarkdownProjectionMap::project_source_text`，为 entity、escape、task checkbox 等后续 replacement 投影铺路。
- 新增 hide+replace 映射测试，覆盖 display text、display length、source->display、display->source 和 hidden range 兼容行为。

验证：

- `cargo check -p updraft_editor`：passed，保留既有 dead_code warnings。
- `cargo test -p markdown_wysiwyg`：26 passed。
- `cargo test -p md_editor`：184 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo perf-test -p md_editor -- --quiet`：passed。当前 mean：rendered draw large 1940.80ms，rendered cached redraw 1997.20ms，rendered resize 1899.10ms，rendered scroll large 2096.00ms，rendered cached-region scroll 2222.60ms，source draw large 1899.90ms，source cached redraw 1859.30ms，source single-row edit large 1914.10ms，source single-row edit length-change 1908.60ms。
- 与上一条进度记录中的 perf run 相比，important case 未见超过约 5% 的 median 回退；最大可疑项是 rendered cached redraw 约 +4.2%，低于当前失败阈值，后续大批次仍需复跑确认。

后续仍未完成：

- 将实际 inline escape/entity/task checkbox 等语义改为生成 `Replace` operation。
- 为 replacement projection 增加 editor 级 cursor/selection/deletion 回归。
- 继续拆分 `markdown_wysiwyg` 模块边界。

### 2026-05-28：Escape/entity replacement projection 首批接入

已完成：

- 从 tree-sitter inline tree 收集 `backslash_escape`、`entity_reference`、`numeric_character_reference` projection replacements。
- inactive rendered projection 将 backslash escape 显示为被 escape 的字符，将 `amp/apos/gt/lt/nbsp/quot` 和 decimal/hex numeric entity 显示为 decoded text。
- active source reveal 覆盖 replacement range：光标进入 escape/entity source range 时不应用 replacement，并通过 active projection source ranges 触发行缓存区分。
- projection marker dependency 索引纳入 replacement source ranges。
- 新增 parser/projection tests 覆盖 escape、named entity、decimal entity、hex entity、active reveal。

验证：

- `cargo check -p updraft_editor`：passed，保留既有 dead_code warnings。
- `cargo test -p markdown_wysiwyg`：28 passed。
- `cargo test -p md_editor`：184 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo perf-test -p md_editor -- --quiet`：第一次 run passed，但 source scroll 部分 case 相比上一轮超过 5% 且 SD 偏大；按性能失败标准已复跑。
- `cargo perf-test -p md_editor -- --quiet` 复跑：passed。复跑 mean：rendered draw large 2005.30ms，rendered cached redraw 1857.60ms，rendered resize 1878.50ms，rendered scroll large 2008.00ms，rendered cached-region scroll 2271.40ms，source draw large 1889.00ms，source cached redraw 1881.30ms，source scroll large 2042.90ms，source cached-region scroll 2217.60ms，source short scroll 266.80ms，source single-row edit large 1932.30ms，source single-row edit length-change 1924.80ms。
- 复跑后未见 important case 相对上一条记录持续超过约 5% 的 median 回退。

后续仍未完成：

- 完整 HTML5 named character reference 表。
- task checkbox replacement projection。
- replacement projection 的 selection/deletion 交互回归。

### 2026-05-28：Replacement projection 的 editor display-row 回归

已完成：

- 新增 `md_editor` rendered display row tests，覆盖 inactive escape/entity replacement 的显示文本。
- 覆盖 replacement source/display offset 映射：display offset 落在 replacement 起点时映射回 source replacement 起点。
- 覆盖 active source reveal：光标进入 escape/entity source range 时显示源码，其它 replacement 仍保持 rendered 显示。

验证：

- `cargo test -p md_editor rendered_display_rows_`：11 passed。
- `cargo test -p md_editor`：186 passed，保留既有 `move_selection_right` dead_code warning。

后续仍未完成：

- replacement projection 的 selection/deletion 交互回归。
- task checkbox replacement projection 及对应 editor tests。

### 2026-05-29：Task checkbox replacement projection

已完成：

- 从 tree-sitter block tree 收集 `task_list_marker_unchecked` 和 `task_list_marker_checked` projection replacements。
- inactive rendered projection 将 `[ ]` 显示为 `☐`，将 `[x]` 显示为 `☑`，并继续通过 replacement operation 保留 source/display offset 映射。
- active source reveal 覆盖 task marker source range：光标进入 `[ ]` / `[x]` 时显示源码 marker，非活动行继续显示 checkbox glyph。
- 新增 `markdown_wysiwyg` projection tests，覆盖 inactive checkbox replacement、active marker reveal 和 active projection source ranges。
- 新增 `md_editor` rendered display-row tests，覆盖 inactive checkbox glyph 显示、active marker source reveal 和 marker 起点映射。

验证：

- `cargo test -p markdown_wysiwyg`：30 passed。
- `cargo test -p md_editor rendered_display_rows_`：13 passed。
- `cargo test -p md_editor`：188 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo check -p updraft_editor`：passed，保留既有 selection dead_code warnings。
- `cargo perf-test -p md_editor -- --quiet`：第一次 run passed。mean：rendered draw large 1890.90ms，rendered cached redraw 1970.00ms，rendered resize 1911.90ms，rendered scroll large 2074.50ms，rendered cached-region scroll 2281.80ms，source draw large 1947.80ms，source cached redraw 1860.70ms，source scroll large 2093.70ms，source cached-region scroll 2232.10ms，source single-row edit large 2007.50ms，source single-row edit length-change 1986.00ms。
- 第一次 run 中 rendered cached redraw 相对上一条完整 perf 记录超过约 5%，按性能失败标准复跑。
- `cargo perf-test -p md_editor -- --quiet` 复跑：passed。复跑 mean：rendered draw large 2019.30ms，rendered cached redraw 1936.80ms，rendered resize 1945.60ms，rendered scroll large 2113.60ms，rendered cached-region scroll 2246.60ms，source draw large 1931.50ms，source cached redraw 1952.10ms，source scroll large 2118.90ms，source cached-region scroll 2216.40ms，source single-row edit large 1925.70ms，source single-row edit length-change 1929.70ms。
- 原可疑的 rendered cached redraw 复跑后低于 5%；rendered scroll large 仅在复跑中略过阈值、首跑未持续，未见同一 important case 连续超过约 5% 的回退。

后续仍未完成：

- 完整 HTML5 named character reference 表。
- replacement projection 的 selection/deletion 交互回归。
- task checkbox toggle 行为。

### 2026-05-29：Replacement projection selection/deletion 交互回归

已完成：

- rendered mode 光标位于 inactive replacement 右边界时，horizontal move/select-left 会跨过整个 replacement source range，而不是落入隐藏的最后一个源码字符。
- rendered mode 在 inactive escape/entity/task marker replacement 右边界 backspace 时，会删除完整 replacement source range。
- replacement 起点仍保持 active source reveal 的逐字符编辑语义：delete 会编辑源码字符，而不是强制删除整个 replacement。
- 新增 visual-row tests 覆盖 inactive escape/entity replacement 右边界移动与选择。
- 新增 interaction tests 覆盖 inactive escape replacement、task marker replacement 的 backspace，以及 active replacement 起点 delete。

验证：

- `cargo test -p md_editor replacement`：5 passed。
- `cargo test -p md_editor`：193 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo check -p updraft_editor`：passed，保留既有 selection dead_code warnings。
- 未跑 perf：本批只改 rendered-mode selection/edit 边界处理和对应 tests，没有改 display row 构建、layout、parse 或 scroll/render 热路径。

后续仍未完成：

- 完整 HTML5 named character reference 表。
- task checkbox toggle 行为。
- 更多 GFM block/inline 覆盖。

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
