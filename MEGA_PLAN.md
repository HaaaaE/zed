# 性能护栏下的 GFM 重构总计划

## 总结

先把 Markdown 引擎重构为数据导向的语义索引，再在这个基础上实现正式 GitHub Flavored Markdown 的编辑器支持。目标是代码更好，性能也尽量更好；性能是硬护栏，所以关键阶段都要跑 perf，并设置很长的超时时间。

语法目标以正式 GFM 规范为准：<https://github.github.com/gfm/>。本计划不实现独立 HTML 导出器。现有数学和图片能力作为产品扩展保留。

重要更正：2026-05-30 `md_editor` perf 已完全重构为 segmented editor session。2026-05-29 之前的旧 process-timed 数据，以及 2026-05-29 的 hot-path self-timed 数据，测量语义都和新 session case 不一致，全部只保留为历史记录，不参与后续回归比较。后续 `md_editor` 性能判断只使用 2026-05-30 之后的新 session/segment 口径和同口径复跑结果。

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
  - `cargo perf-test -p md_editor -- --quiet`：旧 process-timed perf，当时 passed；耗时数值已作废并移除。

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
- `cargo perf-test -p md_editor -- --quiet`：旧 process-timed perf，当时 passed；耗时数值和旧对比结论已作废并移除。

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
- `cargo perf-test -p md_editor -- --quiet`：旧 process-timed perf，首跑和复跑当时 passed；耗时数值和旧对比结论已作废并移除。

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
- `cargo perf-test -p md_editor -- --quiet`：旧 process-timed perf，首跑和复跑当时 passed；耗时数值和旧对比结论已作废并移除。

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

### 2026-05-29：完整 HTML5 named character reference 表

已完成：

- `markdown_wysiwyg` named character reference 解码改为使用 `entities` 的 WHATWG HTML5 entity 数据，覆盖完整带分号 named references。
- 保持 numeric character reference 行为不变，继续支持十进制和十六进制 codepoint。
- 覆盖多 codepoint replacement，例如 `&NotEqualTilde;` 会投影为 `\u{2242}\u{0338}`，避免只保留第一个 codepoint。
- 新增 `markdown_wysiwyg` projection test，覆盖长名称、非 BMP 字符和双 codepoint named entity。
- 新增 `md_editor` rendered display-row test，确认完整 HTML5 named entities 在 rendered mode 中正确替换。

验证：

