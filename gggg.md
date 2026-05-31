
  # `md_editor` 结构收敛与后续拆 crate 计划

  ## Summary

  目标是先把 `md_editor` 内部职责拆清楚，再决定是否新增 crate。第一阶段不新增 workspace crate，不改变 `MarkdownEditor` 对外 API，不改变运行行为；只把当前集中在 `MarkdownEditor` 和 `lib.rs` 的缓
  存、布局状态、选择状态、预热状态整理成明确的内部边界。完成后再按标准评估是否把稳定的纯逻辑模块拆成独立 crate。

  成功标准：

  - `cargo check -p updraft_editor` 通过。
  - `cargo test -p md_sum_tree`、`cargo test -p md_rope`、`cargo test -p md_text`、`cargo test -p markdown_wysiwyg`、`cargo test -p md_editor` 全部通过。
  - `MarkdownEditor` 的外部 API 保持兼容：`new`、`for_text`、`for_text_with_document_path`、`set_mode`、`toggle_mode`、`serialized_text`、`selection`、actions、events 不变。
  - 不引入 `.rules` 禁止的 Zed IDE crate，不改变 GPL/Apache license metadata。
  - 不新增 crate，除非后续阶段满足本文末尾的 crate split 条件。

  ## Key Changes

  - 将 `lib.rs` 中的内部类型移动到归属模块：
    - `RowDisplayStyle` 移到 `layout.rs`。
    - `SourcePrewarmState`、`RenderedPrewarmState` 移到 `cache.rs`。
    - `EditLayoutInvalidation`、`LocalSourceEditInvalidation` 移到 `editor_actions.rs` 或新建 `invalidation.rs`，推荐新建 `invalidation.rs`，由 `editor_actions.rs` 和 `cache.rs` 使用。
    - `TransactionSelectionState` 移到 `selection.rs`。
    - `lib.rs` 只保留 `MarkdownEditor`、`MarkdownEditorMode`、`MarkdownEditorEvent`、actions、构造与少量顶层 glue。

  - 在 `cache.rs` 新增内部状态容器 `DisplayCacheStore`：
    - 包含当前 `display_row_cache`、`row_layout_input_cache`、`row_layout_cache`、`table_layout_cache`、`rendered_display_index`。
    - 提供 `clear_all_display_rows`、`clear_all_layouts`、`clear_rows`、`clear_layout_rows`、`rekey_source_rows_for_local_edit`、`rendered_index` 等方法。
    - `MarkdownEditor` 中对应字段替换为单个 `display_cache: DisplayCacheStore`。
    - 现有 `MarkdownEditor::clear_*` 方法保留为薄 wrapper，避免大面积调用点重写。

  - 在 `cache.rs` 或 `inline_atom.rs` 新增 `InlineAtomMeasurementStore`：
    - 包含当前 `inline_atom_measurement_cache`、`pending_inline_atom_rows`、`pending_inline_atom_remeasure_rows`、`inline_atom_remeasure_scheduled`。
    - `MarkdownEditor` 中对应字段替换为 `inline_atoms: InlineAtomMeasurementStore`。
    - 原有 inline atom 测量、更新、延迟 remeasure 逻辑保持在现有调用路径中，只把字段访问改为 store 方法。

  - 将显示列表与 mode 映射逻辑收拢为内部 helper：
    - 新建 `display_space.rs`。
    - 移入或包装 `display_item_count_for_mode`、`display_item_index_for_cursor`、`rendered_display_index`、`reveal_cursor_row` 的纯映射部分。
    - `MarkdownEditor` 仍负责调用 `MdListState` 和 `Context`，helper 不持有 GPUI entity 生命周期。

  - 暂不拆 crate：
    - `md_editor` 继续作为唯一 GPUI 编辑器 crate。
    - `markdown_wysiwyg`、`md_buffer`、`md_text` 等现有 crate 边界不动。
    - 只有当某个内部模块满足“无 `MarkdownEditor` 依赖、无 GPUI Window/Context 依赖、输入输出稳定、独立测试覆盖完整”时，才进入下一阶段拆 crate。

  ## Public/Internal API

  - 外部 public API 不新增、不删除、不改签名。
  - 新增类型全部为 `pub(crate)` 或更窄可见性：
    - `DisplayCacheStore`
    - `InlineAtomMeasurementStore`
    - `EditLayoutInvalidation`
    - `LocalSourceEditInvalidation`
    - `TransactionSelectionState`
  - 测试中当前直接访问缓存字段的断言改为测试 helper：
    - `editor.display_cache_stats_for_tests()`
    - `editor.inline_atom_stats_for_tests()`
    - 返回只读计数与必要 key 查询能力，不暴露可变 HashMap。
  - 原有行为测试继续使用现有用户行为入口；不把测试改成依赖新内部结构。

  ## Implementation Order

  1. 先做纯移动：
     - 移动内部类型到归属模块。
     - 用 `pub(crate) use` 临时维持现有引用，保证每一步都能编译。
     - 跑 `cargo check -p updraft_editor` 和 `cargo test -p md_editor`。

  2. 引入 `DisplayCacheStore`：
     - 先只迁移字段和 `clear/rekey/rendered_index`。
     - 保留 `MarkdownEditor::clear_display_row_cache`、`clear_row_layout_cache` 等 wrapper。
     - 更新测试中的缓存字段访问为 stats helper。
     - 跑 `cargo test -p md_editor`。

  3. 引入 `InlineAtomMeasurementStore`：
     - 迁移 inline atom 测量字段。
     - 保持现有 remeasure 调度行为不变。
     - 更新 inline atom cache 相关测试为 stats helper。
     - 跑 `cargo test -p md_editor`。

  4. 收拢 display-space 映射：
     - 抽出 mode 到 item index、cursor 到 item、rendered index cache 获取的纯逻辑。
     - 不改 `editor_render.rs`、`editor_mouse.rs` 的行为分支，只减少它们对 `MarkdownEditor` 内部字段的直接依赖。
     - 跑完整测试矩阵。

  5. 最后清理：
     - 移除不再需要的临时 re-export。
     - 删除无用 imports。
     - 不运行会改写文件的 formatter，除非切出执行阶段后明确实施；执行阶段可用 `cargo fmt --check` 或正式 `cargo fmt`。

  ## Test Plan

  - 每个阶段至少运行：
    - `cargo check -p updraft_editor`
    - `cargo test -p md_editor`

  - 最终运行：
    - `cargo test -p md_sum_tree`
    - `cargo test -p md_rope`
    - `cargo test -p md_text`
    - `cargo test -p markdown_wysiwyg`
    - `cargo test -p md_editor`

  - 重点回归场景：
    - source/rendered 模式切换后选择、滚动、缓存失效保持一致。
    - rendered 模式下 heading/list/blockquote/task marker 的 active range 显隐不变。
    - 表格、图片、公式、inline atom 的 layout cache 与 remeasure 行为不变。
    - undo/redo 后 selection history 与 cache rekey 行为不变。
    - 大文件 prewarm 的 anchor reset 和分帧预算行为不变。

  ## Crate Split Criteria

  下一阶段只有满足以下条件才拆 crate：

  - 候选模块不能依赖 `MarkdownEditor`。
  - 候选模块不能依赖 `gpui::Window`、`gpui::Context`、`MdListState`。
  - 候选模块的核心输入输出能用 `BufferSnapshot`、`MarkdownSyntaxTree`、`Selection<Point>`、plain structs 表达。
  - 候选模块已有独立测试，且测试不需要创建 GPUI window。
  - 拆出后不会让 `md_editor` 和新 crate 形成循环依赖。

  优先候选：

  - `md_projection`：如果 `display_row_builder` 与 rendered projection 能稳定脱离 GPUI。
  - `md_layout_core`：只有在 text measurement 与 GPUI shaping 被隔离后再考虑。
  - 不拆 `editor_render`、`editor_mouse`、GPUI action wiring，它们应留在 `md_editor`。

  ## Assumptions

  - 本计划默认这是一次结构性重构，不改变用户可见功能。
  - 第一轮目标是降低 `md_editor` 的内部耦合，而不是减少最终二进制体积。
  - 暂不新增 crate 是明确选择；拆 crate 作为后续阶段，以满足 criteria 为前提。

  ## Progress Log

  ### 2026-05-31

  状态：第一阶段已完成，未新增 workspace crate，未改变 `MarkdownEditor` 对外 API。

  已完成：

  - `RowDisplayStyle` 已移动到 `layout.rs`。
  - `SourcePrewarmState`、`RenderedPrewarmState` 已移动到 `cache.rs`。
  - `EditLayoutInvalidation`、`LocalSourceEditInvalidation` 已移动到新模块 `invalidation.rs`。
  - `TransactionSelectionState` 已移动到 `selection.rs`。
  - `MarkdownEditor` 的显示缓存字段已收敛为 `display_cache: DisplayCacheStore`。
  - `MarkdownEditor` 的 inline atom 测量字段已收敛为 `inline_atoms: InlineAtomMeasurementStore`。
  - `DisplayCacheStore` 已提供显示行、布局、局部 edit rekey、rendered index 缓存相关方法，`MarkdownEditor::clear_*` 等入口保留为薄 wrapper。
  - `InlineAtomMeasurementStore` 已提供统一清理方法；原有测量、pending、延迟 remeasure 行为保持在原调用路径中。
  - 已新增 `display_space.rs`，收拢 mode/cursor 到 display item 的纯映射逻辑；`MarkdownEditor` 仍负责 `MdListState` 和 GPUI 生命周期。
  - 测试中的缓存断言已改为 test-only stats/helper，不暴露可变 HashMap。
  - 当时暂未拆 crate；随后按本文 `Crate Split Criteria` 继续评估候选模块。

  验证结果：

  - `cargo check -p updraft_editor` 通过。
  - `cargo test -p md_sum_tree` 通过。
  - `cargo test -p md_rope` 通过。
  - `cargo test -p md_text` 通过。
  - `cargo test -p markdown_wysiwyg` 通过。
  - `cargo test -p md_editor` 通过。

  后续可选工作：

  - 继续观察 `display_row_builder` 与 rendered projection 是否能稳定满足拆出 `md_projection` 的 criteria。
  - 只有在 text measurement 与 GPUI shaping 隔离后，再评估 `md_layout_core`。

  ### 2026-05-31 Follow-up

  状态：已按 `Crate Split Criteria` 继续推进，不再停留在第一阶段。

  拆分结论：

  - `RenderedDisplayIndex` 已满足拆分条件：不依赖 `MarkdownEditor`，不依赖 `gpui::Window`、`gpui::Context`、`MdListState`，核心输入为 `BufferSnapshot`，输出为 plain index/item structs。
  - 已新增 GPL crate `md_projection`，先承载 rendered display index 逻辑。
  - `md_editor::rendered_index` 保留为内部 re-export，减少本轮调用点扰动并避免循环依赖。
  - 已在 `md_projection` 添加独立单元测试，覆盖 paragraph grouping、structured rows、blank row role/empty paragraph 映射。

  仍不拆的部分：

  - `display_row_builder` 仍依赖 `DisplayRow`、inline atom/rendered element 描述、document path 等 `md_editor` 内部渲染模型，本轮不满足稳定 crate 边界。
  - `rendered_topology` 仍与 `DisplayRowProjectionState` 和 active selection projection 绑定，先留在 `md_editor`。
  - `md_layout_core` 仍受 GPUI shaping/text measurement 影响，尚不满足拆分条件。

  补充验证结果：

  - `cargo test -p md_projection` 通过。
  - `cargo check -p updraft_editor` 通过。
  - 原完整测试矩阵继续通过。
