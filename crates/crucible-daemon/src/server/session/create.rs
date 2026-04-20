use super::super::*;
use crate::optional_param;

use super::spawn_setup_task;
use crucible_config::McpConfig;
use crucible_core::session::SessionType;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn handle_session_create(
    req: Request,
    sm: &Arc<SessionManager>,
    pm: &Arc<ProjectManager>,
    llm_config: &Option<LlmConfig>,
    km: &Arc<KilnManager>,
    event_tx: &broadcast::Sender<SessionEventMessage>,
    am: &Arc<AgentManager>,
    mcp_config: Option<&McpConfig>,
) -> Response {
    let session_type_str = optional_param!(req, "type", as_str).unwrap_or("chat");
    let session_type: SessionType = match session_type_str.parse() {
        Ok(st) => st,
        Err(_) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!("Invalid session type: {}", session_type_str),
            );
        }
    };

    let kiln = optional_param!(req, "kiln", as_str)
        .map(PathBuf::from)
        .unwrap_or_else(crucible_config::crucible_home);

    let workspace = optional_param!(req, "workspace", as_str).map(PathBuf::from);

    let provider_trust_level = resolve_provider_trust_level_for_create(&req, llm_config);
    let classification = resolve_kiln_classification_for_create(&kiln, workspace.as_ref());
    if let Some(classification) = classification {
        if let Err(message) = validate_trust_level(provider_trust_level, classification) {
            return Response::error(req.id, INVALID_PARAMS, message);
        }
    }

    let connected_kilns: Vec<PathBuf> = req
        .params
        .get("connect_kilns")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(PathBuf::from))
                .collect()
        })
        .unwrap_or_default();

    let recording_mode = optional_param!(req, "recording_mode", as_str)
        .and_then(|s| s.parse::<RecordingMode>().ok());
    let custom_recording_path = optional_param!(req, "recording_path", as_str).map(PathBuf::from);

    // Read locally — drives ACP vs internal branching in the setup task
    // below. `resolve_provider_trust_level_for_create` above already reads
    // this field for trust resolution.
    let agent_type = optional_param!(req, "agent_type", as_str)
        .unwrap_or("internal")
        .to_string();

    let project_path = workspace.as_ref().unwrap_or(&kiln);
    if let Err(e) = pm.register_if_missing(project_path) {
        tracing::warn!(path = %project_path.display(), error = %e, "Failed to auto-register project");
    }

    match sm
        .create_session(
            session_type,
            kiln,
            workspace,
            connected_kilns,
            recording_mode,
        )
        .await
    {
        Ok(session) => {
            // Open the kiln in KilnManager so it's discoverable by session.list()
            if let Err(e) = km.open(&session.kiln).await {
                tracing::warn!(kiln = %session.kiln.display(), error = %e, "Failed to open kiln in manager");
            }

            if session.recording_mode == Some(RecordingMode::Granular) {
                let recording_path = match custom_recording_path {
                    Some(ref p) => p.clone(),
                    None => {
                        let session_dir = FileSessionStorage::session_dir_for(&session);
                        session_dir.join("recording.jsonl")
                    }
                };
                let (writer, tx) = RecordingWriter::new(
                    recording_path,
                    session.id.clone(),
                    RecordingMode::Granular,
                    None,
                );
                sm.set_recording_sender(&session.id, tx);
                let _handle = writer.start();
            }

            // Spawn the setup task. Must not be awaited here — the session
            // must be usable the moment `session.create` returns, even while
            // the task is still indexing / listing providers in the
            // background. Any failures inside the task are logged but never
            // reach the caller.
            spawn_setup_task(
                &session,
                agent_type,
                event_tx.clone(),
                am.clone(),
                mcp_config.cloned(),
            );

            Response::success(
                req.id,
                serde_json::json!({
                    "session_id": session.id,
                    "type": session.session_type.as_prefix(),
                    "kiln": session.kiln,
                    "workspace": session.workspace,
                    "state": format!("{}", session.state),
                }),
            )
        }
        Err(e) => internal_error(req.id, e),
    }
}

pub(crate) fn validate_trust_level(
    provider_trust_level: TrustLevel,
    classification: DataClassification,
) -> Result<(), String> {
    if provider_trust_level.satisfies(classification) {
        return Ok(());
    }

    Err(format!(
        "Provider trust level '{}' is insufficient for kiln data classification '{}'. Requires '{}' trust or higher.",
        provider_trust_level,
        classification,
        classification.required_trust_level()
    ))
}

pub(crate) fn resolve_provider_trust_level_for_create(
    req: &Request,
    llm_config: &Option<LlmConfig>,
) -> TrustLevel {
    if optional_param!(req, "agent_type", as_str) == Some("acp") {
        return TrustLevel::Cloud;
    }

    if let Some(provider_key) = optional_param!(req, "provider_key", as_str) {
        if let Some(config) = llm_config
            .as_ref()
            .and_then(|cfg| cfg.get_provider(provider_key))
        {
            return config.effective_trust_level();
        }
    }

    if let Some(provider_name) = optional_param!(req, "provider", as_str) {
        if let Ok(backend) = provider_name.parse::<crucible_config::BackendType>() {
            return backend.default_trust_level();
        }
    }

    llm_config
        .as_ref()
        .and_then(LlmConfig::default_provider)
        .map(|(_, provider)| provider.effective_trust_level())
        .unwrap_or(TrustLevel::Cloud)
}

pub(crate) fn resolve_kiln_classification_for_create(
    kiln: &Path,
    workspace: Option<&PathBuf>,
) -> Option<DataClassification> {
    let workspace_path = workspace.cloned().unwrap_or_else(|| kiln.to_path_buf());
    crate::trust_resolution::resolve_kiln_classification(&workspace_path, kiln)
}