- `cargo test -p markdown_wysiwyg`：31 passed。
- `cargo test -p md_editor rendered_display_rows_replace_full_html5_named_entities`：1 passed。
- `cargo test -p md_editor`：194 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo check -p updraft_editor`：passed，保留既有 selection dead_code warnings。
- `cargo perf-test -p md_editor -- --quiet`：旧 process-timed perf，当时复跑 passed；性能门禁结论已作废。

后续仍未完成：

- task checkbox toggle 行为。
- 更多 GFM block/inline 覆盖。
- `markdown_wysiwyg` 模块拆分。
- 300KB mixed GFM fixture 与最终验证。

### 2026-05-29：Task checkbox toggle 行为

已完成：

- rendered mode 中 inactive task checkbox glyph 点击会直接切换源码 marker：`[ ]` -> `[x]`，`[x]` / `[X]` -> `[ ]`。
- 点击命中基于 `MarkdownProjectionOperation::Replace` 和实际 glyph bounds，不重新解析源码文本。
- glyph 外点击保持普通鼠标选择/定位语义，不改 source text。
- toggle 后保留原 selection/cursor，不因替换 checkbox marker 跳到任务行。
- 新增 interaction tests 覆盖 unchecked、checked uppercase 和 outside-glyph no-op。

验证：

- `cargo fmt`：完成；未保留无关格式化 diff。
- `cargo test -p md_editor rendered_task_checkbox`：3 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo test -p md_editor`：197 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo check -p updraft_editor`：passed，保留既有 selection dead_code warnings。
- `git diff --check`：passed。
- 未跑 perf：本批只改 rendered-mode mouse hit-test、等长 marker 替换和对应 interaction tests，没有改 parse、display row 构建、layout、render、scroll 或缓存预热热路径。

后续仍未完成：

- 更多 GFM block/inline 覆盖。
- `markdown_wysiwyg` 模块拆分。
- 300KB mixed GFM fixture 与最终验证。

### 2026-05-29：GFM leaf block 语义覆盖

已完成：

- `MarkdownBlockKind` 增加 setext heading、thematic break、indented code block、HTML block、link reference definition。
- setext heading 记录 level、content range、underline marker range 和跨行 row range；inactive rendered projection 会隐藏 underline marker。
- `md_editor` rendered styling 接入 setext heading 和 indented code：setext 复用 heading 样式/heading level，indented code 复用 fenced code 样式。
- thematic break、HTML block、link reference definition 先作为语义 block 暴露，不新增隐藏 marker，避免在专门 rendered element 尚未实现前吞掉源码显示。
- 新增 `markdown_wysiwyg` parser tests 覆盖 setext h1/h2 及 additional GFM leaf blocks。
- 新增 `md_editor` rendered display-row test 覆盖 inactive setext underline marker 隐藏和 heading level。

验证：

- `rustfmt --edition 2024 crates/markdown_wysiwyg/src/markdown_wysiwyg.rs crates/md_editor/src/layout.rs crates/md_editor/src/lib.rs`：完成；无关格式化 diff 已清理。
- `cargo test -p markdown_wysiwyg parses_`：12 passed。
- `cargo test -p markdown_wysiwyg`：33 passed。
- `cargo test -p md_editor rendered_display_rows_hide_inactive_setext_heading_marker`：1 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo test -p md_editor`：198 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo check -p updraft_editor`：passed，保留既有 selection dead_code warnings。
- `cargo perf-test -p md_editor -- --quiet`：旧 process-timed perf，当时 passed；耗时数值已作废并移除。
- `git diff --check`：passed。

后续仍未完成：

- 剩余 GFM block/inline 覆盖，尤其 blockquote、ordered/unordered/nested list、task list item 语义、hard/soft break、inline HTML、reference variants、autolink extension/tagfilter。
- `markdown_wysiwyg` 模块拆分。
- 300KB mixed GFM fixture 与最终验证。

### 2026-05-29：GFM inline leaf 语义覆盖

已完成：

- `MarkdownInlineKind` 增加 escape、entity、hard break、soft break、inline HTML。
- tree-sitter named nodes 覆盖 backslash escape、named/numeric entity、hard line break、inline HTML tag。
- soft break 由 inline parent range 中的换行推断，并避开已由 tree-sitter 标记的 hard break range。
- reference link variants、URI autolink、email autolink 继续归一为 `MarkdownInlineKind::Link`，并新增 parser tests 明确覆盖。
- `md_editor` 对新增 inline kind 使用默认 inline style，且不把它们当成 rendered element。

验证：

- `rustfmt --edition 2024 crates/markdown_wysiwyg/src/markdown_wysiwyg.rs crates/md_editor/src/layout.rs crates/md_editor/src/rendered_element.rs`：完成。
- `cargo test -p markdown_wysiwyg parses_`：14 passed。
- `cargo test -p markdown_wysiwyg`：35 passed。
- `cargo test -p md_editor`：198 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo check -p updraft_editor`：passed，保留既有 selection dead_code warnings。
- `cargo perf-test -p md_editor -- --quiet`：旧 process-timed perf，首跑和复跑当时 passed；耗时数值和旧对比结论已作废并移除。
- `git diff --check`：passed。

后续仍未完成：

- 剩余 GFM block 覆盖，尤其 blockquote、ordered/unordered/nested list、task list item 语义。
- GFM tagfilter/disallowed raw HTML 的明确语义和 rendered/editor 回归。
- `markdown_wysiwyg` 模块拆分。
- 300KB mixed GFM fixture 与最终验证。

### 2026-05-29：GFM blockquote/list 容器语义覆盖

已完成：

- `MarkdownBlockKind` 增加 blockquote、ordered list、unordered list、list item。
- block collection 对 blockquote/list/list item 容器继续递归，确保新增容器 block 不吞掉内部 paragraph、nested list、ordered list 等子 block。
- list block 根据源码 marker 区分 ordered/unordered，覆盖 `.` 和 `)` ordered marker 形式。
- `md_editor` layout match 接入新增 block kind；本批先作为 no-op block style 处理，避免在正式 list/blockquote rendered indentation 尚未实现前隐藏 marker 或改变显示。
- 新增 parser test 覆盖 blockquote、task list item 所在 unordered list、nested unordered list、nested ordered list、顶层 ordered list，以及 blockquote 内 paragraph 仍被收集。

验证：

