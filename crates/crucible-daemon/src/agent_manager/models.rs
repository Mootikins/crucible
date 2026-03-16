use super::*;

impl AgentManager {
    /// Resolve provider configuration from either config system.
    ///
    /// Checks `LlmConfig` for configured providers.
    /// Returns `None` if the provider key is not found in either system.
    pub(super) fn resolve_provider_config(&self, provider_key: &str) -> Option<ResolvedProvider> {
        if let Some(llm_provider) = self
            .llm_config
            .as_ref()
            .and_then(|c| c.providers.get(provider_key))
        {
            debug!(
                provider_key = %provider_key,
                source = "llm_config",
                provider_type = %llm_provider.provider_type.as_str(),
                "Resolved provider from llm_config"
            );
            return Some(ResolvedProvider {
                provider_type: llm_provider.provider_type,
                endpoint: Some(llm_provider.endpoint()),
                api_key: llm_provider.api_key.clone(),
                source: "llm_config",
            });
        }

        debug!(
            provider_key = %provider_key,
            "Provider not found in any config"
        );
        None
    }

    /// Parse a model ID into optional provider key and model name.
    ///
    /// Splits on the first `/` and checks if the prefix matches a configured provider key.
    /// Returns `(Some(provider_key), model_name)` if the prefix is a valid provider,
    /// otherwise `(None, model_id)` to treat the entire string as a model name.
    ///
    /// # Examples
    ///
    /// - `"zai/claude-sonnet-4"` → `(Some("zai"), "claude-sonnet-4")` if "zai" is configured
    /// - `"llama3.2"` → `(None, "llama3.2")` (no `/` separator)
    /// - `"unknown/model"` → `(None, "unknown/model")` if "unknown" is not configured
    /// - `"library/llama3:latest"` → `(Some("library"), "llama3:latest")` if "library" is configured
    pub(super) fn parse_provider_model(&self, model_id: &str) -> (Option<String>, String) {
        if let Some((prefix, model_name)) = model_id.split_once('/') {
            if let Some(ref llm_config) = self.llm_config {
                if llm_config.providers.contains_key(prefix) {
                    return (Some(prefix.to_string()), model_name.to_string());
                }
            }
        }
        (None, model_id.to_string())
    }

    pub async fn switch_model(
        &self,
        session_id: &str,
        model_id: &str,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        let model_id = model_id.trim();
        if model_id.is_empty() {
            return Err(AgentError::InvalidModelId(
                "Model ID cannot be empty".to_string(),
            ));
        }

        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }

        let mut session = self
            .session_manager
            .get_session(session_id)
            .ok_or_else(|| AgentError::SessionNotFound(session_id.to_string()))?;

        let mut agent_config = session
            .agent
            .clone()
            .ok_or_else(|| AgentError::NoAgentConfigured(session_id.to_string()))?;

        let (provider_key_opt, model_name) = self.parse_provider_model(model_id);

        if let Some(provider_key) = provider_key_opt {
            if let Some(resolved) = self.resolve_provider_config(&provider_key) {
                info!(
                    session_id = %session_id,
                    provider_key = %provider_key,
                    model = %model_name,
                    source = %resolved.source,
                    "Resolved provider '{}' via {}",
                    provider_key,
                    resolved.source,
                );

                agent_config.provider = resolved.provider_type;
                agent_config.provider_key = Some(provider_key);
                agent_config.endpoint = resolved.endpoint;
                agent_config.model = model_name;
            } else {
                info!(
                    session_id = %session_id,
                    model = %model_id,
                    "No provider config found for prefix, treating as model-only switch"
                );
                agent_config.model = model_id.to_string();
            }
        } else {
            info!(
                session_id = %session_id,
                model = %model_name,
                "Model-only switch (no provider prefix)"
            );
            agent_config.model = model_name;
        }

        session.agent = Some(agent_config.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        self.agent_cache.remove(session_id);

        info!(
            session_id = %session_id,
            model = %agent_config.model,
            provider = %agent_config.provider,
            "Model switched for session (agent cache invalidated)"
        );

        if let Some(tx) = event_tx {
            if !emit_event(
                tx,
                SessionEventMessage::model_switched(
                    session_id,
                    &agent_config.model,
                    agent_config.provider.as_str(),
                ),
            ) {
                tracing::debug!("Failed to emit model_switched event (no subscribers)");
            }
        }

        Ok(())
    }

