//! Pulldown-cmark based markdown parser implementation

use crate::error::ParserResult;
use crate::traits::{MarkdownParser, ParserCapabilities};
use crate::types::*;
use async_trait::async_trait;
use chrono::Utc;
use pulldown_cmark::{Event, HeadingLevel, Parser as CmarkParser, Tag as CmarkTag, TagEnd};
use regex::Regex;
use std::path::Path;

/// Markdown parser using pulldown-cmark
pub struct PulldownParser {
    capabilities: ParserCapabilities,
}

impl PulldownParser {
    /// Create a new pulldown parser
    pub fn new() -> Self {
        Self {
            capabilities: ParserCapabilities {
                name: "PulldownParser",
                version: "0.1.0",
                yaml_frontmatter: true,
                toml_frontmatter: false,
                wikilinks: true,
                tags: true,
                headings: true,
                code_blocks: true,
                tables: true,
                callouts: true,
                latex_expressions: true,
                footnotes: true,
                blockquotes: true,
                horizontal_rules: true,
                full_content: true,
                max_file_size: Some(10 * 1024 * 1024), // 10 MB
                extensions: vec!["md", "markdown"],
            },
        }
    }
}

impl Default for PulldownParser {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarkdownParser for PulldownParser {
    async fn parse_file(&self, path: &Path) -> ParserResult<ParsedNote> {
        // Read file as raw bytes first (matches scanner behavior for consistent hashing)
        let bytes = tokio::fs::read(path).await?;

        // Check file size limit
        if let Some(max_size) = self.capabilities.max_file_size {
            if bytes.len() > max_size {
                return Err(super::error::ParserError::FileTooLarge {
                    size: bytes.len(),
                    max: max_size,
                });
            }
        }

        // Convert to UTF-8 string for parsing
        let content = String::from_utf8(bytes.clone()).map_err(|e| {
            super::error::ParserError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("File is not valid UTF-8: {}", e),
            ))
        })?;

        // Parse with the content (consistent hashing is handled internally)
        self.parse_content(&content, path).await
    }

    async fn parse_content(&self, content: &str, source_path: &Path) -> ParserResult<ParsedNote> {
        // For backwards compatibility, hash the content string
        let raw_bytes = content.as_bytes();

        // Extract frontmatter (YAML between --- delimiters)
        let (frontmatter, body) = extract_frontmatter(content)?;

        // Parse wikilinks with regex
        let wikilinks = extract_wikilinks(body)?;

        // Parse tags with regex
        let tags = extract_tags(body)?;

        // Parse callouts with regex
        let callouts = extract_callouts(body)?;

        // Parse LaTeX expressions with regex
        let latex_expressions = extract_latex(body)?;

        // Parse tables with regex (comprehensive extraction)
        let tables = extract_tables(body)?;

        // Parse content structure with pulldown-cmark
        let doc_content = parse_content_structure(body, tables)?;

        // Calculate content hash using BLAKE3 on raw bytes (matches FileScanner behavior)
        // This ensures parser and scanner produce identical hashes for the same file
        let mut hasher = blake3::Hasher::new();
        hasher.update(raw_bytes);
        let content_hash = hasher.finalize().to_hex().to_string();

        // Get file size from raw bytes (matches actual file size)
        let file_size = raw_bytes.len() as u64;

        Ok(ParsedNote {
            path: source_path.to_path_buf(),
            frontmatter,
            wikilinks,
            tags,
            content: doc_content,
            callouts,
            latex_expressions,
            footnotes: FootnoteMap::new(),
            parsed_at: Utc::now(),
            content_hash,
            file_size,
            parse_errors: Vec::new(),
            inline_links: Vec::new(),
            block_hashes: Vec::new(),
            merkle_root: None,
            metadata: ParsedNoteMetadata::default(),
        })
    }

    fn capabilities(&self) -> ParserCapabilities {
        self.capabilities.clone()
    }

    fn can_parse(&self, path: &Path) -> bool {
        // Check file extension
        if let Some(extension) = path.extension() {
            if let Some(ext_str) = extension.to_str() {
                if self.capabilities.extensions.contains(&ext_str) {
                    return true;
                }
            }
        }

        // Also check common markdown filenames without extensions
        if let Some(filename) = path.file_stem() {
            if let Some(name_str) = filename.to_str() {
                // Common markdown files like README, CHANGELOG, etc.
                let markdown_files = ["README", "CHANGELOG", "LICENSE", "CONTRIBUTING", "INSTALL", "TODO"];
                if markdown_files.contains(&name_str) {
                    return true;
                }
            }
        }

        false
    }
}

/// Extract YAML frontmatter from content
fn extract_frontmatter(content: &str) -> ParserResult<(Option<Frontmatter>, &str)> {
    // Check for YAML frontmatter (--- ... ---)
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end_idx) = rest.find("\n---\n") {
            let yaml = &rest[..end_idx];
            let body = &rest[end_idx + 5..];
            let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);
            return Ok((Some(fm), body));
        }
    }

    // Also handle case where frontmatter ends with ---\r\n (Windows line endings)
    if let Some(rest) = content.strip_prefix("---\r\n") {
        if let Some(end_idx) = rest.find("\r\n---\r\n") {
            let yaml = &rest[..end_idx];
            let body = &rest[end_idx + 7..];
            let fm = Frontmatter::new(yaml.to_string(), FrontmatterFormat::Yaml);
            return Ok((Some(fm), body));
        }
    }

    Ok((None, content))
}

/// Extract wikilinks from content using regex, avoiding code blocks
fn extract_wikilinks(content: &str) -> ParserResult<Vec<Wikilink>> {
    use regex::Regex;
    let re = Regex::new(r"!?\[\[([^\]]+)\]\]").unwrap();

    let mut wikilinks = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_byte_offset = 0;

    // Pre-calculate byte offsets for each line
    let mut line_offsets = Vec::new();
    for line in &lines {
        line_offsets.push(current_byte_offset);
        current_byte_offset += line.len() + 1; // +1 for newline character
    }

    for (line_idx, line) in lines.iter().enumerate() {
        // Skip if we're in a code block
        if is_in_code_block(line, &lines, line_idx) {
            continue;
        }

        // Find wikilinks in this line
        for cap in re.captures_iter(line) {
            let full_match = cap.get(0).unwrap();
            let line_offset = full_match.start();
            let offset = line_offsets[line_idx] + line_offset;
            let is_embed = full_match.as_str().starts_with('!');
            let inner = cap.get(1).unwrap().as_str();

            // Additional check: skip if this appears to be within inline code
            if is_within_inline_code(line, line_offset) {
                continue;
            }

            let link = Wikilink::parse(inner, offset, is_embed);
            wikilinks.push(link);
        }
    }

    Ok(wikilinks)
}

/// Check if a line is part of a code block
fn is_in_code_block(current_line: &str, all_lines: &[&str], line_idx: usize) -> bool {
    // Check if current line starts a code block
    if current_line.trim_start().starts_with("```") {
        return true;
    }

    // Look backwards to see if we're inside a code block
    let mut in_code_block = false;
    for i in (0..=line_idx).rev() {
        let line = all_lines[i];
        if line.trim_start().starts_with("```") {
            in_code_block = !in_code_block;
        }
    }

    in_code_block
}

/// Check if a position is within inline code (backticks)
fn is_within_inline_code(line: &str, position: usize) -> bool {
    let chars: Vec<char> = line.chars().collect();
    let mut backtick_count = 0;

    // Count backticks before the position
    for (i, &ch) in chars.iter().enumerate() {
        if i >= position {
            break;
        }
        if ch == '`' {
            backtick_count += 1;
        }
    }

    // If odd number of backticks before position, we're inside inline code
    backtick_count % 2 == 1
}

/// Extract tags from content using enhanced regex with validation
fn extract_tags(content: &str) -> ParserResult<Vec<Tag>> {
    use regex::Regex;

    // Enhanced regex pattern for tags with comprehensive character support:
    // - Letters (including Unicode)
    // - Numbers
    // - Hyphens, underscores, forward slashes
    // - Periods (for versioning like v1.0)
    // - Plus signs (for tags like c++)
    let re = Regex::new(r"#([\p{L}\p{N}][\p{L}\p{N}\-_./+]*[\p{L}\p{N}a-zA-Z0-9]?)?").unwrap();

    let mut tags = Vec::new();
    let mut seen_tags = std::collections::HashSet::new(); // Prevent duplicates

    for cap in re.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let offset = full_match.start();
        // Only process if we have a tag name (capture group 1)
        if let Some(tag_match) = cap.get(1) {
            let tag_name = tag_match.as_str();

            // Normalize tag name (lowercase, trim whitespace)
            let normalized_tag = normalize_tag_name(tag_name);

            // Enhanced filtering
            if should_include_tag(full_match.as_str(), content, offset) &&
               is_valid_tag(&normalized_tag) &&
               !seen_tags.contains(&normalized_tag) {
                seen_tags.insert(normalized_tag.clone());
                tags.push(Tag::new(&normalized_tag, offset));
            }
        }
    }

    Ok(tags)
}

