//! REPL and slash command handling for OilChatApp.
//!
//! Contains all command parsing and execution logic:
//! `:set`, `:model`, `:export`, `:plugins`, `:mcp`, etc.

use std::path::PathBuf;

use crate::tui::oil::app::Action;
use crate::tui::oil::commands::SetCommand;
use crate::tui::oil::config::{ConfigValue, ModSource};

use super::messages::ChatAppMsg;
use super::model_state::ModelListState;
use super::state::ChatMode;
use super::OilChatApp;

/// Known REPL command names for suggestion matching.
const KNOWN_REPL_COMMANDS: &[&str] = &[
    "quit",
    "q",
    "help",
    "h",
    "clear",
    "undo",
    "model",
    "set",
    "export",
    "messages",
    "msgs",
    "notifications",
    "palette",
    "commands",
    "mcp",
    "plugins",
    "reload",
    "config",
    "pick",
];

/// Minimal Levenshtein distance for command suggestions.
fn levenshtein(a: &str, b: &str) -> usize {
    let a = a.as_bytes();
    let b = b.as_bytes();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
}

/// Suggest the closest known command for a typo.
fn suggest_command<'a>(input: &str, known: &[&'a str]) -> Option<&'a str> {
    known
        .iter()
        .map(|cmd| (*cmd, levenshtein(input, cmd)))
        .filter(|(_, dist)| *dist <= 2)
        .min_by_key(|(_, dist)| *dist)
        .map(|(cmd, _)| cmd)
}

/// Categorized help text for the :help system.
fn help_text(category: Option<&str>) -> String {
    match category {
        None | Some("") => "Crucible Help\n\n\
             :help commands  — List all REPL commands\n\
             :help keys      — Keybindings reference\n\
             :help config    — Configuration options\n\
             :help tools     — Available agent tools\n\
             \n\
             Type :quit to exit, /command for slash commands"
            .to_string(),
        Some("commands") | Some("cmds") => ":quit, :q      — Exit\n\
             :clear          — Clear conversation\n\
             :undo [N]       — Undo last N agent turns (default 1)\n\
             :model <name>   — Switch model (or list available)\n\
             :set <opt>      — Set option (e.g., :set temperature=0.7)\n\
             :export <path>  — Export session to markdown\n\
             :messages       — Toggle notification drawer\n\
             :mcp            — Show MCP server status\n\
             :plugins        — Show loaded plugins\n\
             :reload <name>  — Reload a plugin\n\
             :palette        — Open command palette (F1)\n\
             :config         — Show current configuration\n\
             :help [topic]   — Show help"
            .to_string(),
        Some("keys") | Some("keybindings") | Some("shortcuts") => "Enter          — Send message\n\
             Ctrl+C         — Cancel / clear input\n\
             Ctrl+T         — Toggle thinking display\n\
             Esc            — Cancel streaming / close popup\n\
             BackTab        — Cycle modes (Normal → Plan → Auto)\n\
             F1             — Command palette\n\
             Tab            — Accept autocomplete\n\
             Up/Down        — Navigate popup / history"
            .to_string(),
        Some("config") | Some("settings") => {
            ":set temperature=0.7          — LLM temperature (0.0-2.0)\n\
             :set maxtokens=4096           — Max output tokens\n\
             :set thinkingbudget=med       — Thinking budget preset\n\
             :set contextbudget=128000     — Context token budget (or 'none')\n\
             :set contextstrategy=truncate — Context strategy (truncate|sliding_window)\n\
             :set contextwindow=20         — Sliding window size (message pairs)\n\
             :set precognition             — Toggle auto-RAG\n\
             :set verbose            — Verbose output\n\
             :set thinking           — Show thinking blocks\n\
             :set model=<name>       — Switch LLM model\n\
             :set                    — Show modified settings\n\
             :set all                — Show all settings"
                .to_string()
        }
        Some("tools") => "Agent tools are provided by the daemon and MCP servers.\n\
             Use :mcp to see connected MCP servers and their tool counts.\n\
             Use :plugins to see loaded plugins and their capabilities.\n\
             Use /mode, /plan, /auto to switch agent modes."
            .to_string(),
        Some(other) => format!(
            "Unknown help topic: '{}'. Try :help for available topics.",
            other
        ),
    }
}

