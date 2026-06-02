# Comrak Inline Backend 可替换架构计划

  ## Summary

  - 目标：把 inline 解析改成可替换后端，同时避免新结构偏向 tree-sitter。
  - 核心设计：语义输出和 parser cache 分离。
  - 生产默认先不切 comrak；先在 test/perf_enabled 下验证 comrak inline 的语义等价和性能。
  - 依赖前置：保留 entities = 1.0.1，因为它已验证能让 markdown_wysiwyg + comrak 0.52.0 --no-default-features 共存编译。

  ## Key Architecture

  - 新增纯语义结构：

    struct MarkdownInlineSemantics {
        parent_id: usize,
        parent_range: Range<usize>,
        spans: Vec<MarkdownInlineSpan>,
        replacements: Vec<MarkdownProjectionReplacement>,
    }

  - 新增 inline parse 输出：

    struct InlineParseOutput {
        semantics: Vec<MarkdownInlineSemantics>,
        cache: MarkdownInlineCache,
    }

    enum MarkdownInlineCache {
        TreeSitter {
            inline_trees: Vec<MarkdownInlineTree>,
            inline_tree_by_parent_id: HashMap<usize, usize>,
        },
        None,
    }

  - MarkdownStructure 只持有 Vec<MarkdownInlineSemantics>，不再持有 tree-sitter inline tree。
  - MarkdownParseTree 继续保留 inline_trees / inline_tree_by_parent_id，只作为 tree-sitter inline backend 的增量 cache。
  - MarkdownSemanticsAssembler 只消费 MarkdownInlineSemantics，不再遍历任何 parser AST。

  ## Implementation Changes

  - 第一步只做无行为变化重构：
      - tree-sitter inline parser 仍然按现在逻辑 parse / reuse / incremental。
      - parse 完 MarkdownInlineTree 后，立刻在 inline backend 内转换为 MarkdownInlineSemantics。
      - 当前 collect_inline_spans_for_inline_tree 和 collect_projection_replacements_for_inline_tree 移到 tree-sitter inline backend 侧复用。
      - assembler 改为直接合并 semantics.spans 和 semantics.replacements。
      - 所有现有测试应保持通过。

  - 第二步抽出 backend 选择：

    enum InlineBackendKind {
        TreeSitter,
        #[cfg(any(test, perf_enabled))]
        Comrak,
    }
      - 生产路径默认 TreeSitter。
      - pulldown benchmark/test 路径允许选择 Comrak。

  - 第三步实现 ComrakInlineBackend：
      - 依赖：comrak = { version = "0.52.0", default-features = false }。
      - 配置：
          - extension.strikethrough = true
          - extension.autolink = true
          - extension.math_dollars = true
          - extension.tagfilter = true
          - parse.escaped_char_spans = true
          - parse.sourcepos_chars = false

      - 使用 comrak AST sourcepos 映射到全局 byte range；comrak 默认 sourcepos column 是 1-based byte column。
      - AST 节点转 MarkdownInlineSpan / MarkdownProjectionReplacement。
      - marker ranges 不依赖 comrak 提供，统一从原始 source 的 span range 内扫描恢复。

  - 第四步增加 fallback：
      - 如果 comrak 对某个 parent range 无法生成完整语义，回退 tree-sitter inline backend。
      - stats 记录 fallback count / ratio，不能静默掩盖问题。

  ## Semantics Contract

  - 所有 inline backend 必须产出完全相同口径：
      - source_range
      - content_ranges
      - marker_ranges
      - url
      - tagfilter_disallowed
      - escape/entity projection replacement
      - soft/hard break projection behavior
      - image/math rendered-element candidate

  - 支持的 comrak 映射：
      - Emph -> MarkdownInlineKind::Emphasis
      - Strong -> Strong
      - Strikethrough -> Strikethrough
      - Code -> InlineCode
      - Link -> Link
      - Image -> Image
      - HtmlInline -> InlineHtml
      - Escaped -> Escape
      - Math { dollar_math: true } -> InlineMath
      - SoftBreak / LineBreak -> SoftBreak / HardBreak

  - Entity 需要额外源码扫描补齐，因为 comrak AST 不把普通 HTML entity 直接暴露成我们当前的 Entity span 口径。

  ## Benchmark And Tests

  - benchmark 输出四组：
      - tree-sitter block + tree-sitter inline
      - tree-sitter block + comrak inline
      - pulldown block + tree-sitter inline
      - pulldown block + comrak inline

  - 新增 stats：
      - inline backend kind
      - inline parse mean
      - AST/sourcepos mapping mean
      - marker scan mean
      - fallback count / ratio

  - 语义测试覆盖：
      - emphasis / strong / strikethrough
      - code span
      - inline link / reference link / autolink / email autolink
      - image
      - escape / entity
      - inline HTML tagfilter
      - soft/hard break, CRLF
      - CJK/UTF-8 byte range
      - inline math
      - table cell inline

  - 必跑验证：
      - cargo check -p markdown_wysiwyg --locked
      - cargo test -p markdown_wysiwyg --locked
      - cargo check -p updraft_editor --locked
      - benchmark crate 普通 check
      - benchmark crate RUSTFLAGS='--cfg perf_enabled' check
      - release benchmark smoke，要求真实 semantic diff 为 mismatch_fields=0

  ## Assumptions

  - “comark” 指 comrak。
  - 本计划只做 inline backend，不做 comrak block backend。
  - 第一阶段不切生产默认 backend。
  - 最终目标是 inline backend 可插拔；tree-sitter inline tree 只作为 cache，不再进入 assembler。

  ## Status

  - 2026-06-02:
      - 已完成第一步无行为变化重构：
          - 新增 MarkdownInlineSemantics，保存 parent_id、parent_range、inline spans 和 projection replacements。
          - MarkdownStructure 改为只持有 Vec<MarkdownInlineSemantics>，不再持有 tree-sitter inline tree。
          - MarkdownParseTree 继续保留 inline_trees / inline_tree_by_parent_id 作为 tree-sitter inline 增量 cache。
          - tree-sitter inline parser 仍按原逻辑 parse / reuse / incremental。
          - tree-sitter inline parse 后立即转换为 MarkdownInlineSemantics。
          - MarkdownSemanticsAssembler 改为合并已有 semantics.spans / semantics.replacements，不再遍历 parser AST。
          - pulldown benchmark/test 路径也先通过 tree-sitter inline tree 转换为相同语义结构。
      - 已保留 entities = 1.0.1 依赖前置。
      - 已验证：
          - cargo check -p markdown_wysiwyg --locked
          - cargo test -p markdown_wysiwyg --locked
  - 2026-06-02:
      - 已完成第二步 backend 选择层：
          - 新增 InlineBackendKind，生产默认仍为 TreeSitter。
          - 新增 InlineParseOutput 和 MarkdownInlineCache，把 inline semantics 与 parser cache 分离。
          - parse_markdown 现在返回 MarkdownParseTree cache 和 Vec<MarkdownInlineSemantics>。
          - TreeSitter inline backend 继续保留 inline_trees / inline_tree_by_parent_id cache。
          - pulldown benchmark/test 路径新增 inline backend 选择入口。
          - cfg(test/perf_enabled) 下 Comrak 分支已接入选择点；当前阶段仍复用 tree-sitter inline semantics，留待第三步实现真实 ComrakInlineBackend。
          - 新增测试覆盖 pulldown + Comrak inline backend selection 入口。
      - 已验证：
          - cargo check -p markdown_wysiwyg --locked
          - cargo test -p markdown_wysiwyg --locked
  - 2026-06-02:
      - 已推进第三步 ComrakInlineBackend 初版：
          - 新增 comrak = 0.52.0，保持 default-features = false。
          - cfg(test/perf_enabled) 下 Comrak inline backend selection 不再复用 tree-sitter semantics。
          - 新增 comrak AST/sourcepos 到 MarkdownInlineSemantics 的转换入口。
          - comrak options 已设置 strikethrough / autolink / math_dollars / tagfilter / escaped_char_spans / sourcepos_chars=false。
          - 已映射核心 inline 节点：Emph、Strong、Strikethrough、Code、Link、Image、HtmlInline、Escaped、Math(dollar)、SoftBreak、LineBreak。
          - 已通过源码扫描补齐 entity spans 和 escape/entity projection replacements。
          - 已用源码扫描恢复常见 emphasis / strong / strikethrough / code / math / inline link / image / escape marker ranges。
          - 当前仍未完成完整语义合同和 fallback；reference link、复杂 marker 口径、完整 semantic diff、stats/fallback 仍在后续步骤。
      - 已验证：
          - cargo check -p markdown_wysiwyg --locked
          - cargo test -p markdown_wysiwyg --locked
