# Comrak Inline 未来方向

## 当前迁移原则

当前 comrak inline 工作是兼容优先。

短期规则：

- tree-sitter inline semantics 仍然是兼容性基准。
- comrak inline 输出必须先匹配当前 `MarkdownInlineSemantics` 形状，才能安全替换 tree-sitter。
- comrak inline path 不再运行 tree-sitter inline parser，也不再 per-parent fallback。
- 测试和 benchmark 仍可在 parser 外层比较 tree-sitter/comrak syntax data，用来证明切换 parser 不会静默改变 projection 行为。

这不代表 tree-sitter 当前形状就是长期理想合同。现在有几处兼容规则是在保留历史实现细节，目的是让迁移先做到行为不变。

## 后续应重新审视的兼容代码

### Escape Span

当前兼容行为：

- comrak `Escaped` 被归一化成 tree-sitter `backslash_escape` 的 span 形状。
- `marker_ranges` 强制为空。
- `content_ranges` 覆盖整个 escape 源码范围。
- projection replacement 仍然显示去掉反斜杠后的字符。

原因：

- comrak 自然语义是：反斜杠是语法，后面的字符是显示内容。
- tree-sitter 当前 span 合同把整个 escape 源码范围当作 inline span 内容。

未来 comrak-native 方向：

- 明确定义 escape 的显示内容和语法 marker。
- 如果 projection 行为已有用户可见测试覆盖，就不要继续保留 tree-sitter 的 span 形状副产物。

### SoftBreak / HardBreak

当前兼容行为：

- comrak `SoftBreak` 节点被忽略。
- soft break 通过现有源码扫描 `collect_soft_break_spans` 重新补回。
- break span 使用空 `marker_ranges`，`content_ranges` 覆盖源码 break 范围，以匹配当前 tree-sitter semantics。
- comrak `LineBreak` 保留为 `HardBreak`，但 marker ranges 归一化为空。

原因：

- comrak 会按源码顺序暴露换行 AST 节点。
- 当前 tree-sitter semantics 也是后处理补 soft break，这会影响 span 顺序和 marker/content range 口径。

未来 comrak-native 方向：

- 直接使用 comrak 的 break 节点。
- 独立于 tree-sitter span 顺序来定义 soft/hard break 的 projection 行为。
- 等用户可见 projection 测试覆盖后，删除 tree-sitter 风格的后处理补 span。

### Strikethrough

当前兼容行为：

- comrak `Strikethrough` marker ranges 被归一化成单个 `~` 的 marker ranges。
- 对 `~~text~~`，adapter 会额外合成一个嵌套的 `Strikethrough` span，以匹配当前 tree-sitter 输出。

原因：

- tree-sitter 当前会以嵌套/单波浪线形状暴露 `~~text~~`。
- comrak 暴露的是更直接的语义 strikethrough 节点。

未来 comrak-native 方向：

- 删除合成的嵌套 strikethrough span。
- 让 `~~text~~` 只有一个 canonical 的语义 strikethrough span。
- 更新 projection 和 active marker reveal 相关测试，让测试断言行为，而不是 tree-sitter 的内部嵌套形状。

### Angle Autolink

当前兼容行为：

- comrak 对 `<https://example.com>` 这类 angle autolink 解析出的 resolved URL 会被归一化成 `url=None`。
- email autolink 也使用同样兼容规则。

原因：

- 当前 tree-sitter URL extraction 只读取显式 link destination 节点。
- tree-sitter autolink span 当前不携带 resolved URL。
- comrak 的 resolved URL 更有用，但在 parser 迁移阶段保留它会变成行为变更。

未来 comrak-native 方向：

- 保留 comrak 为 autolink 解析出的 resolved URL。
- 明确 email autolink 应暴露 `mailto:` URL 还是原始邮箱地址。
- 更新消费者和测试，让它们依赖明确的 autolink URL 合同。

### Reference Link / Image

当前兼容行为：

- 对 `[full][ref]`、`[ref][]`、`[shortcut]` 和 reference-style image，adapter 会在 comrak AST 之外做局部源码扫描。
- 扫描不读取全文 reference definitions，也不判断 reference 是否真实存在。
- 产出的 Link/Image span 按当前 tree-sitter 口径处理：`url=None`，marker/content ranges 按源码形状恢复。

