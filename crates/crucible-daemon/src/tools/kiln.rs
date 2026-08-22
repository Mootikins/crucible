//! Kiln operations tools
//!
//! This module provides kiln-specific tools like roots and statistics.

#![allow(missing_docs)]

use super::containment::RootSet;
use super::fs_scope::FsScope;
use super::helpers::json_success;
use rmcp::{model::CallToolResult, tool, tool_router};

#[derive(Clone)]
#[allow(missing_docs)]
pub struct KilnTools {
    /// The kiln as a capability rather than a path — see [`super::fs_scope`].
    /// `get_kiln_info` walks, and a count is an oracle: unfiltered it reports
    /// how many sessions the machine has recorded.
    scope: FsScope,
    /// What `get_kiln_info` calls this kiln when it answers the model.
    ///
    /// The tool used to derive a `"name"` from `scope.anchor().file_name()` —
    /// the directory basename — so a kiln registered `notes` at
    /// `/home/u/Private Vault` reported itself as `Private Vault`. There is no
    /// path fallback: an anchor with no registry entry has no name, and the
    /// tool omits the key.
    name: Option<crucible_core::config::KilnName>,
}

impl KilnTools {
    #[allow(missing_docs)]
    #[must_use]
    pub fn new(kiln_path: String) -> Self {
        Self {
            scope: FsScope::kiln(kiln_path, RootSet::Ambient),
            name: None,
        }
    }

    /// Tell these tools the kiln's registry name. See [`KilnTools::name`].
    #[must_use]
    pub(crate) fn with_name(mut self, name: Option<crucible_core::config::KilnName>) -> Self {
        self.name = name;
        self
    }

    /// Contain these tools to the session's roots — see
    /// [`super::notes::NoteTools::with_containment`].
    #[must_use]
    pub(crate) fn with_containment(mut self, containment: RootSet) -> Self {
        self.scope = self.scope.with_containment(containment);
        self
    }
}

#[tool_router]
impl KilnTools {
    #[tool(description = "Get comprehensive kiln information")]
    pub async fn get_kiln_info(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        // The registry name or nothing. Not `scope.anchor().file_name()`, which
        // reported a fragment of the user's filesystem as if it were a name,
        // and not `"unknown"` either — a placeholder the model would repeat
        // back is a worse answer than a missing key.
        let named = |mut fields: serde_json::Map<String, serde_json::Value>| {
            if let Some(name) = self.name.as_ref() {
                fields.insert("name".to_string(), serde_json::json!(name.as_str()));
            }
            serde_json::Value::Object(fields)
        };

        let mut total_files = 0;
        let mut total_size = 0;
        let mut md_files = 0;

        // Through the scope: the hidden-directory skip this walk used to carry
        // does nothing when the transcripts sit at `{kiln}/sessions/` rather
        // than `{kiln}/.crucible/`, which is the shape of every kiln-less
        // session (whose kiln is the data root).
        let root = self.scope.resolve("")?;
        for entry in self
            .scope
            .walk_files(&root)
            .filter(|e| !in_hidden_directory(self.scope.relativize(e.path())))
        {
            total_files += 1;
            if let Ok(metadata) = entry.metadata() {
                total_size += metadata.len();
            }
            // Notes only. This counter is reported to agents under the key
            // `markdown_files`; widening it to canvases would have made the
            // number quietly disagree with its own name, and renaming the key
            // would break a wire format agents already read.
            if crucible_core::kiln::is_note_file(entry.path()) {
                md_files += 1;
            }
        }

        json_success(named(
            serde_json::json!({
                "total_files": total_files,
                "markdown_files": md_files,
                "total_size_bytes": total_size
            })
            .as_object()
            .expect("object literal")
            .clone(),
        ))
    }
}

