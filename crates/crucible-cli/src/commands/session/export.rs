use super::io::{read_transcript, sessions_dir};
use crate::common::daemon_client;
use crate::config::CliAppConfig;
use anyhow::Result;
use crucible_daemon::SessionId;
use std::path::PathBuf;
use tokio::fs;

pub(super) async fn export(
    config: CliAppConfig,
    id: String,
    output: Option<PathBuf>,
    timestamps: bool,
) -> Result<()> {
    let sessions_path = sessions_dir(&config);
    let session_id = SessionId::parse(&id)?;
    let session_dir = sessions_path.join(session_id.as_str());

    if !session_dir.exists() {
        crate::output::hint("Try: `cru session list` to see available sessions");
        anyhow::bail!("Session not found: {}", id);
    }

    if let Ok(client) = daemon_client().await {
        if let Ok(output_path_str) = client
            .session_export_to_file(session_id.as_str(), output.as_deref(), Some(timestamps))
            .await
        {
            println!("Exported session to: {}", output_path_str);
            return Ok(());
        }
    }

    let transcript = read_transcript(&session_dir).await?;
    let md = crucible_daemon::render_to_markdown(
        &transcript,
        &crucible_daemon::RenderOptions {
            include_timestamps: timestamps,
            ..Default::default()
        },
    );
    let output_path = output.unwrap_or_else(|| session_dir.join("session.md"));
    fs::write(&output_path, &md).await?;
    println!("Exported session to: {}", output_path.display());

    Ok(())
}