/// Normalize tag name to consistent format
fn normalize_tag_name(tag: &str) -> String {
    tag.trim()
        .to_lowercase()
        .replace('_', "-") // Standardize separators
        .replace(" ", "-") // Remove spaces
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '/' || *c == '.' || *c == '+')
        .collect::<String>()
        .trim_matches(|c| c == '-' || c == '/' || c == '.' || c == '+') // Remove leading/trailing separators
        .to_string()
}

/// Enhanced validation for tag names
fn is_valid_tag(tag: &str) -> bool {
    // Empty check
    if tag.is_empty() {
        return false;
    }

    // Length limits (reasonable bounds)
    if tag.len() < 1 || tag.len() > 100 {
        return false;
    }

    // Must start with alphanumeric character
    if !tag.chars().next().unwrap_or(' ').is_alphanumeric() {
        return false;
    }

    // Valid characters check
    for ch in tag.chars() {
        if !is_valid_tag_char(ch) {
            return false;
        }
    }

    // No consecutive separators (hyphens, slashes, dots, plus)
    if tag.contains("--") || tag.contains("//") || tag.contains("..") || tag.contains("++") {
        return false;
    }

    // No leading or trailing separators (already handled in normalize but double-check)
    if tag.starts_with('-') || tag.starts_with('/') || tag.starts_with('.') || tag.starts_with('+') ||
       tag.ends_with('-') || tag.ends_with('/') || tag.ends_with('.') || tag.ends_with('+') {
        return false;
    }

    true
}

/// Check if character is valid in tag names
fn is_valid_tag_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '-' || ch == '/' || ch == '.' || ch == '+' ||
    (ch.is_ascii() && ch.is_control() == false && ch.is_whitespace() == false)
}

/// Enhanced false positive filtering for tags
fn should_include_tag(match_text: &str, content: &str, offset: usize) -> bool {
    // Skip if in code blocks (similar to wikilink logic)
    let lines: Vec<&str> = content.lines().collect();
    let line_idx = content[..offset].lines().count();
    if line_idx < lines.len() && is_in_code_block(lines[line_idx], &lines, line_idx) {
        return false;
    }

    // Skip if within inline code
    let current_line = lines.get(line_idx).map_or("", |v| v);
    let line_offset_in_content = content.lines().take(line_idx).map(|l| l.len() + 1).sum::<usize>();
    let offset_in_line = offset.saturating_sub(line_offset_in_content);
    if is_within_inline_code(current_line, offset_in_line) {
        return false;
    }

    // Enhanced context filtering
    let after_end = offset + match_text.len();
    let before_start = offset.saturating_sub(1);

    // Check character context (word boundaries)
    if after_end < content.len() {
        if let Some(next_char) = content.chars().nth(after_end) {
            if next_char.is_alphanumeric() || next_char == '_' {
                return false; // Part of a word, not a tag
            }
        }
    }

    if before_start < content.len() {
        if let Some(prev_char) = content.chars().nth(before_start) {
            if prev_char.is_alphanumeric() {
                return false; // Part of a word, not a tag
            }
        }
    }

    // Skip common false positive patterns
    let full_context = get_context_around_position(content, offset, 20);
    if looks_like_false_positive(&full_context, match_text) {
        return false;
    }

    true
}

/// Get context around a position for better false positive detection
fn get_context_around_position(content: &str, position: usize, context_size: usize) -> String {
    let start = position.saturating_sub(context_size);
    let end = (position + context_size * 2).min(content.len());
    content[start..end].to_string()
}

/// Check if context suggests this is a false positive
fn looks_like_false_positive(context: &str, tag_match: &str) -> bool {
    let context_lower = context.to_lowercase();
    let tag_lower = tag_match.to_lowercase();

    // Check for URLs that might contain # characters
    if context_lower.contains("http://") || context_lower.contains("https://") {
        return true;
    }

    // Check for CSS color codes
    if tag_match.starts_with("ff") || tag_match.starts_with("#") &&
       (context_lower.contains("color:") || context_lower.contains("background:")) {
        return true;
    }

    // Check for programming language constructs
    if context_lower.contains("define ") || context_lower.contains("const ") ||
       context_lower.contains("let ") || context_lower.contains("var ") {
        return true;
    }

    // Check for file paths with version numbers
    if tag_lower.contains("v") && (tag_lower.contains(".") || tag_lower.contains("-")) {
        if context_lower.contains(".") || context_lower.contains("file:") {
            return true;
        }
    }

    false
}

/// Extract callouts from content using regex
fn extract_callouts(content: &str) -> ParserResult<Vec<Callout>> {
    use regex::Regex;

    // Pattern matches: > [!type] optional title
    // Followed by optional continuation lines starting with >
    let re = Regex::new(r"(?m)^>\s*\[!([a-zA-Z][a-zA-Z0-9-]*)\](?:\s+([^\n]*))?\s*$").unwrap();

    let mut callouts = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;
    let mut current_byte_offset = 0;

    // Pre-calculate byte offsets for each line
    let mut line_offsets = Vec::new();
    for line in &lines {
        line_offsets.push(current_byte_offset);
        current_byte_offset += line.len() + 1; // +1 for newline character
    }

    while i < lines.len() {
        // Check if current line starts a callout
        if let Some(cap) = re.captures(lines[i]) {
            let callout_type = cap.get(1).unwrap().as_str();
            let title = cap.get(2).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty());

            // Use pre-calculated byte offset
            let offset = line_offsets[i];

            // Extract continuation lines (lines starting with > after the header)
            let mut content_lines = Vec::new();
            i += 1; // Move to next line after the callout header

            while i < lines.len() {
                let line = lines[i];
                if line.trim_start().starts_with('>') {
                    // This is a continuation line
                    if let Some(stripped) = line.trim_start().strip_prefix('>') {
                        let content_line = stripped.strip_prefix(' ').unwrap_or(stripped);
                        content_lines.push(content_line.to_string());
                    }
                    i += 1;
                } else if line.trim().is_empty() {
                    // Empty line - include as content but continue
                    content_lines.push(String::new());
                    i += 1;
                } else {
                    // Line doesn't start with > and isn't empty - end of callout
                    break;
                }
            }

            let callout_content = content_lines.join("\n");

            let callout = if let Some(title_text) = title {
                Callout::with_title(callout_type, title_text, callout_content, offset)
            } else {
                Callout::new(callout_type, callout_content, offset)
            };

            callouts.push(callout);
        } else {
            i += 1; // Move to next line
        }
    }

    Ok(callouts)
}

/// Extract LaTeX expressions from content using regex
fn extract_latex(content: &str) -> ParserResult<Vec<LatexExpression>> {
    use regex::Regex;

    let mut expressions = Vec::new();
    let mut block_ranges: Vec<(usize, usize)> = Vec::new(); // Track block math ranges

    // First handle block math: $$...$$ (multi-line support)
    // Use a state machine to find matching $$ delimiters
    let mut chars: Vec<char> = content.chars().collect();
    let mut i = 0;

    while i < chars.len() - 1 {
        if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '$' {
            // Found start of block math
            let start = i;
            i += 2; // Skip the opening $$

            // Find the closing $$
            let mut found_end = false;
            while i < chars.len() - 1 {
                if chars[i] == '$' && i + 1 < chars.len() && chars[i + 1] == '$' {
                    found_end = true;
                    break;
                }
                i += 1;
            }

            if found_end {
                let end = i + 1; // Include the closing $$
                let full_content: String = chars[start..=end].iter().collect();
                let inner_content: String = chars[start + 2..end - 1].iter().collect();
                let expr = inner_content.trim();

                // Record this block range to avoid false positive inline matches
                block_ranges.push((start, end));

                // Basic validation
                if has_balanced_braces(expr) && !has_dangerous_latex_commands(expr) {
                    expressions.push(LatexExpression::new(
                        expr.to_string(),
                        true, // is_block
                        start,
                        end - start + 1,
                    ));
                }

                i = end + 1; // Continue after the closing $$
            } else {
                break; // No closing found
            }
        } else {
            i += 1;
        }
    }

    // Then handle inline math: $...$ (but not $$)
    // Use a simpler pattern that doesn't rely on look-around
    let inline_re = Regex::new(r"\$([^\$\n]+?)\$").unwrap();
    for cap in inline_re.captures_iter(content) {
        let full_match = cap.get(0).unwrap();
        let match_str = full_match.as_str();
        let offset = full_match.start();
        let match_end = full_match.end();
        let expr = cap.get(1).unwrap().as_str().trim();

        // Check if this match falls within any block math range
        let is_within_block = block_ranges.iter().any(|(block_start, block_end)| {
            offset >= *block_start && match_end <= *block_end
        });

        // Only process if not within a block and passes validation
        if !is_within_block && has_balanced_braces(expr) && !has_dangerous_latex_commands(expr) {
            expressions.push(LatexExpression::new(
                expr.to_string(),
                false, // is_inline
                offset,
                full_match.len(),
            ));
        }
    }

    // Sort expressions by their original document order
    expressions.sort_by_key(|expr| expr.offset);

    Ok(expressions)
}