/// Whether a kiln-relative path lives under a dot-directory.
///
/// A *counting* convention, not containment: `get_kiln_info` reports what the
/// kiln holds, and `.git`/`.obsidian` are not it. Containment has already run —
/// this filter can only ever narrow what the scope admitted, never widen it,
/// which is why it is safe to leave as a per-tool rule. Hidden *files* are
/// counted, as they always were; only directories are skipped.
fn in_hidden_directory(relative: &std::path::Path) -> bool {
    let mut components: Vec<_> = relative.components().collect();
    components.pop();
    components
        .iter()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_kiln_tools_creation() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path);
        assert_eq!(kiln_tools.scope.anchor(), temp_dir.path());
    }

    /// `get_kiln_info` is a tool the agent can call directly and repeatedly.
    /// Whatever it says about the kiln must be sayable back to us: a registry
    /// name, or nothing. It used to answer with `anchor().file_name()`, so a
    /// kiln registered `notes` at `/home/u/Private Vault` introduced itself as
    /// `Private Vault`.
    #[tokio::test]
    async fn get_kiln_info_never_names_a_kiln_after_its_directory() {
        let temp_dir = TempDir::new().unwrap();
        let secret = temp_dir.path().join("Private Vault");
        std::fs::create_dir_all(&secret).unwrap();

        let parsed = kiln_info(
            &KilnTools::new(secret.to_string_lossy().to_string())
                .with_name(Some(crate::test_support::kiln_name("notes"))),
        )
        .await;

        assert_eq!(parsed["name"], "notes");
        let rendered = serde_json::to_string(&parsed).unwrap();
        assert!(
            !rendered.contains("Private Vault"),
            "the anchor directory must not reach the model: {rendered}"
        );
    }

    /// No registry entry, no name — not the basename, and not `"unknown"`,
    /// which the model would repeat back as if it meant something.
    #[tokio::test]
    async fn get_kiln_info_omits_the_name_when_no_entry_claims_the_kiln() {
        let temp_dir = TempDir::new().unwrap();
        let secret = temp_dir.path().join("Private Vault");
        std::fs::create_dir_all(&secret).unwrap();

        let parsed = kiln_info(&KilnTools::new(secret.to_string_lossy().to_string())).await;

        assert!(parsed.get("name").is_none(), "no key at all: {parsed}");
        let rendered = serde_json::to_string(&parsed).unwrap();
        assert!(!rendered.contains("Private Vault"), "{rendered}");
        assert!(!rendered.contains("unknown"), "{rendered}");
    }

    async fn kiln_info(tools: &KilnTools) -> serde_json::Value {
        let result = tools.get_kiln_info().await.expect("get_kiln_info succeeds");
        let text = result
            .content
            .first()
            .expect("content")
            .as_text()
            .expect("text")
            .text
            .clone();
        serde_json::from_str(&text).expect("valid JSON")
    }

    #[tokio::test]
    async fn test_get_kiln_info_empty() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path.clone());

        let result = kiln_tools.get_kiln_info().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Check flat structure. No `name`: these tools were built from a
            // bare directory, and the basename is not a kiln name.
            assert!(parsed.get("name").is_none());
            assert_eq!(parsed["total_files"], 0);
            assert_eq!(parsed["markdown_files"], 0);
            assert_eq!(parsed["total_size_bytes"], 0);

            // Verify no nested root or path fields
            assert!(parsed.get("root").is_none() || parsed["root"].is_null());
            assert!(parsed.get("path").is_none() || parsed["path"].is_null());
        }
    }

    #[tokio::test]
    async fn test_get_kiln_info_with_files() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let kiln_tools = KilnTools::new(kiln_path.clone());

        // Create some test files
        std::fs::write(temp_dir.path().join("test1.md"), "# Test Note 1").unwrap();
        std::fs::write(
            temp_dir.path().join("test2.md"),
            "# Test Note 2\nWith more content.",
        )
        .unwrap();
        std::fs::write(temp_dir.path().join("ignore.txt"), "Ignore me").unwrap();

        let result = kiln_tools.get_kiln_info().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            // Check flat structure
            assert!(parsed.get("name").is_none());
            assert_eq!(parsed["total_files"], 3);
            assert_eq!(parsed["markdown_files"], 2);
            assert!(parsed["total_size_bytes"].as_u64().unwrap() > 0);

            // Verify no nested root or path fields
            assert!(parsed.get("root").is_none() || parsed["root"].is_null());
            assert!(parsed.get("path").is_none() || parsed["path"].is_null());
        }
    }

    #[test]
    fn test_tool_router_creation() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        let _kiln_tools = KilnTools::new(kiln_path);

        // This should compile and not panic - the tool_router macro generates the router
        let _router = KilnTools::tool_router();
    }

    #[tokio::test]
    async fn test_get_kiln_info_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let kiln_path = temp_dir.path().to_string_lossy().to_string();

        // Create nested structure
        std::fs::create_dir_all(temp_dir.path().join("sub/deep")).unwrap();
        std::fs::create_dir_all(temp_dir.path().join(".hidden")).unwrap();

        std::fs::write(temp_dir.path().join("root.md"), "# Root").unwrap();
        std::fs::write(temp_dir.path().join("sub/nested.md"), "# Nested").unwrap();
        std::fs::write(temp_dir.path().join("sub/deep/inner.md"), "# Inner").unwrap();
        std::fs::write(temp_dir.path().join("other.txt"), "text file").unwrap();
        std::fs::write(temp_dir.path().join(".hidden/secret.md"), "# Secret").unwrap(); // must be excluded

        let kiln_tools = KilnTools::new(kiln_path);
        let result = kiln_tools.get_kiln_info().await;
        assert!(result.is_ok());

        let call_result = result.unwrap();
        if let Some(content) = call_result.content.first() {
            let raw_text = content.as_text().unwrap();
            let parsed: serde_json::Value = serde_json::from_str(&raw_text.text).unwrap();

            assert_eq!(parsed["total_files"], 4, "should count root.md + sub/nested.md + sub/deep/inner.md + other.txt, excluding .hidden/");
            assert_eq!(parsed["markdown_files"], 3, "should count root.md + sub/nested.md + sub/deep/inner.md, excluding .hidden/secret.md");
        }
    }
}
