# Rendered 段序列重构计划

## Summary

将 rendered 模式从“源码行渲染器”改成“段序列编辑器”，但继续保留 `source row` 作为坐标、range、缓存、重测和 selection 的底层单位。GFM AST 继续提供语义/样式，不再决定 rendered 文档结构。

## Progress

### 2026-05-30

- 已完成第一片：`RenderedDisplayIndex` 的 item 语义已改为段序列方向，当前包含 `Paragraph / Heading / EmptyParagraph / StructuredBlock / TableRow / SourceFallback`。
- 已完成 blank run 解释：连续物理空白 source rows 按奇数规范长度渲染，偶数 run 的多余行标记为 `IgnoredExtra`，渲染阶段不改写源码。
- 已完成 `row_to_item` 映射调整：`EmptyParagraph` 拥有自己的 item；`Separator` / `IgnoredExtra` 不生成 item，只映射到邻近可渲染 item 以维持滚动、selection reveal 和缓存定位。
- 已新增索引测试覆盖 `A\n\nB`、`A\n\n\nB`、`A\n\n\n\nB`、`A\n\n\n\n\nB`、`A\n\n\n\n\n\nB` 的 empty paragraph 数量与 blank role。
- 已完成第二片：新增 `InsertSoftBreak` action 和默认 `shift-enter` keybinding；Source 模式 Enter 仍保留自动缩进；Rendered 模式 Enter 先落基础段分割，写入规范 separator `\n\n`；Rendered 模式 Shift+Enter 写入普通 `\n` 作为段内软断行。
- 已新增编辑测试覆盖 rendered Enter、rendered Shift+Enter、Source Enter 自动缩进。
- 已将 rendered paragraph 内普通 `\n` 的显示从空格改为视觉断行，和 `Shift+Enter` 的输入语义对齐。
- 已修正 rendered paragraph item 的当前段/caret 判断：跨 source rows 的同一段现在按 `source_row_range` 识别，`Shift+Enter` 后的第二个 source row 仍在同一个段节点内显示 caret 和当前段高亮。
- 已修正 softbreak 与自动 wrap 的组合布局：段内视觉换行仍保留，但每个 softbreak 分段内部继续按 wrap width 自动折行。
- 已让 rendered 空段在 Backspace/Delete 下可直接删除，并新增针对性的 interaction 测试。
- 已修正规范空段删除：删除一个 `EmptyParagraph` 时会连同后续 separator 一起收敛为单 separator，Backspace 回到前一段尾，Delete 前进到后一段首。
- 已新增空段内 Enter 测试，覆盖从 1 个 empty paragraph 到 2 个 empty paragraphs 的 `2n + 1` blank run 序列化。
- 已新增 rendered 空段可点击/caret 的交互测试，确认它走 Text layout 且保有可见高度。
- 验证通过：
  - `cargo test -p md_editor rendered_display_index --lib`
  - `cargo test -p md_editor enter --lib`
  - `cargo test -p md_editor rendered_soft_break_caret_stays_inside_paragraph_item --lib`
  - `cargo test -p md_editor rendered_soft_break_segments_still_wrap --lib`
  - `cargo test -p md_editor canonical_empty_paragraph --lib`
  - `cargo test -p md_editor --lib`
  - `cargo check -p updraft_editor`

### 2026-05-31

- 已细化 rendered 段尾 Enter：段尾后已有 separator 时创建一个可见空段；最终段尾没有 separator 时补足规范 blank run，并把光标放到生成的 `EmptyParagraph` 上。
- 已新增最终段尾连续 Enter 测试，确认第一次 Enter 创建可见空段，第二次 Enter 继续增长为空段序列并保持光标在新增空段上。
- 已修正 softbreak 分段内的 wrap 路径：无 inline atom 的段内视觉换行现在复用普通文本 `shape_text(..., Some(wrap_width))` 的折行测量，保证 `Shift+Enter` 造成的视觉分行和挤压造成的自动分行采用同一套宽度/样式计算。
- 已加强 softbreak wrap 测试，除确认 softbreak 前后都能产生视觉行外，也逐行校验实际测量宽度不超过当前 wrap width。
- 验证通过：
  - `cargo test -p md_editor rendered_enter_at_ --lib`
  - `cargo test -p md_editor rendered_consecutive_enter_at_final_paragraph_end_grows_empty_paragraphs --lib`
  - `cargo test -p md_editor rendered_soft_break_ --lib`
  - `cargo test -p md_editor --lib`
- 剩余主要工作：跨段 selection 删除的最小规范 blank run 重建；对 `PAR_PLAN.md` 全量要求做完成审计。

