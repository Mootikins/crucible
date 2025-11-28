//! Simple PoC benchmark: Wikilink extraction performance comparison
//!
//! This benchmark compares wikilink extraction performance between:
//! 1. Pulldown-cmark + regex (current approach)
//! 2. markdown-it with custom plugin (new approach)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

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
More [[links]] with [[references]] everywhere."#,
    ),
    (
        "large",
        r#"# Large Document
## Introduction
Lorem ipsum with [[First Link]] in the text.

### Subsection
More content with [[Another Link|Display]] and [[Reference]].

## Middle Section
Paragraph with [[multiple]] [[wikilinks]] and [[more|aliases]].

### Another Subsection
Even [[more]] [[links]] to [[test]] the [[parser]].

## Conclusion
Final [[links]] and [[references]] here."#,
    ),
    (
        "wikilink_heavy",
        r#"[[Link1]] [[Link2]] [[Link3|Alias]] [[Link4]] [[Link5|Display]]
[[Link6]] [[Link7#Section]] [[Link8#^block]] ![[Embed1]] [[Link9]]
[[Link10]] [[Link11|A]] [[Link12]] [[Link13]] [[Link14|B]]"#,
    ),
];

// Pulldown-cmark + regex approach (current)
fn benchmark_pulldown_regex(c: &mut Criterion) {
    use regex::Regex;

    let wikilink_re = Regex::new(r"(!?)\[\[([^\]]+?)\]\]").unwrap();

    let mut group = c.benchmark_group("pulldown_regex");

    for (name, content) in TEST_DOCS {
        group.bench_with_input(
            BenchmarkId::new("extract_wikilinks", name),
            content,
            |b, content| {
                b.iter(|| {
                    let mut count = 0;
                    for cap in wikilink_re.captures_iter(black_box(content)) {
                        let _is_embed = cap.get(1).map(|m| !m.as_str().is_empty()).unwrap_or(false);
                        let inner = cap.get(2).unwrap().as_str();

                        // Parse target|alias
                        let (_target, _alias) = if let Some(pipe_pos) = inner.find('|') {
                            (&inner[..pipe_pos], Some(&inner[pipe_pos + 1..]))
                        } else {
                            (inner, None)
                        };

                        count += 1;
                    }
                    count
                });
            },
        );
    }

    group.finish();
}

// markdown-it approach (new)
#[cfg(feature = "markdown-it-parser")]
fn benchmark_markdown_it(c: &mut Criterion) {
    use markdown_it::MarkdownIt;
    use markdown_it::Node;

    // Setup parser with wikilink plugin
    fn setup_parser() -> MarkdownIt {
        let mut md = MarkdownIt::new();
        markdown_it::plugins::cmark::add(&mut md);
        crucible_parser::markdown_it::plugins::add_wikilink_plugin(&mut md);
        md
    }

    // Extract wikilinks from AST
    fn count_wikilinks(node: &Node) -> usize {
        let mut count = 0;

        if node
            .cast::<crucible_parser::markdown_it::plugins::wikilink::WikilinkNode>()
            .is_some()
        {
            count += 1;
        }

        for child in node.children.iter() {
            count += count_wikilinks(child);
        }

        count
    }

    let mut group = c.benchmark_group("markdown_it");

    for (name, content) in TEST_DOCS {
        let parser = setup_parser();

        group.bench_with_input(
            BenchmarkId::new("extract_wikilinks", name),
            content,
            |b, content| {
                b.iter(|| {
                    let ast = parser.parse(black_box(content));
                    count_wikilinks(&ast)
                });
            },
        );
    }

    group.finish();
}

#[cfg(feature = "markdown-it-parser")]
criterion_group!(benches, benchmark_pulldown_regex, benchmark_markdown_it);

#[cfg(not(feature = "markdown-it-parser"))]
criterion_group!(benches, benchmark_pulldown_regex);

criterion_main!(benches);
