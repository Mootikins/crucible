use crate::common::daemon_client;
use crate::tui::oil::commands::{validate_set_for_cli, SetEffect, SetError, SetRpcAction};

/// Heuristic to distinguish a session ID from a KEY=VALUE setting.
/// Crucible session IDs follow the `chat-YYYYMMDD-HHMM-xxxx` format (always contain
/// hyphens, never contain `=`), while settings are `key=value` or bare `key`.
fn is_session_id(s: &str) -> bool {
    s.contains('-') && !s.contains('=')
}

fn resolve_set_inputs(
    args: Vec<String>,
    session_id_flag: Option<String>,
) -> (Option<String>, Vec<String>, bool) {
    if let Some(flag_id) = session_id_flag {
        return (Some(flag_id), args, true);
    }

    if args.is_empty() {
        return (None, args, false);
    }

    if is_session_id(&args[0]) {
        let session_id = args[0].clone();
        let settings = args[1..].to_vec();
        return (Some(session_id), settings, false);
    }

    (None, args, false)
}

#[cfg(test)]
fn collect_rpc_actions(
    settings: &[String],
) -> Result<Vec<(String, SetRpcAction)>, (String, SetError)> {
    let mut rpc_actions: Vec<(String, SetRpcAction)> = Vec::new();

    for setting in settings {
        match validate_set_for_cli(setting) {
            Ok(SetEffect::DaemonRpc(action)) => rpc_actions.push((setting.clone(), action)),
            Ok(SetEffect::TuiLocal { key, .. }) => {
                return Err((
                    setting.clone(),
                    SetError::InvalidValue {
                        key,
                        message: "this setting is TUI-local".to_string(),
                    },
                ));
            }
            Err(err) => return Err((setting.clone(), err)),
        }
    }

    Ok(rpc_actions)
}

