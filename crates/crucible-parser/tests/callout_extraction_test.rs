//! Direct test of callout extraction functionality
//!
//! Tests the regex-based callout extraction that's currently implemented.

#[cfg(test)]
mod tests {
    use regex::Regex;

    /// Test the exact regex pattern used in the pulldown parser
    #[test]
    fn test_callout_regex_pattern() {
        // This is the pattern from pulldown.rs line 278
        let re = Regex::new(r"(?m)^>\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\](?:\s+([^\n]*))?\s*$").unwrap();

        let test_cases = vec![
            // (input, expected_type, expected_title)
            ("> [!note]", "note", None),
            ("> [!note] Title here", "note", Some("Title here")),
            ("> [!warning] Important Warning", "warning", Some("Important Warning")),
            ("> [!my-custom-type]", "my-custom-type", None),
            ("> [!tip] Tip with title", "tip", Some("Tip with title")),
        ];

        for (input, expected_type, expected_title) in test_cases {
            if let Some(cap) = re.captures(input) {
                let callout_type = cap.get(1).unwrap().as_str();
                let title = cap.get(2).map(|m| m.as_str().trim()).filter(|s| !s.is_empty());

                assert_eq!(callout_type, expected_type, "Type mismatch for input: {}", input);
                assert_eq!(title, expected_title, "Title mismatch for input: {}", input);
            } else {
                panic!("Regex failed to match: {}", input);
            }
        }
    }

    /// Test malformed inputs that should not match
    #[test]
    fn test_malformed_callout_regex() {
        let re = Regex::new(r"(?m)^>\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\](?:\s+([^\n]*))?\s*$").unwrap();

        let malformed_cases = vec![
            "> [! without closing bracket",
            "> [note] Missing exclamation",
            "> [!123numbers-first] Numbers first",
            "> [] Empty bracket",
            "> [!] No type",
            "Regular text without callout",
            "> [!note",
        ];

        for input in malformed_cases {
            if let Some(_) = re.captures(input) {
                panic!("Regex should not have matched malformed input: {}", input);
            }
        }
    }

    /// Test callout content extraction logic
    #[test]
    fn test_callout_content_extraction() {
        let content = r#"> [!note] Note with title
> First line
> Second line
> Third line

Regular text here

> [!warning]
> Warning content
> More warning"#;

        // First, extract callout headers
        let header_re = Regex::new(r"(?m)^>\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\](?:\s+([^\n]*))?\s*$").unwrap();
        let lines: Vec<&str> = content.lines().collect();

        let mut callouts = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            if let Some(cap) = header_re.captures(lines[i]) {
                let callout_type = cap.get(1).unwrap().as_str();
                let title = cap.get(2).map(|m| m.as_str().trim()).filter(|s| !s.is_empty());

                // Extract content lines
                let mut content_lines = Vec::new();
                i += 1; // Move past header

                while i < lines.len() && lines[i].trim_start().starts_with('>') {
                    if let Some(stripped) = lines[i].trim_start().strip_prefix('>') {
                        let content_line = stripped.strip_prefix(' ').unwrap_or(stripped);
                        content_lines.push(content_line.to_string());
                    }
                    i += 1;
                }

                let callout_content = content_lines.join("\n");
                callouts.push((callout_type, title, callout_content));
            } else {
                i += 1;
            }
        }

        println!("Extracted {} callouts:", callouts.len());
        for (i, (ctype, title, content)) in callouts.iter().enumerate() {
            println!("  [{}] type='{}', title={:?}, content='{}'", i, ctype, title, content);
        }