/// Check if LaTeX expression has balanced braces
fn has_balanced_braces(expr: &str) -> bool {
    let mut depth = 0;
    let mut chars = expr.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => { chars.next(); } // Skip escaped character
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth < 0 { return false; }
            }
            _ => {}
        }
    }

    depth == 0
}

/// Check for dangerous LaTeX commands
fn has_dangerous_latex_commands(expr: &str) -> bool {
    const DANGEROUS: &[&str] = &[
        "\\input", "\\include", "\\write", "\\openout", "\\closeout",
        "\\loop", "\\def", "\\edef", "\\xdef", "\\gdef", "\\let",
        "\\futurelet", "\\newcommand", "\\renewcommand", "\\catcode",
    ];

    for cmd in DANGEROUS {
        if expr.contains(cmd) {
            return true;
        }
    }
    false
}

/// Extract markdown tables with comprehensive structure analysis
fn extract_tables(content: &str) -> ParserResult<Vec<Table>> {

    let mut tables = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_byte_offset = 0;

    // Pre-calculate byte offsets for each line
    let mut line_offsets = Vec::new();
    for line in &lines {
        line_offsets.push(current_byte_offset);
        current_byte_offset += line.len() + 1; // +1 for newline character
    }

    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].trim();

        // Check if line looks like start of a table (contains pipes)
        if line.contains('|') && !line.starts_with('>') && !line.starts_with("```") {
            // Extract the full table
            if let Some(table) = extract_table_at_position(&lines, i, &line_offsets) {
                let rows = table.rows;
                tables.push(table);
                i += rows + 2; // Skip past this table (header + separator + rows)
                continue;
            }
        }
        i += 1;
    }

    Ok(tables)
}

/// Extract a complete table starting at the given position
fn extract_table_at_position(lines: &[&str], start_idx: usize, line_offsets: &[usize]) -> Option<Table> {
    if start_idx >= lines.len() {
        return None;
    }

    // Find header line (first line with pipes)
    let mut header_idx = start_idx;
    while header_idx < lines.len() && !lines[header_idx].trim().contains('|') {
        header_idx += 1;
    }

    if header_idx >= lines.len() {
        return None;
    }

    // Check for separator line (second line with pipes, dashes, and colons)
    if header_idx + 1 >= lines.len() {
        return None;
    }

    let separator_line = lines[header_idx + 1].trim();
    if !separator_line.contains('|') || !separator_line.contains('-') {
        return None;
    }

    // Extract headers
    let header_line = lines[header_idx].trim();
    let headers = parse_table_row(header_line);

    // Validate separator alignment
    if !validate_table_separator(separator_line, headers.len()) {
        return None;
    }

    // Count and extract data rows
    let mut rows = 0;
    let mut raw_content_lines = vec![header_line, separator_line];
    let mut i = header_idx + 2;

    while i < lines.len() {
        let line = lines[i].trim();
        if line.is_empty() || !line.contains('|') {
            break; // End of table
        }

        // Validate row has right number of columns (approximately)
        let row_cells = parse_table_row(line);
        if row_cells.len() == headers.len() || (row_cells.len() > headers.len() && headers.len() == 1) {
            rows += 1;
            raw_content_lines.push(line);
        } else {
            break; // Invalid table structure
        }
        i += 1;
    }

    if rows == 0 {
        return None; // Table with no data rows
    }

    let raw_content = raw_content_lines.join("\n");
    let offset = line_offsets[header_idx];

    Some(Table::new(
        raw_content,
        headers.clone(),
        headers.len(),
        rows,
        offset,
    ))
}

/// Parse a table row into cells, handling empty cells
fn parse_table_row(row: &str) -> Vec<String> {
    let row = row.trim_start_matches('|').trim_end_matches('|').trim();

    if row.is_empty() {
        return vec!["".to_string()];
    }

    let mut cells = Vec::new();
    let mut current_cell = String::new();
    let mut chars = row.chars().peekable();
    let mut in_escape = false;

    while let Some(ch) = chars.next() {
        match ch {
            '\\' => {
                in_escape = true;
                if let Some(next_ch) = chars.peek() {
                    current_cell.push(*next_ch);
                    chars.next(); // Skip the escaped character
                    in_escape = false;
                }
            }
            '|' if !in_escape => {
                cells.push(current_cell.trim().to_string());
                current_cell.clear();
            }
            _ => {
                current_cell.push(ch);
                in_escape = false;
            }
        }
    }

    // Add the last cell
    cells.push(current_cell.trim().to_string());

    // Handle case where row has no separators (single cell)
    if cells.len() == 1 && cells[0].is_empty() && row.contains('|') {
        // Re-parse as empty cells between pipes
        let parts: Vec<&str> = row.split('|').collect();
        cells = parts.iter().map(|s| s.trim().to_string()).collect();
    }

    cells
}

/// Validate table separator line (contains proper dash/colon alignment)
fn validate_table_separator(separator: &str, expected_cols: usize) -> bool {
    let cells = parse_table_row(separator);

    if cells.len() != expected_cols {
        return false;
    }

    // Each cell should contain at least one dash
    for cell in &cells {
        let trimmed = cell.trim_matches(':').trim();
        if !trimmed.chars().any(|c| c == '-') {
            return false;
        }

        // Validate alignment syntax (only : and - allowed)
        for ch in cell.chars() {
            if ch != ':' && ch != '-' && ch != ' ' {
                return false;
            }
        }
    }

    true
}

/// Extract comprehensive list structures using enhanced regex-based analysis
fn extract_lists_comprehensive(content: &str) -> ParserResult<Vec<ListBlock>> {

    let mut lists = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut current_byte_offset = 0;

    // Pre-calculate byte offsets for each line for performance
    let mut line_offsets = Vec::with_capacity(lines.len());
    for line in &lines {
        line_offsets.push(current_byte_offset);
        current_byte_offset += line.len() + 1; // +1 for newline character
    }

    // Standard Markdown patterns with better validation
    let unordered_patterns = vec![
        (r"^(\s*)-\s+(.+)$", ListMarkerStyle::Dash),
        (r"^(\s*)\*\s+(.+)$", ListMarkerStyle::Asterisk),
        (r"^(\s*)\+\s+(.+)$", ListMarkerStyle::Plus),
    ];

    let ordered_patterns = vec![
        (r"^(\s*)(\d+)\.\s+(.+)$", ListMarkerStyle::Arabic),
    ];

    // Standard task patterns (GitHub Flavored Markdown)
    let task_patterns = vec![
        (r"^(\s*)[-*+]\s+\[([ xX])\]\s+(.+)$", false), // unordered tasks
        (r"^(\s*)(\d+)\.\s+\[([ xX])\]\s+(.+)$", true), // ordered tasks
    ];

    // Pre-compile regex patterns for performance
    let mut compiled_patterns = Vec::new();
    for (pattern, style) in unordered_patterns.iter() {
        compiled_patterns.push((Regex::new(pattern).unwrap(), *style, false));
    }
    for (pattern, style) in ordered_patterns.iter() {
        compiled_patterns.push((Regex::new(pattern).unwrap(), *style, true));
    }
    for (pattern, is_ordered) in task_patterns.iter() {
        compiled_patterns.push((Regex::new(pattern).unwrap(), ListMarkerStyle::Dash, *is_ordered));
    }

    let mut i = 0;
    let mut processed_indices = std::collections::HashSet::new();

    while i < lines.len() {
        // Skip if already processed as part of another list
        if processed_indices.contains(&i) {
            i += 1;
            continue;
        }

        let line = lines[i];
        let trimmed = line.trim();

        // Enhanced filtering for non-list content
        if should_skip_line_for_lists(line, &lines, i) {
            i += 1;
            continue;
        }

        // Try to match against all patterns with enhanced scoring
        let mut best_match: Option<ListMatch> = None;
        let mut best_score = 0;

        for (regex, marker_style, is_ordered) in &compiled_patterns {
            if let Some(cap) = regex.captures(line) {
                let match_info = parse_list_match(&cap, *marker_style, *is_ordered, line);
                let score = calculate_match_score(&match_info, line, &lines, i);

                if score > best_score {
                    best_score = score;
                    best_match = Some(match_info);
                }
            }
        }

        if let Some(list_match) = best_match {
            // We found a list item, now extract the complete list with enhanced detection
            if let Some(list) = extract_complete_list_enhanced(
                &lines,
                line_offsets.as_slice(),
                i,
                &list_match,
                &mut processed_indices
            ) {
                let list_items = list.items.clone(); // Clone to avoid move issues
                lists.push(list);

                // Mark all processed indices and skip ahead
                for item in &list_items {
                    let item_line_idx = find_line_index_for_offset(line_offsets.as_slice(), item.offset);
                    if let Some(idx) = item_line_idx {
                        processed_indices.insert(idx);
                    }
                }

                i += list_items.len();
                continue;
            }
        }

        i += 1;
    }

    Ok(lists)
}

