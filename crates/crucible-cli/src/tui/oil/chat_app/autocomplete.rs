//! Popup autocomplete logic for OilChatApp.
//!
//! Trigger detection, item filtering, completion insertion, and popup lifecycle.
//! Uses nucleo fuzzy matching (via `crucible_core::fuzzy`) for ranked results.

use crucible_core::fuzzy::FuzzyMatcher;

use crate::tui::oil::app::Action;
use crate::tui::oil::event::InputAction;
use crucible_oil::node::PopupItemNode;

use super::messages::ChatAppMsg;
use super::model_state::{ModelListState, SessionListState};
use super::repl_command::ReplCommand;
use super::state::AutocompleteKind;
use super::OilChatApp;

/// Split an `@` filter into the path and a trailing line suffix.
///
/// The suffix is `:12`, `:12-14`, or a part of one that the user still types
/// (`:`, `:12-`). The daemon attaches only the lines that the suffix names.
fn split_line_suffix(filter: &str) -> (&str, &str) {
    let Some(colon) = filter.rfind(':') else {
        return (filter, "");
    };
    let tail = &filter[colon + 1..];
    let (start, end) = tail.split_once('-').unwrap_or((tail, ""));
    let digits = |part: &str| part.bytes().all(|b| b.is_ascii_digit());
    if digits(start) && digits(end) && !(start.is_empty() && tail.contains('-')) {
        (&filter[..colon], &filter[colon..])
    } else {
        (filter, "")
    }
}

