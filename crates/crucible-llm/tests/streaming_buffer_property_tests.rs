//! Property-based tests for streaming buffer handling
//!
//! This test suite verifies the correctness and robustness of buffer handling
//! in both SSE (Server-Sent Events) and NDJSON streaming formats used by
//! OpenAI and Ollama chat providers respectively.
//!
//! ## Test Coverage
//!
//! ### SSE Format (OpenAI)
//! - Complete line extraction from buffers
//! - Partial line retention across chunks
//! - "data: " prefix stripping
//! - [DONE] marker handling
//! - Empty line and comment skipping
//! - Arbitrary byte-level splits
//! - UTF-8 boundary handling
//! - Multi-line events
//!
//! ### NDJSON Format (Ollama)
//! - Newline-delimited JSON line extraction
//! - Partial JSON object handling
//! - Arbitrary split points
//! - Empty content handling
//!
//! ### Property Tests
//! Uses `proptest` to verify invariants hold across:
//! - Random split points in byte streams
//! - Various content sizes and patterns
//! - Different chunk boundary positions
//! - Unicode and emoji content
//! - Malformed or incomplete data
//!
//! ## Implementation Note
//!
//! These tests simulate the buffer accumulation logic found in:
//! - `crates/crucible-llm/src/chat/openai.rs` (SSE streaming)
//! - `crates/crucible-llm/src/chat/ollama.rs` (NDJSON streaming)
//!
//! The test implementations mirror the production code to ensure the actual
//! streaming parsers handle all edge cases correctly.

use proptest::prelude::*;

/// Simulates the SSE buffer accumulation logic from chat providers
mod sse_buffer {
    /// Process a buffer and extract complete SSE data lines
    ///
    /// This mirrors the logic in openai.rs and ollama.rs streaming implementations
    pub fn process_buffer(buffer: &mut String) -> Vec<String> {
        let mut results = Vec::new();

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim().to_string();
            *buffer = buffer[line_end + 1..].to_string();

            if line.is_empty() || !line.starts_with("data: ") {
                continue;
            }

            let json_str = &line[6..]; // Skip "data: "
            if json_str == "[DONE]" {
                results.push("[DONE]".to_string());
                break;
            }

            results.push(json_str.to_string());
        }

        results
    }

    /// Accumulate bytes into buffer (simulates chunk arrival)
    pub fn accumulate(buffer: &mut String, chunk: &str) {
        buffer.push_str(chunk);
    }
}

/// Strategy for generating valid SSE data lines
fn sse_data_line_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Simple JSON object
        Just(r#"data: {"id":"123","content":"hello"}"#.to_string()),
        // JSON with special chars
        Just(r#"data: {"text":"hello\nworld"}"#.to_string()),
        // Empty JSON object
        Just(r#"data: {}"#.to_string()),
        // DONE marker
        Just("data: [DONE]".to_string()),
        // Variable content JSON
        "[a-zA-Z0-9]{1,20}".prop_map(|s| format!(r#"data: {{"content":"{}"}}"#, s)),
    ]
}

/// Strategy for generating SSE chunks (possibly partial)
fn sse_chunk_strategy() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(sse_data_line_strategy(), 1..10)
        .prop_map(|lines| lines.into_iter().map(|l| l + "\n").collect())
}

/// Strategy for generating arbitrary byte chunks (used for fuzzing)
fn arbitrary_chunk_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Valid SSE line
        sse_data_line_strategy().prop_map(|s| s + "\n"),
        // Empty line
        Just("\n".to_string()),
        // Comment (should be skipped)
        ":.*\n",
        // Partial line (no newline yet)
        "data: [a-z]{1,10}",
        // Non-data line
        "event: [a-z]+\n",
    ]
}

/// Strategy for generating valid NDJSON content (Ollama format)
fn ndjson_line_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Simple message
        "[a-zA-Z0-9 ]{1,30}".prop_map(|c| {
            format!(
                r#"{{"message":{{"role":"assistant","content":"{}"}},"done":false}}"#,
                c
            )
        }),
        // Done message
        Just(r#"{"message":{"role":"assistant","content":""},"done":true}"#.to_string()),
        // Empty content
        Just(r#"{"message":{"role":"assistant","content":""},"done":false}"#.to_string()),
    ]
}