/// Helper structure to hold list match information
#[derive(Debug, Clone)]
struct ListMatch {
    level: usize,
    marker_style: ListMarkerStyle,
    marker_text: String,
    content: String,
    indent_spaces: usize,
    sequence_number: Option<String>,
    task_status: Option<TaskStatus>,
    is_task: bool,
}

/// Parse regex capture into structured list match information
fn parse_list_match(
    cap: &regex::Captures,
    marker_style: ListMarkerStyle,
    is_ordered: bool,
    line: &str,
) -> ListMatch {
    let indent_match = cap.get(1).unwrap();
    let indent_spaces = indent_match.as_str().len();
    let level = calculate_indentation_level(indent_spaces);

    let (marker_text, content, sequence_number, task_status, is_task) =
        extract_marker_and_content(&cap, marker_style, is_ordered, line);

    ListMatch {
        level,
        marker_style,
        marker_text,
        content,
        indent_spaces,
        sequence_number,
        task_status,
        is_task,
    }
}

/// Calculate indentation level from spaces (supports both tabs and spaces)
fn calculate_indentation_level(indent_spaces: usize) -> usize {
    // Common markdown conventions:
    // - 2 spaces per level for nested lists
    // - 4 spaces per level for code blocks (we're not in code blocks here)
    // - 1 tab = 2-4 spaces depending on editor, but we use 2 as standard
    indent_spaces / 2
}

/// Extract marker text, content, and optional sequence/task info from regex capture
fn extract_marker_and_content(
    cap: &regex::Captures,
    marker_style: ListMarkerStyle,
    is_ordered: bool,
    line: &str,
) -> (String, String, Option<String>, Option<TaskStatus>, bool) {
    let full_match = cap.get(0).unwrap();
    let content_start = full_match.end();
    let content = line[content_start..].trim().to_string();

    if is_task_item(&cap) {
        // Handle task items
        let checkbox = cap.get(2).unwrap().as_str().trim();
        let task_status = if checkbox == "x" || checkbox == "X" {
            TaskStatus::Completed
        } else {
            TaskStatus::Pending
        };

        let marker_text = extract_task_marker_text(&cap, line);
        let sequence_number = if is_ordered {
            extract_sequence_number_for_task(&cap, line)
        } else {
            None
        };

        (marker_text, content, sequence_number, Some(task_status), true)
    } else {
        // Handle regular list items
        let (marker_text, sequence_number) = if is_ordered {
            let number = cap.get(2).unwrap().as_str();
            let delimiter = if line.contains(number) && line.contains(")") { ")" } else { "." };
            let marker_text = format!("{}{}", number, delimiter);
            (marker_text, Some(number.to_string()))
        } else {
            let marker_char = match marker_style {
                ListMarkerStyle::Dash => "-",
                ListMarkerStyle::Asterisk => "*",
                ListMarkerStyle::Plus => "+",
                ListMarkerStyle::Arabic => "1.", // Default for Arabic
            };
            (marker_char.to_string(), None)
        };

        (marker_text, content, sequence_number, None, false)
    }
}

/// Check if this capture represents a task item
fn is_task_item(cap: &regex::Captures) -> bool {
    cap.len() >= 3 && cap.get(2).map_or(false, |m| {
        let text = m.as_str();
        text == "[x]" || text == "[ ]" || text == "[X]" || text == "[x]" || text == "[ X]"
    })
}

/// Extract marker text for task items
fn extract_task_marker_text(cap: &regex::Captures, line: &str) -> String {
    let full_match = cap.get(0).unwrap().as_str();
    let bracket_end = full_match.find("[").unwrap_or(0);
    full_match[..bracket_end].trim().to_string()
}

/// Extract sequence number for ordered task items
fn extract_sequence_number_for_task(cap: &regex::Captures, _line: &str) -> Option<String> {
    cap.get(2).map(|m| m.as_str().to_string())
}

/// Calculate match score to determine best pattern match
fn calculate_match_score(match_info: &ListMatch, line: &str, lines: &[&str], line_idx: usize) -> usize {
    let mut score = 100; // Base score for any match

    // Bonus for proper indentation (multiple of 2)
    if match_info.indent_spaces % 2 == 0 {
        score += 10;
    }

    // Bonus for content that doesn't look like other markdown elements
    if !looks_like_non_list_content(&match_info.content) {
        score += 15;
    }

    // Bonus for consistent list context (previous/next lines are also list items)
    if has_list_context(lines, line_idx) {
        score += 20;
    }

    // Bonus for proper marker format
    if has_proper_marker_format(&match_info.marker_text, &match_info.marker_style) {
        score += 10;
    }

    // Penalty for lines that might be misidentified
    if looks_like_misidentified_list(line) {
        score -= 50;
    }

    score.max(0)
}

/// Check if line should be skipped during list detection
fn should_skip_line_for_lists(line: &str, lines: &[&str], line_idx: usize) -> bool {
    let trimmed = line.trim();

    // Skip empty lines
    if trimmed.is_empty() {
        return true;
    }

    // Skip code blocks and other markdown structures
    if is_in_code_block(line, lines, line_idx) ||
       line.starts_with('>') ||
       line.trim_start().starts_with("```") ||
       trimmed.starts_with("---") ||
       trimmed.starts_with("===") {
        return true;
    }

    // Skip lines that look like headers
    if trimmed.starts_with('#') {
        return true;
    }

    // Skip lines that look like blockquotes that might be confused with lists
    if line.starts_with("> ") || line.starts_with(">>") {
        return true;
    }

    false
}

/// Check if content looks like non-list markdown content
fn looks_like_non_list_content(content: &str) -> bool {
    let trimmed = content.trim();

    // Check for common markdown patterns that aren't list content
    trimmed.starts_with('#') ||           // Header
    trimmed.starts_with('>') ||           // Blockquote
    trimmed.starts_with("```") ||         // Code block
    trimmed.starts_with("---") ||         // Horizontal rule
    trimmed.starts_with("===") ||         // Horizontal rule
    (trimmed.starts_with('[') && trimmed.contains("]: ")) || // Reference link
    (trimmed.starts_with('!') && trimmed.contains('['))      // Image
}

/// Check if surrounding lines suggest list context
fn has_list_context(lines: &[&str], line_idx: usize) -> bool {
    let mut list_context_count = 0;

    // Check previous lines
    for offset in 1..=3 {
        if let Some(prev_line) = lines.get(line_idx.saturating_sub(offset)) {
            if looks_like_list_item(prev_line) {
                list_context_count += 1;
            }
        }
    }

    // Check next lines
    for offset in 1..=3 {
        if let Some(next_line) = lines.get(line_idx + offset) {
            if looks_like_list_item(next_line) {
                list_context_count += 1;
            }
        }
    }

    list_context_count >= 1
}

/// Check if a line looks like a standard Markdown list item
fn looks_like_list_item(line: &str) -> bool {
    let trimmed = line.trim();
    let mut chars = trimmed.chars().peekable();

    // Skip leading whitespace
    while let Some(&' ') = chars.peek() {
        chars.next();
    }

    // Check for standard Markdown list markers
    if let Some(first_char) = chars.next() {
        match first_char {
            '-' | '*' | '+' => {
                // Check if followed by space
                chars.next().map_or(false, |c| c == ' ')
            },
            '0'..='9' => {
                // Check if followed by . and then space (standard ordered lists)
                if let Some(second_char) = chars.next() {
                    second_char == '.' && chars.next().map_or(false, |c| c == ' ')
                } else {
                    false
                }
            },
            _ => false,
        }
    } else {
        false
    }
}

/// Check if marker text has proper format for standard Markdown styles
fn has_proper_marker_format(marker_text: &str, marker_style: &ListMarkerStyle) -> bool {
    match marker_style {
        ListMarkerStyle::Dash => marker_text == "-",
        ListMarkerStyle::Asterisk => marker_text == "*",
        ListMarkerStyle::Plus => marker_text == "+",
        ListMarkerStyle::Arabic => {
            marker_text.len() >= 2 &&
            marker_text.chars().nth(marker_text.len() - 1) == Some('.') &&
            marker_text[..marker_text.len() - 1].chars().all(|c| c.is_ascii_digit())
        },
    }
}

