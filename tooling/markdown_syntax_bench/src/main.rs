use std::{
    env,
    hint::black_box,
    ops::Range,
    time::{Duration, Instant},
};

use markdown_wysiwyg::{
    MarkdownBlock, MarkdownInlineSpan, MarkdownProjectionReplacement, MarkdownSyntaxData,
    MarkdownTable, ProjectionMarkerDependency,
};
#[cfg(not(perf_enabled))]
use pulldown_cmark::{Event, Tag};
use pulldown_cmark::{Options, Parser};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct BenchmarkSyntaxData {
    source_len: usize,
    line_starts: Vec<usize>,
    blocks: Vec<BenchmarkBlock>,
    tables: Vec<BenchmarkTable>,
    inline_spans: Vec<BenchmarkInlineSpan>,
    projection_replacements: Vec<BenchmarkProjectionReplacement>,
    projection_marker_dependencies: Vec<BenchmarkProjectionMarkerDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkBlock {
    kind: String,
    source_range: Range<usize>,
    content_range: Range<usize>,
    marker_ranges: Vec<Range<usize>>,
    row_range: Range<usize>,
    tagfilter_disallowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkTable {
    source_range: Range<usize>,
    row_range: Range<usize>,
    pipe_marker_ranges: Vec<Range<usize>>,
    delimiter_marker_ranges: Vec<Range<usize>>,
    alignments: Vec<String>,
    header_cells: Vec<BenchmarkTableCell>,
    delimiter_cells: Vec<BenchmarkTableCell>,
    body_rows: Vec<Vec<BenchmarkTableCell>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkTableCell {
    source_range: Range<usize>,
    content_range: Range<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkInlineSpan {
    kind: String,
    source_range: Range<usize>,
    content_ranges: Vec<Range<usize>>,
    marker_ranges: Vec<Range<usize>>,
    url: Option<String>,
    tagfilter_disallowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkProjectionReplacement {
    source_range: Range<usize>,
    owner_source_range: Range<usize>,
    display_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BenchmarkProjectionMarkerDependency {
    marker_range: Range<usize>,
    owner_source_range: Range<usize>,
}

#[derive(Debug)]
struct PulldownAssembly {
    data: BenchmarkSyntaxData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SequenceDiff {
    left_len: usize,
    right_len: usize,
    common_prefix_len: usize,
    first_mismatch_index: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SyntaxDiffSummary {
    source_len_matches: bool,
    line_starts: SequenceDiff,
    blocks: SequenceDiff,
    tables: SequenceDiff,
    inline_spans: SequenceDiff,
    projection_replacements: SequenceDiff,
    projection_marker_dependencies: SequenceDiff,
}

#[derive(Default)]
#[cfg(not(perf_enabled))]
struct PulldownBuilder {
    source_len: usize,
    line_starts: Vec<usize>,
    blocks: Vec<BenchmarkBlock>,
    tables: Vec<BenchmarkTable>,
    inline_spans: Vec<BenchmarkInlineSpan>,
    projection_replacements: Vec<Range<usize>>,
    projection_marker_dependencies: Vec<(Range<usize>, Range<usize>)>,
}

#[cfg(not(perf_enabled))]
impl PulldownBuilder {
    fn finish(self) -> PulldownAssembly {
        let projection_replacements = self
            .projection_replacements
            .into_iter()
            .map(|source_range| BenchmarkProjectionReplacement {
                owner_source_range: source_range.clone(),
                source_range,
                display_text: String::new(),
            })
            .collect();
        let projection_marker_dependencies = self
            .projection_marker_dependencies
            .into_iter()
            .map(
                |(marker_range, owner_source_range)| BenchmarkProjectionMarkerDependency {
                    marker_range,
                    owner_source_range,
                },
            )
            .collect();
        PulldownAssembly {
            data: BenchmarkSyntaxData {
                source_len: self.source_len,
                line_starts: self.line_starts,
                blocks: self.blocks,
                tables: self.tables,
                inline_spans: self.inline_spans,
                projection_replacements,
                projection_marker_dependencies,
            },
        }
    }
}

impl BenchmarkSyntaxData {
    fn from_production(data: &MarkdownSyntaxData) -> Self {
        Self {
            source_len: data.source_len(),
            line_starts: data.line_starts().to_vec(),
            blocks: data
                .blocks()
                .iter()
                .map(BenchmarkBlock::from_production)
                .collect(),
            tables: data
                .tables()
                .iter()
                .map(BenchmarkTable::from_production)
                .collect(),
            inline_spans: data
                .inline_spans()
                .iter()
                .map(BenchmarkInlineSpan::from_production)
                .collect(),
            projection_replacements: data
                .projection_replacements()
                .iter()
                .map(BenchmarkProjectionReplacement::from_production)
                .collect(),
            projection_marker_dependencies: data
                .projection_marker_dependencies()
                .iter()
                .map(BenchmarkProjectionMarkerDependency::from_production)
                .collect(),
        }
    }
}

impl BenchmarkBlock {
    fn from_production(block: &MarkdownBlock) -> Self {
        Self {
            kind: format!("{:?}", block.kind),
            source_range: block.source_range.clone(),
            content_range: block.content_range.clone(),
            marker_ranges: block.marker_ranges.clone(),
            row_range: block.row_range.clone(),
            tagfilter_disallowed: block.tagfilter_disallowed,
        }
    }
}

impl BenchmarkTable {
    fn from_production(table: &MarkdownTable) -> Self {
        Self {
            source_range: table.source_range.clone(),
            row_range: table.row_range.clone(),
            pipe_marker_ranges: table.pipe_marker_ranges.clone(),
            delimiter_marker_ranges: table.delimiter_marker_ranges.clone(),
            alignments: table
                .alignments
                .iter()
                .map(|alignment| format!("{alignment:?}"))
                .collect(),
            header_cells: table
                .header
                .cells
                .iter()
                .map(BenchmarkTableCell::from_production)
                .collect(),
            delimiter_cells: table
                .delimiter
                .cells
                .iter()
                .map(BenchmarkTableCell::from_production)
                .collect(),
            body_rows: table
                .body
                .iter()
                .map(|row| {
                    row.cells
                        .iter()
                        .map(BenchmarkTableCell::from_production)
                        .collect()
                })
                .collect(),
        }
    }
}

impl BenchmarkTableCell {
    fn from_production(cell: &markdown_wysiwyg::MarkdownTableCell) -> Self {
        Self {
            source_range: cell.source_range.clone(),
            content_range: cell.content_range.clone(),
        }
    }
}

impl BenchmarkInlineSpan {
    fn from_production(span: &MarkdownInlineSpan) -> Self {
        Self {
            kind: format!("{:?}", span.kind),
            source_range: span.source_range.clone(),
            content_ranges: span.content_ranges.clone(),
            marker_ranges: span.marker_ranges.clone(),
            url: span.url.clone(),
            tagfilter_disallowed: span.tagfilter_disallowed,
        }
    }
}

impl BenchmarkProjectionReplacement {
    fn from_production(replacement: &MarkdownProjectionReplacement) -> Self {
        Self {
            source_range: replacement.source_range.clone(),
            owner_source_range: replacement.owner_source_range.clone(),
            display_text: replacement.display_text.clone(),
        }
    }
}

impl BenchmarkProjectionMarkerDependency {
    fn from_production(dependency: &ProjectionMarkerDependency) -> Self {
        Self {
            marker_range: dependency.marker_range.clone(),
            owner_source_range: dependency.owner_source_range.clone(),
        }
    }
}

fn mixed_fixture(target_bytes: usize) -> String {
    let chunk = concat!(
        r#"
# Source Row Heading

Paragraph source-row with **strong text**, _emphasis_, `inline code`, [a link](https://example.com), and an escaped \* marker.
Extended inline source-row with ~~strike~~, <IFRAME src="x"></IFRAME>, autolink <https://example.com/source-row>, mail <source-row@example.com>, CJK 中文, $x + y$, and ![source-row image](image.png).
Hard break source-row"#,
        "  \n",
        r#"continued source-row

> Blockquote source-row with **inline** content.
> - nested item source-row
"#,
        "> \n",
        r#"
> 1. quoted ordered source-row
>    - [ ] quoted nested task source-row
"#,
        "> \n",
        r#"
- parent item source-row
  paragraph continuation with **inline** content
  > nested quote source-row
  > continuation

> ```rust
> let source_row = true;
> ```

- [x] completed task source-row
- [ ] open task with [link](https://example.com/path?q=1)
- plain list item source-row

| column | value | notes |
| --- | ---: | --- |
| source-row | 123 | **bold cell** and `code` |
| another | 456 | [cell link](https://example.com) |

edge left | edge center | edge right
--- | :---: | ---:
edge 1 | **edge 2** | edge 3

| empty a |  | empty c |
| - | - | - |
|  | **empty b** |  |

| broken a | broken b |
| not a delimiter |

```rust
fn source_row() {
    println!("source-row");
}
```

<div class="source-row">raw html</div>

[source-row-ref]: https://example.com/ref

"#,
    );
    let mut text = String::with_capacity(target_bytes + chunk.len());
    while text.len() < target_bytes {
        text.push_str(chunk);
    }
    text
}

#[cfg(not(perf_enabled))]
fn line_starts(source: &str) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        source
            .bytes()
            .enumerate()
            .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1))
            .filter(|start| *start < source.len()),
    );
    starts
}

fn measure(name: &str, iterations: usize, mut run: impl FnMut() -> usize) {
    let warmup = black_box(run());
    let mut samples = Vec::with_capacity(iterations);
    let mut checksum = warmup;
    for _ in 0..iterations {
        let start = Instant::now();
        checksum = checksum.wrapping_add(black_box(run()));
        samples.push(start.elapsed());
    }
    samples.sort();
    let total = samples.iter().copied().sum::<Duration>();
    let mean = total.as_secs_f64() * 1000.0 / samples.len() as f64;
    let median = samples[samples.len() / 2].as_secs_f64() * 1000.0;
    let min = samples[0].as_secs_f64() * 1000.0;
    let max = samples[samples.len() - 1].as_secs_f64() * 1000.0;
    println!(
        "{name}: mean={mean:.3}ms median={median:.3}ms min={min:.3}ms max={max:.3}ms checksum={checksum}"
    );
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn pulldown_parser_checksum(source: &str) -> usize {
    Parser::new_ext(source, Options::all())
        .into_offset_iter()
        .fold(0usize, |checksum, (event, range)| {
            black_box(event);
            checksum
                .wrapping_add(range.start)
                .wrapping_mul(31)
                .wrapping_add(range.end)
        })
}

#[cfg(not(perf_enabled))]
fn collect_blocks_from_pulldown(source: &str, _events: &[PulldownEvent]) -> Vec<BenchmarkBlock> {
    let mut blocks = Vec::new();
    let line_start_offsets = line_starts(source);
    for (row, line) in source.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            let source_range = line_range(&line_start_offsets, source.len(), row);
            blocks.push(BenchmarkBlock {
                kind: "Blank".to_string(),
                content_range: source_range.start..source_range.start,
                source_range,
                marker_ranges: Vec::new(),
                row_range: row..row + 1,
                tagfilter_disallowed: false,
            });
        }
    }
    blocks
}

#[cfg(not(perf_enabled))]
fn line_range(starts: &[usize], source_len: usize, row: usize) -> Range<usize> {
    let start = starts[row];
    let end = starts.get(row + 1).copied().unwrap_or(source_len);
    start..end
}

#[derive(Clone)]
#[cfg(not(perf_enabled))]
struct PulldownEvent;

#[cfg(perf_enabled)]
fn pulldown_adapter_syntax_data(source: &str) -> PulldownAssembly {
    PulldownAssembly {
        data: BenchmarkSyntaxData::from_production(
            &MarkdownSyntaxData::parse_with_pulldown_for_benchmarks(source),
        ),
    }
}

#[cfg(not(perf_enabled))]
fn pulldown_adapter_syntax_data(source: &str) -> PulldownAssembly {
    let options = Options::all();
    let mut builder = PulldownBuilder {
        source_len: source.len(),
        line_starts: line_starts(source),
        ..PulldownBuilder::default()
    };
    let mut events = Vec::new();
    for (event, range) in Parser::new_ext(source, options).into_offset_iter() {
        match event {
            Event::Start(tag) => {
                collect_start_tag(source, tag, range, &mut builder, &mut events);
            }
            Event::End(tag_end) => {
                let _ = tag_end;
            }
            Event::Text(_) | Event::Code(_) | Event::InlineMath(_) | Event::DisplayMath(_) => {}
            Event::Html(_) | Event::InlineHtml(_) => {}
            Event::SoftBreak | Event::HardBreak => {
                builder.inline_spans.push(BenchmarkInlineSpan {
                    kind: if matches!(event, Event::HardBreak) {
                        "HardBreak".to_string()
                    } else {
                        "SoftBreak".to_string()
                    },
                    source_range: range.clone(),
                    content_ranges: vec![range.clone()],
                    marker_ranges: Vec::new(),
                    url: None,
                    tagfilter_disallowed: false,
                });
            }
            Event::Rule => {}
            Event::TaskListMarker(_) => {}
            Event::FootnoteReference(_) => {}
        }
    }
    builder
        .blocks
        .extend(collect_blocks_from_pulldown(source, &events));
    builder.finish()
}

#[cfg(not(perf_enabled))]
fn collect_start_tag(
    source: &str,
    tag: Tag<'_>,
    range: Range<usize>,
    builder: &mut PulldownBuilder,
    events: &mut Vec<PulldownEvent>,
) {
    let _ = (source, range, events);
    match tag {
        Tag::Paragraph
        | Tag::Heading { .. }
        | Tag::BlockQuote(_)
        | Tag::CodeBlock(_)
        | Tag::List(_)
        | Tag::Item
        | Tag::Table(_)
        | Tag::MetadataBlock(_)
        | Tag::FootnoteDefinition(_)
        | Tag::HtmlBlock
        | Tag::DefinitionList
        | Tag::DefinitionListTitle
        | Tag::DefinitionListDefinition => {}
        Tag::TableHead | Tag::TableRow | Tag::TableCell => {}
        Tag::Emphasis | Tag::Strong | Tag::Strikethrough | Tag::Superscript | Tag::Subscript => {}
        Tag::Link { .. } | Tag::Image { .. } => {}
    }
    builder.blocks.push(BenchmarkBlock {
        kind: "Placeholder".to_string(),
        source_range: 0..0,
        content_range: 0..0,
        marker_ranges: Vec::new(),
        row_range: 0..0,
        tagfilter_disallowed: false,
    });
}

impl SequenceDiff {
    fn compare<T: Eq>(left: &[T], right: &[T]) -> Self {
        let common_prefix_len = left
            .iter()
            .zip(right.iter())
            .take_while(|(left, right)| left == right)
            .count();
        let first_mismatch_index = (left.len() != right.len() || common_prefix_len < left.len())
            .then_some(common_prefix_len);
        Self {
            left_len: left.len(),
            right_len: right.len(),
            common_prefix_len,
            first_mismatch_index,
        }
    }

    fn is_exact(self) -> bool {
        self.first_mismatch_index.is_none()
    }

    fn mismatch_score(self) -> usize {
        usize::from(!self.is_exact())
            .wrapping_add(self.left_len.abs_diff(self.right_len))
            .wrapping_add(self.first_mismatch_index.unwrap_or(0))
    }
}

impl SyntaxDiffSummary {
    fn compare(left: &BenchmarkSyntaxData, right: &BenchmarkSyntaxData) -> Self {
        Self {
            source_len_matches: left.source_len == right.source_len,
            line_starts: SequenceDiff::compare(&left.line_starts, &right.line_starts),
            blocks: SequenceDiff::compare(&left.blocks, &right.blocks),
            tables: SequenceDiff::compare(&left.tables, &right.tables),
            inline_spans: SequenceDiff::compare(&left.inline_spans, &right.inline_spans),
            projection_replacements: SequenceDiff::compare(
                &left.projection_replacements,
                &right.projection_replacements,
            ),
            projection_marker_dependencies: SequenceDiff::compare(
                &left.projection_marker_dependencies,
                &right.projection_marker_dependencies,
            ),
        }
    }

    fn mismatch_count(&self) -> usize {
        usize::from(!self.source_len_matches)
            + usize::from(!self.line_starts.is_exact())
            + usize::from(!self.blocks.is_exact())
            + usize::from(!self.tables.is_exact())
            + usize::from(!self.inline_spans.is_exact())
            + usize::from(!self.projection_replacements.is_exact())
            + usize::from(!self.projection_marker_dependencies.is_exact())
    }

    fn checksum(&self) -> usize {
        usize::from(self.source_len_matches)
            .wrapping_add(self.line_starts.mismatch_score())
            .wrapping_mul(31)
            .wrapping_add(self.blocks.mismatch_score())
            .wrapping_mul(31)
            .wrapping_add(self.tables.mismatch_score())
            .wrapping_mul(31)
            .wrapping_add(self.inline_spans.mismatch_score())
            .wrapping_mul(31)
            .wrapping_add(self.projection_replacements.mismatch_score())
            .wrapping_mul(31)
            .wrapping_add(self.projection_marker_dependencies.mismatch_score())
    }

    fn print(&self, label: &str) {
        println!("{label}: mismatch_fields={}", self.mismatch_count());
        println!("  source_len_matches={}", self.source_len_matches);
        println!("  line_starts={:?}", self.line_starts);
        println!("  blocks={:?}", self.blocks);
        println!("  tables={:?}", self.tables);
        println!("  inline_spans={:?}", self.inline_spans);
        println!(
            "  projection_replacements={:?}",
            self.projection_replacements
        );
        println!(
            "  projection_marker_dependencies={:?}",
            self.projection_marker_dependencies
        );
    }

    fn print_first_mismatches(
        &self,
        left: &BenchmarkSyntaxData,
        right: &BenchmarkSyntaxData,
    ) {
        print_first_mismatch("line_starts", self.line_starts, &left.line_starts, &right.line_starts);
        print_first_mismatch("blocks", self.blocks, &left.blocks, &right.blocks);
        print_first_mismatch("tables", self.tables, &left.tables, &right.tables);
        print_first_mismatch(
            "inline_spans",
            self.inline_spans,
            &left.inline_spans,
            &right.inline_spans,
        );
        print_first_mismatch(
            "projection_replacements",
            self.projection_replacements,
            &left.projection_replacements,
            &right.projection_replacements,
        );
        print_first_mismatch(
            "projection_marker_dependencies",
            self.projection_marker_dependencies,
            &left.projection_marker_dependencies,
            &right.projection_marker_dependencies,
        );
    }
}

fn print_first_mismatch<T: std::fmt::Debug>(
    name: &str,
    diff: SequenceDiff,
    left: &[T],
    right: &[T],
) {
    let Some(index) = diff.first_mismatch_index else {
        return;
    };

    println!("  {name}_first_mismatch[{index}]:");
    println!("    baseline={:?}", left.get(index));
    println!("    candidate={:?}", right.get(index));
}

fn main() {
    let target_bytes = env_usize("MARKDOWN_SYNTAX_BENCH_BYTES", 300 * 1024).max(1);
    let iterations = env_usize("MARKDOWN_SYNTAX_BENCH_ITERATIONS", 30).max(1);
    let source = mixed_fixture(target_bytes);
    println!("fixture_bytes={} iterations={iterations}", source.len());
    let production_baseline = BenchmarkSyntaxData::from_production(
        markdown_wysiwyg::MarkdownSyntaxTree::parse(&source).syntax_data(),
    );
    let initial_pulldown_assembly = pulldown_adapter_syntax_data(&source);
    let initial_diff = SyntaxDiffSummary::compare(&production_baseline, &initial_pulldown_assembly.data);
    initial_diff.print("pulldown_structure_semantics_diff");
    initial_diff.print_first_mismatches(&production_baseline, &initial_pulldown_assembly.data);

    measure("pulldown_cmark_parse_events", iterations, || {
        pulldown_parser_checksum(black_box(&source))
    });

    measure(
        "markdown_wysiwyg_tree_sitter_syntax_data",
        iterations,
        || {
            let tree = markdown_wysiwyg::MarkdownSyntaxTree::parse(black_box(&source));
            tree.syntax_data().checksum_for_benchmarks()
        },
    );

    measure("pulldown_semantics_adapter", iterations, || {
        let assembly = pulldown_adapter_syntax_data(black_box(&source));
        SyntaxDiffSummary::compare(black_box(&production_baseline), &assembly.data).checksum()
    });
}