    pub async fn list_models(
        &self,
        session_id: &str,
        classification: Option<DataClassification>,
    ) -> Result<Vec<String>, AgentError> {
        let cache_key = classification
            .map(|value| format!("classification:{}", value.as_str()))
            .unwrap_or_else(|| "all".to_string());

        if classification.is_none() {
            if let Some(entry) = self.model_cache.get(cache_key.as_str()) {
                let (models, fetched_at) = entry.value();
                if fetched_at.elapsed() < MODEL_CACHE_TTL {
                    return Ok(models.clone());
                }
            }
        }

        let mut all_models = Vec::new();

        for (provider_key, provider_config, _) in self.iter_chat_providers(classification) {
            let models = self.discover_models(&provider_key, &provider_config).await;
            for model in models {
                all_models.push(format!("{}/{}", provider_key, model));
            }
        }

        // Only fall back to session agent's provider when no llm_config providers exist.
        // When providers are configured but return empty (discovery failed, no available_models),
        // that's expected — the user should configure available_models or fix their endpoint.
        if all_models.is_empty()
            && self
                .llm_config
                .as_ref()
                .is_none_or(|c| c.providers.is_empty())
        {
            let (_, agent_config) = self.get_session_with_agent(session_id)?;

            let endpoint = agent_config
                .endpoint
                .unwrap_or_else(|| crucible_config::DEFAULT_OLLAMA_ENDPOINT.to_string());
            let backend = agent_config.provider;

            match model_listing::list_models(backend, &endpoint, None).await {
                Ok(models) if !models.is_empty() => return Ok(models),
                Ok(_) => {
                    debug!(provider = %backend, "Fallback model listing returned empty");
                }
                Err(e) => {
                    warn!(error = %e, provider = %backend, "Fallback model listing failed");
                    all_models.push(format!("[error] {}: {}", backend.as_str(), e));
                }
            }
        }

        if !all_models.iter().any(|model| model.starts_with("[error]")) {
            self.model_cache
                .insert(cache_key, (all_models.clone(), Instant::now()));
        }

        Ok(all_models)
    }

    /// Dispatch model discovery based on backend type.
    ///
    /// Always returns a model list (never fails). Falls back to
    /// `effective_models()` from config when discovery errors or returns empty.
    pub(super) async fn discover_models(
        &self,
        provider_key: &str,
        provider_config: &LlmProviderConfig,
    ) -> Vec<String> {
        if provider_config.available_models.is_some() {
            return provider_config.effective_models();
        }
        let endpoint = provider_config.endpoint();
        let api_key = provider_config.api_key();

        match model_listing::list_models(
            provider_config.provider_type,
            &endpoint,
            api_key.as_deref(),
        )
        .await
        {
            Ok(models) if models.is_empty() => provider_config.effective_models(),
            Ok(models) => models,
            Err(e) => {
                warn!(
                    provider_key = %provider_key,
                    error = %e,
                    "Dynamic model discovery failed, using effective_models fallback"
                );
                provider_config.effective_models()
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // Private helper; grouping into a struct would be over-engineering
    async fn update_agent_config_and_emit<Mutate, OnUpdated>(
        &self,
        session_id: &str,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
        event_type: &str,
        event_payload: serde_json::Value,
        no_subscribers_debug: &str,
        mutator: Mutate,
        on_updated: OnUpdated,
    ) -> Result<(), AgentError>
    where
        Mutate: FnOnce(&mut SessionAgent) -> Result<(), AgentError>,
        OnUpdated: FnOnce(),
    {
        if self.request_state.contains_key(session_id) {
            return Err(AgentError::ConcurrentRequest(session_id.to_string()));
        }

        let (mut session, mut agent_config) = self.get_session_with_agent(session_id)?;
        mutator(&mut agent_config)?;
        session.agent = Some(agent_config);

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        self.invalidate_agent_cache(session_id);
        on_updated();

        if let Some(tx) = event_tx {
            if !emit_event(
                tx,
                SessionEventMessage::new(session_id, event_type, event_payload),
            ) {
                tracing::debug!("{}", no_subscribers_debug);
            }
        }

        Ok(())
    }

    pub async fn set_thinking_budget(
        &self,
        session_id: &str,
        budget: i64,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        self.update_agent_config_and_emit(
            session_id,
            event_tx,
            "thinking_budget_changed",
            serde_json::json!({ "budget": budget }),
            "Failed to emit thinking_budget_changed event (no subscribers)",
            |agent_config| {
                agent_config.thinking_budget = Some(budget);
                Ok(())
            },
            || {
                info!(
                    session_id = %session_id,
                    budget = budget,
                    "Thinking budget updated (agent cache invalidated)"
                );
            },
        )
        .await
    }

    pub fn get_thinking_budget(&self, session_id: &str) -> Result<Option<i64>, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        Ok(agent_config.thinking_budget)
    }

    pub async fn set_system_prompt(
        &self,
        session_id: &str,
        prompt: &str,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        if self.agent_cache.get(session_id).is_some() {
            return Err(AgentError::InvalidConfig(
                "system_prompt is locked after the first message has been sent".to_string(),
            ));
        }
        self.update_agent_config_and_emit(
            session_id,
            event_tx,
            "system_prompt_changed",
            serde_json::json!({ "system_prompt": prompt }),
            "Failed to emit system_prompt_changed event (no subscribers)",
            |agent_config| {
                agent_config.system_prompt = prompt.to_string();
                Ok(())
            },
            || {
                info!(
                    session_id = %session_id,
                    "System prompt updated (agent cache invalidated)"
                );
            },
        )
        .await
    }

    pub fn get_system_prompt(&self, session_id: &str) -> Result<Option<String>, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        let prompt = &agent_config.system_prompt;
        if prompt.is_empty() {
            Ok(None)
        } else {
            Ok(Some(prompt.clone()))
        }
    }

    pub async fn set_precognition(
        &self,
        session_id: &str,
        enabled: bool,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        self.update_agent_config_and_emit(
            session_id,
            event_tx,
            "precognition_toggled",
            serde_json::json!({ "enabled": enabled }),
            "Failed to emit precognition_toggled event (no subscribers)",
            |agent_config| {
                agent_config.precognition_enabled = enabled;
                Ok(())
            },
            || {
                info!(
                    session_id = %session_id,
                    enabled = enabled,
                    "Precognition toggle updated (agent cache invalidated)"
                );
            },
        )
        .await
    }

    pub fn get_precognition(&self, session_id: &str) -> Result<bool, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        Ok(agent_config.precognition_enabled)
    }