/// Check if line might be misidentified as a list
fn looks_like_misidentified_list(line: &str) -> bool {
    let trimmed = line.trim();

    // Check for common patterns that look like lists but aren't
    trimmed.contains("http") || // URLs
    trimmed.contains("://") || // Other protocols
    (trimmed.starts_with('-') && trimmed.len() > 2 && !trimmed.contains(" ")) || // Dashes in text
    (trimmed.starts_with('*') && trimmed.contains("**")) || // Bold text
    (trimmed.contains("/*") || trimmed.contains("*/")) // Comments
}

/// Find line index for a given byte offset
fn find_line_index_for_offset(line_offsets: &[usize], target_offset: usize) -> Option<usize> {
    line_offsets.iter()
        .enumerate()
        .find(|(_, &offset)| offset == target_offset)
        .map(|(idx, _)| idx)
}

/// Enhanced function to extract a complete list with improved nesting and validation
fn extract_complete_list_enhanced(
    lines: &[&str],
    line_offsets: &[usize],
    start_idx: usize,
    initial_match: &ListMatch,
    processed_indices: &mut std::collections::HashSet<usize>,
) -> Option<ListBlock> {
    let mut items = Vec::new();
    let mut raw_content_lines = Vec::new();
    let mut i = start_idx;
    let mut list_type = if initial_match.marker_style.is_ordered() { ListType::Ordered } else { ListType::Unordered };
    let mut has_tasks = initial_match.is_task;
    let mut max_depth = 0;
    let mut top_level_count = 0;
    let mut primary_marker = initial_match.marker_style;

    // Enhanced patterns for comprehensive list item matching
    let patterns = compile_enhanced_list_patterns();

    // Add the first item from the initial match
    let first_item = create_list_item_from_match(
        &initial_match,
        line_offsets[i],
        &mut has_tasks,
    );
    items.push(first_item);
    raw_content_lines.push(lines[i]);
    max_depth = initial_match.level;
    if initial_match.level == 0 {
        top_level_count = 1;
    }

    i += 1; // Move past the first item

    // Continue extracting items while they belong to the same logical list
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Enhanced list boundary detection
        if should_stop_list_extraction(line, &lines, i, &items, list_type) {
            break;
        }

        // Try to parse the current line as a list item
        if let Some(list_item) = parse_enhanced_list_item(
            line,
            line_offsets[i],
            &patterns,
            &items,
            &mut has_tasks,
            &mut primary_marker,
        ) {
            items.push(list_item);
            raw_content_lines.push(line);
            processed_indices.insert(i);

            // Update depth statistics
            max_depth = max_depth.max(items.last().unwrap().level);
            if items.last().unwrap().level == 0 {
                top_level_count += 1;
            }
        } else {
            // Line doesn't match list patterns, but might be continuation
            if looks_like_list_continuation(line, &items) {
                raw_content_lines.push(line);
                processed_indices.insert(i);
            } else {
                break;
            }
        }

        i += 1;
    }

    if items.is_empty() {
        return None;
    }

    // Enhanced list analysis and validation
    validate_and_enhance_list_structure(&mut items);

    // Calculate list tightness with improved logic
    let is_tight = calculate_list_tightness_enhanced(&raw_content_lines, &items);

    // Update nested flags with improved detection
    update_nested_flags_enhanced(&mut items);

    let raw_content = raw_content_lines.join("\n");
    let offset = line_offsets[start_idx];
    let item_count = items.len(); // Calculate before moving items

    Some(ListBlock {
        list_type,
        items,
        offset,
        item_count,
        max_depth,
        marker_style: primary_marker,
        has_tasks,
        is_tight,
        raw_content,
        top_level_count,
    })
}

/// Compile standard Markdown regex patterns for list detection
fn compile_enhanced_list_patterns() -> Vec<(regex::Regex, ListMarkerStyle, bool)> {
    vec![
        // Standard unordered patterns
        (regex::Regex::new(r"^(\s*)-(.+)$").unwrap(), ListMarkerStyle::Dash, false),
        (regex::Regex::new(r"^(\s*)\*(.+)$").unwrap(), ListMarkerStyle::Asterisk, false),
        (regex::Regex::new(r"^(\s*)\+(.+)$").unwrap(), ListMarkerStyle::Plus, false),

        // Standard ordered patterns (Arabic numerals only)
        (regex::Regex::new(r"^(\s*)(\d+)\.(.+)$").unwrap(), ListMarkerStyle::Arabic, true),

        // Standard task patterns (GitHub Flavored Markdown)
        (regex::Regex::new(r"^(\s*)[-*+]\s+\[([ xX])\]\s+(.+)$").unwrap(), ListMarkerStyle::Dash, false),
        (regex::Regex::new(r"^(\s*)(\d+)\.\s+\[([ xX])\]\s+(.+)$").unwrap(), ListMarkerStyle::Arabic, true),
    ]
}

/// Create a ListItem from a ListMatch
fn create_list_item_from_match(
    list_match: &ListMatch,
    offset: usize,
    has_tasks: &mut bool,
) -> ListItem {
    let mut item = ListItem::with_metadata(
        list_match.content.clone(),
        list_match.level,
        list_match.marker_style,
        list_match.marker_text.clone(),
        list_match.sequence_number.clone(),
        offset,
        list_match.indent_spaces,
    );

    if let Some(task_status) = list_match.task_status {
        item.task_status = Some(task_status);
        *has_tasks = true;
    }

    item
}

/// Enhanced parsing of list items with better validation
fn parse_enhanced_list_item(
    line: &str,
    offset: usize,
    patterns: &[(regex::Regex, ListMarkerStyle, bool)],
    existing_items: &[ListItem],
    has_tasks: &mut bool,
    primary_marker: &mut ListMarkerStyle,
) -> Option<ListItem> {
    for (regex, marker_style, is_ordered) in patterns {
        if let Some(cap) = regex.captures(line) {
            let indent_spaces = cap.get(1).map_or(0, |m| m.as_str().len());
            let level = calculate_indentation_level(indent_spaces);

            // Enhanced validation: check if this level makes sense in context
            if !is_valid_list_level(level, existing_items) {
                continue;
            }

            let (marker_text, content, sequence_number, task_status) =
                extract_list_item_details(&cap, *marker_style, *is_ordered, line);

            // Update primary marker if this is a different style but compatible
            update_primary_marker_if_compatible(primary_marker, *marker_style);

            let mut item = ListItem::with_metadata(
                content,
                level,
                *marker_style,
                marker_text,
                sequence_number,
                offset,
                indent_spaces,
            );

            if let Some(status) = task_status {
                item.task_status = Some(status);
                *has_tasks = true;
            }

            return Some(item);
        }
    }

    None
}

/// Check if a list level is valid in the context of existing items
fn is_valid_list_level(level: usize, existing_items: &[ListItem]) -> bool {
    if existing_items.is_empty() {
        return true; // First item can be any level (treated as level 0)
    }

    let last_item = existing_items.last().unwrap();

    // Level should not jump more than 1 at a time
    if level > last_item.level + 1 {
        return false;
    }

    // Level should not go negative
    if level > last_item.level && existing_items.iter().any(|item| item.level < level) {
        // This is a nested level, check if it has a parent
        return existing_items.iter().rev().any(|item| item.level == level - 1);
    }

    true
}

/// Extract details from regex capture for list items
fn extract_list_item_details(
    cap: &regex::Captures,
    marker_style: ListMarkerStyle,
    is_ordered: bool,
    line: &str,
) -> (String, String, Option<String>, Option<TaskStatus>) {
    let content_part = cap.get(cap.len() - 1).unwrap().as_str().trim();

    // Check if this is a task item
    let (content, task_status) = if cap.len() >= 4 && cap.get(2).map_or(false, |m| {
        let text = m.as_str();
        text == "[x]" || text == "[ ]" || text == "[X]"
    }) {
        let checkbox = cap.get(2).unwrap().as_str();
        let task_status = if checkbox.trim() == "x" || checkbox.trim() == "X" {
            TaskStatus::Completed
        } else {
            TaskStatus::Pending
        };

        // For task items, content is in the last capture group
        let content = if cap.len() >= 5 {
            cap.get(4).unwrap().as_str().trim().to_string()
        } else {
            content_part.to_string()
        };

        (content, Some(task_status))
    } else {
        (content_part.to_string(), None)
    };

    let (marker_text, sequence_number) = if is_ordered {
        if let Some(number_match) = cap.get(2) {
            let number = number_match.as_str();
            let delimiter = if line.contains(')') { ")" } else { "." };
            let marker_text = format!("{}{}", number, delimiter);
            (marker_text, Some(number.to_string()))
        } else {
            // Fallback for edge cases - should never happen with standard markdown
            let marker_char = match marker_style {
                ListMarkerStyle::Dash => "-",
                ListMarkerStyle::Asterisk => "*",
                ListMarkerStyle::Plus => "+",
                ListMarkerStyle::Arabic => "1.",
            };
            (marker_char.to_string(), None)
        }
    } else {
        let marker_char = match marker_style {
            ListMarkerStyle::Dash => "-",
            ListMarkerStyle::Asterisk => "*",
            ListMarkerStyle::Plus => "+",
            ListMarkerStyle::Arabic => "1.", // Default for Arabic
        };
        (marker_char.to_string(), None)
    };

    (marker_text, content, sequence_number, task_status)
}