impl OilChatApp {
    pub(super) fn handle_slash_command(&mut self, cmd: &str) -> Action<ChatAppMsg> {
        let parts: Vec<&str> = cmd[1..].splitn(2, ' ').collect();
        let command = parts[0].to_lowercase();

        match command.as_str() {
            "mode" => {
                let next = self.mode.cycle();
                self.set_mode_with_status(next)
            }
            "default" | "normal" => self.set_mode_with_status(ChatMode::Normal),
            "plan" => self.set_mode_with_status(ChatMode::Plan),
            "auto" => self.set_mode_with_status(ChatMode::Auto),
            "undo" => {
                let count = parts
                    .get(1)
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
                    .max(1);
                Action::Send(ChatAppMsg::Undo(count))
            }
            _ => Action::Send(ChatAppMsg::ExecuteSlashCommand(cmd.to_string())),
        }
    }

    pub(super) fn set_mode_with_status(&mut self, mode: ChatMode) -> Action<ChatAppMsg> {
        self.mode = mode;
        self.status = "Ready".to_string();
        Action::Send(ChatAppMsg::ModeChanged(mode.as_str().to_string()))
    }

    pub(super) fn handle_repl_command(&mut self, cmd: &str) -> Action<ChatAppMsg> {
        let command = &cmd[1..];

        if command == "set" || command.starts_with("set ") {
            return self.handle_set_command(command);
        }

        if command == "config show" || command == "config" {
            return self.handle_config_show_command();
        }

        match command {
            "q" | "quit" => Action::Quit,
            "help" | "h" => self.handle_help_repl(None),
            _ if command.starts_with("help ") || command.starts_with("h ") => {
                let topic = command
                    .strip_prefix("help ")
                    .or_else(|| command.strip_prefix("h "))
                    .unwrap_or("")
                    .trim();
                self.handle_help_repl(Some(topic))
            }
            "messages" | "msgs" | "notifications" => {
                self.notification_area.toggle();
                Action::Continue
            }
            "palette" | "commands" => {
                self.popup.show = true;
                self.popup.kind = super::state::AutocompleteKind::Command;
                self.popup.filter.clear();
                self.popup.selected = 0;
                Action::Continue
            }
            "mcp" => {
                self.handle_mcp_command();
                Action::Continue
            }
            "pick" => self.open_picker(None),
            _ if command.starts_with("pick ") => {
                let source = command
                    .strip_prefix("pick ")
                    .expect("starts_with guard")
                    .trim();
                self.open_picker(Some(source))
            }
            "plugins" => {
                self.handle_plugins_command();
                Action::Continue
            }
            "model" => self.handle_model_repl(None),
            _ if command.starts_with("model ") => {
                let name = command
                    .strip_prefix("model ")
                    .expect("starts_with guard")
                    .trim();
                self.handle_model_repl(Some(name))
            }
            "clear" => Action::Send(ChatAppMsg::ClearHistory),
            "undo" => Action::Send(ChatAppMsg::Undo(1)),
            _ if command.starts_with("undo ") => {
                let count_str = command
                    .strip_prefix("undo ")
                    .expect("starts_with guard")
                    .trim();
                let count = count_str.parse::<usize>().unwrap_or(1).max(1);
                Action::Send(ChatAppMsg::Undo(count))
            }
            "reload" => self.handle_reload_repl(None),
            _ if command.starts_with("reload ") => {
                let name = command
                    .strip_prefix("reload ")
                    .expect("starts_with guard")
                    .trim();
                self.handle_reload_repl(Some(name))
            }
            _ if command.starts_with("export ") => {
                let path = command
                    .strip_prefix("export ")
                    .expect("starts_with guard")
                    .trim();
                self.handle_export_command(path)
            }
            _ => {
                // Extract the base command word for suggestion matching
                let base_cmd = command.split_whitespace().next().unwrap_or(command);
                let mut msg = format!("Unknown REPL command: {}", cmd);
                if let Some(suggestion) = suggest_command(base_cmd, KNOWN_REPL_COMMANDS) {
                    msg.push_str(&format!(" Did you mean :{} ?", suggestion));
                }
                self.notification_area
                    .add(crucible_core::types::Notification::warning(msg));
                Action::Continue
            }
        }
    }

