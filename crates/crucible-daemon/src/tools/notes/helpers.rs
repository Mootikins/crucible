//! Internal helpers for note CRUD operations.

use crate::tools::fs_scope::{FsScope, WritablePath};
use crucible_core::kiln::{is_note_file, KilnFileKind};
use std::path::Path;

pub(crate) fn ensure_md_suffix(path: String) -> String {
    let pb = Path::new(&path);
    if pb.extension().is_some() {
        path
    } else {
        format!("{path}.md")
    }
}

/// The one door the note tools write through.
///
/// Containment and the protected set first — [`FsScope::resolve_for_write`] —
/// then the rule that gives `create_note` its `ToolSurface::Daemon`
/// classification: **a note tool writes notes**.
///
/// That rule was missing entirely. [`ensure_md_suffix`] appends `.md` only when
/// the path has no extension at all, so `create_note {"path":
/// "plugins/evil/init.lua"}` wrote Lua verbatim and `"hooks/pre-commit.sh"`
/// wrote a shell script. Neither is in the protected set — protection covers
/// `.crucible/` and the other harnesses' control directories, not every tree a
/// `runtimepath` might name — and both are loaded and executed with host
/// privileges on the daemon's next start. Because the surface is `Daemon` and
/// `isolation_refusal` lets `Daemon` through unconditionally, that was host code
/// execution from inside a containerized session, where `write_file` and `bash`
/// are refused (design: "The container escape", CVE-2026-25725's shape).
///
/// The verdict is [`is_note_file`], not a list spelled here: `KilnFileKind` is
/// already the codebase's answer to "what is a note", and a second copy is how
/// the fourteen hand-rolled `extension == "md"` checks happened.
///
/// Judged on **both** the written name and the resolved path, because a symlink
/// is a rename: `note.md -> init.lua` would otherwise carry Lua through a `.md`
/// spelling. Same both-forms discipline as the control-directory check.
pub(super) fn resolve_note_write(
    scope: &FsScope,
    path: &str,
) -> Result<WritablePath, rmcp::ErrorData> {
    let resolved = scope.resolve_for_write(path)?;
    reject_non_note(path, resolved.as_path())?;
    Ok(resolved)
}

/// The extension rule alone, for the write sinks that do not hold an
/// [`FsScope`]. Every caller of this is a note-writing tool.
pub(crate) fn reject_non_note(user_path: &str, resolved: &Path) -> Result<(), rmcp::ErrorData> {
    if is_note_file(Path::new(user_path)) && is_note_file(resolved) {
        return Ok(());
    }
    Err(rmcp::ErrorData::invalid_params(
        format!(
            "'{user_path}' is not a note: the note tools write {}. A tool that could \
             author any extension could write a plugin, a git hook or a shell script \
             into a tree the daemon or the shell later executes, which is why this is \
             refused rather than renamed. Use `write_file` for files that are not notes.",
            KilnFileKind::NOTE_EXTENSIONS
                .iter()
                .map(|ext| format!(".{ext}"))
                .collect::<Vec<_>>()
                .join(" or ")
        ),
        None,
    ))
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
