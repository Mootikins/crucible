//! Execute justfile recipes

use crate::{JustError, Result};
use std::path::Path;
use tokio::process::Command;

/// Result of executing a recipe
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
}

/// Execute a just recipe with arguments
pub async fn execute_recipe(dir: &Path, recipe: &str, args: &[String]) -> Result<ExecutionResult> {
    let mut cmd = Command::new("just");
    cmd.arg(recipe);
    cmd.args(args);
    cmd.current_dir(dir);

    let output = cmd.output().await?;

    Ok(ExecutionResult {
        success: output.status.success(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
        stderr: String::from_utf8_lossy(&output.stderr).to_string(),
        exit_code: output.status.code(),
    })
}

/// Execute with timeout (in seconds)
pub async fn execute_recipe_with_timeout(
    dir: &Path,
    recipe: &str,
    args: &[String],
    timeout_secs: u64,
) -> Result<ExecutionResult> {
    use tokio::time::{timeout, Duration};

    match timeout(
        Duration::from_secs(timeout_secs),
        execute_recipe(dir, recipe, args),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => Err(JustError::CommandError(format!(
            "Recipe '{}' timed out after {}s",
            recipe, timeout_secs
        ))),
    }
}
