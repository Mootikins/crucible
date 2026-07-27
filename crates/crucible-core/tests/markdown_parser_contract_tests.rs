use std::path::Path;

use crucible_core::parser::{CrucibleParser, MarkdownParser};
use tempfile::tempdir;
use tokio::fs;

/// Asserts the behavioral contract that ALL MarkdownParser implementations must satisfy.
/// Some optional fields (like plain_text) may not be populated by all parsers — test those
/// separately per implementation in the extended contract below.
async fn assert_markdown_parser_contract(parser: &dyn MarkdownParser) {
    assert!(parser.can_parse(Path::new("note.md")));
    assert!(parser.can_parse(Path::new("note.markdown")));
    assert!(!parser.can_parse(Path::new("note.txt")));

    let source_path = Path::new("contract.md");
    let content = "# Contract Title\n\nSee [[Target]] and #contract_tag.";
    let parsed = parser
        .parse_content(content, source_path)
        .await
        .expect("parse_content should succeed for valid markdown");

    assert_eq!(parsed.path, source_path);
    // word_count > 0 is a universal contract (all parsers must count words)
    assert!(
        parsed.content.word_count > 0,
        "parsed note should report words"
    );

    let dir = tempdir().expect("tempdir should be created");
    let file_path = dir.path().join("from_file.md");
    fs::write(&file_path, content)
        .await
        .expect("test markdown file should be written");

    let from_file = parser
        .parse_file(&file_path)
        .await
        .expect("parse_file should succeed for existing markdown files");

    assert_eq!(from_file.path, file_path);

    let capabilities = parser.capabilities();
    assert!(!capabilities.name.is_empty());
    assert!(!capabilities.extensions.is_empty());
    assert!(
        capabilities.extensions.contains(&"md"),
        "parser must advertise markdown support"
    );
}

/// Extended contract for parsers that populate plain_text (e.g. CrucibleParser).
async fn assert_plain_text_contract(parser: &dyn MarkdownParser) {
    let source_path = Path::new("contract.md");
    let content = "# Contract Title\n\nSee [[Target]] and #contract_tag.";
    let parsed = parser
        .parse_content(content, source_path)
        .await
        .expect("parse_content should succeed");
    assert!(
        parsed.content.plain_text.contains("Contract Title"),
        "parser should retain source content in plain_text"
    );
}

#[tokio::test]
async fn contract_crucible_parser_satisfies_markdown_parser_contract() {
    let parser = CrucibleParser::new();
    assert_markdown_parser_contract(&parser).await;
}

#[tokio::test]
async fn contract_crucible_parser_populates_plain_text() {
    let parser = CrucibleParser::new();
    assert_plain_text_contract(&parser).await;
}

#[tokio::test]
async fn contract_parse_file_returns_error_for_missing_path() {
    let missing = Path::new("definitely_missing_contract_file.md");

    let crucible = CrucibleParser::new();
    let crucible_result = crucible.parse_file(missing).await;

    assert!(
        crucible_result.is_err(),
        "parse_file should return an error for missing files"
    );
}

/// Parsing must never panic on multi-byte text.
///
/// The bug that motivated these: the footnote extension walked a `Vec<char>`
/// while slicing `content` by byte, so the two indices agreed only for ASCII.
/// One em dash before an inline footnote desynchronized them and the slice
/// landed mid-codepoint, panicking the thread — which, in the daemon, closed
/// the client's connection mid-request. A `cru status` against a kiln with an
/// em dash in it failed outright.
///
/// A panic is never an acceptable outcome for user content: notes are
/// arbitrary text, and every extension here slices by byte offset somewhere.
/// So this covers the whole extension set, not just footnotes — the same
/// mistake in any of them would surface here.
mod never_panics_on_multibyte {
    use crucible_core::parser::{CrucibleParser, MarkdownParser};
    use std::path::Path;

