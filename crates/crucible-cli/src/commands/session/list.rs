use super::helpers::truncate;
use super::io::{list_session_dirs, read_session_events, sessions_dir};
use crate::common::daemon_client;
use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use crucible_daemon::{LogEvent, SessionId, SessionType};

pub(super) async fn list(
    config: CliConfig,
    limit: u32,
    session_type: Option<String>,
    format: String,
    state: Option<String>,
    all: bool,
    include_children: bool,
) -> Result<()> {
    let client = daemon_client().await?;

    super::acp::rpc::list(
        &client,
        &config,
        session_type.as_deref(),
        state.as_deref(),
        &format,
        Some(limit),
        include_children,
    )
    .await?;

    if all {
        println!();
        println!("Persisted sessions:");
        list_persisted(config, limit, session_type, format).await?;
    }

    Ok(())
}

pub(super) async fn list_persisted(
    config: CliConfig,
    limit: u32,
    session_type: Option<String>,
    format: String,
) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions found.");
        println!("Sessions will be stored in: {}", sessions_path.display());
        return Ok(());
    }

    if let Ok(client) = daemon_client().await {
        if let Ok(result) = client
            .session_list_persisted(
                &config.kiln_path,
                session_type.as_deref(),
                Some(limit as usize),
            )
            .await
        {
            let sessions = result
                .get("sessions")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if sessions.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }
            match format.as_str() {
                "json" => {
                    let json = serde_json::to_string_pretty(&sessions)?;
                    println!("{json}");
                }
                _ => {
                    println!("Sessions (newest first):\n");
                    for s in &sessions {
                        let id = s["id"].as_str().unwrap_or("?");
                        let msg_count = s["message_count"].as_u64().unwrap_or(0);
                        let title = s["title"].as_str().unwrap_or("(empty)");
                        println!("  {} ({} messages)", id, msg_count);
                        println!("    {}\n", title);
                    }
                }
            }
            return Ok(());
        }
    }

    let mut ids = list_session_dirs(&sessions_path).await?;

    if let Some(type_filter) = session_type {
        let filter_type: SessionType = type_filter
            .parse()
            .map_err(|_| anyhow!("Invalid session type: {}", type_filter))?;
        ids.retain(|id_str| {
            SessionId::parse(id_str)
                .map(|id| id.session_type() == filter_type)
                .unwrap_or(false)
        });
    }

    ids.reverse();
    ids.truncate(limit as usize);

    if ids.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    match format.as_str() {
        "json" => {
            let json = serde_json::to_string_pretty(&ids)?;
            println!("{json}");
        }
        _ => {
            println!("Sessions (newest first):\n");
            for id_str in &ids {
                let session_dir = sessions_path.join(id_str);
                let events = read_session_events(&session_dir).await.unwrap_or_default();
                let msg_count = events
                    .iter()
                    .filter(|e| matches!(e, LogEvent::User { .. } | LogEvent::Assistant { .. }))
                    .count();

                let title = events
                    .iter()
                    .find_map(|e| match e {
                        LogEvent::User { content, .. } => Some(truncate(content, 50)),
                        _ => None,
                    })
                    .unwrap_or_else(|| "(empty)".to_string());

                println!("  {} ({} messages)", id_str, msg_count);
                println!("    {}\n", title);
            }
        }
    }

    Ok(())
}