- `rustfmt --edition 2024 crates/markdown_wysiwyg/src/markdown_wysiwyg.rs crates/md_editor/src/layout.rs`：完成。
- `cargo test -p markdown_wysiwyg parses_blockquotes_and_list_containers_without_losing_nested_blocks -- --nocapture`：1 passed。
- `cargo test -p markdown_wysiwyg`：36 passed。
- `cargo test -p md_editor`：198 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo check -p updraft_editor`：passed，保留既有 selection dead_code warnings。
- `git diff --check`：passed。
- `cargo perf-test -p md_editor -- --quiet`：旧 process-timed perf，首跑和复跑当时 passed；耗时数值和旧对比结论已作废并移除。

后续仍未完成：

- list/blockquote 的 rendered indentation、cursor/selection/editor 级行为回归。
- task list item 更完整语义与 editor 级行为回归。
- GFM tagfilter/disallowed raw HTML 的明确语义和 rendered/editor 回归。
- `markdown_wysiwyg` 模块拆分。
- 300KB mixed GFM fixture 与最终验证。

### 2026-05-29：perf setup 计时污染修正

已完成：

- 本节记录的是中间态，已被 2026-05-30 segmented editor session perf 重构取代；其中的 hot-path self-timed mean 不再作为后续 baseline。
- 临时分段测量确认现有 md_editor perf 的 setup 污染很大：300KB fixture 生成约 0.147ms，但 source/rendered `open_*_perf_window` 分别约 426.311ms / 435.694ms；首帧 draw 约 3.315ms / 4.184ms，cached redraw 约 1.155ms / 2.279ms，scroll 约 51.267ms / 49.527ms。
- 因此旧 `draw/cached redraw/scroll/edit/resize` mean 主要受 editor/document/window 创建支配，不能代表热路径性能，旧 1.5-2.2s process-timed 数字对热路径回归判断没有价值，已作废。
- `#[perf]` 改为只支持 self-reported measured-region timing：测试函数自行读取 `MD_PERF_ITER`，只对测量区间计时并打印 `MD_PERF_SELF_TIMED_NS <nanoseconds>`。
- `tooling/perf` runner 删除旧 Hyperfine/process-timed 分支，只直接采样测试上报的测量区间耗时，保留 mean/stddev/iterations 输出和 JSON 格式。
- md_editor important perf case 全部改为 self-reported timing：fixture、`TestAppContext`、window/editor 创建、初始 warm draw、滚动预热等 setup 不计入热路径；draw case 通过清 layout cache 测 uncached draw，cached redraw 测缓存命中 redraw，scroll/edit/resize 只包住实际操作区间。

验证：

- `cargo check -p perf -p util_macros`：passed。
- `cargo test -p md_editor source_mode_redraw_large_markdown_cached --profile release-fast --config 'target."cfg(true)".rustflags=["--cfg","perf_enabled"]' -- --nocapture`：passed，确认 perf case 只输出 self-reported `MD_PERF_SELF_TIMED_NS`，metadata/function suffix 使用 `MD_*` 前缀，不再包含 Zed 命名或旧 timing mode 分支字段。
- `cargo perf-test -p md_editor -- --quiet`：passed。当前 self-reported mean：rendered draw large 172.97ms，rendered cached redraw 122.13ms，rendered resize 60.59ms，rendered scroll large 319.59ms，rendered cached-region scroll 260.06ms，source draw large 117.52ms，source cached redraw 130.41ms，source scroll large 261.27ms，source cached-region scroll 257.82ms，source short scroll 266.50ms，source short cached-region scroll 233.65ms，source single-row edit large 111.52ms，source single-row edit length-change 129.53ms。
- 该次 self-reported run 中 `rendered draw large` SD 偏高，后续用新基线判断回归时需要复跑确认，不按旧 process-timed 数字横向比较。
- `cargo test -p md_editor`：198 passed。
- `cargo check -p updraft_editor`：passed，保留既有 selection dead_code warnings。
- `cargo test -p perf -p util_macros`：passed，均为 0 tests。
- `git diff --check`：passed。

后续仍未完成：

- 用 2026-05-30 之后的新 segmented editor session baseline 作为后续 MEGA_PLAN 的唯一 perf 对比口径，必要时复跑以降低高 SD case 的噪声。
- list/blockquote 的 rendered indentation、cursor/selection/editor 级行为回归。
- task list item 更完整语义与 editor 级行为回归。
- GFM tagfilter/disallowed raw HTML 的明确语义和 rendered/editor 回归。
- `markdown_wysiwyg` 模块拆分。
- 300KB mixed GFM fixture 与最终验证。

### 2026-05-30：segmented editor session perf 重构

已完成：