原因：

- 当前 tree-sitter inline 输出是语法形状合同：看起来像 reference link/image 就暴露对应 span。
- comrak 的自然语义需要 reference definition 才会把 reference link 解析成真实 Link。
- 在迁移阶段，为了保持 projection/marker hiding 行为不变，先让 comrak path 兼容 tree-sitter 的 syntactic 形状。

未来 comrak-native 方向：

- 删除这层 reference link/image 局部扫描。
- 让 comrak 的 reference resolution 成为 canonical：不存在 definition 的 reference 不应强行暴露成 Link/Image。
- 对 resolved reference link/image 明确定义是否保留 comrak URL，以及 URL/title 如何进入 `MarkdownInlineSemantics`。
- 更新测试，让它们断言 comrak-native reference 语义，而不是 tree-sitter 的 syntactic approximation。

### Entity Span

当前兼容行为：

- entity 从源码中扫描并加入 comrak 输出。
- projection replacement 基于扫描出的 entity span 生成。

原因：

- comrak 不以当前 tree-sitter inline span 合同的形状暴露普通 HTML entity。

未来 comrak-native 方向：

- 只有在 UI 需要源码范围来支持 active marker reveal/projection 时才保留源码扫描。
- 否则，把 entity replacement 行为定义在 projection 层，而不是作为 parser 兼容细节。

### 源码 Marker Ranges

当前兼容行为：

- emphasis、strong、strikethrough、code、math、link、image、escape 等 marker ranges 通过扫描 comrak source range 附近的源码恢复。

原因：

- comrak AST 给的是语义节点和 source position，但不提供 Zed projection 需要的所有 marker range。
- 当前 UI 需要 marker ranges 来隐藏/显示 Markdown 语法。

未来 comrak-native 方向：

- 只为了 UI/projection 需求保留 marker 扫描。
- 一旦 comrak 成为 canonical parser，就停止把扫描结果塑造成 tree-sitter 的历史怪癖。

## Block / Inline 边界

当前架构：

- block parser 负责生成 inline parent 列表。
- inline backend 只消费 inline parents，不再为 pulldown block 和 tree-sitter block 分别写一套 inline 入口。
- `MarkdownInlineParent` 是通用边界：包含 parent id 和 parent source range。
- tree-sitter inline 需要 node 来构造 included ranges / 做增量复用，这只保留在 tree-sitter block 路径内部的 parent descriptor 里。
- Comrak inline 只读取 `MarkdownInlineParent`，不读取 tree-sitter inline tree，不构造 tree-sitter included ranges。

长期方向：

- 新 block parser 只要能产出稳定的 inline parents，就应该能自由搭配现有 inline backend。
- 不再为了某个 block parser 写专门的 inline backend glue。
- 保持 Comrak inline 的 `inline_range_build_ns == 0` 和 `inline_parse_ns == 0`，避免边界收口时把 tree-sitter inline 成本带回来。

## Comparator / 诊断层

当前兼容行为：

- Comrak inline 正常路径直接返回 comrak semantics。
- Comrak inline 不运行 tree-sitter inline parser，不生成 tree-sitter inline trees，也不做 per-parent fallback。
- benchmark 仍比较四组 syntax data 输出，用外层 semantic diff 观察 tree-sitter/comrak 差异。
- stats 仍输出 parent count、fallback count、fallback ratio；当前 Comrak inline 的 fallback count 应保持为 0。

原因：

- 这样可以证明 Comrak inline 已独立于 tree-sitter inline parser。
- 外层 semantic diff 仍保留迁移过程的可观测性，避免静默回归。

未来 comrak-native 方向：

- 移除或重命名 fallback 统计字段，避免把已经删除的回退路径误认为仍存在。
- 如果后续清理仍有用，可以保留 benchmark/test-only comparator 或显式诊断工具。

## 目标终态

长期目标不是“让 comrak 假装成 tree-sitter”。

长期目标是：

- comrak inline semantics 成为 canonical。
- 正常解析不再依赖 tree-sitter inline parser。
- 上面列出的兼容 shim 被删除，或替换成明确的 comrak-native semantics。
- 测试断言用户可见的编辑器/projection 行为和清晰的语义合同，而不是 tree-sitter 的实现细节。
