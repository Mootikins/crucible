use super::io::{history_transcript, read_transcript, sessions_dir, transcript_text};
use crate::common::daemon_client;
use crate::config::CliAppConfig;
use crate::output;
use anyhow::Result;
use crucible_daemon::{render_to_markdown, RenderOptions, SessionId};

pub(super) async fn show(config: CliAppConfig, id: String, format: String) -> Result<()> {
    let client = daemon_client().await.ok();

    if let Some(client) = &client {
        if let Ok(result) = client.session_get(&id).await {
            match format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&result)?;
                    println!("{json}");
                }
                _ => {
                    println!("Session ID: {}", result.id.as_str());
                    println!("Type: {}", result.session_type.as_prefix());
                    println!("State: {}", result.state);
                    let kilns: Vec<&str> = result.kilns.iter().map(|k| k.as_str()).collect();
                    println!(
                        "Kilns: {}",
                        if kilns.is_empty() {
                            "(none)".to_string()
                        } else {
                            kilns.join(", ")
                        }
                    );
                    let started = result
                        .started_at
                        .with_timezone(&chrono::Local)
                        .format("%Y-%m-%d %H:%M:%S")
                        .to_string();
                    println!("Started: {}", started);
                    if let Some(title) = result.title.as_deref() {
                        println!("Title: {}", title);
                    }
                }
            }
            return Ok(());
        }
    }

    let sessions_path = sessions_dir(&config);
    let session_id = SessionId::parse(&id)?;
    let session_dir = sessions_path.join(session_id.as_str());

    if !session_dir.exists() {
        output::hint("Try: `cru session list` to see available sessions");
        anyhow::bail!("Session not found: {}", id);
    }

    // The daemon folds the log. Only when no daemon answers does the CLI
    // run the same fold on the file.
    let transcript = match &client {
        Some(client) => match history_transcript(client, session_id.as_str()).await {
            Ok(transcript) => transcript,
            Err(_) => read_transcript(&session_dir).await?,
        },
        None => read_transcript(&session_dir).await?,
    };

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&transcript)?;
            println!("{json}");
        }
        "markdown" | "md" => {
            println!(
                "{}",
                render_to_markdown(&transcript, &RenderOptions::default())
            );
        }
        _ => print!("{}", transcript_text(&id, &transcript)),
    }

    Ok(())
}
