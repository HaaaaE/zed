use std::{
    hint::black_box,
    time::{Duration, Instant},
};

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

fn measure(iterations: usize, mut run: impl FnMut() -> usize) {
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
        "mean={mean:.3}ms median={median:.3}ms min={min:.3}ms max={max:.3}ms checksum={checksum}"
    );
}

fn main() {
    let target_bytes = 300 * 1024;
    let iterations = 30;
    let source = mixed_fixture(target_bytes);
    println!("fixture_bytes={} iterations={iterations}", source.len());
    measure(iterations, || {
        let tree = markdown_wysiwyg::MarkdownSyntaxTree::parse(black_box(&source));
        tree.syntax_data().checksum_for_benchmarks()
    });
}
