//! Popup autocomplete logic for OilChatApp.
//!
//! Trigger detection, item filtering, completion insertion, and popup lifecycle.
//! Uses nucleo fuzzy matching (via `crucible_core::fuzzy`) for ranked results.

use crucible_core::fuzzy::FuzzyMatcher;

use crate::tui::oil::app::Action;
use crate::tui::oil::event::InputAction;
use crate::tui::oil::node::PopupItemNode;

use super::messages::ChatAppMsg;
use super::model_state::ModelListState;
use super::state::AutocompleteKind;
use super::OilChatApp;

impl OilChatApp {
    pub(super) fn check_autocomplete_trigger(&mut self) -> Option<Action<ChatAppMsg>> {
        let content = self.input.content();
        let cursor = self.input.cursor();

        if let Some((kind, trigger_pos, filter)) = self.detect_trigger(content, cursor) {
            let needs_model_fetch = kind == AutocompleteKind::Model
                && matches!(
                    self.model_list_state,
                    ModelListState::NotLoaded | ModelListState::Failed(_)
                );

            self.popup.kind = kind;
            self.popup.trigger_pos = trigger_pos;
            self.popup.filter = filter;
            self.popup.selected = 0;
            self.popup.show = !self.get_popup_items().is_empty();

            // Force popup visible during Loading state so user sees a loading indicator
            if self.popup.kind == AutocompleteKind::Model
                && matches!(self.model_list_state, ModelListState::Loading)
            {
                self.popup.show = true;
            }

            if needs_model_fetch {
                self.popup.show = true;
                return Some(Action::Send(ChatAppMsg::FetchModels));
            }
        } else if self.popup.kind != AutocompleteKind::None {
            self.popup.kind = AutocompleteKind::None;
            self.popup.filter.clear();
            self.popup.show = false;
        }
        None
    }

    pub(super) fn detect_trigger(
        &self,
        content: &str,
        cursor: usize,
    ) -> Option<(AutocompleteKind, usize, String)> {
        let before_cursor = &content[..cursor];

        if let Some(slash_pos) = before_cursor.rfind('/') {
            let preceded_by_whitespace = slash_pos == 0
                || before_cursor[..slash_pos]
                    .chars()
                    .last()
                    .is_some_and(char::is_whitespace);
            if preceded_by_whitespace {
                let filter = &before_cursor[slash_pos + 1..];
                if !filter.contains(char::is_whitespace) {
                    return Some((
                        AutocompleteKind::SlashCommand,
                        slash_pos,
                        filter.to_string(),
                    ));
                }
            }
        }

        if let Some(at_pos) = before_cursor.rfind('@') {
            let after_at = &before_cursor[at_pos + 1..];
            if !after_at.contains(char::is_whitespace) {
                return Some((AutocompleteKind::File, at_pos, after_at.to_string()));
            }
        }

        if let Some(bracket_pos) = before_cursor.rfind("[[") {
            let after_bracket = &before_cursor[bracket_pos + 2..];
            if !after_bracket.contains("]]") {
                return Some((
                    AutocompleteKind::Note,
                    bracket_pos,
                    after_bracket.to_string(),
                ));
            }
        }

        if let Some(colon_pos) = before_cursor.rfind(':') {
            let preceded_by_whitespace = colon_pos == 0
                || before_cursor[..colon_pos]
                    .chars()
                    .last()
                    .is_some_and(char::is_whitespace);
            if preceded_by_whitespace {
                let after_colon = &before_cursor[colon_pos + 1..];
                if let Some(space_pos) = after_colon.find(char::is_whitespace) {
                    let command = after_colon[..space_pos].to_string();
                    let args_part = after_colon[space_pos..].trim_start();
                    let filter = args_part
                        .split_whitespace()
                        .last()
                        .unwrap_or("")
                        .to_string();
                    let trigger_pos = cursor - filter.len();

                    if command == "model" {
                        return Some((AutocompleteKind::Model, trigger_pos, filter));
                    }

                    if command == "set" {
                        return Some((
                            AutocompleteKind::SetOption { option: None },
                            trigger_pos,
                            filter,
                        ));
                    }

                    let arg_index = args_part.split_whitespace().count();
                    return Some((
                        AutocompleteKind::CommandArg { command, arg_index },
                        trigger_pos,
                        filter,
                    ));
                } else {
                    return Some((
                        AutocompleteKind::ReplCommand,
                        colon_pos,
                        after_colon.to_string(),
                    ));
                }
            }
        }

        None
    }