/// Update primary marker if the new marker is compatible with the current list
fn update_primary_marker_if_compatible(primary_marker: &mut ListMarkerStyle, new_marker: ListMarkerStyle) {
    // Only update if markers are compatible (both ordered or both unordered)
    if primary_marker.is_ordered() == new_marker.is_ordered() {
        *primary_marker = new_marker;
    }
}

/// Enhanced check for when to stop list extraction
fn should_stop_list_extraction(
    line: &str,
    lines: &[&str],
    line_idx: usize,
    items: &[ListItem],
    list_type: ListType,
) -> bool {
    let trimmed = line.trim();

    // Stop on empty lines (but allow single empty lines within loose lists)
    if trimmed.is_empty() {
        // Check if this is a single empty line or multiple
        let consecutive_empty = count_consecutive_empty_lines(lines, line_idx);
        return consecutive_empty > 1;
    }

    // Stop on headers
    if trimmed.starts_with('#') {
        return true;
    }

    // Stop on code blocks
    if line.trim_start().starts_with("```") {
        return true;
    }

    // Stop on horizontal rules
    if trimmed.starts_with("---") || trimmed.starts_with("===") {
        return true;
    }

    // Stop on blockquotes that aren't continuation of list items
    if line.starts_with(">") && !is_blockquote_list_continuation(line, items) {
        return true;
    }

    // Stop if we encounter a different list type that doesn't fit the hierarchy
    if let Some(last_item) = items.last() {
        if looks_like_incompatible_list_item(line, last_item, list_type) {
            return true;
        }
    }

    false
}

/// Count consecutive empty lines starting from a given index
fn count_consecutive_empty_lines(lines: &[&str], start_idx: usize) -> usize {
    let mut count = 0;
    let mut i = start_idx;

    while i < lines.len() {
        if lines[i].trim().is_empty() {
            count += 1;
            i += 1;
        } else {
            break;
        }
    }

    count
}

/// Check if a blockquote is a continuation of a list item
fn is_blockquote_list_continuation(line: &str, items: &[ListItem]) -> bool {
    // This is a simplified check - in a more complex implementation,
    // you might want to track if the current list item started with a blockquote
    line.starts_with("> ") && items.iter().any(|item| item.content.starts_with("> "))
}

/// Check if a line looks like an incompatible list item (standard Markdown only)
fn looks_like_incompatible_list_item(line: &str, last_item: &ListItem, list_type: ListType) -> bool {
    if !looks_like_list_item(line) {
        return false;
    }

    let indent_spaces = line.len() - line.trim_start().len();
    let level = calculate_indentation_level(indent_spaces);

    // If the indentation doesn't make sense in the context of the last item
    if level > last_item.level + 1 {
        return true;
    }

    // If it's a completely different list type at the same level (standard Markdown only)
    let line_is_ordered = line.chars().any(|c| c.is_ascii_digit()) && line.contains('.');

    if level == last_item.level && line_is_ordered != list_type.is_ordered() {
        return true;
    }

    false
}

/// Check if a line looks like a continuation of the current list item
fn looks_like_list_continuation(line: &str, items: &[ListItem]) -> bool {
    if items.is_empty() {
        return false;
    }

    let last_item = items.last().unwrap();
    let indent_spaces = line.len() - line.trim_start().len();

    // Continuation should have more indentation than the last list item
    // but less than or equal to what would be expected for a nested item
    indent_spaces > last_item.indent_spaces &&
    indent_spaces <= (last_item.level + 1) * 2 + 1
}

/// Validate and enhance list structure
fn validate_and_enhance_list_structure(items: &mut Vec<ListItem>) {
    // Ensure consistent levels and fix common issues
    for i in 1..items.len() {
        let current_level = items[i].level;
        let previous_level = items[i - 1].level;

        // Fix invalid level jumps
        if current_level > previous_level + 1 {
            items[i].level = previous_level + 1;
            items[i].indent_spaces = items[i].level * 2;
        }
    }

    // Normalize marker styles within the list when appropriate
    normalize_marker_styles(items);
}

/// Normalize marker styles for consistency within the list
fn normalize_marker_styles(items: &mut [ListItem]) {
    if items.is_empty() {
        return;
    }

    // Find the most common marker style for each level
    let mut level_markers = std::collections::HashMap::new();

    for item in items.iter() {
        let entry = level_markers.entry(item.level).or_insert((ListMarkerStyle::Dash, 0));
        entry.1 += 1;
        if entry.1 == 1 {
            entry.0 = item.marker;
        }
    }

    // Apply the most common style to inconsistent items (optional enhancement)
    // This is a conservative approach - we don't force normalization unless it's clearly wrong
}

/// Calculate list tightness with enhanced logic
fn calculate_list_tightness_enhanced(raw_content_lines: &[&str], items: &[ListItem]) -> bool {
    let mut has_blank_lines = false;

    // Check for blank lines between items at the same level
    for (i, line) in raw_content_lines.iter().enumerate() {
        if line.trim().is_empty() {
            has_blank_lines = true;

            // Check if this blank line separates items at the same level
            if i > 0 && i < raw_content_lines.len() - 1 {
                let prev_line = raw_content_lines[i - 1];
                let next_line = raw_content_lines[i + 1];

                if let (Some(prev_item), Some(next_item)) = find_items_for_lines(prev_line, next_line, items) {
                    if prev_item.level == next_item.level {
                        return false; // List is loose
                    }
                }
            }
        }
    }

    !has_blank_lines
}

/// Find items corresponding to lines for tightness calculation
fn find_items_for_lines<'a>(prev_line: &'a str, next_line: &'a str, items: &'a [ListItem]) -> (Option<&'a ListItem>, Option<&'a ListItem>) {
    // This is a simplified implementation - in a more sophisticated version,
    // you'd track line-to-item mappings during parsing
    let prev_indent = prev_line.len() - prev_line.trim_start().len();
    let next_indent = next_line.len() - next_line.trim_start().len();

    let prev_level = calculate_indentation_level(prev_indent);
    let next_level = calculate_indentation_level(next_indent);

    let prev_item = items.iter().find(|item| item.level == prev_level);
    let next_item = items.iter().find(|item| item.level == next_level);

    (prev_item, next_item)
}

/// Update nested flags with enhanced detection
fn update_nested_flags_enhanced(items: &mut [ListItem]) {
    for i in 0..items.len() {
        let current_level = items[i].level;

        // Check if there are any items at a deeper level after this item
        let has_nested = items.iter().skip(i + 1)
            .take_while(|item| item.level >= current_level)
            .any(|item| item.level > current_level);

        items[i].set_nested(has_nested);
    }
}