impl OilChatApp {
    pub(super) fn check_autocomplete_trigger(&mut self) -> Option<Action<ChatAppMsg>> {
        let content = self.input.content();
        let cursor = self.input.cursor();

        if let Some((kind, trigger_pos, filter)) = self.detect_trigger(content, cursor) {
            // `/resume ` typed by hand: the list may never have loaded.
            let needs_session_fetch = kind == AutocompleteKind::Session
                && matches!(
                    self.session_list,
                    SessionListState::NotLoaded | SessionListState::Failed(_)
                );
            let needs_model_fetch = kind == AutocompleteKind::Model
                && matches!(
                    self.model_list_state,
                    ModelListState::NotLoaded | ModelListState::Failed
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
            if self.popup.kind == AutocompleteKind::Session {
                // The picker stays open with a row that says what happens.
                self.popup.show = true;
                if needs_session_fetch {
                    self.session_list = SessionListState::Loading;
                    return Some(Action::Send(ChatAppMsg::FetchSessions));
                }
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

        // `/resume <filter>`: the argument picks a session. The slash
        // trigger below stops at the first space, so it cannot see this.
        const RESUME: &str = "/resume ";
        if let Some(rest) = before_cursor.strip_prefix(RESUME) {
            let filter = rest.trim_start();
            return Some((
                AutocompleteKind::Session,
                cursor - filter.len(),
                filter.to_string(),
            ));
        }

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

                    // `:lua` / `:=` bodies are Lua expressions — never
                    // arg-complete them (Enter must submit the eval, not
                    // accept a file completion).
                    if command == "lua" || command.starts_with('=') {
                        return None;
                    }

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

                    return Some((
                        AutocompleteKind::CommandArg { command },
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

    pub(crate) fn get_popup_items(&self) -> Vec<PopupItemNode> {
        let filter = self.popup.filter.to_lowercase();

        match self.popup.kind {
            AutocompleteKind::File => {
                let (path, _) = split_line_suffix(&filter);
                Self::filter_to_popup_items(&self.workspace_files, path, "file", 15)
            }
            AutocompleteKind::Note => {
                Self::filter_to_popup_items(&self.kiln_notes, &filter, "note", 15)
            }
            AutocompleteKind::Command => {
                let owned: Vec<(String, String, String)> = self
                    .slash_command_rows()
                    .into_iter()
                    .map(|(name, desc)| (format!("/{}", name), desc, "command".to_string()))
                    .collect();
                let refs: Vec<(&str, &str, &str)> = owned
                    .iter()
                    .map(|(n, d, k)| (n.as_str(), d.as_str(), k.as_str()))
                    .collect();
                Self::filter_commands(&refs, &filter)
            }
            AutocompleteKind::SlashCommand => {
                let owned: Vec<(String, String, String)> = self
                    .slash_command_rows()
                    .into_iter()
                    .map(|(name, desc)| (format!("/{}", name), desc, "command".to_string()))
                    .collect();
                let refs: Vec<(&str, &str, &str)> = owned
                    .iter()
                    .map(|(n, d, k)| (n.as_str(), d.as_str(), k.as_str()))
                    .collect();
                Self::filter_commands(&refs, &filter)
            }
            AutocompleteKind::ReplCommand => {
                Self::filter_commands(&ReplCommand::popup_entries(), &filter)
            }
            AutocompleteKind::Model => {
                if matches!(self.model_list_state, ModelListState::Loading)
                    && self.available_models.is_empty()
                {
                    vec![PopupItemNode {
                        label: "Loading models...".to_string(),
                        kind: Some("info".to_string()),
                        description: None,
                    }]
                } else if matches!(self.model_list_state, ModelListState::Failed)
                    && self.available_models.is_empty()
                {
                    vec![PopupItemNode {
                        label: "Failed to load models".to_string(),
                        kind: Some("error".to_string()),
                        description: None,
                    }]
                } else {
                    Self::filter_to_popup_items(&self.available_models, &filter, "model", 100)
                }
            }
            AutocompleteKind::CommandArg { ref command } => {
                self.get_command_arg_completions(command, &filter)
            }
            AutocompleteKind::SetOption { ref option } => {
                self.get_set_option_completions(option.as_deref(), &filter)
            }
            AutocompleteKind::Pick { ref source } => self.get_pick_items(source, &filter),
            AutocompleteKind::Session => self.session_popup_items(&filter),
            AutocompleteKind::None => vec![],
        }
    }

    /// Each row of the `:plugin-mode` menu: its label, the plugin, the value
    /// that the row sets, and whether the session holds that value now.
    pub(super) fn plugin_approval_rows(
        &self,
    ) -> impl Iterator<Item = (String, String, crucible_core::session::PluginApproval, bool)> + '_
    {
        self.plugin_approvals.iter().flat_map(|(plugin, now)| {
            crucible_core::session::PluginApproval::all().map(move |value| {
                (
                    format!("{plugin} · {}", value.as_str()),
                    plugin.clone(),
                    value,
                    value == *now,
                )
            })
        })
    }

    /// The rows of the `/resume` picker. A row that is not a session has the
    /// kind `info` or `error`, and Enter on it does nothing.
    fn session_popup_items(&self, filter: &str) -> Vec<PopupItemNode> {
        let note = |label: &str, kind: &str, description: Option<String>| {
            vec![PopupItemNode {
                label: label.to_string(),
                description,
                kind: Some(kind.to_string()),
            }]
        };
        let sessions = match &self.session_list {
            SessionListState::NotLoaded | SessionListState::Loading => {
                return note("Loading sessions...", "info", None);
            }
            SessionListState::Failed(reason) => {
                return note("Failed to list sessions", "error", Some(reason.clone()));
            }
            SessionListState::Loaded(sessions) if sessions.is_empty() => {
                return note("No other session in this workspace", "info", None);
            }
            SessionListState::Loaded(sessions) => sessions,
        };

        // The title and the id are both searchable; the daemon's order
        // (newest first) stays when there is no filter.
        let haystacks: Vec<String> = sessions
            .iter()
            .map(|s| format!("{} {}", s.title.as_deref().unwrap_or_default(), s.id))
            .collect();
        let order: Vec<usize> = if filter.is_empty() {
            (0..sessions.len()).collect()
        } else {
            FuzzyMatcher::new()
                .match_items(filter, &haystacks)
                .into_iter()
                .map(|(idx, _)| idx)
                .collect()
        };
        order
            .into_iter()
            .map(|idx| {
                let session = &sessions[idx];
                PopupItemNode {
                    label: session.id.clone(),
                    description: Some(format!(
                        "{} · {}",
                        session.title.as_deref().unwrap_or("(untitled)"),
                        session.when
                    )),
                    kind: Some("session".to_string()),
                }
            })
            .collect()
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
        use crate::tui::oil::config::{CompletionSource, SHORTCUTS};

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
                    CompletionSource::Themes => Self::filter_commands(
                        &[
                            ("base16-ocean.dark", "", "syntax_theme"),
                            ("Solarized (dark)", "", "syntax_theme"),
                            ("Solarized (light)", "", "syntax_theme"),
                            ("InspiredGitHub", "", "syntax_theme"),
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
            PickSource::Status => self
                .status_items
                .iter()
                .filter(|entry| {
                    filter.is_empty()
                        || entry.text.to_lowercase().contains(&filter.to_lowercase())
                        || entry.id.to_lowercase().contains(&filter.to_lowercase())
                })
                .map(|entry| PopupItemNode {
                    label: format!("{} [{}]", entry.text, entry.id),
                    description: Some(entry.plugin.clone()),
                    kind: Some("status".into()),
                })
                .collect(),
            PickSource::PluginApproval => self
                .plugin_approval_rows()
                .filter(|(label, ..)| {
                    filter.is_empty() || label.to_lowercase().contains(&filter.to_lowercase())
                })
                .map(|(label, _, _, current)| PopupItemNode {
                    label,
                    description: current.then(|| "current".to_string()),
                    kind: Some("approval".into()),
                })
                .collect(),
            PickSource::Notes => Self::filter_to_popup_items(&self.kiln_notes, filter, "note", 50),
            PickSource::Files => {
                Self::filter_to_popup_items(&self.workspace_files, filter, "file", 50)
            }
            PickSource::Commands => {
                let owned: Vec<(String, String, String)> = self
                    .slash_command_rows()
                    .into_iter()
                    .map(|(name, desc)| (format!("/{}", name), desc, "command".to_string()))
                    .collect();
                let refs: Vec<(&str, &str, &str)> = owned
                    .iter()
                    .map(|(n, d, k)| (n.as_str(), d.as_str(), k.as_str()))
                    .collect();
                let mut items = Self::filter_commands(&refs, filter);
                items.extend(Self::filter_commands(&ReplCommand::popup_entries(), filter));
                items
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
                    .slash_command_rows()
                    .into_iter()
                    .map(|(name, desc)| (format!("/{}", name), desc, "command".to_string()))
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
                // A line suffix after the cursor (`@READ|:12`) belongs to the
                // path. A space before it makes the daemon see two tokens.
                // A suffix typed before the cursor (`@READ:12|`) stays too.
                let (_, typed) = split_line_suffix(&self.popup.filter);
                let typed = typed.to_string();
                let after = &self.input.content()[self.input.cursor()..];
                let separator = if Self::starts_with_line_suffix(after) {
                    ""
                } else {
                    " "
                };
                self.replace_at_trigger(format!("@{label}{typed}{separator}"));
            }
            AutocompleteKind::Note => {
                self.replace_at_trigger(format!("[[{}]] ", label));
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
                    }
                    PickSource::Notes => {
                        self.set_input(&format!("[[{}]] ", label));
                    }
                    PickSource::Commands => {
                        self.set_input(label);
                    }
                    PickSource::Status | PickSource::PluginApproval => {
                        self.set_input("");
                    }
                }
            }
            AutocompleteKind::Session => {
                self.set_input(&format!("/resume {label}"));
            }
            AutocompleteKind::None => {}
        }

        self.close_popup();
    }

    /// Whether `text` starts with `:` and a digit, the start of `:12` or `:12-14`.
    fn starts_with_line_suffix(text: &str) -> bool {
        text.strip_prefix(':')
            .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
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
}

#[cfg(test)]
mod tests {
    //! US-501 autocomplete candidate-generation matrix.
    //!
    //! Exercises every documented trigger kind through the real
    //! `detect_trigger` + `get_popup_items` path, plus filter narrowing,
    //! dismissal, and token-replacement on accept. These are the inline
    //! unit tests the story doc calls out as the T1 gap for US-501.

    use super::*;
    use crate::tui::oil::chat_app::state::{AutocompleteKind, PickSource};

    fn app() -> OilChatApp {
        let mut app = OilChatApp::default();
        app.set_workspace_files(vec![
            "src/main.rs".into(),
            "src/lib.rs".into(),
            "README.md".into(),
        ]);
        app.set_kiln_notes(vec![
            "Rust Guide".into(),
            "Testing Notes".into(),
            "Roadmap".into(),
        ]);
        app.on_message(ChatAppMsg::CommandsLoaded(vec![
            crucible_core::types::BuiltinCommand::Help.entry(),
            crucible_core::types::BuiltinCommand::Undo.entry(),
        ]));
        // Loaded state so Model triggers don't request a fetch.
        app.set_available_models(vec![
            "gpt-4o".into(),
            "gpt-4o-mini".into(),
            "claude-sonnet-5".into(),
        ]);
        app
    }

    /// Set input, run trigger detection, and return (kind, candidate labels).
    fn probe(app: &mut OilChatApp, content: &str) -> (AutocompleteKind, Vec<String>) {
        app.set_input_content(content);
        app.check_autocomplete_trigger();
        let labels = app.get_popup_items().into_iter().map(|i| i.label).collect();
        (app.popup.kind.clone(), labels)
    }

    // ─── Trigger matrix: each kind produces candidates ─────────────────

    #[test]
    fn slash_trigger_lists_registered_commands() {
        let mut app = app();
        let (kind, labels) = probe(&mut app, "/");
        assert_eq!(kind, AutocompleteKind::SlashCommand);
        assert!(labels.contains(&"/help".to_string()));
        assert!(labels.contains(&"/undo".to_string()));
    }

    #[test]
    fn at_trigger_lists_workspace_files() {
        let mut app = app();
        let (kind, labels) = probe(&mut app, "@");
        assert_eq!(kind, AutocompleteKind::File);
        assert!(labels.iter().any(|l| l == "src/main.rs"));
    }

    #[test]
    fn double_bracket_trigger_lists_notes() {
        let mut app = app();
        let (kind, labels) = probe(&mut app, "[[");
        assert_eq!(kind, AutocompleteKind::Note);
        assert!(labels.iter().any(|l| l == "Rust Guide"));
    }

    #[test]
    fn colon_trigger_lists_repl_commands() {
        let mut app = app();
        let (kind, labels) = probe(&mut app, ":");
        assert_eq!(kind, AutocompleteKind::ReplCommand);
        assert!(labels.iter().any(|l| l == ":quit"));
        assert!(labels.iter().any(|l| l == ":set"));
    }

    #[test]
    fn model_trigger_lists_available_models() {
        let mut app = app();
        let (kind, labels) = probe(&mut app, ":model ");
        assert_eq!(kind, AutocompleteKind::Model);
        assert!(labels.iter().any(|l| l == "gpt-4o"));
        assert!(labels.iter().any(|l| l == "claude-sonnet-5"));
    }

    #[test]
    fn set_trigger_lists_option_shortcuts() {
        let mut app = app();
        let (kind, labels) = probe(&mut app, ":set ");
        assert!(matches!(kind, AutocompleteKind::SetOption { option: None }));
        assert!(!labels.is_empty(), "set options should be non-empty");
    }

    /// `:lua` / `:=` bodies are Lua expressions, not command arguments —
    /// arg-completing them hijacks Enter (accepting a file completion instead
    /// of submitting the eval).
    #[test]
    fn lua_body_does_not_trigger_arg_completion() {
        let mut app = app();
        let (kind, _) = probe(&mut app, ":lua 21 * 2");
        assert_eq!(
            kind,
            AutocompleteKind::None,
            ":lua body must not autocomplete"
        );
        let (kind, _) = probe(&mut app, ":= cru.config");
        assert_eq!(
            kind,
            AutocompleteKind::None,
            ":= body must not autocomplete"
        );
    }

    #[test]
    fn export_arg_trigger_completes_file_paths() {
        let mut app = app();
        let (kind, labels) = probe(&mut app, ":export ");
        assert!(matches!(
            kind,
            AutocompleteKind::CommandArg { ref command, .. } if command == "export"
        ));
        assert!(labels.iter().any(|l| l == "src/main.rs"));
    }

    #[test]
    fn mcp_arg_trigger_completes_server_names() {
        let mut app = app();
        app.set_mcp_servers(vec![crate::tui::oil::chat_app::McpServerDisplay {
            name: "github".into(),
            prefix: "gh".into(),
            tool_count: 4,
            connected: true,
        }]);
        let (kind, labels) = probe(&mut app, ":mcp ");
        assert!(matches!(
            kind,
            AutocompleteKind::CommandArg { ref command, .. } if command == "mcp"
        ));
        assert!(labels.iter().any(|l| l == "github"));
    }

    #[test]
    fn pick_trigger_lists_from_source() {
        let mut app = app();
        app.open_picker(Some("notes"));
        assert!(matches!(
            app.popup.kind,
            AutocompleteKind::Pick {
                source: PickSource::Notes
            }
        ));
        let labels: Vec<String> = app.get_popup_items().into_iter().map(|i| i.label).collect();
        assert!(labels.iter().any(|l| l == "Rust Guide"));
    }

    // ─── Filtering narrows as the user types ───────────────────────────

    #[test]
    fn filter_narrows_file_candidates() {
        let mut app = app();
        let (_, all) = probe(&mut app, "@");
        let (_, filtered) = probe(&mut app, "@README");
        assert!(filtered.len() < all.len());
        assert!(filtered.iter().any(|l| l == "README.md"));
    }

    #[test]
    fn filter_narrows_model_candidates() {
        let mut app = app();
        let (_, all) = probe(&mut app, ":model ");
        let (_, filtered) = probe(&mut app, ":model claude");
        assert!(filtered.len() < all.len());
        assert!(filtered.iter().all(|l| l.contains("claude")));
    }

    // ─── Dismissal without insertion ───────────────────────────────────

    #[test]
    fn esc_dismisses_popup_without_touching_input() {
        let mut app = app();
        probe(&mut app, "@src");
        assert!(app.popup.show);
        app.close_popup();
        assert!(!app.popup.show);
        assert_eq!(app.popup.kind, AutocompleteKind::None);
        // input untouched by close_popup
        assert_eq!(app.input_content(), "@src");
    }

    #[test]
    fn trigger_clears_when_token_no_longer_matches() {
        let mut app = app();
        probe(&mut app, "@src");
        assert!(app.popup.show);
        // A trailing space ends the @-token; trigger should clear.
        let (kind, _) = probe(&mut app, "@src done ");
        assert_eq!(kind, AutocompleteKind::None);
        assert!(!app.popup.show);
    }

    // ─── Accepting a completion replaces the token correctly ───────────

    #[test]
    fn accept_file_replaces_at_token() {
        let mut app = app();
        probe(&mut app, "@READ");
        app.insert_autocomplete_selection("README.md");
        assert_eq!(app.input_content(), "@README.md ");
        assert!(!app.popup.show);
    }

    #[test]
    fn accept_file_keeps_a_line_suffix() {
        // The user typed `@READ:12` and moved the cursor back to complete the
        // path. The `:12` after the cursor must stay attached to the path, or
        // the daemon reads `README.md` and the stray `:12` as two things.
        let mut app = app();
        app.set_input_and_cursor("@READ:12 see", "@READ".len());
        app.check_autocomplete_trigger();
        assert_eq!(app.popup.kind, AutocompleteKind::File);
        app.insert_autocomplete_selection("README.md");
        assert_eq!(app.input_content(), "@README.md:12 see");
        assert_eq!(app.input.cursor(), "@README.md".len());
    }

    #[test]
    fn accept_file_keeps_a_typed_line_suffix() {
        let mut app = app();
        let (kind, labels) = probe(&mut app, "@READ:12-14");
        assert_eq!(kind, AutocompleteKind::File);
        assert!(labels.contains(&"README.md".to_string()), "got: {labels:?}");
        app.insert_autocomplete_selection("README.md");
        assert_eq!(app.input_content(), "@README.md:12-14 ");
    }

    #[test]
    fn a_line_suffix_splits_only_from_digits() {
        assert_eq!(split_line_suffix("a.rs:12"), ("a.rs", ":12"));
        assert_eq!(split_line_suffix("a.rs:12-"), ("a.rs", ":12-"));
        assert_eq!(split_line_suffix("a.rs:"), ("a.rs", ":"));
        assert_eq!(split_line_suffix("a:b"), ("a:b", ""));
        assert_eq!(split_line_suffix("a.rs:-3"), ("a.rs:-3", ""));
    }

    #[test]
    fn accept_note_wraps_in_wikilink() {
        let mut app = app();
        probe(&mut app, "[[Rust");
        app.insert_autocomplete_selection("Rust Guide");
        assert_eq!(app.input_content(), "[[Rust Guide]] ");
    }

    #[test]
    fn accept_model_sets_full_command() {
        let mut app = app();
        probe(&mut app, ":model gpt");
        app.insert_autocomplete_selection("gpt-4o");
        assert_eq!(app.input_content(), ":model gpt-4o");
    }

    #[test]
    fn model_picker_status_rows_cannot_be_selected() {
        // Set in the initializer rather than assigned after: clippy's
        // `field_reassign_with_default` is denied by `just lint clippy`, and
        // this was the one occurrence keeping that gate red. Pre-dates the
        // Luau work.
        let mut app = OilChatApp {
            model_list_state: ModelListState::Loading,
            ..Default::default()
        };
        app.set_input(":model ");
        app.check_autocomplete_trigger();

        let action = app.select_popup_item();

        assert!(matches!(action, Action::Continue));
        assert!(app.popup.show, "loading indicator should remain visible");
        assert_eq!(app.input_content(), ":model ");

        app.model_list_state = ModelListState::Failed;
        let action = app.select_popup_item();

        assert!(matches!(action, Action::Continue));
        assert!(app.popup.show, "failure indicator should remain visible");
        assert_eq!(app.input_content(), ":model ");
    }
}