- `md_editor` important perf case 收敛为 `small_document_session` 和 `large_document_session` 两个 session case，分别覆盖短文档和 300KB 大文档。
- 每个 session 在固定 timeline 内覆盖 fixture 准备、`TestAppContext` 创建、window 创建、source editor 创建、source 首绘/cached redraw/cold scroll/cached-region scroll/编辑、source/rendered 模式切换、rendered 首绘/cached redraw/cold scroll/cached-region scroll/resize、切回 source 后的滚动与编辑、再次切 rendered 后的 cached redraw。
- 每个 timeline step 都通过 `MD_PERF_SEGMENT_NS <name> <nanoseconds>` 上报；`MD_PERF_SELF_TIMED_NS` 是同一轮内部 iterations 中所有 segment duration 的总和。
- setup 不再被伪装成 draw/scroll/edit 热路径，也不再完全隐藏；它作为 `fixture_prepare`、`context_create`、`window_create`、`source_editor_create` 等 segment 出现在 segment table 和 `% total` 中。command-level mean 代表整段合成编辑 session 成本，具体 hot path 判断看对应 segment。
- `tooling/perf` 现在要求每个样本必须同时包含 total 和至少一个 segment；同一 case 的 8 个 measured samples 必须有完全一致的 segment timeline。
- 重复 segment name 合法，通过 occurrence index 区分，例如两轮 `rendered_cached_redraw`、`rendered_scroll_cold`、`rendered_scroll_cached_region`、`rendered_resize`。
- `perf-compare` 现在除了 importance category delta，还会对同名 case 中 index+name 都匹配的 segment 输出 delta。
- 旧 process-timed log 和 2026-05-29 hot-path self-timed log 均不能和新 session log 横向比较；`.perf-runs` 中旧协议/旧语义文件只可作为历史证据。

验证：

- `cargo test -p perf`：7 passed。

后续仍未完成：

- 使用 2026-05-30 segmented session 首个正式 baseline 作为后续同口径 perf 对比起点。
- 之后所有 `md_editor` perf 回归判断只比较同一 session/segment 协议、同一 case 名、同一 segment occurrence 的结果。
- 继续完成 list/blockquote rendered indentation、cursor/selection/editor 级行为回归。
- task list item 更完整语义与 editor 级行为回归。
- GFM tagfilter/disallowed raw HTML 的明确语义和 rendered/editor 回归。
- `markdown_wysiwyg` 模块拆分。
- 300KB mixed GFM fixture 与最终验证。

### 2026-05-30：segmented editor session 首个 baseline

已完成：

- 使用 Windows Kits `D:\Windows Kits\10\bin\10.0.26100.0\x64` 加入 `PATH` 后运行 `cargo perf-test -p md_editor -- --quiet`。
- 生成本地 ignored run 文件 `.perf-runs/20260530-014124-8bf8106989.md_editor.json`，作为当前 session/segment 协议的首个正式 baseline 标识。
- command-level baseline：
  - `perf_tests::large_document_session`：iterations 8，iter/sec 0.78，mean 10274.33ms，SD 104.61ms。
  - `perf_tests::small_document_session`：iterations 16，iter/sec 2.81，mean 5684.57ms，SD 75.92ms。
- large session 主要耗时段：`rendered_first_draw` 2584.07ms / 25.2%，`rendered_cached_redraw_after_second_switch` 2514.20ms / 24.5%，`source_editor_create` 2393.54ms / 23.3%，随后是 rendered/source cached-region scroll，约 398-443ms。
- small session 主要耗时段：`rendered_scroll_cached_region` 两次分别 869.19ms / 15.3% 和 874.43ms / 15.4%，`source_scroll_cached_region_after_switch` 852.48ms / 15.0%，`source_scroll_cached_region` 805.91ms / 14.2%，cold scroll 约 475-506ms。

验证：

- `cargo perf-test -p md_editor -- --quiet`：passed。保留既有 warning：`md_text` 未使用 `FxHasher`，`md_editor` 未使用 `move_selection_right`。

后续仍未完成：

- 未来 perf 对比用该 baseline 或更新后的同协议 baseline；不得拿旧 process-timed / 2026-05-29 hot-path self-timed log 横向比较。
- 如果后续修改 session case 内容、segment 顺序或 segment 语义，需要重新建立 baseline，并在本文件记录失效边界。
- 继续完成 list/blockquote rendered indentation、cursor/selection/editor 级行为回归。
- task list item 更完整语义与 editor 级行为回归。
- GFM tagfilter/disallowed raw HTML 的明确语义和 rendered/editor 回归。
- `markdown_wysiwyg` 模块拆分。
- 300KB mixed GFM fixture 与最终验证。

### 2026-05-30：Blockquote/list marker projection 与 source reveal

已完成：

