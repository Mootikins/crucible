//! Internal helpers for note CRUD operations.

use std::path::Path;

pub(super) fn ensure_md_suffix(path: String) -> String {
    let pb = Path::new(&path);
    if pb.extension().is_some() {
        path
    } else {
        format!("{path}.md")
    }
}

/// Serialize frontmatter to YAML format with delimiters
pub(super) fn serialize_frontmatter_to_yaml(
    frontmatter: &serde_json::Value,
) -> Result<String, String> {
    // If frontmatter is empty object, return empty string
    if let Some(obj) = frontmatter.as_object() {
        if obj.is_empty() {
            return Ok(String::new());
        }
    }

    // Serialize to YAML
    let yaml_str = serde_yaml::to_string(frontmatter)
        .map_err(|e| format!("Failed to serialize frontmatter: {e}"))?;

    // Add delimiters
    Ok(format!("---\n{yaml_str}---\n"))
}

/// Extract content without frontmatter
pub(super) fn extract_content_without_frontmatter(content: &str) -> String {
    // Check if starts with ---
    if !content.starts_with("---\n") && !content.starts_with("---\r\n") {
        return content.to_string();
    }

    // Find closing ---
    let rest = &content[4..]; // Skip opening ---\n
    if let Some(end_pos) = rest.find("\n---\n") {
        // Return content after closing ---
        rest[end_pos + 5..].to_string()
    } else if let Some(end_pos) = rest.find("\r\n---\r\n") {
        rest[end_pos + 7..].to_string()
    } else {
        // No closing delimiter found, return original
        content.to_string()
    }
}