pub async fn execute(args: Vec<String>, session_id_flag: Option<String>) -> anyhow::Result<()> {
    let (resolved_session_id, settings, used_deprecated) =
        resolve_set_inputs(args, session_id_flag);

    if used_deprecated {
        eprintln!("warning: --session flag is deprecated. Use positional SESSION_ID instead: cru set <SESSION_ID> <SETTINGS>");
    }

    if settings.is_empty() {
        eprintln!("error: no settings provided. Use KEY=VALUE syntax (e.g. model=llama3).");
        std::process::exit(1);
    }

    let mut rpc_actions: Vec<(String, SetRpcAction)> = Vec::new();
    for setting in &settings {
        match validate_set_for_cli(setting) {
            Ok(SetEffect::DaemonRpc(action)) => rpc_actions.push((setting.clone(), action)),
            Ok(SetEffect::TuiLocal { key, .. }) => {
                eprintln!(
                    "error: '{}' is a TUI-local setting. Use `cru chat --set {}` instead.",
                    key, setting
                );
                std::process::exit(1);
            }
            Err(SetError::Parse(e)) => {
                eprintln!("error: failed to parse '{}': {}", setting, e);
                std::process::exit(1);
            }
            Err(SetError::NotSupportedAsCli) => {
                eprintln!(
                    "error: '{}' is not supported as a CLI setting. \
                     Use KEY=VALUE syntax (e.g. model=llama3).",
                    setting
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

    let session_id = resolved_session_id
        .or_else(|| std::env::var("CRU_SESSION").ok())
        .unwrap_or_else(|| {
            eprintln!(
                "error: no session specified. Use positional SESSION_ID or set CRU_SESSION env var.\n\
                 \n\
                 Tip: find session IDs with `cru session list`"
            );
            std::process::exit(1);
        });

    let client = daemon_client().await?;

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
            SetRpcAction::SetMaxIterations(max_iterations) => {
                client
                    .session_set_max_iterations(&session_id, *max_iterations)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set max iterations: {}", e))?;
            }
            SetRpcAction::SetExecutionTimeout(timeout_secs) => {
                client
                    .session_set_execution_timeout(&session_id, *timeout_secs)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set execution timeout: {}", e))?;
            }
            SetRpcAction::SetContextBudget(budget) => {
                client
                    .session_set_context_budget(&session_id, *budget)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set context budget: {}", e))?;
            }
            SetRpcAction::SetContextStrategy(strategy) => {
                client
                    .session_set_context_strategy(&session_id, strategy)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set context strategy: {}", e))?;
            }
            SetRpcAction::SetContextWindow(window) => {
                client
                    .session_set_context_window(&session_id, *window)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set context window: {}", e))?;
            }
            SetRpcAction::SetOutputValidation(validation) => {
                client
                    .session_set_output_validation(&session_id, validation)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set output validation: {}", e))?;
            }
            SetRpcAction::SetValidationRetries(retries) => {
                client
                    .session_set_validation_retries(&session_id, *retries)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set validation retries: {}", e))?;
            }
            SetRpcAction::SetPrecognitionResults(count) => {
                client
                    .session_set_precognition_results(&session_id, *count)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to set precognition results: {}", e))?;
            }
        }
        println!("Set {} on session {}", setting_str, session_id);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::oil::commands::{
        validate_set_for_cli, CliValue, SetEffect, SetError, SetRpcAction,
    };

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
            SetEffect::TuiLocal { key, value: CliValue::Enable } if key == "perm.autoconfirm_session"
        ));
    }

    #[test]
    fn validate_tui_local_set() {
        let effect = validate_set_for_cli("perm.autoconfirm_session=true").unwrap();
        assert!(matches!(
            effect,
            SetEffect::TuiLocal { key, value: CliValue::Set(v) }
                if key == "perm.autoconfirm_session" && v == "true"
        ));
    }

    #[test]
    fn validate_toggle_produces_tui_local_toggle_sentinel() {
        let effect = validate_set_for_cli("verbose!").unwrap();
        assert!(matches!(
            effect,
            SetEffect::TuiLocal { key, value: CliValue::Toggle } if key == "verbose"
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
    fn collect_rpc_actions_rejects_tui_local_key() {
        let err = collect_rpc_actions(&["verbose=true".to_string()]).unwrap_err();
        assert!(
            matches!(err, (input, SetError::InvalidValue { key, .. }) if input == "verbose=true" && key == "verbose")
        );
    }

    #[test]
    fn collect_rpc_actions_rejects_missing_explicit_value_for_model() {
        let err = collect_rpc_actions(&["model".to_string()]).unwrap_err();
        assert!(
            matches!(err, (input, SetError::InvalidValue { key, .. }) if input == "model" && key == "model")
        );
    }

    #[test]
    fn is_session_id_with_dashes() {
        assert!(is_session_id("chat-123"));
        assert!(is_session_id("chat-20260217-1030"));
        assert!(is_session_id("session-abc-def"));
    }

    #[test]
    fn is_session_id_rejects_settings() {
        assert!(!is_session_id("model=llama3"));
        assert!(!is_session_id("temperature=0.5"));
        assert!(!is_session_id("thinking"));
    }

    #[test]
    fn resolve_set_inputs_with_positional_session_id() {
        let args = vec!["chat-123".to_string(), "model=llama3".to_string()];
        let (session_id, settings, used_deprecated) = resolve_set_inputs(args, None);
        assert_eq!(session_id, Some("chat-123".to_string()));
        assert!(!used_deprecated);
        assert_eq!(settings, vec!["model=llama3"]);
    }

    #[test]
    fn resolve_set_inputs_with_deprecated_flag() {
        let args = vec!["model=llama3".to_string()];
        let (session_id, settings, used_deprecated) =
            resolve_set_inputs(args, Some("chat-456".to_string()));
        assert_eq!(session_id, Some("chat-456".to_string()));
        assert!(used_deprecated);
        assert_eq!(settings, vec!["model=llama3"]);
    }

    #[test]
    fn resolve_set_inputs_treats_setting_as_first_arg() {
        let args = vec!["model=llama3".to_string(), "temperature=0.5".to_string()];
        let (session_id, settings, used_deprecated) = resolve_set_inputs(args, None);
        assert_eq!(session_id, None);
        assert!(!used_deprecated);
        assert_eq!(
            settings,
            vec!["model=llama3".to_string(), "temperature=0.5".to_string()]
        );
    }

    #[test]
    fn resolve_set_inputs_flag_takes_precedence() {
        let args = vec!["chat-123".to_string(), "model=llama3".to_string()];
        let (session_id, settings, used_deprecated) =
            resolve_set_inputs(args, Some("chat-456".to_string()));
        assert_eq!(session_id, Some("chat-456".to_string()));
        assert!(used_deprecated);
        assert_eq!(
            settings,
            vec!["chat-123".to_string(), "model=llama3".to_string()]
        );
    }
}