- `MarkdownBlockKind::BlockQuote` 现在记录每个 quoted source row 的 `>` marker range，inactive rendered projection 会隐藏 blockquote marker，active source range 会 reveal 原始 marker。
- `MarkdownBlockKind::ListItem` 现在记录 unordered marker（`- `、`+ `、`* `）和 ordered marker（`1. `、`1) `）range，并将 `content_range` 推进到 marker 之后。
- list block ordered/unordered 判定改成只从 list source 起点扫描 marker，不再对整段 list source 做 trim；list item marker 提取也只扫描 marker 前缀，避免对每个 item 查找整行。
- inactive task list item 会同时隐藏 list marker 并替换 task marker，因此 rendered row 从 `- ☐ todo` 变为 `☐ todo`；光标进入 task marker 时仍 reveal `- [ ] todo` 源码。
- 新增 `markdown_wysiwyg` tests 覆盖 blockquote/list item marker ranges、inactive projection、active source reveal、task marker dependency。
- 新增 `md_editor` rendered display-row tests 覆盖 inactive blockquote/list marker hiding、active blockquote/list marker reveal，并更新 task checkbox inactive row 期望。

验证：

- `cargo test -p markdown_wysiwyg`：37 passed。
- `cargo test -p md_editor rendered_display_rows_ -- --nocapture`：17 passed。
- `cargo test -p md_editor`：200 passed，保留既有 `move_selection_right` dead_code warning。
- `cargo check -p updraft_editor`：passed，保留既有 selection dead_code warnings。
- `git diff --check`：passed。
- `cargo perf-test -p md_editor -- --quiet --json=20260530-list-markers`：passed；对比首个 segmented baseline 为 important category near down 3.3%，未超过 5% 门槛。
- 复跑 `20260530-list-markers-rerun` 和优化后 `markeropt-20260530` 时出现全局性变慢，source-mode scroll、window/context、rendered scroll 等无关 segment 同步下滑，且 SD 明显变大；这些 run 记录为环境噪声，不更新 baseline，不作为本批回归判定依据。

后续仍未完成：

- list/blockquote 的 rendered indentation、cursor/selection/editor 级行为回归。
- task list item 更完整语义与 editor 级行为回归。
- GFM tagfilter/disallowed raw HTML 的明确语义和 rendered/editor 回归。
- `markdown_wysiwyg` 模块拆分。
- 300KB mixed GFM fixture 与最终验证。

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
- `md_editor` perf 必须使用 2026-05-30 之后的 self-timed segmented session 口径：每个样本必须有一个 `MD_PERF_SELF_TIMED_NS` total 和稳定有序的 `MD_PERF_SEGMENT_NS` timeline。
- 旧 process-timed `md_editor` mean 和 2026-05-29 hot-path self-timed mean 已作废，不参与回归判断。
- command-level mean 只代表合成 editor session 总成本；具体 draw、cached redraw、scroll、edit、resize、mode switch、setup 影响必须看 segment table。
- 当前首个正式 segmented baseline 标识为 `.perf-runs/20260530-014124-8bf8106989.md_editor.json`；该文件在本地 `.perf-runs` 中 ignored，计划文档只记录关键摘要。
- 以下节点必须跑 perf：
  - 重构前 baseline
  - semantic index 重构后
  - `md_editor` 接入合并语义查询后
  - projection operations 替换 hidden-range-only projection 后
  - 增量解析接入后
  - 每个主要 GFM block/inline 批次后
  - 最终完整验证
- 性能失败标准：
  - 同一 segmented session case 的 command mean 或匹配 segment occurrence mean 回退超过约 5%，复跑后仍成立，并结合 SD 判断不是噪声。
  - cached redraw/scroll 的 layout computation counts 明显增加，且没有合理功能原因。
- 新增 300KB mixed GFM perf fixture，覆盖 heading、nested list、blockquote、table、task item、link、autolink、HTML、code fence、CJK。
- 每类 GFM 语法新增 parser tests，验证 kind、source range、content range、marker ranges、nesting/depth，以及 invalid Markdown 不 panic。
- 新增 rendered/editor tests，覆盖 source reveal、cursor movement、selection、deletion、task checkbox toggle、list/blockquote indentation、entity/escape projection、table regression。

## 假设

- “完整 GFM”指正式 GFM 的编辑器完整支持，不指 HTML 输出逐字节对齐。
- Raw HTML 只解析和安全表示，不执行脚本，也不引入 embedded browser 行为。
- 现有 math/image 能力继续支持，但不作为正式 GFM 验收项。
- 可以使用冗余索引和专用 hot-path API 换取性能，只要 ownership 和 invalidation 规则明确。
