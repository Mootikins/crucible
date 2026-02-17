use crucible_rpc::DaemonClient;

use crate::tui::oil::commands::{validate_set_for_cli, SetEffect, SetError, SetRpcAction};

#[derive(Debug, Clone, PartialEq)]
enum CliSetError {
    MissingSession,
    TuiLocalSetting { key: String, input: String },
    Parse { input: String, error: String },
    NotSupportedAsCli { input: String },
    InvalidValue { key: String, message: String },
    UnknownKey(String),
}

fn resolve_session_id(
    explicit_session_id: Option<String>,
    env_session_id: Option<String>,
) -> Result<String, CliSetError> {
    explicit_session_id
        .or(env_session_id)
        .ok_or(CliSetError::MissingSession)
}

fn collect_rpc_actions(settings: &[String]) -> Result<Vec<(String, SetRpcAction)>, CliSetError> {
    let mut rpc_actions: Vec<(String, SetRpcAction)> = Vec::new();

    for setting in settings {
        match validate_set_for_cli(setting) {
            Ok(SetEffect::DaemonRpc(action)) => rpc_actions.push((setting.clone(), action)),
            Ok(SetEffect::TuiLocal { key, .. }) => {
                return Err(CliSetError::TuiLocalSetting {
                    key,
                    input: setting.clone(),
                });
            }
            Err(SetError::Parse(e)) => {
                return Err(CliSetError::Parse {
                    input: setting.clone(),
                    error: e.to_string(),
                });
            }
            Err(SetError::NotSupportedAsCli) => {
                return Err(CliSetError::NotSupportedAsCli {
                    input: setting.clone(),
                });
            }
            Err(SetError::InvalidValue { key, message }) => {
                return Err(CliSetError::InvalidValue { key, message });
            }
            Err(SetError::UnknownKey(key)) => {
                return Err(CliSetError::UnknownKey(key));
            }
        }
    }

    Ok(rpc_actions)
}

