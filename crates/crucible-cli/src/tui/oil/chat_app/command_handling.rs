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
use super::state::{next_mode, DEFAULT_MODE};
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
            "mode" => self.cycle_mode(),
            "default" => self.set_mode_with_status(DEFAULT_MODE),
            // Every declared mode is its own slash command, so a Lua-declared
            // `review` gets `/review` for free. Placed above the plugin arm so
            // a plugin still cannot shadow `/plan`.
            _ if self.available_modes.contains(&command) => {
                self.set_mode_with_status(&command.clone())
            }
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
            // Plugin-declared commands run via the daemon's plugin registry —
            // an invocation, not a chat message. Checked after the built-ins
            // so a plugin cannot shadow /plan or /help.
            _ if self.plugin_command_names.contains(command.as_str()) => {
                Action::Send(ChatAppMsg::RunPluginCommand {
                    name: command,
                    args: parts
                        .get(1)
                        .map(|s| s.trim().to_string())
                        .unwrap_or_default(),
                })
            }
            _ => Action::Send(ChatAppMsg::ExecuteSlashCommand(cmd.to_string())),
        }
    }

    /// Advance to the next mode the daemon offers (`/mode`, Shift+Tab).
    ///
    /// A current mode absent from the daemon's list cycles nowhere: stepping
    /// into a mode `set_mode` would reject is worse than staying put.
    pub(super) fn cycle_mode(&mut self) -> Action<ChatAppMsg> {
        match next_mode(&self.mode, &self.available_modes) {
            Some(next) => self.set_mode_with_status(&next),
            None => Action::Continue,
        }
    }

    pub(super) fn set_mode_with_status(&mut self, mode: &str) -> Action<ChatAppMsg> {
        self.mode = mode.into();
        self.status = "Ready".to_string();
        Action::Send(ChatAppMsg::ModeChanged(mode.to_string()))
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
            "syntax_theme" => match self.runtime_config.get("syntax_theme") {
                Some(ConfigValue::String(name)) => {
                    crate::formatting::syntax::set_active_theme(&name);
                }
                // Reset (`:set syntax_theme&`) removed the entry — rendering
                // must revert to the config-seeded theme, not keep the override.
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
#[path = "command_handling_tests.rs"]
mod tests;