    pub async fn set_temperature(
        &self,
        session_id: &str,
        temperature: f64,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        self.update_agent_config_and_emit(
            session_id,
            event_tx,
            "temperature_changed",
            serde_json::json!({ "temperature": temperature }),
            "Failed to emit temperature_changed event (no subscribers)",
            |agent_config| {
                agent_config.temperature = Some(temperature);
                Ok(())
            },
            || {
                info!(
                    session_id = %session_id,
                    temperature = temperature,
                    "Temperature updated (agent cache invalidated)"
                );
            },
        )
        .await
    }

    pub fn get_temperature(&self, session_id: &str) -> Result<Option<f64>, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        Ok(agent_config.temperature)
    }

    pub async fn add_notification(
        &self,
        session_id: &str,
        notification: crucible_core::types::Notification,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        let mut session = self.get_session(session_id)?;

        session.notifications.add(notification.clone());

        self.session_manager
            .update_session(&session)
            .await
            .map_err(AgentError::Session)?;

        info!(
            session_id = %session_id,
            notification_id = %notification.id,
            "Notification added"
        );

        if let Some(tx) = event_tx {
            if !emit_event(
                tx,
                SessionEventMessage::new(
                    session_id,
                    "notification_added",
                    serde_json::json!({ "notification_id": notification.id }),
                ),
            ) {
                tracing::debug!("Failed to emit notification_added event (no subscribers)");
            }
        }

        Ok(())
    }

    pub async fn list_notifications(
        &self,
        session_id: &str,
    ) -> Result<Vec<crucible_core::types::Notification>, AgentError> {
        let session = self.get_session(session_id)?;
        Ok(session.notifications.list())
    }

    pub async fn dismiss_notification(
        &self,
        session_id: &str,
        notification_id: &str,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<bool, AgentError> {
        let mut session = self.get_session(session_id)?;

        let success = session.notifications.dismiss(notification_id);

        if success {
            self.session_manager
                .update_session(&session)
                .await
                .map_err(AgentError::Session)?;

            info!(
                session_id = %session_id,
                notification_id = %notification_id,
                "Notification dismissed"
            );

            if let Some(tx) = event_tx {
                if !emit_event(
                    tx,
                    SessionEventMessage::new(
                        session_id,
                        "notification_dismissed",
                        serde_json::json!({ "notification_id": notification_id }),
                    ),
                ) {
                    tracing::debug!("Failed to emit notification_dismissed event (no subscribers)");
                }
            }
        }

        Ok(success)
    }

    pub async fn set_max_tokens(
        &self,
        session_id: &str,
        max_tokens: Option<u32>,
        event_tx: Option<&broadcast::Sender<SessionEventMessage>>,
    ) -> Result<(), AgentError> {
        self.update_agent_config_and_emit(
            session_id,
            event_tx,
            "max_tokens_changed",
            serde_json::json!({ "max_tokens": max_tokens }),
            "Failed to emit max_tokens_changed event (no subscribers)",
            |agent_config| {
                agent_config.max_tokens = max_tokens;
                Ok(())
            },
            || {
                info!(
                    session_id = %session_id,
                    max_tokens = ?max_tokens,
                    "Max tokens updated (agent cache invalidated)"
                );
            },
        )
        .await
    }

    pub fn get_max_tokens(&self, session_id: &str) -> Result<Option<u32>, AgentError> {
        let (_, agent_config) = self.get_session_with_agent(session_id)?;
        Ok(agent_config.max_tokens)
    }
}