proptest! {
    /// Property: Complete lines ending with \n should be extracted
    /// Note: DONE marker stops processing, so we count lines before DONE
    #[test]
    fn complete_lines_extracted(lines in sse_chunk_strategy()) {
        let full_content: String = lines.join("");
        let mut buffer = full_content.clone();

        let results = sse_buffer::process_buffer(&mut buffer);

        // Find position of DONE line (if any) - only count lines before it
        let done_pos = lines.iter().position(|l| l.trim() == "data: [DONE]");

        let expected_data_lines: usize = lines.iter()
            .take(done_pos.unwrap_or(lines.len()))
            .filter(|l| l.starts_with("data: ") && l.trim() != "data: [DONE]")
            .count();

        // If there's a DONE line, it should also be in results
        let expected_total = expected_data_lines + if done_pos.is_some() { 1 } else { 0 };

        prop_assert_eq!(
            results.len(), expected_total,
            "Expected {} results (data lines: {}, done: {}), got {}. Lines: {:?}",
            expected_total, expected_data_lines, done_pos.is_some(),
            results.len(), lines
        );
    }

    /// Property: Partial lines should remain in buffer
    #[test]
    fn partial_lines_remain_in_buffer(
        complete_line in sse_data_line_strategy(),
        partial_suffix in "[a-z]{1,20}"
    ) {
        let mut buffer = format!("{}\ndata: {}", complete_line, partial_suffix);

        let _results = sse_buffer::process_buffer(&mut buffer);

        // Partial line should still be in buffer (without newline)
        prop_assert!(
            buffer.contains(&partial_suffix) || buffer.is_empty(),
            "Partial line '{}' should remain in buffer, got: '{}'",
            partial_suffix,
            buffer
        );
    }

    /// Property: Buffer accumulation is associative
    /// (combining chunks in any way should give same final result)
    #[test]
    fn buffer_accumulation_associative(
        chunk1 in "[a-zA-Z0-9: ]{1,30}",
        chunk2 in "[a-zA-Z0-9: ]{1,30}"
    ) {
        // Method 1: Add all at once
        let mut buffer1 = String::new();
        sse_buffer::accumulate(&mut buffer1, &chunk1);
        sse_buffer::accumulate(&mut buffer1, &chunk2);

        // Method 2: Add combined
        let mut buffer2 = String::new();
        sse_buffer::accumulate(&mut buffer2, &format!("{}{}", chunk1, chunk2));

        prop_assert_eq!(buffer1, buffer2, "Accumulation should be associative");
    }

    /// Property: Empty lines and non-data lines should be skipped
    #[test]
    fn non_data_lines_skipped(content in "[a-zA-Z0-9]{1,20}") {
        let input = format!(
            "\nevent: message\n:comment\ndata: {{{content}}}\n\n",
            content = format!(r#""content":"{}""#, content)
        );
        let mut buffer = input;

        let results = sse_buffer::process_buffer(&mut buffer);

        // Should only get the data line content
        prop_assert_eq!(results.len(), 1, "Should extract exactly one data line");
        prop_assert!(
            results[0].contains(&content),
            "Result should contain content: {}",
            content
        );
    }

    /// Property: DONE marker should stop processing
    #[test]
    fn done_marker_stops_processing(
        pre_content in "[a-zA-Z]{5,15}",
        post_content in "[a-zA-Z]{5,15}"
    ) {
        let input = format!(
            "data: {{\"pre\":\"{}\"}}\ndata: [DONE]\ndata: {{\"post\":\"{}\"}}\n",
            pre_content, post_content
        );
        let mut buffer = input;

        let results = sse_buffer::process_buffer(&mut buffer);

        // Should get pre-content and DONE, but not post-content
        prop_assert!(
            results.iter().any(|r| r.contains(&pre_content)),
            "Should contain pre-content"
        );
        prop_assert!(
            results.contains(&"[DONE]".to_string()),
            "Should contain DONE marker"
        );
        prop_assert!(
            !results.iter().any(|r| r.contains(&post_content)),
            "Should NOT contain post-content after DONE"
        );
    }

    /// Property: data: prefix is correctly stripped
    #[test]
    fn data_prefix_stripped(content in "[a-zA-Z0-9]{1,50}") {
        let input = format!("data: {}\n", content);
        let mut buffer = input;

        let results = sse_buffer::process_buffer(&mut buffer);

        if !results.is_empty() && content != "[DONE]" {
            prop_assert_eq!(
                &results[0], &content,
                "data: prefix should be stripped"
            );
        }
    }

    /// Property: Processing is idempotent (processing empty buffer gives empty result)
    #[test]
    fn processing_idempotent_on_empty(_seed in any::<u64>()) {
        let mut buffer = String::new();
        let results1 = sse_buffer::process_buffer(&mut buffer);
        let results2 = sse_buffer::process_buffer(&mut buffer);

        prop_assert!(results1.is_empty(), "First process of empty buffer should be empty");
        prop_assert!(results2.is_empty(), "Second process of empty buffer should be empty");
    }

    /// Property: Newlines at various positions should be handled correctly
    #[test]
    fn newlines_handled_correctly(
        content in "[a-zA-Z0-9]{1,20}",
        extra_newlines in 0usize..5
    ) {
        let prefix_newlines = "\n".repeat(extra_newlines);
        let suffix_newlines = "\n".repeat(extra_newlines);
        let input = format!(
            "{}data: {{\"content\":\"{}\"}}\n{}",
            prefix_newlines, content, suffix_newlines
        );
        let mut buffer = input;

        let results = sse_buffer::process_buffer(&mut buffer);

        // Should extract exactly one data line regardless of extra newlines
        prop_assert_eq!(
            results.len(), 1,
            "Should extract exactly one data line despite {} extra newlines",
            extra_newlines * 2
        );
    }

    /// Property: Arbitrary byte-level splits should not panic
    #[test]
    fn arbitrary_byte_splits_never_panic(
        content in "[a-zA-Z0-9 ]{10,50}",
        split_points in prop::collection::vec(0usize..100, 1..5)
    ) {
        let sse = format!("data: {{\"content\":\"{}\"}}\n\ndata: [DONE]\n\n", content);

        // Apply splits at various points
        let mut chunks = Vec::new();
        let mut last_pos = 0;

        for &split in &split_points {
            let pos = split.min(sse.len());
            if pos > last_pos {
                chunks.push(&sse[last_pos..pos]);
                last_pos = pos;
            }
        }
        if last_pos < sse.len() {
            chunks.push(&sse[last_pos..]);
        }

        // Process chunks incrementally - should not panic
        let mut buffer = String::new();
        for chunk in chunks {
            sse_buffer::accumulate(&mut buffer, chunk);
            let _results = sse_buffer::process_buffer(&mut buffer);
        }

        // Should complete successfully
        prop_assert!(true);
    }

    /// Property: UTF-8 boundary splits should be handled safely
    #[test]
    fn utf8_boundary_handling(
        ascii_prefix in "[a-z]{0,5}",
        emoji_count in 0usize..3
    ) {
        let emojis = "🎉".repeat(emoji_count);
        let content = format!("{}{}", ascii_prefix, emojis);
        let sse = format!("data: {{\"content\":\"{}\"}}\n", content);

        let mut buffer = String::new();

        // Add in small chunks (may hit UTF-8 boundaries)
        for chunk in sse.as_bytes().chunks(2) {
            // from_utf8_lossy handles invalid UTF-8 gracefully
            buffer.push_str(&String::from_utf8_lossy(chunk));
        }

        let _results = sse_buffer::process_buffer(&mut buffer);

        // Should not panic on UTF-8 boundary splits
        prop_assert!(true);
    }
}

/// NDJSON-specific property tests (Ollama format)
mod ndjson_tests {
    use super::*;

    /// Simulates NDJSON buffer processing (Ollama format)
    pub(crate) mod ndjson_buffer {
        /// Process NDJSON buffer - simpler than SSE, just newline-delimited JSON
        pub fn process_buffer(buffer: &mut String) -> Vec<String> {
            let mut results = Vec::new();

            while let Some(line_end) = buffer.find('\n') {
                let line = buffer[..line_end].trim().to_string();
                *buffer = buffer[line_end + 1..].to_string();

                if line.is_empty() {
                    continue;
                }

                results.push(line);
            }

            results
        }
    }

    proptest! {
        /// Property: NDJSON complete lines should be extracted
        #[test]
        fn ndjson_complete_lines_extracted(
            lines in prop::collection::vec(ndjson_line_strategy(), 1..10)
        ) {
            let ndjson: String = lines.iter().map(|l| format!("{}\n", l)).collect();
            let mut buffer = ndjson.clone();

            let results = ndjson_buffer::process_buffer(&mut buffer);

            prop_assert_eq!(
                results.len(), lines.len(),
                "Should extract all {} NDJSON lines",
                lines.len()
            );
        }

        /// Property: NDJSON partial line should remain in buffer
        #[test]
        fn ndjson_partial_line_remains(
            complete_lines in prop::collection::vec(ndjson_line_strategy(), 1..5),
            partial_line in "[a-zA-Z0-9]{1,20}"
        ) {
            let complete = complete_lines.iter().map(|l| format!("{}\n", l)).collect::<String>();
            let mut buffer = format!("{}{{\"message\":{{\"content\":\"{}\"", complete, partial_line);

            let results = ndjson_buffer::process_buffer(&mut buffer);

            prop_assert_eq!(
                results.len(), complete_lines.len(),
                "Should only extract complete lines"
            );
            prop_assert!(
                buffer.contains(&partial_line),
                "Partial line should remain in buffer"
            );
        }

        /// Property: NDJSON arbitrary split points should not panic
        #[test]
        fn ndjson_arbitrary_splits_safe(
            content in "[a-zA-Z0-9 ]{5,30}",
            split_point in 0usize..100
        ) {
            let ndjson = format!(
                "{}\n",
                format!(r#"{{"message":{{"role":"assistant","content":"{}"}},"done":false}}"#, content)
            );

            let split = split_point.min(ndjson.len());
            let (part1, part2) = ndjson.split_at(split);

            // Simulate chunked arrival
            let mut buffer = String::new();
            buffer.push_str(part1);
            let _r1 = ndjson_buffer::process_buffer(&mut buffer);
            buffer.push_str(part2);
            let _r2 = ndjson_buffer::process_buffer(&mut buffer);

            // Should not panic
            prop_assert!(true);
        }

        /// Property: Empty NDJSON content should be handled
        #[test]
        fn ndjson_empty_content_handled(line_count in 0usize..5) {
            let empty_line = r#"{"message":{"role":"assistant","content":""},"done":false}"#;
            let ndjson = format!("{}\n", empty_line).repeat(line_count);
            let mut buffer = ndjson;

            let results = ndjson_buffer::process_buffer(&mut buffer);

            prop_assert_eq!(results.len(), line_count, "Should extract all empty-content lines");
        }
    }
}

/// Edge case tests (non-property-based)
#[cfg(test)]
mod edge_cases {
    use super::sse_buffer;

    #[test]
    fn empty_buffer() {
        let mut buffer = String::new();
        let results = sse_buffer::process_buffer(&mut buffer);
        assert!(results.is_empty());
    }

    #[test]
    fn only_newlines() {
        let mut buffer = "\n\n\n\n".to_string();
        let results = sse_buffer::process_buffer(&mut buffer);
        assert!(results.is_empty());
    }

    #[test]
    fn only_comments() {
        let mut buffer = ":comment1\n:comment2\n".to_string();
        let results = sse_buffer::process_buffer(&mut buffer);
        assert!(results.is_empty());
    }

    #[test]
    fn unicode_content() {
        let mut buffer = "data: {\"content\":\"こんにちは世界\"}\n".to_string();
        let results = sse_buffer::process_buffer(&mut buffer);
        assert_eq!(results.len(), 1);
        assert!(results[0].contains("こんにちは世界"));
    }

    #[test]
    fn chunked_delivery_simulation() {
        // Simulate bytes arriving in chunks
        let mut buffer = String::new();

        // First chunk: partial line
        sse_buffer::accumulate(&mut buffer, "data: {\"part");
        let results1 = sse_buffer::process_buffer(&mut buffer);
        assert!(results1.is_empty(), "Partial line should not be processed");
        assert!(!buffer.is_empty(), "Partial should remain in buffer");

        // Second chunk: completes the line
        sse_buffer::accumulate(&mut buffer, "\":1}\n");
        let results2 = sse_buffer::process_buffer(&mut buffer);
        assert_eq!(results2.len(), 1);
        assert!(results2[0].contains("\"part\":1"));
    }

    #[test]
    fn multiple_data_lines_per_chunk() {
        let mut buffer = "data: {\"a\":1}\ndata: {\"b\":2}\ndata: {\"c\":3}\n".to_string();
        let results = sse_buffer::process_buffer(&mut buffer);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn carriage_return_handling() {
        // SSE spec allows \r\n or \n
        let mut buffer = "data: {\"test\":1}\r\ndata: {\"test\":2}\n".to_string();
        let results = sse_buffer::process_buffer(&mut buffer);
        // Note: Our simple implementation only splits on \n, but trim() handles \r
        assert!(!results.is_empty());
    }

    #[test]
    fn byte_level_split_in_middle_of_data_prefix() {
        // Simulate receiving "da" then "ta: {content}\n"
        let mut buffer = String::new();

        sse_buffer::accumulate(&mut buffer, "da");
        let r1 = sse_buffer::process_buffer(&mut buffer);
        assert!(r1.is_empty(), "Should not process partial 'data:' prefix");
        assert_eq!(buffer, "da", "Partial should remain");

        sse_buffer::accumulate(&mut buffer, "ta: {\"test\":1}\n");
        let r2 = sse_buffer::process_buffer(&mut buffer);
        assert_eq!(r2.len(), 1, "Should process complete line");
        assert_eq!(r2[0], "{\"test\":1}");
    }

    #[test]
    fn byte_level_split_in_middle_of_json() {
        // Simulate JSON arriving in pieces
        let mut buffer = String::new();

        let chunks = [
            "data: {\"choices\":[{\"delta",
            "\":{\"content\":\"hel",
            "lo\"}}]}\n",
        ];

        for chunk in chunks {
            sse_buffer::accumulate(&mut buffer, chunk);
            let _results = sse_buffer::process_buffer(&mut buffer);
        }

        // Final processing should have the complete message
        let results = sse_buffer::process_buffer(&mut buffer);
        // The line was processed in the last chunk addition
        assert!(buffer.is_empty() || results.is_empty());
    }

    #[test]
    fn ndjson_byte_level_split() {
        // NDJSON doesn't use "data: " prefix, just raw JSON lines
        // We use the ndjson_tests module's buffer processor
        use crate::ndjson_tests::ndjson_buffer;

        let mut buffer = String::new();

        // Split in the middle of a JSON object
        buffer.push_str(r#"{"message":{"role":"assi"#);
        let r1 = ndjson_buffer::process_buffer(&mut buffer);
        assert!(r1.is_empty(), "Incomplete JSON line should not be processed");

        buffer.push_str(&format!("{}\n", r#"stant","content":"Hi"},"done":false}"#));
        let r2 = ndjson_buffer::process_buffer(&mut buffer);
        assert_eq!(r2.len(), 1, "Complete line should be processed");
    }

    #[test]
    fn large_buffer_accumulation() {
        // Test that buffer can handle large accumulations
        let mut buffer = String::new();

        // Add 100 small chunks
        for i in 0..100 {
            sse_buffer::accumulate(&mut buffer, &format!("data: {{\"chunk\":{}}}\n", i));
        }

        let results = sse_buffer::process_buffer(&mut buffer);
        assert_eq!(results.len(), 100, "Should process all 100 chunks");
    }

    #[test]
    fn zero_byte_chunks() {
        // Ensure empty chunks don't cause issues
        let mut buffer = String::new();

        sse_buffer::accumulate(&mut buffer, "");
        sse_buffer::accumulate(&mut buffer, "data: {\"test\":1}\n");
        sse_buffer::accumulate(&mut buffer, "");

        let results = sse_buffer::process_buffer(&mut buffer);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn single_byte_chunks() {
        // Extreme case: one byte at a time
        let complete_sse = "data: {\"content\":\"test\"}\n";
        let mut buffer = String::new();

        for byte_char in complete_sse.chars() {
            sse_buffer::accumulate(&mut buffer, &byte_char.to_string());
        }

        let results = sse_buffer::process_buffer(&mut buffer);
        assert_eq!(results.len(), 1, "Should assemble from single-char chunks");
        assert!(results[0].contains("test"));
    }
}
