//! Core workspace tools for file and shell operations
//!
//! These tools provide essential workspace operations for agents:
//! - `read_file`: Read file contents with optional line range
//! - `edit_file`: Edit file via search/replace
//! - `write_file`: Write content to file
//! - `bash`: Execute shell commands
//! - `glob`: Find files by pattern
//! - `grep`: Search file contents
//!
//! ## Design
//!
//! - All tools operate on absolute paths or relative to workspace root
//! - Uses `ToolRef` for unified tool representation
//! - Compatible with both Rig (direct) and MCP (gateway) modes

#![allow(clippy::missing_errors_doc)] // Tool methods have obvious error conditions
#![allow(clippy::doc_markdown)] // Parameter names in docs don't need backticks
#![allow(clippy::needless_pass_by_value)] // Tools take owned strings for JSON compat

use super::helpers::{text_success, McpResultExt};
use crate::tools::containment::RootSet;
use crate::tools::fs_scope::{ContainedPath, FsScope, WritablePath};
use rmcp::model::{CallToolResult, ContentBlock, Tool};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::process::Command;

/// Workspace tools for file and shell operations
#[derive(Debug, Clone)]
pub struct WorkspaceTools {
    /// This tool set's filesystem capability: the workspace root every relative
    /// path and `bash` invocation is anchored at, plus the default-deny
    /// allowlist every path a tool touches must clear. Built once, by the
    /// caller, from the session's kilns, workspace and own storage directory —
    /// see [`crate::agent_manager::scope::session_containment`].
    ///
    /// There is no separate `workspace_root` field, and that is deliberate: a
    /// root path a tool can join onto is the ambient authority this type is
    /// supposed to have given up. Caller input becomes a path exactly once,
    /// through [`FsScope::resolve`].
    scope: FsScope,
    /// Default timeout for bash commands (ms)
    default_timeout_ms: u64,
    /// Extra environment variables injected into bash commands
    env_vars: HashMap<String, String>,
    /// Project `[security.shell]` policy, resolved at construction (never
    /// re-read at call time). `None` or an empty policy = no restriction
    /// beyond the permission gate.
    shell_policy: Option<crucible_core::config::ShellPolicy>,
}