    pub(super) fn toggle_command_palette(&mut self) {
        if self.popup.show {
            self.close_popup();
        } else {
            self.popup.show = true;
            self.popup.kind = AutocompleteKind::Command;
            self.popup.filter.clear();
        }
        self.popup.selected = 0;
    }

    pub(super) fn close_popup(&mut self) {
        self.popup.show = false;
        self.popup.kind = AutocompleteKind::None;
        self.popup.filter.clear();
    }

    pub(super) fn get_popup_items(&self) -> Vec<PopupItemNode> {
        let filter = self.popup.filter.to_lowercase();

        match self.popup.kind {
            AutocompleteKind::File => {
                Self::filter_to_popup_items(&self.workspace_files, &filter, "file", 15)
            }
            AutocompleteKind::Note => {
                Self::filter_to_popup_items(&self.kiln_notes, &filter, "note", 15)
            }
            AutocompleteKind::Command => Self::filter_commands(
                &[
                    ("semantic_search", "Search notes by meaning", "tool"),
                    ("create_note", "Create a new note", "tool"),
                    ("/mode", "Cycle chat mode", "command"),
                    ("/help", "Show help", "command"),
                ],
                &filter,
            ),
            AutocompleteKind::SlashCommand => {
                let owned: Vec<(String, String, String)> = self
                    .slash_commands
                    .iter()
                    .map(|(name, desc)| (format!("/{}", name), desc.clone(), "command".to_string()))
                    .collect();
                let refs: Vec<(&str, &str, &str)> = owned
                    .iter()
                    .map(|(n, d, k)| (n.as_str(), d.as_str(), k.as_str()))
                    .collect();
                Self::filter_commands(&refs, &filter)
            }
            AutocompleteKind::ReplCommand => Self::filter_commands(
                &[
                    (":quit", "Exit chat", "core"),
                    (":help", "Show help", "core"),
                    (":clear", "Clear conversation history", "core"),
                    (":palette", "Open command palette", "core"),
                    (":model", "Switch model", "core"),
                    (":pick", "Fuzzy picker (notes, files, commands)", "core"),
                    (":mcp", "List MCP servers", "mcp"),
                    (":plugins", "Show plugin status", "core"),
                    (":export", "Export session to file", "core"),
                    (":messages", "Toggle notification drawer", "core"),
                    (":reload", "Reload plugin(s)", "core"),
                    (":set", "View/modify runtime options", "core"),
                ],
                &filter,
            ),
            AutocompleteKind::Model => {
                Self::filter_to_popup_items(&self.available_models, &filter, "model", 100)
            }
            AutocompleteKind::CommandArg {
                ref command,
                arg_index,
            } => self.get_command_arg_completions(command, arg_index, &filter),
            AutocompleteKind::SetOption { ref option } => {
                self.get_set_option_completions(option.as_deref(), &filter)
            }
            AutocompleteKind::Pick { ref source } => self.get_pick_items(source, &filter),
            AutocompleteKind::None => vec![],
        }
    }

    pub(super) fn filter_to_popup_items(
        items: &[String],
        filter: &str,
        kind: &str,
        limit: usize,
    ) -> Vec<PopupItemNode> {
        if filter.is_empty() {
            return items
                .iter()
                .take(limit)
                .map(|s| PopupItemNode {
                    label: s.clone(),
                    description: None,
                    kind: Some(kind.to_string()),
                })
                .collect();
        }

        let mut matcher = FuzzyMatcher::new();
        let matches = matcher.match_items(filter, items);

        matches
            .into_iter()
            .take(limit)
            .map(|(idx, _score)| PopupItemNode {
                label: items[idx].clone(),
                description: None,
                kind: Some(kind.to_string()),
            })
            .collect()
    }

    pub(super) fn filter_commands(
        commands: &[(&str, &str, &str)],
        filter: &str,
    ) -> Vec<PopupItemNode> {
        if filter.is_empty() {
            return commands
                .iter()
                .map(|(label, desc, kind)| PopupItemNode {
                    label: label.to_string(),
                    description: Some(desc.to_string()),
                    kind: Some(kind.to_string()),
                })
                .collect();
        }

        let labels: Vec<String> = commands.iter().map(|(l, _, _)| l.to_string()).collect();
        let mut matcher = FuzzyMatcher::new();
        let matches = matcher.match_items(filter, &labels);

        matches
            .into_iter()
            .map(|(idx, _score)| {
                let (label, desc, kind) = commands[idx];
                PopupItemNode {
                    label: label.to_string(),
                    description: Some(desc.to_string()),
                    kind: Some(kind.to_string()),
                }
            })
            .collect()
    }