        assert_eq!(callouts.len(), 2);
        assert_eq!(callouts[0].0, "note");
        assert_eq!(callouts[0].1, Some("Note with title"));
        assert_eq!(callouts[0].2, "First line\nSecond line\nThird line");
        assert_eq!(callouts[1].0, "warning");
        assert_eq!(callouts[1].1, None);
        assert_eq!(callouts[1].2, "Warning content\nMore warning");
    }

    /// Test the issue from the failing test - multiple callouts
    #[test]
    fn test_failing_test_case() {
        // This is the exact content from the failing parser_selection_tests.rs test
        let content = r#"> [!note]
> Simple note callout
> With multiple lines

Text between callouts.

> [!warning] Important Warning
> This is a warning with title

Another text section.

> [!tip]
> Tip without title"#;

        let header_re = Regex::new(r"(?m)^>\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\](?:\s+([^\n]*))?\s*$").unwrap();
        let lines: Vec<&str> = content.lines().collect();

        let mut callouts = Vec::new();
        let mut i = 0;

        while i < lines.len() {
            if let Some(cap) = header_re.captures(lines[i]) {
                let callout_type = cap.get(1).unwrap().as_str();
                let title = cap.get(2).map(|m| m.as_str().trim()).filter(|s| !s.is_empty());

                // Extract content lines
                let mut content_lines = Vec::new();
                i += 1; // Move past header

                while i < lines.len() && lines[i].trim_start().starts_with('>') {
                    if let Some(stripped) = lines[i].trim_start().strip_prefix('>') {
                        let content_line = stripped.strip_prefix(' ').unwrap_or(stripped);
                        content_lines.push(content_line.to_string());
                    }
                    i += 1;
                }

                let callout_content = content_lines.join("\n");
                callouts.push((callout_type, title, callout_content));
            } else {
                i += 1;
            }
        }

        println!("=== Failing test case analysis ===");
        println!("Extracted {} callouts:", callouts.len());
        for (i, (ctype, title, content)) in callouts.iter().enumerate() {
            println!("  [{}] type='{}', title={:?}, content_length={}", i, ctype, title, content.len());
        }

        // This should succeed with our logic
        assert_eq!(callouts.len(), 3, "Our logic should extract 3 callouts");
        assert_eq!(callouts[0].0, "note");
        assert_eq!(callouts[1].0, "warning");
        assert_eq!(callouts[2].0, "tip");
    }

    /// Test edge cases
    #[test]
    fn test_edge_cases() {
        let header_re = Regex::new(r"(?m)^>\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\](?:\s+([^\n]*))?\s*$").unwrap();

        let edge_cases = vec![
            // Special characters in title
            ("> [!note] Title with émojis 🎉 and spëcial chars!", "note", Some("Title with émojis 🎉 and spëcial chars!")),

            // Unicode content
            ("> [!tip] 中文标题", "tip", Some("中文标题")),

            // Hyphens in type - content after ] is treated as title by regex
            ("> [!my-custom-type] Custom content", "my-custom-type", Some("Custom content")),

            // Empty content callout
            ("> [!note] Empty callout", "note", Some("Empty callout")),
        ];

        for (input, expected_type, expected_title) in edge_cases {
            if let Some(cap) = header_re.captures(input) {
                let callout_type = cap.get(1).unwrap().as_str();
                let title = cap.get(2).map(|m| m.as_str().trim()).filter(|s| !s.is_empty());

                assert_eq!(callout_type, expected_type, "Type mismatch for: {}", input);
                assert_eq!(title, expected_title, "Title mismatch for: {}", input);
            } else {
                panic!("Failed to match edge case: {}", input);
            }
        }
    }

    /// Test performance with many callouts
    #[test]
    fn test_performance_many_callouts() {
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!(r#"> [!note] Callout {}
> This is callout number {} with some content

"#, i, i));
        }

        let header_re = Regex::new(r"(?m)^>\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\](?:\s+([^\n]*))?\s*$").unwrap();
        let start = std::time::Instant::now();

        let matches: Vec<_> = header_re.captures_iter(&content).collect();
        let duration = start.elapsed();

        println!("Found {} callout headers in {:?}", matches.len(), duration);
        assert_eq!(matches.len(), 100, "Should find all 100 callout headers");
        assert!(duration.as_millis() < 100, "Regex matching should be fast");
    }
}