pub async fn execute(settings: Vec<String>, session_id: Option<String>) -> anyhow::Result<()> {
    let session_id = match resolve_session_id(session_id, std::env::var("CRU_SESSION").ok()) {
        Ok(id) => id,
        Err(CliSetError::MissingSession) => {
            eprintln!(
                "error: no session specified. Use --session <ID> or set CRU_SESSION env var.\n\
                 \n\
                 Tip: find session IDs with `cru session daemon list`"
            );
            std::process::exit(1);
        }
        Err(_) => unreachable!("resolve_session_id only returns MissingSession"),
    };

    let rpc_actions = match collect_rpc_actions(&settings) {
        Ok(actions) => actions,
        Err(CliSetError::TuiLocalSetting { key, input }) => {
            eprintln!(
                "error: '{}' is a TUI-local setting. Use `cru chat --set {}` instead.",
                key, input
            );
            std::process::exit(1);
        }
        Err(CliSetError::Parse { input, error }) => {
            eprintln!("error: failed to parse '{}': {}", input, error);
            std::process::exit(1);
        }
        Err(CliSetError::NotSupportedAsCli { input }) => {
            eprintln!(
                "error: '{}' is not supported as a CLI setting. \
                 Use KEY=VALUE syntax (e.g. model=llama3).",
                input
            );
            std::process::exit(1);
        }
        Err(CliSetError::InvalidValue { key, message }) => {
            eprintln!("error: invalid value for '{}': {}", key, message);
            std::process::exit(1);
        }
        Err(CliSetError::UnknownKey(key)) => {
            eprintln!(
                "error: unknown setting '{}'. Valid keys: model, temperature, thinkingbudget, maxtokens",
                key
            );
            std::process::exit(1);
        }
        Err(CliSetError::MissingSession) => unreachable!("session already resolved"),
    };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::commands::{SetEffect, SetError, SetRpcAction, validate_set_for_cli};

    #[test]
    fn validate_model_ok() {
        let effect = validate_set_for_cli("model=llama3").unwrap();
        assert!(matches!(
            effect,
            SetEffect::DaemonRpc(SetRpcAction::SwitchModel(m)) if m == "llama3"
        ));
    }

    #[test]
    fn validate_temperature_out_of_range() {
        let err = validate_set_for_cli("temperature=3.0").unwrap_err();
        assert!(matches!(err, SetError::InvalidValue { .. }));
    }

    #[test]
    fn validate_read_only_rejected() {
        let err = validate_set_for_cli("model?").unwrap_err();
        assert_eq!(err, SetError::NotSupportedAsCli);
        let err2 = validate_set_for_cli("").unwrap_err();
        assert_eq!(err2, SetError::NotSupportedAsCli);
    }

    #[test]
    fn validate_unknown_key() {
        let err = validate_set_for_cli("nonexistent=value").unwrap_err();
        assert!(matches!(err, SetError::UnknownKey(_)));
    }

    #[test]
    fn validate_tui_local_enable() {
        let effect = validate_set_for_cli("perm.autoconfirm_session").unwrap();
        assert!(matches!(
            effect,
            SetEffect::TuiLocal { key, value: None } if key == "perm.autoconfirm_session"
        ));
    }

    #[test]
    fn validate_tui_local_set() {
        let effect = validate_set_for_cli("perm.autoconfirm_session=true").unwrap();
        assert!(matches!(
            effect,
            SetEffect::TuiLocal { key, value: Some(v) }
                if key == "perm.autoconfirm_session" && v == "true"
        ));
    }

    #[test]
    fn validate_toggle_produces_tui_local_toggle_sentinel() {
        let effect = validate_set_for_cli("verbose!").unwrap();
        assert!(matches!(
            effect,
            SetEffect::TuiLocal { key, value: Some(v) } if key == "verbose" && v == "__toggle__"
        ));
    }

    #[test]
    fn validate_maxtokens_none() {
        let effect = validate_set_for_cli("maxtokens=none").unwrap();
        assert!(matches!(
            effect,
            SetEffect::DaemonRpc(SetRpcAction::SetMaxTokens(None))
        ));
    }

    #[test]
    fn validate_daemon_rpc_keys_thinkingbudget_and_maxtokens_number() {
        assert!(matches!(
            validate_set_for_cli("thinkingbudget=high").unwrap(),
            SetEffect::DaemonRpc(SetRpcAction::SetThinkingBudget(Some(_)))
        ));
        assert!(matches!(
            validate_set_for_cli("maxtokens=4096").unwrap(),
            SetEffect::DaemonRpc(SetRpcAction::SetMaxTokens(Some(4096)))
        ));
    }

    #[test]
    fn resolve_session_id_prefers_argument_over_env() {
        let session = resolve_session_id(
            Some("arg-session".to_string()),
            Some("env-session".to_string()),
        )
        .unwrap();
        assert_eq!(session, "arg-session");
    }

    #[test]
    fn resolve_session_id_uses_cru_session_env() {
        let session = resolve_session_id(None, Some("env-session".to_string())).unwrap();
        assert_eq!(session, "env-session");
    }

    #[test]
    fn resolve_session_id_without_any_source_errors() {
        let err = resolve_session_id(None, None).unwrap_err();
        assert_eq!(err, CliSetError::MissingSession);
    }

    #[test]
    fn collect_rpc_actions_rejects_tui_local_key() {
        let err = collect_rpc_actions(&["verbose=true".to_string()]).unwrap_err();
        assert_eq!(
            err,
            CliSetError::TuiLocalSetting {
                key: "verbose".to_string(),
                input: "verbose=true".to_string(),
            }
        );
    }

    #[test]
    fn collect_rpc_actions_rejects_missing_explicit_value_for_model() {
        let err = collect_rpc_actions(&["model".to_string()]).unwrap_err();
        assert!(matches!(err, CliSetError::InvalidValue { key, .. } if key == "model"));
    }
}
