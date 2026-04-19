//! `cru lua` CLI command — evaluate Lua code in the daemon's plugin runtime

use anyhow::{anyhow, Context, Result};
use std::io::Read;
use std::path::PathBuf;

use crate::common::daemon_client;

/// Evaluate Lua code in the daemon's plugin runtime.
///
/// Source precedence: `--file` > `-` (stdin) > positional `code` string.
/// Connects to the running daemon and calls `lua.eval` RPC.
/// Use `=` prefix for expressions (e.g., `=1+1`).
pub async fn execute(code: Option<String>, file: Option<PathBuf>) -> Result<()> {
    let source = match (file, code) {
        (Some(path), _) => std::fs::read_to_string(&path)
            .with_context(|| format!("reading Lua file {}", path.display()))?,
        (None, Some(c)) if c == "-" => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading Lua from stdin")?;
            buf
        }
        (None, Some(c)) => c,
        (None, None) => {
            return Err(anyhow!(
                "no Lua code provided: pass code as an argument, use --file, or '-' for stdin"
            ));
        }
    };

    let client = daemon_client().await?;
    let response = client
        .call("lua.eval", serde_json::json!({ "code": source }))
        .await?;

    if let Some(result) = response.get("result").and_then(|r| r.as_str()) {
        if result != "nil" {
            println!("{result}");
        }
    }

    Ok(())
}