impl WorkspaceTools {
    /// Create new workspace tools
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            scope: FsScope::workspace(workspace_root.into(), RootSet::Ambient),
            default_timeout_ms: 120_000,
            env_vars: HashMap::new(),
            shell_policy: None,
        }
    }

    /// Add environment variables to inject into bash commands
    #[must_use]
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.insert(key.into(), value.into());
        self
    }

    /// Set default timeout for bash commands
    #[must_use]
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.default_timeout_ms = timeout_ms;
        self
    }

    /// Contain every file operation to `containment` — a default-deny
    /// allowlist, built by the caller so that the roots and the carve-outs
    /// inside them are decided in one place rather than assembled here from
    /// two loose vectors.
    ///
    /// Note what this does NOT do: it does not fold the scope's anchor into
    /// the allowlist. The anchor is where relative paths and `bash` are
    /// anchored; whether the session may *read* it is a policy question its
    /// caller answers. Adding it here was how a detached workspace put `""`
    /// into the allowlist.
    #[must_use]
    pub(crate) fn with_containment(mut self, containment: RootSet) -> Self {
        self.scope = self.scope.with_containment(containment);
        self
    }

    /// Apply a project shell policy to the `bash` tool.
    #[must_use]
    pub fn with_shell_policy(mut self, policy: Option<crucible_core::config::ShellPolicy>) -> Self {
        self.shell_policy = policy;
        self
    }

    /// Expand `$NAME` / `${NAME}` from this tool set's OWN env map.
    ///
    /// Deliberately not the process environment: `$HOME` and `$PATH` must stay
    /// literal, or path expansion would become a way around containment. The
    /// map holds exactly what `with_env` was given — `CRU_SESSION`,
    /// `CRU_SESSION_DIR` — so this expands the references Crucible itself
    /// hands the model and nothing else.
    ///
    /// `bash` got this for free from the shell, which is why spilled output was
    /// reachable there and nowhere else.
    fn expand_env_vars(&self, path: &str) -> String {
        if !path.contains('$') {
            return path.to_string();
        }
        let mut out = String::with_capacity(path.len());
        let mut rest = path;
        while let Some(idx) = rest.find('$') {
            out.push_str(&rest[..idx]);
            let after = &rest[idx + 1..];
            let (name, consumed) = if let Some(stripped) = after.strip_prefix('{') {
                match stripped.find('}') {
                    Some(end) => (&stripped[..end], end + 2),
                    None => ("", 0),
                }
            } else {
                let end = after
                    .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
                    .unwrap_or(after.len());
                (&after[..end], end)
            };
            match self.env_vars.get(name) {
                Some(value) => {
                    out.push_str(value);
                    rest = &after[consumed..];
                }
                // Unknown name: leave it exactly as written, so containment
                // judges the literal string rather than a half-expansion.
                None => {
                    out.push('$');
                    rest = after;
                }
            }
        }
        out.push_str(rest);
        out
    }

    /// Resolve a path (absolute or relative to the anchor) through the scope.
    ///
    /// Both resolved forms must clear containment, and what comes back is a
    /// [`ContainedPath`] carrying the canonical one — so the path checked is
    /// the path opened, with no symlink swapped in between, and no tool below
    /// can reach the filesystem without having asked.
    fn resolve_path(&self, path: &str) -> Result<ContainedPath, rmcp::ErrorData> {
        self.scope.resolve(&self.expand_env_vars(path))
    }

    /// The same, for a path about to be MODIFIED.
    ///
    /// Everything `resolve_path` checks, plus the protected set no
    /// configuration reopens and the write-denied roots. A separate return
    /// type rather than a flag: `write_file` and `edit_file` cannot be handed
    /// a path that only cleared the read check, because it does not typecheck.
    fn resolve_path_for_write(&self, path: &str) -> Result<WritablePath, rmcp::ErrorData> {
        self.scope.resolve_for_write(&self.expand_env_vars(path))
    }

    /// Get tool definitions for registration
    #[must_use]
    pub fn tool_definitions() -> Vec<Tool> {
        super::workspace_defs::tool_definitions()
    }

    /// Read file contents with optional line range
    pub async fn read_file(
        &self,
        path: String,
        offset: Option<usize>,
        limit: Option<usize>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let resolved = self.resolve_path(&path)?;

        let content = tokio::fs::read_to_string(resolved.as_path())
            .await
            .mcp_err_ctx("Read error")?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = offset.unwrap_or(1).saturating_sub(1);
        let count = limit.unwrap_or(usize::MAX);

        let output: Vec<String> = lines
            .iter()
            .skip(start)
            .take(count)
            .enumerate()
            .map(|(i, line)| format!("{:>6}\t{}", start + i + 1, line))
            .collect();

        let result = format!(
            "{}\n\n[{} lines read, {} total]",
            output.join("\n"),
            output.len(),
            total_lines
        );

        Ok(text_success(result))
    }

    /// Edit file by replacing text (old_string must match exactly)
    pub async fn edit_file(
        &self,
        path: String,
        old_string: String,
        new_string: String,
        replace_all: Option<bool>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let resolved = self.resolve_path_for_write(&path)?;

        let content = tokio::fs::read_to_string(resolved.as_path())
            .await
            .mcp_err_ctx("Read error")?;

        if !content.contains(&old_string) {
            return Ok(text_success("Error: old_string not found in file"));
        }

        let (new_content, count) = if replace_all.unwrap_or(false) {
            let count = content.matches(&old_string).count();
            (content.replace(&old_string, &new_string), count)
        } else {
            (content.replacen(&old_string, &new_string, 1), 1)
        };

        tokio::fs::write(resolved.as_path(), &new_content)
            .await
            .mcp_err_ctx("Write error")?;

        Ok(text_success(format!("Replaced {count} occurrence(s)")))
    }

    /// Write content to file (creates parent directories if needed)
    pub async fn write_file(
        &self,
        path: String,
        content: String,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let resolved = self.resolve_path_for_write(&path)?;

        // Create parent directories if needed
        if let Some(parent) = resolved.as_path().parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .mcp_err_ctx("Mkdir error")?;
        }

        tokio::fs::write(resolved.as_path(), &content)
            .await
            .mcp_err_ctx("Write error")?;

        Ok(text_success(format!(
            "Written {} bytes to {}",
            content.len(),
            path
        )))
    }

    /// Execute bash command (use for git, npm, cargo, etc.)
    pub async fn bash(
        &self,
        command: String,
        timeout_ms: Option<u64>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // Project `[security.shell]` policy: blacklist blocks; a non-empty
        // whitelist restricts to listed prefixes. Checked per shell
        // STATEMENT (split on ;/&&/||/|/newlines, whitespace-trimmed) so
        // `git log; curl ...` can't ride a `git` whitelist entry. This is
        // defense-in-depth against straightforward misuse, not a sandbox —
        // env tricks and `eval` are out of scope; use the permission gate
        // for authoritative control. An unset/empty policy imposes nothing.
        if let Some(policy) = &self.shell_policy {
            if !(policy.blacklist.is_empty() && policy.whitelist.is_empty()) {
                let statements =
                    crucible_core::config::components::permissions::split_chained_commands(
                        &command,
                    );
                let statements: Vec<&str> = if statements.is_empty() {
                    vec![command.trim()]
                } else {
                    statements.iter().map(|s| s.trim()).collect()
                };
                let violation = statements.iter().any(|stmt| {
                    let blocked = policy
                        .blacklist
                        .iter()
                        .any(|prefix| stmt.starts_with(prefix.as_str()));
                    let whitelisted = policy.whitelist.is_empty()
                        || policy
                            .whitelist
                            .iter()
                            .any(|prefix| stmt.starts_with(prefix.as_str()));
                    blocked || !whitelisted
                });
                if violation {
                    return Err(rmcp::ErrorData::invalid_params(
                        format!(
                            "Command blocked by the project shell policy \
                             ([security.shell] in .crucible/project.toml): {command}"
                        ),
                        None,
                    ));
                }
            }
        }

        let timeout =
            std::time::Duration::from_millis(timeout_ms.unwrap_or(self.default_timeout_ms));

        let mut cmd = Command::new("bash");
        cmd.arg("-c").arg(&command);
        cmd.current_dir(self.scope.anchor());
        for (key, value) in &self.env_vars {
            cmd.env(key, value);
        }

        let output = tokio::time::timeout(timeout, cmd.output())
            .await
            .map_err(|_| {
                rmcp::ErrorData::internal_error(
                    format!("Command timed out after {}ms", timeout.as_millis()),
                    None,
                )
            })?
            .mcp_err_ctx("Exec error")?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let exit_code = output.status.code().unwrap_or(-1);

        let result = if output.status.success() {
            stdout.to_string()
        } else {
            format!("Exit code: {exit_code}\nStdout:\n{stdout}\nStderr:\n{stderr}")
        };

        Ok(text_success(result))
    }

    /// Find files matching glob pattern (e.g., '**/*.rs')
    pub fn glob(
        &self,
        pattern: String,
        path: Option<String>,
        limit: Option<usize>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        // No `path` means the anchor, and the anchor goes through the same
        // door as any other name — it is where relative paths land, not a
        // grant, so a session that may not read its own workspace may not
        // enumerate it either.
        let search_path = self.resolve_path(path.as_deref().unwrap_or(""))?;

        // Containment must cover the PATTERN too, not just the search path.
        // Two ways a pattern leaves the search path, and neither check may be
        // conditional on the tool set being contained: the instance the daemon
        // builds for plugin tool calls is ambient (`server/mod.rs`), and that
        // is precisely the caller an untrusted message can reach. `..` was
        // gated on containment being configured, and so `../../etc/*` walked
        // out of exactly the instance that needed the guard most.
        //
        // The allowed-roots filter on the yielded paths below stays as
        // belt-and-braces for the contained case; it cannot substitute for
        // these, because with no roots configured it admits everything.
        let pattern_path = std::path::Path::new(&pattern);

        // The glob crate special-cases literal `..` components and genuinely
        // walks up, so `../../etc/*` would enumerate host files.
        if pattern_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(rmcp::ErrorData::invalid_params(
                format!(
                    "Glob pattern '{pattern}' contains '..' — patterns are relative to the \
                     search path and may not traverse upward"
                ),
                None,
            ));
        }

        // An absolute pattern discards the base entirely under `Path::join`.
        if pattern_path.is_absolute() {
            return Err(rmcp::ErrorData::invalid_params(
                format!(
                    "Glob pattern '{pattern}' is absolute — patterns are relative to the \
                     search path"
                ),
                None,
            ));
        }

        let full_pattern = search_path.as_path().join(&pattern);
        let pattern_str = full_pattern.to_string_lossy();
        let max_results = limit.unwrap_or(100);

        let paths: Vec<String> = glob::glob(&pattern_str)
            .mcp_err_ctx("Glob error")?
            .filter_map(std::result::Result::ok)
            .filter(|p| self.scope.admits(p))
            .take(max_results + 1)
            .map(|p| self.scope.relativize(&p).display().to_string())
            .collect();

        let truncated = paths.len() > max_results;
        let files: Vec<&str> = paths.iter().take(max_results).map(String::as_str).collect();

        let result = if truncated {
            format!(
                "{}\n\n[{} files, truncated at {}]",
                files.join("\n"),
                files.len(),
                max_results
            )
        } else {
            format!("{}\n\n[{} files]", files.join("\n"), files.len())
        };

        Ok(text_success(result))
    }

    /// Whether one `--null`-formatted ripgrep output line (`path\0line:text`)
    /// names a file this session may read.
    ///
    /// A line with no NUL carries no path — rg's own diagnostics, and the
    /// blank separators of a heading format we do not ask for — so it is
    /// dropped: nothing downstream can attribute it to a permitted file.
    fn grep_line_is_permitted(&self, line: &str) -> bool {
        match line.split_once('\0') {
            Some((path, _)) => self.scope.admits(Path::new(path)),
            None => false,
        }
    }

    /// Search file contents with regex (uses ripgrep)
    pub async fn grep(
        &self,
        pattern: String,
        path: Option<String>,
        glob: Option<String>,
        limit: Option<usize>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let search_path = self.resolve_path(path.as_deref().unwrap_or(""))?;

        let max_matches = limit.unwrap_or(50);

        // Every flag must precede `--`; the pattern and paths follow it.
        // Without the separator rg parses a leading-dash pattern as a flag,
        // and `--pre=<cmd>` runs <cmd> against every file walked — arbitrary
        // execution from a tool that never reaches the permission gate.
        // `--glob` in particular has to move ahead of the pattern: appended
        // after it, it would become a positional argument.
        let mut cmd = Command::new("rg");
        // `--null` terminates the printed path with NUL, which is what makes
        // the containment filter below exact: the ordinary `path:line:text`
        // format is ambiguous for any path containing a colon.
        //
        // `--with-filename` because rg omits the path entirely when it is
        // given exactly one FILE to search — and a line with no path is a line
        // the filter cannot clear, so single-file greps would return nothing.
        cmd.arg("--line-number")
            .arg("--with-filename")
            .arg("--null")
            .arg("--max-count")
            .arg("1000");

        if let Some(g) = glob {
            cmd.arg("--glob").arg(g);
        }

        cmd.arg("--").arg(&pattern).arg(search_path.as_path());

        let output = cmd.output().await.mcp_err_ctx("Grep error")?;

        // Containment applies to what ripgrep YIELDS, not only to where it
        // was pointed. `resolve_path` above checks the start directory and rg
        // then recurses — so a denied subtree under an allowed root (every
        // other session's transcript, under the data root) came back verbatim.
        // `glob` has post-filtered its results since it grew containment;
        // `grep` had no equivalent, which is the wider hole of the two because
        // it prints file *contents*.
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<String> = stdout
            .lines()
            .filter(|line| self.grep_line_is_permitted(line))
            .map(|line| line.replacen('\0', ":", 1))
            .take(max_matches + 1)
            .collect();
        let truncated = lines.len() > max_matches;

        let result_lines: Vec<&str> = lines.iter().map(String::as_str).take(max_matches).collect();

        let result = if truncated {
            format!(
                "{}\n\n[{} matches, truncated at {}]",
                result_lines.join("\n"),
                result_lines.len(),
                max_matches
            )
        } else {
            format!(
                "{}\n\n[{} matches]",
                result_lines.join("\n"),
                result_lines.len()
            )
        };

        Ok(text_success(result))
    }
}