/// Comprehensive language mapping and normalization for code block detection
/// Returns normalized language name or None if invalid/unrecognized
fn normalize_code_block_language(lang_input: &str) -> Option<String> {
    if lang_input.is_empty() {
        return None;
    }

    // Extract the primary language (handle cases like "javascript,rust" or "python {data-raw=true}")
    let primary_lang = extract_primary_language(lang_input);

    // Normalize to lowercase and trim whitespace
    let normalized = primary_lang.trim().to_lowercase();

    // Check for security/concerning patterns
    if contains_suspicious_patterns(&normalized) {
        return None;
    }

    // Map common aliases and variations to canonical names
    match normalized.as_str() {
        // Web technologies
        "js" | "jsx" | "javascript" | "node" | "nodejs" => Some("javascript".to_string()),
        "ts" | "tsx" | "typescript" => Some("typescript".to_string()),
        "html" | "htm" | "html5" | "xhtml" => Some("html".to_string()),
        "css" | "css3" | "scss" | "sass" | "less" | "stylus" => Some("css".to_string()),
        "vue" | "vuejs" => Some("vue".to_string()),
        "svelte" | "sveltejs" => Some("svelte".to_string()),

        // Programming languages
        "py" | "python" | "python3" | "py3" => Some("python".to_string()),
        "rs" | "rust" | "rustlang" => Some("rust".to_string()),
        "go" | "golang" => Some("go".to_string()),
        "java" | "java8" | "java11" | "java17" => Some("java".to_string()),
        "c" | "c89" | "c99" | "c11" | "c17" => Some("c".to_string()),
        "cpp" | "c++" | "cxx" | "cc" => Some("cpp".to_string()),
        "cs" | "csharp" | "c#" | "dotnet" => Some("csharp".to_string()),
        "php" | "php7" | "php8" => Some("php".to_string()),
        "rb" | "ruby" | "ruby2" | "ruby3" => Some("ruby".to_string()),
        "swift" | "swift5" => Some("swift".to_string()),
        "kt" | "kotlin" | "kts" => Some("kotlin".to_string()),
        "scala" | "scala2" | "scala3" => Some("scala".to_string()),
        "dart" | "dartlang" => Some("dart".to_string()),
        "lua" | "lua5" => Some("lua".to_string()),
        "r" | "rlang" => Some("r".to_string()),
        "matlab" | "octave" => Some("matlab".to_string()),
        "perl" | "perl5" | "perl6" | "raku" => Some("perl".to_string()),

        // Data formats
        "json" | "json5" | "jsonc" => Some("json".to_string()),
        "yaml" | "yml" => Some("yaml".to_string()),
        "toml" => Some("toml".to_string()),
        "xml" | "xhtml" | "svg" => Some("xml".to_string()),
        "csv" | "tsv" => Some("csv".to_string()),
        "sql" | "mysql" | "postgresql" | "postgres" | "sqlite" | "oracle" => Some("sql".to_string()),

        // Shell and scripting
        "sh" | "bash" | "zsh" | "fish" | "shell" | "shellscript" => Some("bash".to_string()),
        "ps1" | "powershell" | "pwsh" => Some("powershell".to_string()),
        "bat" | "batch" | "cmd" => Some("batch".to_string()),

        // Configuration files
        "dockerfile" | "docker" => Some("dockerfile".to_string()),
        "make" | "makefile" | "gnu-make" => Some("makefile".to_string()),
        "ini" | "cfg" | "conf" | "config" => Some("ini".to_string()),
        "env" | "dotenv" => Some("env".to_string()),

        // Documentation and markup
        "md" | "markdown" | "mdown" | "mkd" => Some("markdown".to_string()),
        "tex" | "latex" => Some("latex".to_string()),
        "rst" | "restructuredtext" => Some("rst".to_string()),
        "adoc" | "asciidoc" | "asciidoctor" => Some("asciidoc".to_string()),

        // Version control
        "gitignore" | "git" | "diff" | "patch" => Some("git".to_string()),

        // Build tools
        "gradle" | "groovy" => Some("gradle".to_string()),
        "maven" | "pom" => Some("maven".to_string()),
        "npm" | "package.json" => Some("npm".to_string()),
        "yarn" | "yarn.lock" => Some("yarn".to_string()),
        "pip" | "requirements.txt" => Some("pip".to_string()),
        "cargo" | "cargo.toml" => Some("cargo".to_string()),

        // Web infrastructure
        "nginx" | "nginx.conf" => Some("nginx".to_string()),
        "apache" | "httpd" | ".htaccess" => Some("apache".to_string()),

        // Databases
        "mongodb" | "mongo" => Some("mongodb".to_string()),
        "redis" => Some("redis".to_string()),

        // Other common languages
        "plaintext" | "text" | "txt" => Some("plaintext".to_string()),
        "log" | "logs" => Some("log".to_string()),
        "diff" | "patch" => Some("diff".to_string()),
        "regex" | "regexp" => Some("regex".to_string()),

        // If the language is already in a good format, return it as-is
        _ => {
            // Additional validation for unknown languages
            if is_valid_language_format(&normalized) {
                Some(normalized)
            } else {
                None
            }
        }
    }
}

/// Extract the primary language from a potentially complex language specification
fn extract_primary_language(lang_input: &str) -> &str {
    // Handle cases like "javascript,rust" - take the first part
    if let Some(comma_pos) = lang_input.find(',') {
        return &lang_input[..comma_pos];
    }

    // Handle cases like "python {data-raw=true}" - extract before first space or brace
    if let Some(space_pos) = lang_input.find(' ') {
        return &lang_input[..space_pos];
    }
    if let Some(brace_pos) = lang_input.find('{') {
        return &lang_input[..brace_pos];
    }

    lang_input
}

/// Check for suspicious or potentially malicious patterns in language tags
fn contains_suspicious_patterns(lang: &str) -> bool {
    // Check for dangerous patterns that could indicate injection attempts
    let suspicious_patterns = [
        "javascript:",
        "data:",
        "vbscript:",
        "file:",
        "ftp:",
        "http:",
        "https:",
        "<script",
        "</script",
        "onclick",
        "onload",
        "onerror",
        "eval(",
        "alert(",
        "document.",
        "window.",
        "global.",
        "process.",
        "require(",
        "import(",
        "exec(",
        "system(",
        "shell_exec(",
        "`", // backticks
        "$(",
        "${",
        ";", // command separators
        "&&",
        "||",
        "|", // pipes
        ">",
        "<",
        "..", // directory traversal
        "\\",
    ];

    let lowercase = lang.to_lowercase();
    for pattern in &suspicious_patterns {
        if lowercase.contains(pattern) {
            return true;
        }
    }

    false
}

/// Validate that the language format is acceptable for unknown languages
fn is_valid_language_format(lang: &str) -> bool {
    if lang.is_empty() || lang.len() > 50 {
        return false;
    }

    // Only allow alphanumeric characters, hyphens, underscores, and dots
    for ch in lang.chars() {
        if !ch.is_alphanumeric() && ch != '-' && ch != '_' && ch != '.' {
            return false;
        }
    }

    // Don't allow languages that are just numbers or special characters
    if !lang.chars().any(|ch| ch.is_alphabetic()) {
        return false;
    }

    true
}

/// Enhanced code block language extraction with comprehensive validation
fn extract_and_validate_code_language(lang_tag: &pulldown_cmark::CodeBlockKind) -> Option<String> {
    match lang_tag {
        pulldown_cmark::CodeBlockKind::Fenced(lang_str) => {
            if lang_str.is_empty() {
                None
            } else {
                normalize_code_block_language(lang_str)
            }
        }
        pulldown_cmark::CodeBlockKind::Indented => None, // Indented blocks have no language
    }
}

