//! Session management commands
//!
//! Commands for listing, viewing, resuming, and managing chat sessions.

use crate::cli::SessionCommands;
use crate::common::daemon_client;
use crate::config::CliConfig;
use anyhow::{anyhow, Result};
use crucible_core::config::BackendType;
use std::str::FromStr;

mod acp;
mod cleanup;
mod export;
pub(crate) mod helpers;
mod io;
mod list;
mod reindex;
mod resume;
mod search;
mod show;

#[cfg(test)]
mod tests;

pub use helpers::resolve_session_id;

use acp::rpc;
use helpers::{resolve_permission_mode, resolve_send_inputs, warn_deprecated};

pub async fn execute(config: CliConfig, cmd: SessionCommands) -> Result<()> {
    match cmd {
        SessionCommands::List {
            limit,
            session_type,
            format,
            state,
            all,
            include_children,
        } => {
            list::list(
                config,
                limit,
                session_type,
                format,
                state,
                all,
                include_children,
            )
            .await
        }
        SessionCommands::Search {
            query,
            limit,
            format,
        } => search::search(config, query, limit, format).await,
        SessionCommands::Show { id, format } => {
            let session_id = resolve_session_id(id)?;
            show::show(config, session_id, format).await
        }
        SessionCommands::Open { id } => {
            let session_id = resolve_session_id(id)?;
            resume::resume(config, session_id).await
        }
        SessionCommands::Export {
            id,
            output,
            timestamps,
        } => {
            let session_id = resolve_session_id(id)?;
            export::export(config, session_id, output, timestamps).await
        }
        SessionCommands::Reindex { force } => reindex::reindex(config, force).await,
        SessionCommands::Cleanup {
            older_than,
            dry_run,
        } => cleanup::cleanup(config, older_than, dry_run).await,
        SessionCommands::Create {
            session_type,
            agent,
            recording_mode,
            quiet,
            format,
            title,
            workspace,
            permissions,
        } => {
            let permission_mode = resolve_permission_mode(permissions.as_deref())?;
            let client = daemon_client().await?;
            rpc::create(
                &client,
                &config,
                rpc::CreateParams {
                    session_type: &session_type,
                    agent: agent.as_deref(),
                    recording_mode: recording_mode.as_deref(),
                    quiet,
                    format: &format,
                    title: title.as_deref(),
                    workspace: workspace.as_deref(),
                    permission_mode,
                },
            )
            .await
        }
        SessionCommands::Pause { session_id, format } => {
            let session_id = resolve_session_id(session_id)?;
            let client = daemon_client().await?;
            rpc::pause(&client, &session_id, &format).await
        }
        SessionCommands::Resume { session_id, format } => {
            let session_id = resolve_session_id(session_id)?;
            let client = daemon_client().await?;
            rpc::resume(&client, &session_id, &format).await
        }
        SessionCommands::Unpause { session_id } => {
            let session_id = resolve_session_id(session_id)?;
            warn_deprecated("unpause", "resume");
            let client = daemon_client().await?;
            rpc::resume(&client, &session_id, "text").await
        }
        SessionCommands::End { session_id, format } => {
            let session_id = resolve_session_id(session_id)?;
            let client = daemon_client().await?;
            rpc::end(&client, &session_id, &format).await
        }
        SessionCommands::Send {
            session_id_pos,
            message,
            session_id_flag,
            raw,
            permissions,
        } => {
            let permission_mode = resolve_permission_mode(permissions.as_deref())?;
            let (resolved_session_id, resolved_message_arg, used_deprecated_flag) =
                resolve_send_inputs(session_id_pos, message, session_id_flag);
            if used_deprecated_flag {
                warn_deprecated("--session", "positional SESSION_ID");
            }

            let session_id = resolve_session_id(resolved_session_id)?;
            let message = match resolved_message_arg {
                Some(msg) => crate::commands::stdin::resolve_message(&msg)?,
                None => {
                    if crate::commands::stdin::stdin_is_piped() {
                        crate::commands::stdin::read_stdin_message()?
                    } else {
                        anyhow::bail!("No message provided. Pass a message or pipe stdin.")
                    }
                }
            };
            rpc::send(&config, &session_id, &message, raw, permission_mode).await
        }
        SessionCommands::Configure {
            session_id,
            provider,
            model,
            endpoint,
            format,
        } => {
            let session_id = resolve_session_id(session_id)?;
            let provider_type = BackendType::from_str(&provider)
                .map_err(|e| anyhow!("Invalid provider '{}': {}", provider, e))?;
            let client = daemon_client().await?;
            rpc::configure(
                &client,
                &config,
                &session_id,
                provider_type,
                &model,
                endpoint,
                &format,
            )
            .await
        }
        SessionCommands::Subscribe { session_ids } => rpc::subscribe(&session_ids).await,
        SessionCommands::Load { session_id } => {
            let session_id = resolve_session_id(session_id)?;
            let client = daemon_client().await?;
            rpc::load(&client, &config, &session_id).await
        }
        SessionCommands::Replay {
            recording_path,
            speed,
            raw,
        } => rpc::replay(&config, &recording_path, speed, raw).await,
    }
}