// =============================================================================
// ToolExecutor implementation for internal agents
// =============================================================================

use async_trait::async_trait;
use crucible_core::traits::tools::{
    ExecutionContext, ToolDefinition, ToolError, ToolExecutor, ToolResult, ToolSurface,
};

#[async_trait]
impl ToolExecutor for WorkspaceTools {
    async fn execute_tool(
        &self,
        name: &str,
        params: serde_json::Value,
        context: &ExecutionContext,
    ) -> ToolResult<serde_json::Value> {
        // Helper to extract string param
        let get_str = |key: &str| -> Option<String> {
            params
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(String::from)
        };
        let get_optional_str = |key: &str| -> Option<String> {
            params
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(String::from)
        };
        #[allow(clippy::cast_possible_truncation)]
        let get_optional_usize = |key: &str| -> Option<usize> {
            params
                .get(key)
                .and_then(serde_json::Value::as_u64)
                .map(|n| n as usize)
        };
        let get_optional_u64 =
            |key: &str| -> Option<u64> { params.get(key).and_then(serde_json::Value::as_u64) };
        let get_optional_bool =
            |key: &str| -> Option<bool> { params.get(key).and_then(serde_json::Value::as_bool) };

        // Convert CallToolResult to JSON
        let convert_result =
            |result: Result<CallToolResult, rmcp::ErrorData>| -> ToolResult<serde_json::Value> {
                match result {
                    Ok(call_result) => {
                        // Extract text content from result
                        let text: String = call_result
                            .content
                            .iter()
                            .filter_map(|c| match c {
                                ContentBlock::Text(t) => Some(t.text.clone()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        Ok(serde_json::json!({ "result": text }))
                    }
                    Err(e) => Err(ToolError::ExecutionFailed(e.message.to_string())),
                }
            };

        match name {
            "read_file" => {
                let path = get_str("path")
                    .ok_or_else(|| ToolError::InvalidParameters("path is required".into()))?;
                let offset = get_optional_usize("offset");
                let limit = get_optional_usize("limit");
                convert_result(self.read_file(path, offset, limit).await)
            }
            "edit_file" => {
                let path = get_str("path")
                    .ok_or_else(|| ToolError::InvalidParameters("path is required".into()))?;
                let old_string = get_str("old_string")
                    .ok_or_else(|| ToolError::InvalidParameters("old_string is required".into()))?;
                let new_string = get_str("new_string")
                    .ok_or_else(|| ToolError::InvalidParameters("new_string is required".into()))?;
                let replace_all = get_optional_bool("replace_all");
                convert_result(
                    self.edit_file(path, old_string, new_string, replace_all)
                        .await,
                )
            }
            "write_file" => {
                let path = get_str("path")
                    .ok_or_else(|| ToolError::InvalidParameters("path is required".into()))?;
                let content = get_str("content")
                    .ok_or_else(|| ToolError::InvalidParameters("content is required".into()))?;
                convert_result(self.write_file(path, content).await)
            }
            "bash" => {
                let command = get_str("command")
                    .ok_or_else(|| ToolError::InvalidParameters("command is required".into()))?;
                let timeout_ms = get_optional_u64("timeout_ms");
                let background = get_optional_bool("background").unwrap_or(false);

                if background {
                    return Err(ToolError::ExecutionFailed(
                        "Background execution requires BackgroundSpawner context. \
                         Use the agent layer for background bash tasks."
                            .into(),
                    ));
                }
                // Merge hook-injected env vars with struct-level env vars
                if context.env_vars.is_empty() {
                    convert_result(self.bash(command, timeout_ms).await)
                } else {
                    let mut with_hook_env = self.clone();
                    for (k, v) in &context.env_vars {
                        with_hook_env.env_vars.insert(k.clone(), v.clone());
                    }
                    convert_result(with_hook_env.bash(command, timeout_ms).await)
                }
            }
            "glob" => {
                let pattern = get_str("pattern")
                    .ok_or_else(|| ToolError::InvalidParameters("pattern is required".into()))?;
                let path = get_optional_str("path");
                let limit = get_optional_usize("limit");
                convert_result(self.glob(pattern, path, limit))
            }
            "grep" => {
                let pattern = get_str("pattern")
                    .ok_or_else(|| ToolError::InvalidParameters("pattern is required".into()))?;
                let path = get_optional_str("path");
                let glob = get_optional_str("glob");
                let limit = get_optional_usize("limit");
                convert_result(self.grep(pattern, path, glob, limit).await)
            }
            _ => Err(ToolError::NotFound(format!("Unknown tool: {name}"))),
        }
    }

    async fn list_tools(&self) -> ToolResult<Vec<ToolDefinition>> {
        // Convert MCP Tool definitions to ToolDefinition
        let mcp_tools = Self::tool_definitions();
        let tools = mcp_tools
            .into_iter()
            .map(|t| ToolDefinition {
                name: t.name.to_string(),
                description: t.description.map(|d| d.to_string()).unwrap_or_default(),
                category: Some("workspace".to_string()),
                parameters: Some(serde_json::Value::Object((*t.input_schema).clone())),
                returns: None,
                required_permissions: vec![],
                examples: vec![],
            })
            .collect();
        Ok(tools)
    }

    /// Deferred to the per-tool table rather than answered `Host` outright.
    ///
    /// Every tool this executor serves today *is* `Host` — and
    /// `every_workspace_tool_is_host_surface` asserts exactly that — but the
    /// answer has to come from the same place every other executor's does, or
    /// "the executor decides" survives here and the next tool added inherits a
    /// classification instead of receiving one.
    fn surface(&self, tool: &str) -> ToolSurface {
        crate::tools::surface::classify(tool)
    }
}

#[cfg(test)]
mod tests;