    pub(super) fn get_set_option_completions(
        &self,
        option: Option<&str>,
        filter: &str,
    ) -> Vec<PopupItemNode> {
        use crate::tui::oil::config::{CompletionSource, SHORTCUTS, THINKING_PRESETS};

        match option {
            None => {
                let labels: Vec<String> = SHORTCUTS.iter().map(|s| s.short.to_string()).collect();
                let indices = if filter.is_empty() {
                    (0..labels.len()).map(|i| (i, 0u32)).collect::<Vec<_>>()
                } else {
                    let mut matcher = FuzzyMatcher::new();
                    matcher.match_items(filter, &labels)
                };
                indices
                    .into_iter()
                    .map(|(idx, _)| {
                        let s = &SHORTCUTS[idx];
                        let current_value = self.runtime_config.get(s.short);
                        let value_str =
                            current_value.map(|v| format!("={}", v)).unwrap_or_default();
                        PopupItemNode {
                            label: s.short.to_string(),
                            description: Some(format!("{}{}", s.description, value_str)),
                            kind: Some("option".to_string()),
                        }
                    })
                    .collect()
            }
            Some(opt) => {
                let source = self.runtime_config.completions_for(opt);
                match source {
                    CompletionSource::Models => {
                        Self::filter_to_popup_items(&self.available_models, filter, "model", 100)
                    }
                    CompletionSource::ThinkingPresets => {
                        let labels: Vec<String> = THINKING_PRESETS
                            .iter()
                            .map(|p| p.name.to_string())
                            .collect();
                        let indices = if filter.is_empty() {
                            (0..labels.len()).map(|i| (i, 0u32)).collect::<Vec<_>>()
                        } else {
                            let mut matcher = FuzzyMatcher::new();
                            matcher.match_items(filter, &labels)
                        };
                        indices
                            .into_iter()
                            .map(|(idx, _)| {
                                let p = &THINKING_PRESETS[idx];
                                PopupItemNode {
                                    label: p.name.to_string(),
                                    description: p.tokens.map(|t| format!("~{} tokens", t)),
                                    kind: Some("preset".to_string()),
                                }
                            })
                            .collect()
                    }
                    CompletionSource::Themes => Self::filter_commands(
                        &[
                            ("base16-ocean.dark", "", "theme"),
                            ("Solarized (dark)", "", "theme"),
                            ("Solarized (light)", "", "theme"),
                            ("InspiredGitHub", "", "theme"),
                        ],
                        filter,
                    )
                    .into_iter()
                    .map(|mut p| {
                        p.description = None;
                        p
                    })
                    .collect(),
                    CompletionSource::Static(values) => {
                        let owned: Vec<String> = values.iter().map(|v| v.to_string()).collect();
                        Self::filter_to_popup_items(&owned, filter, "value", owned.len())
                    }
                    CompletionSource::None => vec![],
                }
            }
        }
    }

    pub(super) fn get_command_arg_completions(
        &self,
        command: &str,
        _arg_index: usize,
        filter: &str,
    ) -> Vec<PopupItemNode> {
        match command {
            "export" => self.complete_file_paths(filter),
            "mcp" => self.complete_mcp_servers(filter),
            _ => self.complete_file_paths(filter),
        }
    }

    pub(super) fn complete_file_paths(&self, filter: &str) -> Vec<PopupItemNode> {
        Self::filter_to_popup_items(&self.workspace_files, filter, "path", 15)
    }

    pub(super) fn complete_mcp_servers(&self, filter: &str) -> Vec<PopupItemNode> {
        if filter.is_empty() {
            return self
                .mcp_servers
                .iter()
                .map(|s| PopupItemNode {
                    label: s.name.clone(),
                    description: Some(format!("{} tools", s.tool_count)),
                    kind: Some("mcp".to_string()),
                })
                .collect();
        }

        let names: Vec<String> = self.mcp_servers.iter().map(|s| s.name.clone()).collect();
        let mut matcher = FuzzyMatcher::new();
        let matches = matcher.match_items(filter, &names);

        matches
            .into_iter()
            .map(|(idx, _)| {
                let s = &self.mcp_servers[idx];
                PopupItemNode {
                    label: s.name.clone(),
                    description: Some(format!("{} tools", s.tool_count)),
                    kind: Some("mcp".to_string()),
                }
            })
            .collect()
    }

