//! REPL and slash command handling for OilChatApp.
//!
//! Contains all command parsing and execution logic:
//! `:set`, `:model`, `:export`, `:plugins`, `:mcp`, etc.

use std::path::PathBuf;

use crate::tui::oil::app::Action;
use crate::tui::oil::commands::{
    classify_set_value, SetCommand, SetEffect, SetError, SetRpcAction,
};
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
    "lua",
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

/// Parse a `:set` value string into the JSON scalar it reads as: bool,
/// integer, float, else string. Keeps `:set x=3` and Lua `cru.config.get`
/// agreeing on types.
fn parse_config_scalar(value: &str) -> serde_json::Value {
    if let Ok(b) = value.parse::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(n) = value.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    if let Ok(f) = value.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    serde_json::Value::String(value.to_string())
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
             :set <opt>      — Set option (e.g., :set thinkingbudget=high)\n\
             :export <path>  — Export session to markdown\n\
             :messages       — Toggle notification drawer\n\
             :mcp            — Show MCP server status\n\
             :plugins        — Show loaded plugins\n\
             :reload <name>  — Reload a plugin\n\
             :lua <expr>     — Evaluate Lua (daemon-side; := shorthand)\n\
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
            ":set thinkingbudget=med       — Thinking budget preset\n\
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
            // Alias for :help — the command palette advertises "/help".
            "help" => {
                self.handle_help_repl(parts.get(1).map(|s| s.trim()).filter(|s| !s.is_empty()))
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

        // `:lua <expr>` / `:= <expr>` — Lua escape hatch (evaluated daemon-side
        // via lua.eval; the default command line never evals implicitly).
        if command == "lua" {
            self.notification_area
                .add(crucible_core::types::Notification::warning(
                    "Usage: :lua <expr>  (or := <expr>)".to_string(),
                ));
            return Action::Continue;
        }
        if let Some(code) = command
            .strip_prefix("lua ")
            .or_else(|| command.strip_prefix('='))
        {
            let code = code.trim();
            if code.is_empty() {
                self.notification_area
                    .add(crucible_core::types::Notification::warning(
                        "Usage: :lua <expr>  (or := <expr>)".to_string(),
                    ));
                return Action::Continue;
            }
            return Action::Send(ChatAppMsg::EvalLua(code.to_string()));
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

        // Open the model selection popup. Models are prefetched at startup
        // (daemon cache is warm), so they should be available immediately.
        // If not loaded yet, trigger a background fetch.
        self.input.set_content(":model ");
        self.popup.kind = super::state::AutocompleteKind::Model;
        self.popup.trigger_pos = self.input.cursor();
        self.popup.filter.clear();
        self.popup.selected = 0;
        self.popup.show = true;

        if matches!(
            self.model_list_state,
            ModelListState::NotLoaded | ModelListState::Failed(_)
        ) {
            self.model_list_state = ModelListState::Loading;
            Action::Send(ChatAppMsg::FetchModels)
        } else {
            Action::Continue
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

    /// Dispatches `:set key=value` through the shared classifier so the live
    /// TUI and CLI `--set` accept exactly the same keys and values. Keys the
    /// classifier doesn't know stay TUI-local (plugin/dynamic runtime keys).
    fn dispatch_set_key(&mut self, key: &str, value: String) -> Action<ChatAppMsg> {
        if key.starts_with("perm.") {
            return self.handle_perm_set(key, &value);
        }
        match classify_set_value(key.to_string(), value.clone()) {
            Ok(SetEffect::DaemonRpc(action)) => self.apply_daemon_set_action(key, &value, action),
            Ok(SetEffect::TuiLocal { .. }) => {
                self.runtime_config.set_str(key, &value, ModSource::Command);
                self.sync_runtime_to_fields(key);
                self.send_setting_ack(key, &value);
                Action::Continue
            }
            // Unknown (plugin/dynamic) keys: store locally for `:set key?`
            // round-trips AND mirror into the daemon app-config store so
            // `:lua cru.config.get(key)` and plugins see the same value.
            Err(SetError::UnknownKey(_)) => {
                self.runtime_config.set_str(key, &value, ModSource::Command);
                self.sync_runtime_to_fields(key);
                self.send_setting_ack(key, &value);
                Action::Send(ChatAppMsg::ConfigSet {
                    key: key.to_string(),
                    value: parse_config_scalar(&value),
                })
            }
            Err(e) => {
                self.warn_invalid(e.to_string());
                Action::Continue
            }
        }
    }

    /// Record a validated session-scoped setting locally (runtime config +
    /// ack message), then emit the daemon-sync message for it.
    fn apply_daemon_set_action(
        &mut self,
        key: &str,
        value: &str,
        action: SetRpcAction,
    ) -> Action<ChatAppMsg> {
        match &action {
            SetRpcAction::SwitchModel(model) => {
                self.model = model.clone();
                self.runtime_config.set_dynamic(
                    key,
                    ConfigValue::String(model.clone()),
                    ModSource::Command,
                    &self.current_provider.clone(),
                );
                self.send_setting_ack("model", model);
            }
            SetRpcAction::SetThinkingBudget(budget) => {
                self.runtime_config.set_str(key, value, ModSource::Command);
                let budget = budget.unwrap_or_default();
                self.add_system_message(format!("  thinkingbudget={} ({})", value, budget));
            }
            SetRpcAction::SetMaxIterations(n) => {
                self.runtime_config.set_str(key, value, ModSource::Command);
                let display = n.map_or("none".to_string(), |n| n.to_string());
                self.send_setting_ack("maxiterations", &display);
            }
            SetRpcAction::SetExecutionTimeout(n) => {
                self.runtime_config.set_str(key, value, ModSource::Command);
                let display = n.map_or("none".to_string(), |n| format!("{}s", n));
                self.send_setting_ack("executiontimeout", &display);
            }
            SetRpcAction::SetContextBudget(n) => {
                self.runtime_config.set_str(key, value, ModSource::Command);
                let display = n.map_or("none".to_string(), |n| n.to_string());
                self.send_setting_ack("context_budget", &display);
            }
            SetRpcAction::SetContextStrategy(normalized) => {
                self.runtime_config
                    .set_str(key, normalized, ModSource::Command);
                self.send_setting_ack("context_strategy", normalized);
            }
            SetRpcAction::SetContextWindow(n) => {
                self.runtime_config.set_str(key, value, ModSource::Command);
                let display = n.map_or("none".to_string(), |n| n.to_string());
                self.send_setting_ack("context_window", &display);
            }
            SetRpcAction::SetOutputValidation(v) => {
                self.runtime_config.set_str(key, v, ModSource::Command);
                self.send_setting_ack("output_validation", v);
            }
            SetRpcAction::SetValidationRetries(n) => {
                self.runtime_config.set_str(key, value, ModSource::Command);
                self.send_setting_ack("validation_retries", n);
            }
            SetRpcAction::SetPrecognitionResults(n) => {
                self.runtime_config.set_str(key, value, ModSource::Command);
                self.send_setting_ack("precognition.results", n);
            }
            SetRpcAction::SetAutocompactThreshold(t) => {
                self.runtime_config.set_str(key, value, ModSource::Command);
                let display = match t {
                    Some(v) if *v == 0.0 => "off".to_string(),
                    Some(v) => v.to_string(),
                    None => "default".to_string(),
                };
                self.send_setting_ack("autocompact_threshold", &display);
            }
        }
        match action.into_chat_msg() {
            Some(msg) => Action::Send(msg),
            None => Action::Continue,
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
        let valid_keys = [
            "perm.show_diff",
            "perm.autoconfirm_session",
            "perm.full_commands",
        ];

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
            "show_diffs" => {
                if let Some(val) = self.runtime_config.get("show_diffs") {
                    self.show_diffs = val.as_bool().unwrap_or(true);
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
            "perm.full_commands" => {
                if let Some(val) = self.runtime_config.get("perm.full_commands") {
                    self.permission.perm_full_commands = val.as_bool().unwrap_or(true);
                }
            }
            "theme" => match self.runtime_config.get("theme") {
                Some(ConfigValue::String(name)) => {
                    crate::formatting::syntax::set_active_theme(&name);
                }
                // Reset (`:set theme&`) removed the entry — rendering must
                // revert to the config-seeded theme, not keep the override.
                _ => crate::formatting::syntax::clear_theme_override(),
            },
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

    // ════════════════════════════════════════════════════════════════
    // US-104: `:set` runtime-config dispatch matrix
    // ════════════════════════════════════════════════════════════════

    use crate::tui::oil::app::App;
    use crate::tui::oil::chat_app::ChatAppMsg;
    use test_case::test_case;

    fn app() -> OilChatApp {
        OilChatApp::init()
    }

    /// Run a `:set` body (e.g. `"thinkingbudget=high"`) through the real
    /// command handler and return the resulting action.
    fn run_set(app: &mut OilChatApp, body: &str) -> Action<ChatAppMsg> {
        app.handle_set_command(&format!("set {body}"))
    }

    // Every session-scoped key must emit a daemon-sync `Action::Send` so
    // multi-client state stays consistent (see AGENTS.md cross-layer checklist).
    #[test_case("model=gpt-4o" ; "model")]
    #[test_case("thinkingbudget=high" ; "thinking budget")]
    #[test_case("maxiterations=5" ; "max iterations")]
    #[test_case("executiontimeout=30" ; "execution timeout")]
    #[test_case("contextbudget=128000" ; "context budget")]
    #[test_case("contextstrategy=truncate" ; "context strategy")]
    #[test_case("contextwindow=20" ; "context window")]
    #[test_case("outputvalidation=off" ; "output validation")]
    #[test_case("validationretries=2" ; "validation retries")]
    #[test_case("precognition.results=8" ; "precognition results")]
    #[test_case("autocompact_threshold=0.8" ; "autocompact threshold")]
    #[test_case("autocompactthreshold=0.8" ; "autocompact threshold alias")]
    #[test_case("contextstrategy=summarize" ; "context strategy summarize")]
    fn set_session_key_emits_daemon_sync(body: &str) {
        let mut app = app();
        let action = run_set(&mut app, body);
        assert!(
            matches!(action, Action::Send(_)),
            "session-scoped `:set {body}` must emit a daemon-sync message, got {:?}",
            std::mem::discriminant(&action)
        );
    }

    // Precise variant mapping for the load-bearing keys.
    #[test]
    fn set_thinkingbudget_maps_to_set_thinking_budget() {
        let mut app = app();
        assert!(matches!(
            run_set(&mut app, "thinkingbudget=high"),
            Action::Send(ChatAppMsg::SetThinkingBudget(_))
        ));
    }

    #[test]
    fn set_model_maps_to_switch_model() {
        let mut app = app();
        assert!(matches!(
            run_set(&mut app, "model=gpt-4o"),
            Action::Send(ChatAppMsg::SwitchModel(m)) if m == "gpt-4o"
        ));
    }

    // Regression: `:set autocompact_threshold=…` used to fall through to the
    // generic runtime-config arm and silently skip the daemon RPC while the
    // CLI `--set` path handled it (routing-seam drift).
    #[test]
    fn set_autocompact_threshold_maps_to_daemon_msg() {
        let mut app = app();
        assert!(matches!(
            run_set(&mut app, "autocompact_threshold=0.8"),
            Action::Send(ChatAppMsg::SetAutocompactThreshold(Some(t))) if (t - 0.8).abs() < f32::EPSILON
        ));
        assert!(matches!(
            run_set(&mut app, "autocompact_threshold=off"),
            Action::Send(ChatAppMsg::SetAutocompactThreshold(Some(t))) if t == 0.0
        ));
        assert!(matches!(
            run_set(&mut app, "autocompact_threshold=default"),
            Action::Send(ChatAppMsg::SetAutocompactThreshold(None))
        ));
    }

    #[test]
    fn set_autocompact_threshold_out_of_range_warns() {
        let mut app = app();
        let action = run_set(&mut app, "autocompact_threshold=1.5");
        assert!(matches!(action, Action::Continue));
        assert!(app.has_notifications());
    }

    // Regression: live `:set` rejected `summarize` while `--set` accepted it.
    #[test]
    fn set_contextstrategy_summarize_accepted() {
        let mut app = app();
        assert!(matches!(
            run_set(&mut app, "contextstrategy=summarize"),
            Action::Send(ChatAppMsg::SetContextStrategy(s)) if s == "summarize"
        ));
    }

    #[test]
    fn set_contextstrategy_normalizes_value() {
        let mut app = app();
        assert!(matches!(
            run_set(&mut app, "contextstrategy=sliding_window"),
            Action::Send(ChatAppMsg::SetContextStrategy(s)) if s == "sliding_window"
        ));
    }

    // Set → query round-trips on the same key.
    #[test]
    fn set_then_query_round_trips() {
        let mut app = app();
        run_set(&mut app, "thinkingbudget=high");
        let stored = app
            .runtime_config
            .get("thinkingbudget")
            .expect("value stored");
        assert_eq!(stored.as_string(), Some("high"));
    }

    // Invalid values surface a warning and do NOT emit a daemon sync.
    #[test_case("contextbudget=abc" ; "non-numeric budget")]
    #[test_case("maxiterations=xyz" ; "non-numeric iterations")]
    #[test_case("thinkingbudget=boguspreset" ; "unknown preset")]
    #[test_case("contextstrategy=nonsense" ; "unknown strategy")]
    #[test_case("validationretries=-1" ; "negative retries")]
    fn set_invalid_value_warns_and_no_send(body: &str) {
        let mut app = app();
        let action = run_set(&mut app, body);
        assert!(
            matches!(action, Action::Continue),
            "invalid `:set {body}` must not emit a daemon sync"
        );
        assert!(
            app.has_notifications(),
            "invalid `:set {body}` should surface a warning"
        );
    }

    /// Unknown (plugin/dynamic) keys are stored locally AND mirrored to the
    /// daemon app-config store, so `:lua cru.config.get(key)` sees them.
    #[test]
    fn set_unknown_key_mirrors_to_daemon_config() {
        let mut app = app();
        let action = run_set(&mut app, "myplugin.debug=true");
        assert!(
            matches!(
                action,
                Action::Send(ChatAppMsg::ConfigSet { ref key, ref value })
                    if key == "myplugin.debug" && *value == serde_json::json!(true)
            ),
            "unknown :set keys should mirror to the daemon config store"
        );
        // Still stored locally for `:set key?` round-trips (set_str infers bool).
        let stored = app.runtime_config.get("myplugin.debug").expect("stored");
        assert_eq!(stored.as_bool(), Some(true));
    }

    #[test]
    fn set_unknown_key_value_typing() {
        let mut app = app();
        assert!(matches!(
            run_set(&mut app, "myplugin.retries=3"),
            Action::Send(ChatAppMsg::ConfigSet { value, .. }) if value == serde_json::json!(3)
        ));
        assert!(matches!(
            run_set(&mut app, "myplugin.name=hello world"),
            Action::Send(ChatAppMsg::ConfigSet { value, .. })
                if value == serde_json::json!("hello world")
        ));
    }

    #[test]
    fn set_invalid_perm_key_warns() {
        let mut app = app();
        let action = run_set(&mut app, "perm.bogus=true");
        assert!(matches!(action, Action::Continue));
        assert!(app.has_notifications());
    }

    /// `perm.full_commands` defaults on and round-trips through `:set` into
    /// the permission state that new modals are constructed from.
    #[test]
    fn set_perm_full_commands_round_trips() {
        let mut app = app();
        assert!(
            app.permission.perm_full_commands,
            "full display is the default"
        );

        run_set(&mut app, "perm.full_commands=false");
        assert!(!app.permission.perm_full_commands);

        run_set(&mut app, "perm.full_commands=true");
        assert!(app.permission.perm_full_commands);
    }

    /// `:set theme=<valid syntect theme>` updates the process-wide
    /// highlighting state that diff/code renders read (US-104 honesty: the
    /// knob must do what its ack claims).
    #[test]
    fn set_theme_updates_active_syntax_theme() {
        let _guard = crate::formatting::syntax::ACTIVE_STATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut app = app();
        let action = run_set(&mut app, "theme=Solarized (dark)");
        assert!(matches!(action, Action::Continue));
        assert_eq!(
            crate::formatting::syntax::active_theme_name(),
            "Solarized (dark)"
        );
        let stored = app.runtime_config.get("theme").expect("stored");
        assert_eq!(stored.as_string(), Some("Solarized (dark)"));
    }

    /// `:set theme&` must revert the RENDERED theme, not just the stored
    /// value — otherwise the query reports the default while diffs/code
    /// blocks keep highlighting with the old override.
    #[test]
    fn set_theme_reset_reverts_active_theme_to_seed() {
        let _guard = crate::formatting::syntax::ACTIVE_STATE_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        crate::formatting::syntax::seed_from_config(&crucible_core::config::HighlightingConfig {
            enabled: true,
            theme: "base16-eighties.dark".to_string(),
        });
        let mut app = app();
        run_set(&mut app, "theme=InspiredGitHub");
        assert_eq!(
            crate::formatting::syntax::active_theme_name(),
            "InspiredGitHub"
        );
        run_set(&mut app, "theme&");
        assert_eq!(
            crate::formatting::syntax::active_theme_name(),
            "base16-eighties.dark",
            "reset must revert rendering to the config-seeded theme"
        );
    }

    #[test]
    fn set_theme_invalid_value_warns_and_leaves_state() {
        let mut app = app();
        let action = run_set(&mut app, "theme=no-such-theme");
        assert!(matches!(action, Action::Continue));
        assert!(
            app.has_notifications(),
            "invalid theme should surface a warning listing valid themes"
        );
        assert!(
            app.runtime_config.get("theme").is_none(),
            "rejected value must not be stored"
        );
    }

    #[test]
    fn set_reset_returns_to_base() {
        let mut app = app();
        run_set(&mut app, "thinking=false");
        // `&` resets the key back to its base value.
        let action = app.handle_set_command("set thinking&");
        assert!(matches!(action, Action::Continue));
    }

    #[test]
    fn set_query_unmodified_key_is_continue() {
        let mut app = app();
        let action = app.handle_set_command("set thinkingbudget?");
        assert!(matches!(action, Action::Continue));
    }

    // ════════════════════════════════════════════════════════════════
    // US-103: slash & REPL command dispatch
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn slash_plan_sets_mode_and_syncs() {
        let mut app = app();
        let action = app.handle_slash_command("/plan");
        assert!(matches!(action, Action::Send(ChatAppMsg::ModeChanged(m)) if m == "plan"));
        assert_eq!(app.mode(), ChatMode::Plan);
    }

    #[test]
    fn slash_mode_cycles() {
        let mut app = app();
        assert_eq!(app.mode(), ChatMode::Normal);
        app.handle_slash_command("/mode");
        assert_eq!(app.mode(), ChatMode::Plan);
    }

    #[test]
    fn unknown_slash_forwards_to_agent() {
        let mut app = app();
        let action = app.handle_slash_command("/deploy now");
        assert!(matches!(
            action,
            Action::Send(ChatAppMsg::ExecuteSlashCommand(c)) if c == "/deploy now"
        ));
    }

    #[test]
    fn repl_quit_returns_quit() {
        let mut app = app();
        assert!(matches!(app.handle_repl_command(":quit"), Action::Quit));
        assert!(matches!(app.handle_repl_command(":q"), Action::Quit));
    }

    // ════════════════════════════════════════════════════════════════
    // US-108: `:lua` escape hatch
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn repl_lua_dispatches_eval() {
        let mut app = app();
        assert!(matches!(
            app.handle_repl_command(":lua 1 + 1"),
            Action::Send(ChatAppMsg::EvalLua(code)) if code == "1 + 1"
        ));
    }

    #[test]
    fn repl_eq_shorthand_dispatches_eval() {
        let mut app = app();
        assert!(matches!(
            app.handle_repl_command(":= cru.config.get('model')"),
            Action::Send(ChatAppMsg::EvalLua(code)) if code == "cru.config.get('model')"
        ));
    }

    #[test]
    fn repl_lua_without_body_warns_usage() {
        let mut app = app();
        let action = app.handle_repl_command(":lua");
        assert!(matches!(action, Action::Continue));
        assert!(
            app.has_notifications(),
            ":lua with no code should show usage"
        );
    }

    #[test]
    fn lua_evaled_success_renders_system_message() {
        let mut app = app();
        app.on_message(ChatAppMsg::LuaEvaled {
            output: "2".to_string(),
            is_error: false,
        });
        // Rendered into the viewport as a system message, not just statusline.
        let tree = crate::tui::oil::tests::helpers::view_with_default_ctx(&app);
        let output = crucible_oil::ansi::strip_ansi(&crucible_oil::render_to_string(&tree, 80));
        assert!(
            output.contains('2'),
            "eval result should be visible: {output}"
        );
    }

    #[test]
    fn lua_evaled_error_surfaces_notification() {
        let mut app = app();
        app.on_message(ChatAppMsg::LuaEvaled {
            output: "attempt to index a nil value".to_string(),
            is_error: true,
        });
        assert!(app.has_notifications());
    }

    #[test]
    fn repl_clear_dispatches_clear_history() {
        let mut app = app();
        assert!(matches!(
            app.handle_repl_command(":clear"),
            Action::Send(ChatAppMsg::ClearHistory)
        ));
    }

    #[test]
    fn repl_messages_toggles_drawer() {
        let mut app = app();
        assert!(!app.notification_area.is_visible());
        app.handle_repl_command(":messages");
        assert!(app.notification_area.is_visible());
    }

    #[test]
    fn repl_model_no_arg_opens_picker_and_fetches() {
        let mut app = app();
        let action = app.handle_repl_command(":model");
        assert!(matches!(action, Action::Send(ChatAppMsg::FetchModels)));
        assert!(app.popup.show);
    }

    #[test]
    fn repl_config_show_is_continue() {
        let mut app = app();
        assert!(matches!(
            app.handle_repl_command(":config"),
            Action::Continue
        ));
    }

    #[test]
    fn repl_export_without_session_warns() {
        let mut app = app();
        let action = app.handle_repl_command(":export out.md");
        assert!(matches!(action, Action::Continue));
        assert!(app.has_notifications());
    }

    #[test]
    fn unknown_repl_suggests_nearest_match() {
        let mut app = app();
        // typo of :quit — within levenshtein distance 2
        let action = app.handle_repl_command(":quti");
        assert!(matches!(action, Action::Continue));
        assert!(app.has_notifications(), "typo should surface a suggestion");
    }

    // ════════════════════════════════════════════════════════════════
    // US-902: `/undo` dispatch
    // ════════════════════════════════════════════════════════════════

    #[test]
    fn slash_undo_dispatches_single_turn() {
        let mut app = app();
        assert!(matches!(
            app.handle_slash_command("/undo"),
            Action::Send(ChatAppMsg::Undo(1))
        ));
    }

    #[test]
    fn slash_undo_with_count_dispatches_n() {
        let mut app = app();
        assert!(matches!(
            app.handle_slash_command("/undo 3"),
            Action::Send(ChatAppMsg::Undo(3))
        ));
    }

    #[test]
    fn repl_undo_dispatches() {
        let mut app = app();
        assert!(matches!(
            app.handle_repl_command(":undo"),
            Action::Send(ChatAppMsg::Undo(1))
        ));
        assert!(matches!(
            app.handle_repl_command(":undo 2"),
            Action::Send(ChatAppMsg::Undo(2))
        ));
    }

    #[test]
    fn undo_count_floors_at_one() {
        let mut app = app();
        // "/undo 0" must not revert zero turns — floored to 1.
        assert!(matches!(
            app.handle_slash_command("/undo 0"),
            Action::Send(ChatAppMsg::Undo(1))
        ));
    }
}