## Key Changes

- 在 `rendered_index.rs` 重建 `RenderedDisplayIndex`：item 语义改为 `Paragraph / Heading / EmptyParagraph / StructuredBlock / TableRow / SourceFallback`。
- `source_row_range`、`source_range`、`row_to_item` 保留；但 `row_to_item` 不再把 blank row 全部塌到邻近 item，而是按 blank role 映射。
- 引入内部 `BlankRowRole`：
  - `Separator`：段分割，不生成可编辑 item。
  - `EmptyParagraph`：生成 `EmptyParagraph` item，可放光标。
  - `IgnoredExtra`：非规范 blank run 渲染时向下取最近规范 run，源码不立即重写。
- blank run 按“连续物理空白 source rows”解释：
  - `1` 行：仅分割，`0` 个空段。
  - `2` 行：非规范，按 `1` 行渲染，`0` 个空段。
  - `3` 行：`1` 个空段。
  - `4` 行：非规范，按 `3` 行渲染，`1` 个空段。
  - 通用公式：`effective = run_len` 若为奇数，否则 `run_len - 1`；`empty_count = (effective - 1) / 2`。
- separator 不显示为空段，只提供段间 spacing；`EmptyParagraph` 才显示为可编辑空行。

## Editing Behavior

- Source 模式保持现有源码编辑行为。
- Rendered 模式 `Enter` 执行段操作：
  - 正文/标题中间：split 成两个段，源码写入规范段分割。
  - 段尾：创建下一段；连续 Enter 产生规范空段结构。
  - 空段内 Enter：新增一个空段，blank run 规范化为 `2n + 1` 个 blank source rows。
- 新增 `Shift+Enter` action；Rendered 模式插入普通 `\n` 作为段内断行。
- Rendered 模式 paragraph 内普通 `\n` 显示为视觉换行，不再 inactive 投影成空格；这只改变 rendered 编辑语义，GFM 兼容显示可在 Source/preview 另行处理。
- Rendered 模式 Backspace/Delete 按段结构处理：
  - 空段上删除：删除该 `EmptyParagraph` item 并规范化 blank run。
  - 段首 Backspace：与前一段合并。
  - 跨段 selection 删除：删除选中段内容并重建最小规范 blank run。

## Implementation Notes

- `DisplayRow` 继续携带 `source_row_range` 和源码坐标；新增 item kind/blank role 信息，避免 layout/edit 层反查 blank 含义。
- cache key 继续使用 `item_id + source_range + source_row_range + projection state`；`item_id` 对空段使用 source row + role 生成，保证稳定。
- paragraph item 内部仍复用现有 inline projection、inline atom、table/block layout；只替换 rendered item 构建和 rendered-mode 编辑入口。
- 删除或改写“blank rows collapsed between blocks”相关测试和逻辑；保留 source-row invalidation、remeasure、selection 映射机制。

## Test Plan

- Index tests：
  - `A\n\nB`：两个 paragraph item，0 个 empty paragraph。
  - `A\n\n\nB`：非规范，按 `A\n\nB` 渲染，0 个 empty paragraph，源码不变。
  - `A\n\n\n\nB`：生成 1 个 `EmptyParagraph` item。
  - `A\n\n\n\n\nB`：非规范，仍生成 1 个 `EmptyParagraph` item。
  - `A\n\n\n\n\n\nB`：生成 2 个 `EmptyParagraph` items。
- Render tests：
  - separator 只产生段间 spacing，不可放光标。
  - empty paragraph 有可见高度、可点击、可显示 caret。
  - paragraph 内 `Shift+Enter` 的普通 `\n` 显示为视觉断行。
- Editing tests：
  - Rendered `Enter` split paragraph 后源码为规范 blank run。
  - 连续 `Enter` 创建空段并序列化为 `2n + 1` blank source rows。
  - 空段 Backspace/Delete 删除一个空段并保持规范结构。
  - Source 模式 Enter 行为不变。
- Regression tests：
  - 现有 paragraph merge、hard break、inline marker hiding、table row、code block、image/math block 行为保持。
  - rendered cache invalidation 仍按受影响 `source_row_range` 局部清理。

## Assumptions

- “两个 `\n`/两个空白行不规范，就当一个处理”按物理 blank source row 解释：偶数 blank run 渲染时向下取前一个奇数，但不在纯渲染阶段改源码。
- `Shift+Enter` 使用普通 `\n`，rendered 模式把它作为段内视觉断行；不使用反斜杠或两个空格 hard break。
- 本轮重构只改变 rendered 编辑/显示主路径，不改变 Source 模式的 Markdown 源码体验。