    fn get_pick_items(
        &self,
        source: &super::state::PickSource,
        filter: &str,
    ) -> Vec<PopupItemNode> {
        use super::state::PickSource;

        match source {
            PickSource::Notes => Self::filter_to_popup_items(&self.kiln_notes, filter, "note", 50),
            PickSource::Files => {
                Self::filter_to_popup_items(&self.workspace_files, filter, "file", 50)
            }
            PickSource::Commands => {
                let owned: Vec<(String, String, String)> = self
                    .slash_commands
                    .iter()
                    .map(|(name, desc)| (format!("/{}", name), desc.clone(), "command".to_string()))
                    .collect();
                let refs: Vec<(&str, &str, &str)> = owned
                    .iter()
                    .map(|(n, d, k)| (n.as_str(), d.as_str(), k.as_str()))
                    .collect();
                let mut items = Self::filter_commands(&refs, filter);
                items.extend(Self::filter_commands(
                    &[
                        (":quit", "Exit chat", "core"),
                        (":help", "Show help", "core"),
                        (":clear", "Clear conversation history", "core"),
                        (":model", "Switch model", "core"),
                        (":set", "View/modify runtime options", "core"),
                        (":pick", "Open picker", "core"),
                    ],
                    filter,
                ));
                items
            }
            PickSource::Sessions => {
                // Sessions aren't tracked in TUI state yet; show empty.
                vec![]
            }
            PickSource::All => {
                let mut items = Vec::new();
                items.extend(Self::filter_to_popup_items(
                    &self.kiln_notes,
                    filter,
                    "note",
                    20,
                ));
                items.extend(Self::filter_to_popup_items(
                    &self.workspace_files,
                    filter,
                    "file",
                    20,
                ));
                let owned: Vec<(String, String, String)> = self
                    .slash_commands
                    .iter()
                    .map(|(name, desc)| (format!("/{}", name), desc.clone(), "command".to_string()))
                    .collect();
                let refs: Vec<(&str, &str, &str)> = owned
                    .iter()
                    .map(|(n, d, k)| (n.as_str(), d.as_str(), k.as_str()))
                    .collect();
                items.extend(Self::filter_commands(&refs, filter));
                items
            }
        }
    }

    pub(super) fn insert_autocomplete_selection(&mut self, label: &str) {
        match &self.popup.kind {
            AutocompleteKind::File => {
                self.replace_at_trigger(format!("@{} ", label));
                self.add_context_if_new(format!("@{}", label));
            }
            AutocompleteKind::Note => {
                self.replace_at_trigger(format!("[[{}]] ", label));
                self.add_context_if_new(format!("[[{}]]", label));
            }
            AutocompleteKind::Command => {
                self.status = format!("Selected: {}", label);
            }
            AutocompleteKind::SlashCommand | AutocompleteKind::ReplCommand => {
                self.set_input(label);
            }
            AutocompleteKind::Model => {
                self.set_input(&format!(":model {}", label));
            }
            AutocompleteKind::CommandArg { .. } => {
                self.replace_at_trigger(format!("{} ", label));
            }
            AutocompleteKind::SetOption { option } => {
                let cmd = match option {
                    None => format!(":set {}", label),
                    Some(opt) => format!(":set {}={}", opt, label),
                };
                self.set_input(&cmd);
            }
            AutocompleteKind::Pick { ref source } => {
                use super::state::PickSource;
                match source {
                    PickSource::Files | PickSource::All => {
                        self.set_input(&format!("@{} ", label));
                        self.add_context_if_new(format!("@{}", label));
                    }
                    PickSource::Notes => {
                        self.set_input(&format!("[[{}]] ", label));
                        self.add_context_if_new(format!("[[{}]]", label));
                    }
                    PickSource::Commands => {
                        self.set_input(label);
                    }
                    PickSource::Sessions => {}
                }
            }
            AutocompleteKind::None => {}
        }

        self.close_popup();
    }

    pub(super) fn replace_at_trigger(&mut self, replacement: String) {
        let content = self.input.content().to_string();
        let trigger_pos = self.popup.trigger_pos;
        let prefix = &content[..trigger_pos];
        let suffix = &content[self.input.cursor()..];
        let new_content = format!("{}{}{}", prefix, replacement, suffix);
        let new_cursor = prefix.len() + replacement.len();

        self.set_input_and_cursor(&new_content, new_cursor);
    }

    pub(super) fn set_input(&mut self, content: &str) {
        self.input.handle(InputAction::Clear);
        for ch in content.chars() {
            self.input.handle(InputAction::Insert(ch));
        }
    }

    pub(super) fn set_input_and_cursor(&mut self, content: &str, cursor: usize) {
        self.set_input(content);
        while self.input.cursor() > cursor {
            self.input.handle(InputAction::Left);
        }
    }

    pub(super) fn add_context_if_new(&mut self, item: String) {
        if !self.attached_context.contains(&item) {
            self.attached_context.push(item);
        }
    }
}
