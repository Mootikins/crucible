//! Benchmark: Full parsing + tree building

#![allow(deprecated)] // criterion::black_box is deprecated

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use crucible_parser::{CrucibleParser, MarkdownParser};
use std::path::PathBuf;

#[cfg(feature = "markdown-it-parser")]
use crucible_parser::MarkdownItParser;

const TEST_DOCS: &[(&str, &str)] = &[
    (
        "small",
        r#"# Simple Note
Just a basic paragraph with one [[link]]."#,
    ),
    (
        "medium",
        r#"# Medium Document
## Section 1
This has [[Link One]] and [[Link Two|Alias]].
## Section 2
More [[links]] with [[references]] and #tags everywhere."#,
    ),
    (
        "large",
        r#"# Large Document
## Introduction
Lorem ipsum with [[First Link]] and #tag1 in the text.

### Subsection
More content with [[Another Link|Display]] and [[Reference]] plus #tag2.

```rust
fn main() {
    println!("code");
}
```

## Middle Section
Paragraph with [[multiple]] [[wikilinks]] and [[more|aliases]] #project/ai.

### Another Subsection
Even [[more]] [[links]] to [[test]] the [[parser]] with #tags.

## Conclusion
Final [[links]] and [[references]] here #conclusion."#,
    ),
    (
        "wikilink_heavy",
        r#"[[Link1]] [[Link2]] [[Link3|Alias]] #tag1 [[Link4]] [[Link5|Display]]
[[Link6]] [[Link7#Section]] #tag2 [[Link8#^block]] ![[Embed1]] [[Link9]] #nested/tag
[[Link10]] [[Link11|A]] #tag3 [[Link12]] [[Link13]] [[Link14|B]] #another/nested/tag"#,
    ),
];

fn benchmark_crucible_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("crucible_parser");
    let path = PathBuf::from("test.md");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    for (name, content) in TEST_DOCS {
        group.bench_with_input(BenchmarkId::new("parse", name), content, |b, content| {
            let parser = CrucibleParser::new();
            b.iter(|| {
                runtime.block_on(async { parser.parse_content(black_box(content), &path).await })
            });
        });
    }

    group.finish();
}

#[cfg(feature = "markdown-it-parser")]
fn benchmark_markdown_it_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("markdown_it_parser");
    let path = PathBuf::from("test.md");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    for (name, content) in TEST_DOCS {
        group.bench_with_input(BenchmarkId::new("parse", name), content, |b, content| {
            let parser = MarkdownItParser::new();
            b.iter(|| {
                runtime.block_on(async { parser.parse_content(black_box(content), &path).await })
            });
        });
    }

    group.finish();
}

#[cfg(feature = "markdown-it-parser")]
criterion_group!(
    benches,
    benchmark_crucible_parser,
    benchmark_markdown_it_parser
);

#[cfg(not(feature = "markdown-it-parser"))]
criterion_group!(benches, benchmark_crucible_parser);

criterion_main!(benches);
