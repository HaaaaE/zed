use std::{
    hint::black_box,
    ops::Range,
    time::{Duration, Instant},
};

use markdown_wysiwyg::{
    MarkdownBlock, MarkdownInlineSpan, MarkdownProjectionReplacement, MarkdownSyntaxData,
    MarkdownTable, ProjectionMarkerDependency,
};
use pulldown_cmark::{Event, Options, Parser, Tag};

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

#[derive(Default)]
struct PulldownBuilder {
    source_len: usize,
    line_starts: Vec<usize>,
    blocks: Vec<BenchmarkBlock>,
    tables: Vec<BenchmarkTable>,
    inline_spans: Vec<BenchmarkInlineSpan>,
    projection_replacements: Vec<Range<usize>>,
    projection_marker_dependencies: Vec<(Range<usize>, Range<usize>)>,
}

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
            .map(|(marker_range, owner_source_range)| {
                BenchmarkProjectionMarkerDependency {
                    marker_range,
                    owner_source_range,
                }
            })
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
            blocks: data.blocks().iter().map(BenchmarkBlock::from_production).collect(),
            tables: data.tables().iter().map(BenchmarkTable::from_production).collect(),
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
    let chunk = r#"
# Source Row Heading

Paragraph source-row with **strong text**, _emphasis_, `inline code`, [a link](https://example.com), and an escaped \* marker.

> Blockquote source-row with **inline** content.
> - nested item source-row

- [x] completed task source-row
- [ ] open task with [link](https://example.com/path?q=1)
- plain list item source-row

| column | value | notes |
| --- | ---: | --- |
| source-row | 123 | **bold cell** and `code` |
| another | 456 | [cell link](https://example.com) |

```rust
fn source_row() {
    println!("source-row");
}
```

<div class="source-row">raw html</div>

[source-row-ref]: https://example.com/ref

"#;
    let mut text = String::with_capacity(target_bytes + chunk.len());
    while text.len() < target_bytes {
        text.push_str(chunk);
    }
    text
}

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

fn collect_blocks_from_pulldown(source: &str, _events: &[PulldownEvent]) -> Vec<BenchmarkBlock> {
    let mut blocks = Vec::new();
    for (row, line) in source.split_inclusive('\n').enumerate() {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed.is_empty() {
            blocks.push(BenchmarkBlock {
                kind: "Blank".to_string(),
                source_range: line_range(source, row),
                content_range: line_range(source, row).start..line_range(source, row).start,
                marker_ranges: Vec::new(),
                row_range: row..row + 1,
                tagfilter_disallowed: false,
            });
        }
    }
    blocks
}

fn line_range(source: &str, row: usize) -> Range<usize> {
    let starts = line_starts(source);
    let start = starts[row];
    let end = starts.get(row + 1).copied().unwrap_or(source.len());
    start..end
}

#[derive(Clone)]
struct PulldownEvent;

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
    builder.blocks.extend(collect_blocks_from_pulldown(source, &events));
    builder.finish()
}

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

fn compare_exact(left: &BenchmarkSyntaxData, right: &BenchmarkSyntaxData) -> usize {
    let mut diff = 0usize;
    if left.source_len != right.source_len {
        diff += 1;
    }
    if left.line_starts != right.line_starts {
        diff += 1;
    }
    if left.blocks != right.blocks {
        diff += 1;
    }
    if left.tables != right.tables {
        diff += 1;
    }
    if left.inline_spans != right.inline_spans {
        diff += 1;
    }
    if left.projection_replacements != right.projection_replacements {
        diff += 1;
    }
    if left.projection_marker_dependencies != right.projection_marker_dependencies {
        diff += 1;
    }
    diff
}

fn main() {
    let target_bytes = 300 * 1024;
    let iterations = 30;
    let source = mixed_fixture(target_bytes);
    println!("fixture_bytes={} iterations={iterations}", source.len());

    measure("markdown_wysiwyg_tree_sitter_syntax_data", iterations, || {
        let tree = markdown_wysiwyg::MarkdownSyntaxTree::parse(black_box(&source));
        tree.syntax_data().checksum_for_benchmarks()
    });

    measure("pulldown_exact_adapter", iterations, || {
        let assembly = pulldown_adapter_syntax_data(black_box(&source));
        compare_exact(
            &BenchmarkSyntaxData::from_production(
                markdown_wysiwyg::MarkdownSyntaxTree::parse(&source).syntax_data(),
            ),
            &assembly.data,
        )
    });
}
