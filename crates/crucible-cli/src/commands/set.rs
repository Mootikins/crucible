use crucible_rpc::DaemonClient;

use crate::tui::oil::commands::{validate_set_for_cli, SetEffect, SetError, SetRpcAction};

pub async fn execute(settings: Vec<String>, session_id: Option<String>) -> anyhow::Result<()> {
    let session_id = session_id
        .or_else(|| std::env::var("CRU_SESSION").ok())
        .unwrap_or_else(|| {
            eprintln!(
                "error: no session specified. Use --session <ID> or set CRU_SESSION env var.\n\
                 \n\
                 Tip: find session IDs with `cru session daemon list`"
            );
            std::process::exit(1);
        });

    let mut rpc_actions: Vec<(String, SetRpcAction)> = Vec::new();

    for s in &settings {
        match validate_set_for_cli(s) {
            Ok(SetEffect::DaemonRpc(action)) => {
                rpc_actions.push((s.clone(), action));
            }
            Ok(SetEffect::TuiLocal { key, .. }) => {
                eprintln!(
                    "error: '{}' is a TUI-local setting. Use `cru chat --set {}` instead.",
                    key, s
                );
                std::process::exit(1);
            }
            Err(SetError::Parse(e)) => {
                eprintln!("error: failed to parse '{}': {}", s, e);
                std::process::exit(1);
            }
            Err(SetError::NotSupportedAsCli) => {
                eprintln!(
                    "error: '{}' is not supported as a CLI setting. \
                     Use KEY=VALUE syntax (e.g. model=llama3).",
                    s
                );
                std::process::exit(1);
            }
            Err(SetError::InvalidValue { key, message }) => {
                eprintln!("error: invalid value for '{}': {}", key, message);
                std::process::exit(1);
            }
            Err(SetError::UnknownKey(key)) => {
                eprintln!(
                    "error: unknown setting '{}'. Valid keys: model, temperature, thinkingbudget, maxtokens",
                    key
                );
                std::process::exit(1);
            }
        }
    }

    let client = DaemonClient::connect_or_start()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to daemon: {}", e))?;

    for (setting_str, action) in &rpc_actions {
        match action {
            SetRpcAction::SwitchModel(model) => {
                client
                    .session_switch_model(&session_id, model)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to switch model: {}", e))?;
            }
            SetRpcAction::SetThinkingBudget(budget) => {
                client
                    .session_set_thinking_budget(&session_id, *budget)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set thinking budget: {}", e))?;
            }
            SetRpcAction::SetTemperature(temp) => {
                client
                    .session_set_temperature(&session_id, *temp)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set temperature: {}", e))?;
            }
            SetRpcAction::SetMaxTokens(max_tokens) => {
                client
                    .session_set_max_tokens(&session_id, *max_tokens)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set max tokens: {}", e))?;
            }
        }
        println!("Set {} on session {}", setting_str, session_id);
    }

    Ok(())
}