/// Parse note structure with pulldown-cmark and comprehensive list extraction
fn parse_content_structure(body: &str, tables: Vec<Table>) -> ParserResult<NoteContent> {
    let parser = CmarkParser::new(body);

    let mut headings = Vec::new();
    let mut code_blocks = Vec::new();
    let mut paragraphs = Vec::new();
    let mut horizontal_rules = Vec::new();
    let mut plain_text = String::new();
    let mut current_offset = 0;
    let mut in_heading = false;
    let mut current_heading_level: u8 = 0;
    let mut current_heading_text = String::new();
    let mut current_heading_offset = 0;
    let mut in_code_block = false;
    let mut current_code_lang: Option<String> = None;
    let mut current_code_content = String::new();
    let mut current_code_offset = 0;
    let mut in_paragraph = false;
    let mut current_paragraph_text = String::new();
    let mut current_paragraph_offset = 0;

    for event in parser {
        match event {
            Event::Start(CmarkTag::Heading {
                level,
                id: _,
                classes: _,
                attrs: _,
            }) => {
                // Close any open paragraph
                if in_paragraph {
                    if !current_paragraph_text.trim().is_empty() {
                        paragraphs.push(Paragraph::new(
                            current_paragraph_text.clone(),
                            current_paragraph_offset,
                        ));
                    }
                    in_paragraph = false;
                    current_paragraph_text.clear();
                }

                in_heading = true;
                current_heading_level = heading_level_to_u8(level);
                current_heading_text.clear();
                current_heading_offset = current_offset;
            }
            Event::End(TagEnd::Heading(_)) => {
                if in_heading {
                    headings.push(Heading::new(
                        current_heading_level,
                        current_heading_text.clone(),
                        current_heading_offset,
                    ));
                    in_heading = false;
                }
            }
            Event::Start(CmarkTag::Paragraph) => {
                // Close any open paragraph (shouldn't happen but be safe)
                if in_paragraph {
                    if !current_paragraph_text.trim().is_empty() {
                        paragraphs.push(Paragraph::new(
                            current_paragraph_text.clone(),
                            current_paragraph_offset,
                        ));
                    }
                    current_paragraph_text.clear();
                }

                in_paragraph = true;
                current_paragraph_offset = current_offset;
            }
            Event::End(TagEnd::Paragraph) => {
                if in_paragraph {
                    if !current_paragraph_text.trim().is_empty() {
                        paragraphs.push(Paragraph::new(
                            current_paragraph_text.clone(),
                            current_paragraph_offset,
                        ));
                    }
                    in_paragraph = false;
                    current_paragraph_text.clear();
                }
            }
            Event::Start(CmarkTag::CodeBlock(kind)) => {
                // Close any open paragraph/code block
                if in_paragraph {
                    if !current_paragraph_text.trim().is_empty() {
                        paragraphs.push(Paragraph::new(
                            current_paragraph_text.clone(),
                            current_paragraph_offset,
                        ));
                    }
                    in_paragraph = false;
                    current_paragraph_text.clear();
                }

                in_code_block = true;
                // Enhanced language detection with validation and normalization
                current_code_lang = extract_and_validate_code_language(&kind);
                current_code_content.clear();
                current_code_offset = current_offset;
            }
            Event::End(TagEnd::CodeBlock) => {
                if in_code_block {
                    code_blocks.push(CodeBlock::new(
                        current_code_lang.clone(),
                        current_code_content.clone(),
                        current_code_offset,
                    ));
                    in_code_block = false;
                }
            }
            Event::Text(text) => {
                if in_heading {
                    current_heading_text.push_str(&text);
                } else if in_code_block {
                    current_code_content.push_str(&text);
                } else if in_paragraph {
                    current_paragraph_text.push_str(&text);
                    // Also add to plain_text for backward compatibility
                    plain_text.push_str(&text);
                    plain_text.push(' ');
                } else {
                    plain_text.push_str(&text);
                    plain_text.push(' '); // Add space between text nodes
                }
                current_offset += text.len();
            }
            Event::Code(code) => {
                if in_code_block {
                    current_code_content.push_str(&code);
                } else if in_paragraph {
                    current_paragraph_text.push_str(&code);
                    // Also add to plain_text for backward compatibility
                    plain_text.push_str(&code);
                    plain_text.push(' ');
                } else {
                    plain_text.push_str(&code);
                    plain_text.push(' ');
                }
                current_offset += code.len();
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_code_block {
                    current_code_content.push('\n');
                } else if in_paragraph {
                    current_paragraph_text.push(' ');
                } else if !in_heading {
                    plain_text.push(' ');
                }
                current_offset += 1;
            }
            Event::Rule => {
                // Horizontal rule detected
                // Determine style based on the raw content (default to dash)
                // Note: pulldown-cmark doesn't expose the original characters used,
                // so we'll default to "dash" for now
                let style = "dash".to_string();
                let raw_content = "---".to_string();

                horizontal_rules.push(HorizontalRule::new(
                    raw_content,
                    style,
                    current_offset,
                ));

                current_offset += 3; // Approximate length
            }
            _ => {}
        }
    }

    // Close any open paragraph at the end
    if in_paragraph && !current_paragraph_text.trim().is_empty() {
        paragraphs.push(Paragraph::new(
            current_paragraph_text.clone(),
            current_paragraph_offset,
        ));
    }

    // Extract comprehensive lists using regex-based analysis
    let lists = extract_lists_comprehensive(body)?;

    // Calculate word and character counts
    let word_count = plain_text.split_whitespace().count();
    let char_count = plain_text.chars().count();

    // Truncate to 1000 chars if needed
    let plain_text = if plain_text.len() > 1000 {
        let mut truncated: String = plain_text.chars().take(1000).collect();
        truncated.push_str("...");
        truncated
    } else {
        plain_text
    };

    Ok(NoteContent {
        plain_text,
        headings,
        code_blocks,
        paragraphs,
        lists,
        inline_links: Vec::new(),
        wikilinks: Vec::new(),
        tags: Vec::new(),
        latex_expressions: Vec::new(),
        callouts: Vec::new(),
        blockquotes: Vec::new(),
        footnotes: FootnoteMap::new(),
        tables,
        horizontal_rules,
        word_count,
        char_count,
    })
}

/// Extract task list content and status
/// Returns (content_without_checkbox, is_completed)
fn extract_task_content(text: &str) -> Option<(String, bool)> {
    // Check for task list patterns: [x] or [ ]
    let trimmed = text.trim();

    if let Some(task_text) = trimmed.strip_prefix("[x] ") {
        Some((task_text.trim().to_string(), true))
    } else {
        trimmed
            .strip_prefix("[ ] ")
            .map(|task_text| (task_text.trim().to_string(), false))
    }
}

/// Convert pulldown-cmark HeadingLevel to u8
fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_parse_simple_markdown() {
        let parser = PulldownParser::new();
        let content = "# Hello World\n\nThis is a test.";
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).await.unwrap();
        assert_eq!(doc.content.headings.len(), 1);
        assert_eq!(doc.content.headings[0].text, "Hello World");

        assert!(doc.content.plain_text.contains("This is a test"));
    }

    #[tokio::test]
    async fn test_parse_frontmatter() {
        let parser = PulldownParser::new();
        let content = r#"---
title: Test Note
tags: [rust, testing]
---

# Content

Body text here."#;
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).await.unwrap();
        assert!(doc.frontmatter.is_some());
        let fm = doc.frontmatter.unwrap();
        assert_eq!(fm.get_string("title"), Some("Test Note".to_string()));
    }

    #[tokio::test]
    async fn test_parse_wikilinks() {
        let parser = PulldownParser::new();
        let content = "See [[Other Note]] and [[Reference|alias]].";
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).await.unwrap();
        assert_eq!(doc.wikilinks.len(), 2);
        assert_eq!(doc.wikilinks[0].target, "Other Note");
        assert_eq!(doc.wikilinks[1].target, "Reference");
        assert_eq!(doc.wikilinks[1].alias, Some("alias".to_string()));
    }

    #[tokio::test]
    async fn test_parse_tags() {
        let parser = PulldownParser::new();
        let content = "This has #rust and #testing tags, plus #project/ai.";
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).await.unwrap();
        assert_eq!(doc.tags.len(), 3);
        assert_eq!(doc.tags[0].name, "rust");
        assert_eq!(doc.tags[1].name, "testing");
        assert_eq!(doc.tags[2].name, "project/ai");
    }

    #[tokio::test]
    async fn test_multiple_callouts() {
        let parser = PulldownParser::new();
        let content = r#"# Test

> [!note]
> Simple note callout
> With multiple lines

Text between callouts.

> [!warning] Important Warning
> This is a warning with title

Another text section.

> [!tip]
> Tip without title
"#;
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).await.unwrap();

        assert_eq!(doc.callouts.len(), 3, "Should extract 3 callouts");
        assert_eq!(doc.callouts[0].callout_type, "note");
        assert_eq!(doc.callouts[1].callout_type, "warning");
        assert_eq!(doc.callouts[2].callout_type, "tip");
    }

    #[tokio::test]
    async fn test_latex_extraction() {
        let parser = PulldownParser::new();
        let content = r#"# Test

Inline math: $x^2 + y^2 = z^2$

Block math:
$$
\int_0^\infty e^{-x^2} dx = \frac{\sqrt{\pi}}{2}
$$

More inline: $\alpha + \beta = \gamma$
"#;
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).await.unwrap();


        assert_eq!(doc.latex_expressions.len(), 3, "Should extract 3 LaTeX expressions");
        assert!(!doc.latex_expressions[0].is_block, "First should be inline");
        assert!(doc.latex_expressions[1].is_block, "Second should be block");
        assert!(!doc.latex_expressions[2].is_block, "Third should be inline");

        assert_eq!(doc.latex_expressions[0].expression, "x^2 + y^2 = z^2");
        assert_eq!(doc.latex_expressions[2].expression, "\\alpha + \\beta = \\gamma");
    }

    #[tokio::test]
    async fn test_wikilinks_no_code_block_extraction() {
        let parser = PulldownParser::new();
        let content = r#"# Test

Regular wikilink: [[Note]]

```
This should NOT be extracted: [[CodeBlockNote]]
```

Inline code with `[[InlineCodeNote]]` should not be extracted.

Regular wikilink again: [[AnotherNote]]
```rust
// This should also not be extracted
let note = "[[RustCodeNote]]";
```

Final wikilink: [[FinalNote]]
"#;
        let path = PathBuf::from("test.md");

        let doc = parser.parse_content(content, &path).await.unwrap();

        
        // Should only extract 3 wikilinks, ignoring those in code blocks and inline code
        assert_eq!(doc.wikilinks.len(), 3, "Should extract only 3 wikilinks, not those in code blocks");
        assert_eq!(doc.wikilinks[0].target, "Note");
        assert_eq!(doc.wikilinks[1].target, "AnotherNote");
        assert_eq!(doc.wikilinks[2].target, "FinalNote");
    }
}

