//! Benchmark comparing CrucibleParser vs MarkdownItParser
//!
//! CrucibleParser uses regex-based extraction for custom syntax (wikilinks, tags, etc.)
//! MarkdownItParser uses markdown-it with custom plugins for AST-based parsing.

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use crucible_parser::{CrucibleParser, MarkdownParser};
use std::path::PathBuf;

#[cfg(feature = "markdown-it-parser")]
use crucible_parser::MarkdownItParser;

const SMALL_DOC: &str = r#"# Simple Note

Just a basic paragraph with some text."#;

const MEDIUM_DOC: &str = r#"# Medium Complexity Note

## Introduction

This is a more complex document with [[wikilinks]] and #tags.

## Features

- First item
- Second item with [[Another Link|Alias]]
- Third item

## Code Example

```rust
fn main() {
    println!("Hello, world!");
}
```

## Conclusion

Summary with more [[references]] and #project tags."#;

const LARGE_DOC: &str = r#"# Large Document

## Section 1

Lorem ipsum dolor sit amet, consectetur adipiscing elit. [[First Link]] sed do
eiusmod tempor incididunt ut labore et dolore magna aliqua. #important

### Subsection 1.1

More content with [[wikilinks]] everywhere. Ut enim ad minim veniam, quis
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.

### Subsection 1.2

Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore
eu fugiat nulla pariatur. [[Another Reference|Display Name]] with #tags.

## Section 2

```python
def hello():
    print("Hello, world!")
    return [[link_in_code]]  # Should this be parsed?
```

More paragraphs with [[multiple]] [[wikilinks]] in [[succession]].

### Subsection 2.1

Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia
deserunt mollit anim id est laborum. #nested/tag/path

## Section 3

- List item one with [[Link One]]
- List item two with [[Link Two|Alias]]
- List item three #tagged

### Subsection 3.1

Final content with [[last link]] and wrap-up. #conclusion"#;

const WIKILINK_HEAVY: &str = r#"# Wikilink Test

This document has [[many]] [[wikilinks]] including [[ones|with aliases]] and
[[some#with-headings]] and even ![[embeds]]. More [[links]] and [[more|aliases]]
throughout. [[Yet]] [[another]] [[link]]."#;

fn benchmark_crucible_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("crucible_parser");
    let path = PathBuf::from("test.md");

    group.bench_with_input(
        BenchmarkId::new("parse", "small"),
        SMALL_DOC,
        |b, content| {
            let parser = CrucibleParser::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async { parser.parse_content(black_box(content), &path).await });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("parse", "medium"),
        MEDIUM_DOC,
        |b, content| {
            let parser = CrucibleParser::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async { parser.parse_content(black_box(content), &path).await });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("parse", "large"),
        LARGE_DOC,
        |b, content| {
            let parser = CrucibleParser::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async { parser.parse_content(black_box(content), &path).await });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("parse", "wikilink_heavy"),
        WIKILINK_HEAVY,
        |b, content| {
            let parser = CrucibleParser::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async { parser.parse_content(black_box(content), &path).await });
        },
    );

    group.finish();
}

#[cfg(feature = "markdown-it-parser")]
fn benchmark_markdown_it_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("markdown_it_parser");
    let path = PathBuf::from("test.md");

    group.bench_with_input(
        BenchmarkId::new("parse", "small"),
        SMALL_DOC,
        |b, content| {
            let parser = MarkdownItParser::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async { parser.parse_content(black_box(content), &path).await });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("parse", "medium"),
        MEDIUM_DOC,
        |b, content| {
            let parser = MarkdownItParser::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async { parser.parse_content(black_box(content), &path).await });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("parse", "large"),
        LARGE_DOC,
        |b, content| {
            let parser = MarkdownItParser::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async { parser.parse_content(black_box(content), &path).await });
        },
    );

    group.bench_with_input(
        BenchmarkId::new("parse", "wikilink_heavy"),
        WIKILINK_HEAVY,
        |b, content| {
            let parser = MarkdownItParser::new();
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter(|| async { parser.parse_content(black_box(content), &path).await });
        },
    );

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
