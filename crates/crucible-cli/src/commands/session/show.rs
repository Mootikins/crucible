use super::io::{display_events_text, format_events_markdown, read_session_events, sessions_dir};
use crate::common::daemon_client;
use crate::config::CliConfig;
use crate::output;
use anyhow::Result;
use crucible_daemon::{LogEvent, SessionId};

pub(super) async fn show(config: CliConfig, id: String, format: String) -> Result<()> {
    let client = daemon_client().await.ok();

    if let Some(client) = &client {
        if let Ok(result) = client.session_get(&id).await {
            match format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&result)?;
                    println!("{json}");
                }
                _ => {
                    println!(
                        "Session ID: {}",
                        result["session_id"].as_str().unwrap_or("?")
                    );
                    println!("Type: {}", result["type"].as_str().unwrap_or("?"));
                    println!("State: {}", result["state"].as_str().unwrap_or("?"));
                    println!("Kiln: {}", result["kiln"].as_str().unwrap_or("?"));
                    let started = result["started_at"]
                        .as_str()
                        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                        .map(|dt| {
                            dt.with_timezone(&chrono::Local)
                                .format("%Y-%m-%d %H:%M:%S")
                                .to_string()
                        })
                        .unwrap_or_else(|| "?".to_string());
                    println!("Started: {}", started);
                    if let Some(title) = result["title"].as_str() {
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

    if let Some(client) = &client {
        let loaded = match format.as_str() {
            "json" => {
                if let Ok(events_json) = client.session_load_events(&session_dir).await {
                    let json = serde_json::to_string_pretty(&events_json)?;
                    println!("{json}");
                    true
                } else {
                    false
                }
            }
            "markdown" | "md" => {
                if let Ok(md) = client
                    .session_render_markdown(&session_dir, None, None, None, None)
                    .await
                {
                    println!("{md}");
                    true
                } else {
                    false
                }
            }
            _ => {
                if let Ok(events_json) = client.session_load_events(&session_dir).await {
                    if let Ok(events) = serde_json::from_value::<Vec<LogEvent>>(events_json) {
                        display_events_text(&id, &events);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };
        if loaded {
            return Ok(());
        }
    }

    let events = read_session_events(&session_dir).await?;

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&events)?;
            println!("{json}");
        }
        "markdown" | "md" => {
            let md = format_events_markdown(&events, false);
            println!("{md}");
        }
        _ => {
            display_events_text(&id, &events);
        }
    }

    Ok(())
}
