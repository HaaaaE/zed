阅读约束：本文为准；除 FUTURE_COMRAK.md 可作辅助参考外，禁止阅读任何其他 .md 文件；cargo fmt 格式化了任何东西都应接受，禁止回滚格式化结果；需要 commit 时直接凭记忆提交，因为只有你在改代码，只能调用一次 pwsh 完成 add+commit，禁止在需要 commit 时查看 diff；commit 完后只允许用一个 pwsh 命令检查是否成功：`git status --short; git log -1 --oneline`。

# Comrak-Only Markdown Parser Refactor

## Summary

把产品 parser 改成一次全文 comrak AST parse + 一次语义收集。不再保留 tree-sitter/pulldown/block-inline backend 矩阵，不追求旧语义兼容；旧语义里 tree-sitter 形状错误的地方直接改成 comrak-native。

核心目标：

- 产品路径只有 comrak：`parse_document(source, options)`。
- block/inline 不再是 parser 分层，只是 AST collector 的内部分类。
- 保留 flat range semantics 和 range index，因为 editor 需要快查。
- marker/source 扫描只服务 projection/reveal，不再伪装成 parser 语义。
- 会妨碍重构的旧 comparator/tests 直接删除或重写。

## Key Changes

- 在 `markdown_wysiwyg` 建立新核心类型：
  - `MarkdownDocument` 取代 `MarkdownSyntaxTree`。
  - `MarkdownDocumentData` 取代 `MarkdownSyntaxData`。
  - 保留 `MarkdownBlock`、`MarkdownTable`、`MarkdownInlineSpan`、`MarkdownProjectionMap` 这类 editor-facing 数据，但重新定义语义。
- 删除产品公开 API：
  - `MarkdownBackendSelection`
  - `MarkdownBlockBackendKind`
  - `MarkdownInlineBackendKind`
  - `parse_with_backends`
  - `parse_tree`
  - `block_tree`
  - `inline_trees`
  - `MarkdownParseTree`
  - `MarkdownInlineTree`
  - `MarkdownInlineParent`
- `md_buffer` 改成缓存 `Arc<MarkdownDocument>`，`parse_markdown_source` 只调用 `MarkdownDocument::parse(source)`。
- `md_editor` / `md_projection` 继续通过 range query 消费语义，调用名可从 `syntax_tree()` 改成 `markdown_document()`；不引入 parser/backend 概念。
- 从产品 `Cargo.toml` 移除 `pulldown-cmark`、`tree-sitter`、`tree-sitter-md`。旧 benchmark comparator 要么删掉，要么改成 comrak-only timing 工具，不再引用 backend matrix。

## Implementation

新 parser flow：

```text
source
  -> line_starts(source)
  -> comrak::parse_document(arena, source, options)
  -> ComrakDocumentCollector
  -> MarkdownDocumentData
  -> range/projection queries
```

comrak options 固定为产品语义：

- `extension.table = true`
- `extension.strikethrough = true`
- `extension.autolink = true`
- `extension.tasklist = true`
- `extension.math_dollars = true`
- `extension.tagfilter = true`
- `parse.escaped_char_spans = true`
- `parse.sourcepos_chars = false`

AST collector 一次遍历完成：

- block nodes: `Paragraph`, `Heading`, `BlockQuote`, `List`, `Item`, `TaskItem`, `CodeBlock`, `HtmlBlock`, `ThematicBreak`, `Table`, `TableRow`, `TableCell`
- inline nodes: `Emph`, `Strong`, `Strikethrough`, `Code`, `Link`, `Image`, `HtmlInline`, `Escaped`, `SoftBreak`, `LineBreak`, `Math`

具体实现约束：

- `Sourcepos -> byte Range` 使用全局 `line_starts` 转换；所有 public ranges 继续是 byte offsets。
- block ids 改成 deterministic synthetic id：由 block kind、source range、preorder ordinal 生成，不依赖 parser node id。
- 保留轻量 source scanner，但只允许用于 comrak AST 不暴露的 editor metadata：
  - blank line blocks
  - reference definition source blocks，用于 rendered mode 隐藏 definitions
  - block/inline marker ranges
  - table pipes/delimiter marker ranges
  - HTML tagfilter disallowed check
  - entity projection replacements