    fn handle_help_repl(&mut self, topic: Option<&str>) -> Action<ChatAppMsg> {
        let text = help_text(topic);
        if topic.is_none() {
            // For the overview, also append the slash command list
            let slash_list: String = self
                .slash_commands
                .iter()
                .map(|(name, _)| format!("/{}", name))
                .collect::<Vec<_>>()
                .join(" ");
            if slash_list.is_empty() {
                self.add_system_message(text);
            } else {
                self.add_system_message(format!("{}\n\nSlash commands: {}", text, slash_list));
            }
        } else {
            self.add_system_message(text);
        }
        Action::Continue
    }

    fn handle_model_repl(&mut self, name: Option<&str>) -> Action<ChatAppMsg> {
        if let Some(model_name) = name {
            if model_name.is_empty() {
                self.notification_area
                    .add(crucible_core::types::Notification::warning(
                        "Usage: :model <name>".to_string(),
                    ));
                return Action::Continue;
            }
            return self.handle_set_command(&format!("set model {}", model_name));
        }

        tracing::debug!(target: "crucible_cli::tui::oil::model_flow", state = ?self.model_list_state, "handle_repl_command: model pressed");
        match &self.model_list_state {
            ModelListState::NotLoaded => {
                if !self.model_fetch_message_shown {
                    self.add_system_message("Fetching available models...".to_string());
                    self.model_fetch_message_shown = true;
                }
                Action::Send(ChatAppMsg::FetchModels)
            }
            ModelListState::Loading => {
                if !self.model_fetch_message_shown {
                    self.model_list_state = ModelListState::NotLoaded;
                    self.add_system_message("Retrying model fetch...".to_string());
                    self.model_fetch_message_shown = true;
                }
                Action::Send(ChatAppMsg::FetchModels)
            }
            ModelListState::Loaded => {
                if self.available_models.is_empty() {
                    self.add_system_message(
                        "No models configured. Use :model <name> to switch manually.".to_string(),
                    );
                    Action::Continue
                } else {
                    let current = &self.model;
                    let models_list = self
                        .available_models
                        .iter()
                        .map(|m| {
                            if m == current {
                                format!("  \u{2022} {}  \u{2190} current", m)
                            } else {
                                format!("  \u{2022} {}", m)
                            }
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let msg = format!(
                        "Available models ({}):\n{}",
                        self.available_models.len(),
                        models_list
                    );
                    self.add_system_message(msg);
                    Action::Send(ChatAppMsg::FetchModels)
                }
            }
            ModelListState::Failed(reason) => {
                self.add_system_message(format!(
                    "Retrying model fetch (last error: {})...",
                    reason
                ));
                Action::Send(ChatAppMsg::FetchModels)
            }
        }
    }

    fn handle_reload_repl(&mut self, name: Option<&str>) -> Action<ChatAppMsg> {
        match name {
            Some("") => {
                self.notification_area
                    .add(crucible_core::types::Notification::warning(
                        "Usage: :reload <plugin_name>".to_string(),
                    ));
                Action::Continue
            }
            Some(plugin_name) => Action::Send(ChatAppMsg::ReloadPlugin(plugin_name.to_string())),
            None => {
                // Empty name signals "reload all plugins"
                Action::Send(ChatAppMsg::ReloadPlugin(String::new()))
            }
        }
    }

    pub(super) fn handle_export_command(&mut self, path: &str) -> Action<ChatAppMsg> {
        if path.is_empty() {
            self.notification_area
                .add(crucible_core::types::Notification::warning(
                    "Usage: :export <path>".to_string(),
                ));
            return Action::Continue;
        }

        let expanded = shellexpand::full(path)
            .map(|p| p.into_owned())
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Path expansion failed, using original");
                path.to_string()
            });
        let export_path = PathBuf::from(expanded);

        if let Some(parent) = export_path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                self.notification_area
                    .add(crucible_core::types::Notification::warning(format!(
                        "Parent directory does not exist: {}",
                        parent.display()
                    )));
                return Action::Continue;
            }
        }

        if self.session_dir.is_none() {
            self.notification_area
                .add(crucible_core::types::Notification::warning(
                    "No active session — nothing to export".to_string(),
                ));
            return Action::Continue;
        }

        Action::Send(ChatAppMsg::ExportSession(export_path))
    }

    pub(super) fn handle_set_command(&mut self, command: &str) -> Action<ChatAppMsg> {
        let input = command.strip_prefix("set").unwrap_or(command).trim();

        match SetCommand::parse(input) {
            Ok(cmd) => match cmd {
                SetCommand::ShowModified => {
                    let output = self.runtime_config.format_modified();
                    self.add_system_message(output);
                    Action::Continue
                }
                SetCommand::ShowAll => {
                    let output = self.runtime_config.format_all();
                    self.add_system_message(output);
                    Action::Continue
                }
                SetCommand::Query { key } => {
                    let output = self.runtime_config.format_query(&key);
                    self.add_system_message(output);
                    Action::Continue
                }
                SetCommand::QueryHistory { key } => {
                    let output = self.runtime_config.format_history(&key);
                    self.add_system_message(output);
                    Action::Continue
                }
                SetCommand::Enable { key } => self.handle_set_enable(&key),
                SetCommand::Disable { key } => self.handle_set_disable(&key),
                SetCommand::Toggle { key } => self.handle_set_toggle(&key),
                SetCommand::Reset { key } => {
                    self.runtime_config.reset(&key);
                    self.sync_runtime_to_fields(&key);
                    let output = self.runtime_config.format_query(&key);
                    self.add_system_message(format!("Reset: {}", output.trim()));
                    Action::Continue
                }
                SetCommand::Pop { key } => {
                    if self.runtime_config.pop(&key).is_some() {
                        self.sync_runtime_to_fields(&key);
                        let output = self.runtime_config.format_query(&key);
                        self.add_system_message(output);
                    } else {
                        self.add_system_message(format!("  {} is at base value", key));
                    }
                    Action::Continue
                }
                SetCommand::Set { key, value } => self.dispatch_set_key(&key, value),
            },
            Err(e) => {
                self.warn_invalid(format!("Parse error: {}", e));
                Action::Continue
            }
        }
    }

    /// Dispatches `:set key=value` to the appropriate per-key handler.
    fn dispatch_set_key(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        match key {
            "model" => self.handle_set_model(key, value),
            "thinkingbudget" => self.handle_set_thinking_budget(key, value),
            "temperature" => self.handle_set_temperature(key, value),
            "maxtokens" => self.handle_set_max_tokens(key, value),
            "maxiterations" => self.handle_set_max_iterations(key, value),
            "executiontimeout" => self.handle_set_execution_timeout(key, value),
            "contextbudget" | "context_budget" => self.handle_set_context_budget(key, value),
            "contextstrategy" | "context_strategy" => self.handle_set_context_strategy(key, value),
            "contextwindow" | "context_window" => self.handle_set_context_window(key, value),
            "outputvalidation" | "output_validation" => {
                self.handle_set_output_validation(key, value)
            }
            "validationretries" | "validation_retries" => {
                self.handle_set_validation_retries(key, value)
            }
            "precognition.results" => self.handle_set_precognition_results(key, value),
            k if k.starts_with("perm.") => self.handle_perm_set(key, &value),
            _ => {
                self.runtime_config.set_str(key, &value, ModSource::Command);
                self.sync_runtime_to_fields(key);
                self.send_setting_ack(key, &value);
                Action::Continue
            }
        }
    }

    fn handle_set_model(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        self.model = value.clone();
        self.runtime_config.set_dynamic(
            key,
            ConfigValue::String(value.clone()),
            ModSource::Command,
            &self.current_provider.clone(),
        );
        self.send_setting_ack("model", &value);
        Action::Send(ChatAppMsg::SwitchModel(value))
    }

    fn handle_set_thinking_budget(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        use crate::tui::oil::config::ThinkingPreset;
        if let Some(preset) = ThinkingPreset::by_name(&value) {
            let budget = preset.to_budget();
            self.runtime_config.set_str(key, &value, ModSource::Command);
            self.add_system_message(format!("  thinkingbudget={} ({})", value, budget));
            Action::Send(ChatAppMsg::SetThinkingBudget(budget))
        } else {
            let valid = ThinkingPreset::names().collect::<Vec<_>>().join(", ");
            self.warn_invalid(format!("Unknown preset '{}'. Valid: {}", value, valid));
            Action::Continue
        }
    }

    fn handle_set_temperature(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        match value.parse::<f64>() {
            Ok(temp) if (0.0..=2.0).contains(&temp) => {
                self.runtime_config.set_str(key, &value, ModSource::Command);
                self.send_setting_ack("temperature", temp);
                Action::Send(ChatAppMsg::SetTemperature(temp))
            }
            Ok(_) => {
                self.warn_invalid("Temperature must be between 0.0 and 2.0");
                Action::Continue
            }
            Err(_) => {
                self.warn_invalid(format!("Invalid temperature value: {}", value));
                Action::Continue
            }
        }
    }

    fn handle_set_max_tokens(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        let max_tokens = if value == "none" || value == "null" {
            None
        } else {
            match value.parse::<u32>() {
                Ok(n) => Some(n),
                Err(_) => {
                    self.warn_invalid(format!(
                        "Invalid max_tokens value: {} (use a number or 'none')",
                        value
                    ));
                    return Action::Continue;
                }
            }
        };
        self.runtime_config.set_str(key, &value, ModSource::Command);
        let display = max_tokens.map_or("none".to_string(), |n| n.to_string());
        self.send_setting_ack("maxtokens", &display);
        Action::Send(ChatAppMsg::SetMaxTokens(max_tokens))
    }

    fn handle_set_max_iterations(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        let max_iterations = if value == "none" || value == "null" {
            None
        } else {
            match value.parse::<u32>() {
                Ok(n) => Some(n),
                Err(_) => {
                    self.warn_invalid(format!(
                        "Invalid max_iterations value: {} (use a number or 'none')",
                        value
                    ));
                    return Action::Continue;
                }
            }
        };
        self.runtime_config.set_str(key, &value, ModSource::Command);
        let display = max_iterations.map_or("none".to_string(), |n| n.to_string());
        self.send_setting_ack("maxiterations", &display);
        Action::Send(ChatAppMsg::SetMaxIterations(max_iterations))
    }

    fn handle_set_execution_timeout(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        let timeout_secs = if value == "none" || value == "null" {
            None
        } else {
            match value.parse::<u64>() {
                Ok(n) => Some(n),
                Err(_) => {
                    self.warn_invalid(format!(
                        "Invalid execution_timeout value: {} (use seconds or 'none')",
                        value
                    ));
                    return Action::Continue;
                }
            }
        };
        self.runtime_config.set_str(key, &value, ModSource::Command);
        let display = timeout_secs.map_or("none".to_string(), |n| format!("{}s", n));
        self.send_setting_ack("executiontimeout", &display);
        Action::Send(ChatAppMsg::SetExecutionTimeout(timeout_secs))
    }

    fn handle_set_context_budget(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        let budget = if value == "none" || value == "null" {
            None
        } else {
            match value.parse::<usize>() {
                Ok(n) => Some(n),
                Err(_) => {
                    self.warn_invalid(format!(
                        "Invalid context_budget value: {} (use a number or 'none')",
                        value
                    ));
                    return Action::Continue;
                }
            }
        };
        self.runtime_config.set_str(key, &value, ModSource::Command);
        let display = budget.map_or("none".to_string(), |n| n.to_string());
        self.send_setting_ack("context_budget", &display);
        Action::Send(ChatAppMsg::SetContextBudget(budget))
    }

    fn handle_set_context_strategy(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        let normalized = match value.to_lowercase().as_str() {
            "truncate" => "truncate".to_string(),
            "sliding_window" | "slidingwindow" => "sliding_window".to_string(),
            _ => {
                self.warn_invalid(format!(
                    "Unknown context strategy '{}'. Valid: truncate, sliding_window",
                    value
                ));
                return Action::Continue;
            }
        };
        self.runtime_config
            .set_str(key, &normalized, ModSource::Command);
        self.send_setting_ack("context_strategy", &normalized);
        Action::Send(ChatAppMsg::SetContextStrategy(normalized))
    }

    fn handle_set_context_window(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        let window = if value == "none" || value == "null" {
            None
        } else {
            match value.parse::<usize>() {
                Ok(n) => Some(n),
                Err(_) => {
                    self.warn_invalid(format!(
                        "Invalid context_window value: {} (use a number or 'none')",
                        value
                    ));
                    return Action::Continue;
                }
            }
        };
        self.runtime_config.set_str(key, &value, ModSource::Command);
        let display = window.map_or("none".to_string(), |n| n.to_string());
        self.send_setting_ack("context_window", &display);
        Action::Send(ChatAppMsg::SetContextWindow(window))
    }

    fn handle_set_output_validation(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        // Validate the value parses before sending
        match value.parse::<crucible_core::session::OutputValidation>() {
            Ok(v) => {
                let display = v.to_string();
                self.runtime_config
                    .set_str(key, &display, ModSource::Command);
                self.send_setting_ack("output_validation", &display);
                Action::Send(ChatAppMsg::SetOutputValidation(display))
            }
            Err(e) => {
                self.warn_invalid(format!("Invalid output_validation: {}", e));
                Action::Continue
            }
        }
    }

    fn handle_set_validation_retries(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        match value.parse::<u32>() {
            Ok(n) => {
                self.runtime_config.set_str(key, &value, ModSource::Command);
                self.send_setting_ack("validation_retries", &value);
                Action::Send(ChatAppMsg::SetValidationRetries(n))
            }
            Err(_) => {
                self.warn_invalid(format!(
                    "Invalid validation_retries value: {} (use a non-negative integer)",
                    value
                ));
                Action::Continue
            }
        }
    }

    fn handle_set_precognition_results(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        match value.parse::<usize>() {
            Ok(n) if (1..=20).contains(&n) => {
                self.runtime_config.set_str(key, &value, ModSource::Command);
                self.send_setting_ack("precognition.results", n);
                Action::Send(ChatAppMsg::SetPrecognitionResults(n))
            }
            _ => {
                self.warn_invalid("precognition.results must be 1-20");
                Action::Continue
            }
        }
    }

    fn handle_set_enable(&mut self, key: &str) -> Action<ChatAppMsg> {
        if let Some(current) = self.runtime_config.get(key) {
            if current.as_bool().is_some() {
                self.runtime_config
                    .set(key, ConfigValue::Bool(true), ModSource::Command);
                self.sync_runtime_to_fields(key);
                self.send_setting_ack(key, true);
            } else {
                let output = self.runtime_config.format_query(key);
                self.add_system_message(output);
            }
        } else {
            self.runtime_config
                .set(key, ConfigValue::Bool(true), ModSource::Command);
            self.sync_runtime_to_fields(key);
            self.send_setting_ack(key, true);
        }
        Action::Continue
    }

    fn handle_set_disable(&mut self, key: &str) -> Action<ChatAppMsg> {
        match self.runtime_config.disable(key, ModSource::Command) {
            Ok(()) => {
                self.sync_runtime_to_fields(key);
                self.send_setting_ack(key, false);
            }
            Err(e) => {
                self.warn_invalid(e.to_string());
            }
        }
        Action::Continue
    }

    fn handle_set_toggle(&mut self, key: &str) -> Action<ChatAppMsg> {
        match self.runtime_config.toggle(key, ModSource::Command) {
            Ok(new_val) => {
                self.sync_runtime_to_fields(key);
                self.send_setting_ack(key, new_val);
            }
            Err(e) => {
                self.warn_invalid(e.to_string());
            }
        }
        Action::Continue
    }

    /// Adds a warning notification for invalid input.
    fn warn_invalid(&mut self, msg: impl Into<String>) {
        self.notification_area
            .add(crucible_core::types::Notification::warning(msg.into()));
    }

    /// Acknowledges a setting change with a formatted system message.
    fn send_setting_ack(&mut self, key: &str, value: impl std::fmt::Display) {
        self.add_system_message(format!("  {}={}", key, value));
    }

    pub(super) fn handle_config_show_command(&mut self) -> Action<ChatAppMsg> {
        let mut output = String::from("Configuration:\n");

        let temp = self
            .runtime_config
            .get("temperature")
            .unwrap_or(ConfigValue::String("0.7".to_string()));
        output.push_str(&format!("  temperature: {}\n", temp));

        let tokens = self
            .runtime_config
            .get("maxtokens")
            .unwrap_or(ConfigValue::String("none".to_string()));
        output.push_str(&format!("  max_tokens: {}\n", tokens));

        let budget = self
            .runtime_config
            .get("thinkingbudget")
            .unwrap_or(ConfigValue::String("none".to_string()));
        output.push_str(&format!("  thinking_budget: {}\n", budget));

        let mode = self
            .runtime_config
            .get("mode")
            .unwrap_or(ConfigValue::String("normal".to_string()));
        output.push_str(&format!("  mode: {}\n", mode));

        output.push_str(&format!(
            "  precognition: {}\n",
            self.precognition.precognition
        ));
        output.push_str(&format!(
            "  precognition.results: {}\n",
            self.precognition.precognition_results
        ));

        let ctx_budget = self
            .runtime_config
            .get("context_budget")
            .unwrap_or(ConfigValue::String("none".to_string()));
        output.push_str(&format!("  context_budget: {}\n", ctx_budget));

        let ctx_strategy = self
            .runtime_config
            .get("context_strategy")
            .unwrap_or(ConfigValue::String("truncate".to_string()));
        output.push_str(&format!("  context_strategy: {}\n", ctx_strategy));

        let ctx_window = self
            .runtime_config
            .get("context_window")
            .unwrap_or(ConfigValue::String("none".to_string()));
        output.push_str(&format!("  context_window: {}\n", ctx_window));

        let out_val = self
            .runtime_config
            .get("output_validation")
            .unwrap_or(ConfigValue::String("none".to_string()));
        output.push_str(&format!("  output_validation: {}\n", out_val));

        let val_retries = self
            .runtime_config
            .get("validation_retries")
            .unwrap_or(ConfigValue::String("3".to_string()));
        output.push_str(&format!("  validation_retries: {}\n", val_retries));

        self.add_system_message(output);
        Action::Continue
    }

    pub(super) fn handle_perm_set(&mut self, key: &str, value: &str) -> Action<ChatAppMsg> {
        let valid_keys = ["perm.show_diff", "perm.autoconfirm_session"];

        if !valid_keys.contains(&key) {
            self.notification_area
                .add(crucible_core::types::Notification::warning(format!(
                    "Unknown permission setting: {}. Valid: {}",
                    key,
                    valid_keys.join(", ")
                )));
            return Action::Continue;
        }

        let bool_value = match value.to_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => true,
            "false" | "0" | "no" | "off" => false,
            _ => {
                self.notification_area
                    .add(crucible_core::types::Notification::warning(format!(
                        "Invalid value for {}: '{}'. Use true/false",
                        key, value
                    )));
                return Action::Continue;
            }
        };

        self.runtime_config
            .set(key, ConfigValue::Bool(bool_value), ModSource::Command);
        self.sync_runtime_to_fields(key);

        self.notification_area
            .add(crucible_core::types::Notification::toast(format!(
                "Permission setting updated: {}={}",
                key, bool_value
            )));

        Action::Continue
    }

    pub(super) fn sync_runtime_to_fields(&mut self, key: &str) {
        match key {
            "thinking" => {
                if let Some(val) = self.runtime_config.get("thinking") {
                    self.show_thinking = val.as_bool().unwrap_or(true);
                }
            }
            "model" => {
                if let Some(ConfigValue::String(m)) = self
                    .runtime_config
                    .get_dynamic("model", &self.current_provider.clone())
                {
                    self.model = m;
                }
            }
            "perm.show_diff" => {
                if let Some(val) = self.runtime_config.get("perm.show_diff") {
                    self.permission.perm_show_diff = val.as_bool().unwrap_or(true);
                }
            }
            "perm.autoconfirm_session" => {
                if let Some(val) = self.runtime_config.get("perm.autoconfirm_session") {
                    self.permission.perm_autoconfirm_session = val.as_bool().unwrap_or(false);
                }
            }
            "precognition" => {
                if let Some(val) = self.runtime_config.get("precognition") {
                    self.precognition.precognition = val.as_bool().unwrap_or(true);
                }
            }
            "precognition.results" => {
                if let Some(val) = self.runtime_config.get("precognition.results") {
                    if let Some(n) = val.as_int() {
                        self.precognition.precognition_results = (n as usize).clamp(1, 20);
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn apply_cli_override(
        &mut self,
        key: &str,
        value: crate::tui::oil::commands::CliValue,
    ) {
        match value {
            crate::tui::oil::commands::CliValue::Enable => {
                self.runtime_config
                    .set(key, ConfigValue::Bool(true), ModSource::Cli);
            }
            crate::tui::oil::commands::CliValue::Disable => {
                self.runtime_config
                    .set(key, ConfigValue::Bool(false), ModSource::Cli);
            }
            crate::tui::oil::commands::CliValue::Toggle => {
                let _ = self.runtime_config.toggle(key, ModSource::Cli);
            }
            crate::tui::oil::commands::CliValue::Set(v) => {
                self.runtime_config.set_str(key, &v, ModSource::Cli);
            }
        }
        self.sync_runtime_to_fields(key);
    }

    pub(super) fn handle_plugins_command(&mut self) {
        if self.plugin_status.is_empty() {
            self.add_system_message("No plugins found".to_string());
            return;
        }

        let mut lines = vec![format!("Plugins ({}):", self.plugin_status.len())];
        for entry in &self.plugin_status {
            let (icon, state_label) = match entry.state.as_str() {
                "Active" => ("✓", "active"),
                "Error" => ("✗", "error"),
                "Disabled" => ("○", "disabled"),
                "Discovered" => ("◌", "discovered"),
                "Loaded" => ("✓", "loaded"),
                _ => ("?", entry.state.as_str()),
            };
            let version_part = if entry.version.is_empty() {
                String::new()
            } else {
                format!(" v{}", entry.version)
            };
            let detail = if let Some(ref err) = entry.error {
                format!("({}: {})", state_label, err)
            } else {
                format!("({})", state_label)
            };
            lines.push(format!(
                "  {} {}{} {}",
                icon, entry.name, version_part, detail
            ));
        }
        self.add_system_message(lines.join("\n"));
    }

    pub(super) fn open_picker(&mut self, source: Option<&str>) -> Action<ChatAppMsg> {
        use super::state::{AutocompleteKind, PickSource};

        let pick_source = match source {
            None | Some("all") => PickSource::All,
            Some("notes" | "note") => PickSource::Notes,
            Some("sessions" | "session") => PickSource::Sessions,
            Some("commands" | "command" | "cmd") => PickSource::Commands,
            Some("files" | "file") => PickSource::Files,
            Some(unknown) => {
                self.notification_area
                    .add(crucible_core::types::Notification::warning(format!(
                        "Unknown pick source: '{}'. Valid: notes, sessions, commands, files",
                        unknown
                    )));
                return Action::Continue;
            }
        };

        self.popup.show = true;
        self.popup.kind = AutocompleteKind::Pick {
            source: pick_source,
        };
        self.popup.filter.clear();
        self.popup.selected = 0;
        // Clear input so the picker starts fresh
        self.set_input("");
        Action::Continue
    }

    pub(super) fn handle_mcp_command(&mut self) {
        if self.mcp_servers.is_empty() {
            self.add_system_message("No MCP servers configured".to_string());
            return;
        }

        let mut lines = vec![format!("MCP Servers ({}):", self.mcp_servers.len())];
        for server in &self.mcp_servers {
            let status = if server.connected { "●" } else { "○" };
            lines.push(format!(
                "  {} {} ({}_) - {} tools",
                status, server.name, server.prefix, server.tool_count
            ));
        }
        self.add_system_message(lines.join("\n"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- levenshtein tests ---

    #[test]
    fn levenshtein_identical_strings() {
        assert_eq!(levenshtein("quit", "quit"), 0);
    }

    #[test]
    fn levenshtein_single_char_difference() {
        assert_eq!(levenshtein("quit", "qut"), 1); // deletion
        assert_eq!(levenshtein("quit", "quiit"), 1); // insertion
        assert_eq!(levenshtein("quit", "qxit"), 1); // substitution
    }

    #[test]
    fn levenshtein_empty_strings() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", ""), 3);
        assert_eq!(levenshtein("", "abc"), 3);
    }

    #[test]
    fn levenshtein_completely_different() {
        assert_eq!(levenshtein("abc", "xyz"), 3);
    }

    // --- suggest_command tests ---

    #[test]
    fn suggest_command_exact_match() {
        let known = &["quit", "help", "clear", "model"];
        assert_eq!(suggest_command("quit", known), Some("quit"));
    }

    #[test]
    fn suggest_command_typo_within_distance_2() {
        let known = &["quit", "help", "clear", "model"];
        assert_eq!(suggest_command("quiy", known), Some("quit"));
        assert_eq!(suggest_command("hlep", known), Some("help"));
        assert_eq!(suggest_command("claer", known), Some("clear"));
    }

    #[test]
    fn suggest_command_no_match_beyond_distance_2() {
        let known = &["quit", "help", "clear", "model"];
        assert_eq!(suggest_command("xyzzy", known), None);
        assert_eq!(suggest_command("abcdef", known), None);
    }

    #[test]
    fn suggest_command_picks_closest() {
        let known = &["model", "mode", "models"];
        // "modl" is distance 1 from both "model" and "mode";
        // min_by_key returns the first minimum, so "model" wins
        let result = suggest_command("modl", known);
        assert!(result.is_some());
    }

    #[test]
    fn suggest_command_empty_input() {
        let known = &["quit", "help"];
        // Empty string is distance 4 from "quit" — beyond threshold of 2
        assert_eq!(suggest_command("", known), None);
    }
}