    /// Text that is cheap to get wrong: each of these is multi-byte in UTF-8,
    /// and the last few are multi-codepoint graphemes.
    const MULTIBYTE: &[&str] = &[
        "—",         // em dash, 3 bytes — the one that actually broke
        "…",         // ellipsis
        "é",         // 2 bytes
        "日本語",    // CJK, 3 bytes each
        "🙂",        // 4 bytes, outside the BMP
        "👩‍💻",        // ZWJ sequence: several codepoints, one grapheme
        "e\u{0301}", // combining acute — a grapheme spanning two codepoints
        "\u{202E}",  // bidi override, which is also what we strip elsewhere
    ];

    /// Syntax fragments whose extensions index by byte offset.
    const SYNTAX: &[&str] = &[
        "^an inline footnote^",
        "text[^1] and\n\n[^1]: definition",
        "[[A Wikilink]]",
        "[[Target|alias]]",
        "[inline](https://example.com)",
        "#a_tag",
        "> [!NOTE]\n> callout body",
        "$x^2 + y^2$",
        "$$\n\\int_0^1 f(x)\\,dx\n$$",
        "```rust\nfn main() {}\n```",
        "| a | b |\n|---|---|\n| 1 | 2 |",
        "---",
        "> quoted",
    ];

    /// Every multibyte string against every syntax fragment, in the positions
    /// where a byte/char mix-up bites: before, inside, and after.
    #[tokio::test]
    async fn every_extension_survives_multibyte_content() {
        let parser = CrucibleParser::new();
        let path = Path::new("multibyte.md");

        for mb in MULTIBYTE {
            for syn in SYNTAX {
                // `repeat` pushes the offsets well past any single-character
                // slack, so an off-by-a-few does not accidentally pass.
                let padding = mb.repeat(40);

                for content in [
                    format!("{padding} {syn}"),
                    format!("{syn} {padding}"),
                    format!("{padding} {syn} {padding}"),
                    format!("{syn}{mb}{syn}"),
                    // Directly adjacent, so a +1/-1 lands inside the codepoint.
                    format!("{mb}{syn}{mb}"),
                ] {
                    let result = parser.parse_content(&content, path).await;
                    assert!(
                        result.is_ok(),
                        "parsing failed for {mb:?} with {syn:?}: {:?}",
                        result.err()
                    );
                }
            }
        }
    }

    /// A note that is nothing but multibyte text and syntax markers, which is
    /// closer to how real notes look than any single fragment.
    #[tokio::test]
    async fn a_dense_multibyte_note_parses() {
        let parser = CrucibleParser::new();

        let mut content = String::from("---\ntitle: Тест — 日本語\ntags: [café]\n---\n\n");
        for (i, mb) in MULTIBYTE.iter().enumerate() {
            content.push_str(&format!(
                "## Section {mb} {i}\n\nProse with {mb} and ^an inline note {mb}^ then \
                 [[Link {mb}]], #tag{i}, and `code {mb}`.\n\n\
                 > [!NOTE] Callout {mb}\n> body {mb}\n\n"
            ));
        }

        let parsed = parser
            .parse_content(&content, Path::new("dense.md"))
            .await
            .expect("a dense multibyte note must parse");

        assert!(parsed.content.word_count > 0);
    }

    /// Offsets the parser reports must be usable for slicing. A byte offset
    /// that is not on a char boundary panics the moment any consumer indexes
    /// with it — which is how the original bug reached the daemon.
    #[tokio::test]
    async fn reported_offsets_land_on_char_boundaries() {
        let parser = CrucibleParser::new();
        let content = format!(
            "{pad} ^inline^ [[Link]] #tag [text](url)",
            pad = "— 日本語 🙂 ".repeat(30)
        );

        let parsed = parser
            .parse_content(&content, Path::new("offsets.md"))
            .await
            .expect("parse must succeed");

        // No frontmatter here, so body offsets and content offsets coincide;
        // with frontmatter these would need `parsed.body_offset` added first.
        for wl in &parsed.content.wikilinks {
            assert!(
                content.is_char_boundary(wl.offset),
                "wikilink offset {} splits a codepoint",
                wl.offset
            );
        }
        for def in parsed.content.footnotes.definitions.values() {
            assert!(
                content.is_char_boundary(def.offset),
                "footnote offset {} splits a codepoint",
                def.offset
            );
        }
    }
}