- 删除 `MarkdownStructure -> MarkdownSyntaxData` 中间层。最终结构直接由 comrak collector builder 产出。
- 删除 incremental tree-sitter path。`reparse_after_edit_range` 暂时全量 parse；这是当前默认路径已有行为，避免假增量层继续污染设计。

## New Semantics

- `MarkdownInlineSpan.source_range` 是 comrak semantic node 的源码范围。
- `marker_ranges` 只表示 UI 要隐藏/reveal 的 Markdown syntax delimiters。
- `content_ranges` 只表示实际内容源码范围，不再模仿 tree-sitter 子节点。
- `Link` / `Image` 的 `url` 永远使用 comrak resolved URL，包括 autolink 和 reference link。
- 未被 comrak resolve 的 reference-like text 不产生 `Link` / `Image` span。
- `Strikethrough` 只有一个 span；删除 synthetic nested strike。
- `SoftBreak` / `HardBreak` 直接来自 comrak node，不再后扫补 span。
- `Escape` 的 marker 是反斜杠，content 是 escaped char；projection replacement 显示 escaped char。
- `Entity` 不再需要作为 parser inline span；只生成 projection replacement，用于 inactive rendered text 显示 decoded entity。
- `LinkReferenceDefinition` 作为 source-only block metadata 保留，用于 rendered mode 隐藏定义行；它不是 comrak AST semantic node。
- block/table/inline query indexes 继续保留，保证 `blocks_in_source_range`、`inline_spans_in_source_range`、`projection_for_source_range`、`range_semantics_for_source_range` 不退化成全量扫描。

## Test Plan

删除测试：

- tree-sitter/pulldown backend matching tests
- inline tree parser state tests
- fallback/fallback ratio tests
- `parse_with_backends` matrix tests
- asserting tree-sitter-compatible reference/escape/break/strike/autolink shape 的测试

新增/重写 comrak-native tests：

- default parse has no backend state and no per-parent inline parse
- resolved reference link exposes URL
- unresolved shortcut/reference text does not become link
- angle autolink exposes comrak URL
- email autolink exposes exactly comrak URL
- `~~text~~` produces one strikethrough span
- escaped char marker/content ranges differ from old tree-sitter shape
- soft/hard breaks come from comrak and project correctly
- entity replacement works without entity parser span
- table rows/cells/alignments come from comrak table AST
- task item checked state and marker projection work
- reference definition lines are hidden in rendered mode

Keep editor behavior tests that assert user-visible projection/rendering, but update expected output where old parser semantics were wrong.

Verification commands:

- `cargo check -p updraft_editor`
- `cargo test -p markdown_wysiwyg`
- `cargo test -p md_buffer`
- `cargo test -p md_projection`
- `cargo test -p md_editor`

## Performance Defaults

- One comrak parse per document refresh.
- No per-inline-parent `parse_document`.
- No tree-sitter included ranges.
- No backend comparator in product path.
- Preserve end-to-end perf comparability for the same user scenarios: cold document parse/open, edit-triggered refresh, rendered/source row build, projection/query work.
- Old parser-internal metrics may be deleted or renamed; do not preserve `tree_sitter_*`, `pulldown_*`, inline range build, inline parse, or fallback stats for compatibility.
- If the benchmark harness is rewritten, keep a production workload mode that can run the same input documents and commands on old/new commits and report total elapsed time for those user scenarios.
- Source scans must be linear or local-to-node-range only.
- Preserve prefix maximum indexes for range overlap queries.
- Add/keep parser stats that can prove:
  - `document_parse_count == 1`
  - `inline_parse_count == 0`
  - no backend fallback fields remain

## Assumptions

- Breaking public APIs inside this workspace is allowed.
- Old tree-sitter-compatible semantics are not compatibility targets.
- Full parse on edit is acceptable for this refactor; real incremental comrak can be a later separate project.
- The editor-facing flat range data model is worth keeping for speed; the parser/backend layering is not.
