//! `cru lua` CLI command — evaluate Lua code in the daemon's plugin runtime

use anyhow::Result;

use crate::common::daemon_client;

/// Evaluate Lua code in the daemon's plugin runtime.
///
/// Connects to the running daemon and calls `lua.eval` RPC.
/// Use `=` prefix for expressions (e.g., `=1+1`).
pub async fn execute(code: String) -> Result<()> {
    let client = daemon_client().await?;
    let response = client
        .call("lua.eval", serde_json::json!({ "code": code }))
        .await?;

    if let Some(result) = response.get("result").and_then(|r| r.as_str()) {
        if result != "nil" {
            println!("{result}");
        }
    }

    Ok(())
}
