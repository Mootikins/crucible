use super::io::sessions_dir;
use crate::config::CliConfig;
use crate::output;
use anyhow::Result;
use crucible_daemon::SessionId;

pub(super) async fn resume(config: CliConfig, id: String) -> Result<()> {
    let session_id = SessionId::parse(&id)?;
    let sessions_path = sessions_dir(&config);
    let session_dir = sessions_path.join(session_id.as_str());

    if !session_dir.exists() {
        output::hint("Try: `cru session list` to see available sessions");
        anyhow::bail!("Session not found: {}", id);
    }

    crate::commands::chat::execute(crate::commands::chat::ExecuteParams {
        config,
        agent_name: None,
        query: None,
        read_only: false,
        no_context: false,
        context_size: None,
        provider_key: None,
        max_context_tokens: 16384,
        env_overrides: vec![],
        resume_session_id: Some(id),
        set_overrides: vec![],
        record: None,
        replay: None,
        replay_speed: 1.0,
        replay_auto_exit: None,
    })
    .await
}
