use crate::kiln_validate::{
    expand_tilde, validate_kiln_path, ValidationResult, ValidationSeverity,
};
use crate::tui::oil::node::{col, row, spinner, styled, text, Node};
use crate::tui::oil::style::Style;
use crate::tui::oil::theme::ThemeTokens;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WizardStep {
    Welcome,
    ConfigureKiln,
    ShowWarning,
    ConfirmCreate,
    DetectingProviders,
    SelectProvider,
    FetchingModels,
    SelectModel,
    Complete,
}

#[derive(Debug, Clone)]
pub struct DetectedProviderInfo {
    pub name: String,
    pub provider_type: String,
    pub reason: String,
    pub default_model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WizardConfig {
    pub kiln_path: PathBuf,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone)]
pub enum SetupWizardMsg {
    Key(KeyEvent),
    ProvidersDetected(Vec<DetectedProviderInfo>),
    ModelsLoaded(Vec<String>),
    Tick,
}

#[derive(Debug, Clone)]
pub enum SetupWizardOutput {
    None,
    Close,
    Complete(WizardConfig),
    NeedsProviderDetection,
    NeedsModelFetch(String),
}

pub struct SetupWizard {
    step: WizardStep,
    path_input: String,
    path_cursor: usize,
    resolved_path: Option<PathBuf>,
    validation: Option<ValidationResult>,
    warning_selected: usize,
    providers: Vec<DetectedProviderInfo>,
    provider_selected: usize,
    models: Vec<String>,
    model_selected: usize,
    spinner_frame: usize,
    error_message: Option<String>,
}

impl SetupWizard {
    pub fn new() -> Self {
        Self {
            step: WizardStep::Welcome,
            path_input: String::new(),
            path_cursor: 0,
            resolved_path: None,
            validation: None,
            warning_selected: 0,
            providers: Vec::new(),
            provider_selected: 0,
            models: Vec::new(),
            model_selected: 0,
            spinner_frame: 0,
            error_message: None,
        }
    }

    pub fn step(&self) -> &WizardStep {
        &self.step
    }

    pub fn update(&mut self, msg: SetupWizardMsg) -> SetupWizardOutput {
        match msg {
            SetupWizardMsg::Key(key) => self.handle_key(key),
            SetupWizardMsg::ProvidersDetected(providers) => {
                self.providers = providers;
                self.provider_selected = 0;
                if self.providers.is_empty() {
                    self.providers.push(DetectedProviderInfo {
                        name: "Ollama (default)".into(),
                        provider_type: "ollama".into(),
                        reason: "No providers detected — using default".into(),
                        default_model: Some("llama3.2".into()),
                    });
                }
                self.step = WizardStep::SelectProvider;
                SetupWizardOutput::None
            }
            SetupWizardMsg::ModelsLoaded(models) => {
                self.models = models;
                self.model_selected = 0;
                if self.models.is_empty() {
                    let default = self
                        .providers
                        .get(self.provider_selected)
                        .and_then(|p| p.default_model.clone())
                        .unwrap_or_else(|| "llama3.2".into());
                    self.models.push(default);
                }
                self.step = WizardStep::SelectModel;
                SetupWizardOutput::None
            }
            SetupWizardMsg::Tick => {
                self.spinner_frame = self.spinner_frame.wrapping_add(1);
                SetupWizardOutput::None
            }
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> SetupWizardOutput {
        if key.code == KeyCode::Esc {
            return SetupWizardOutput::Close;
        }

        match &self.step {
            WizardStep::Welcome => self.handle_welcome_key(key),
            WizardStep::ConfigureKiln => self.handle_kiln_key(key),
            WizardStep::ShowWarning => self.handle_warning_key(key),
            WizardStep::ConfirmCreate => self.handle_confirm_create_key(key),
            WizardStep::DetectingProviders | WizardStep::FetchingModels => SetupWizardOutput::None,
            WizardStep::SelectProvider => self.handle_provider_key(key),
            WizardStep::SelectModel => self.handle_model_key(key),
            WizardStep::Complete => self.handle_complete_key(key),
        }
    }

    fn handle_welcome_key(&mut self, key: KeyEvent) -> SetupWizardOutput {
        if key.code == KeyCode::Enter {
            self.step = WizardStep::ConfigureKiln;
        }
        SetupWizardOutput::None
    }

    fn handle_kiln_key(&mut self, key: KeyEvent) -> SetupWizardOutput {
        match key.code {
            KeyCode::Enter => {
                if self.path_input.trim().is_empty() {
                    self.error_message = Some("Please enter a path for your kiln.".into());
                    return SetupWizardOutput::None;
                }
                self.error_message = None;
                let expanded = expand_tilde(self.path_input.trim());
                let result = validate_kiln_path(&expanded);
                self.resolved_path = Some(expanded);
                self.validation = Some(result.clone());

                if result.is_blocked() {
                    let msg = result
                        .findings_by_severity(ValidationSeverity::HardBlock)
                        .first()
                        .map(|f| f.message.clone())
                        .unwrap_or_else(|| "Invalid path.".into());
                    self.error_message = Some(msg);
                    self.resolved_path = None;
                    self.validation = None;
                    return SetupWizardOutput::None;
                }

                if result.has_strong_warnings() {
                    self.warning_selected = 1;
                    self.step = WizardStep::ShowWarning;
                    return SetupWizardOutput::None;
                }

                if !result.path_exists {
                    self.warning_selected = 0;
                    self.step = WizardStep::ConfirmCreate;
                    return SetupWizardOutput::None;
                }

                self.step = WizardStep::DetectingProviders;
                SetupWizardOutput::NeedsProviderDetection
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return SetupWizardOutput::None;
                }
                self.path_input.insert(self.path_cursor, c);
                self.path_cursor += 1;
                self.error_message = None;
                SetupWizardOutput::None
            }
            KeyCode::Backspace => {
                if self.path_cursor > 0 {
                    self.path_cursor -= 1;
                    self.path_input.remove(self.path_cursor);
                }
                self.error_message = None;
                SetupWizardOutput::None
            }
            KeyCode::Left => {
                self.path_cursor = self.path_cursor.saturating_sub(1);
                SetupWizardOutput::None
            }
            KeyCode::Right => {
                if self.path_cursor < self.path_input.len() {
                    self.path_cursor += 1;
                }
                SetupWizardOutput::None
            }
            KeyCode::Home => {
                self.path_cursor = 0;
                SetupWizardOutput::None
            }
            KeyCode::End => {
                self.path_cursor = self.path_input.len();
                SetupWizardOutput::None
            }
            _ => SetupWizardOutput::None,
        }
    }

    fn handle_warning_key(&mut self, key: KeyEvent) -> SetupWizardOutput {
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                self.warning_selected = 1 - self.warning_selected;
                SetupWizardOutput::None
            }
            KeyCode::Enter => {
                if self.warning_selected == 0 {
                    if let Some(ref result) = self.validation {
                        if !result.path_exists {
                            self.warning_selected = 0;
                            self.step = WizardStep::ConfirmCreate;
                            return SetupWizardOutput::None;
                        }
                    }
                    self.step = WizardStep::DetectingProviders;
                    SetupWizardOutput::NeedsProviderDetection
                } else {
                    self.step = WizardStep::ConfigureKiln;
                    self.resolved_path = None;
                    self.validation = None;
                    SetupWizardOutput::None
                }
            }
            _ => SetupWizardOutput::None,
        }
    }

    fn handle_confirm_create_key(&mut self, key: KeyEvent) -> SetupWizardOutput {
        match key.code {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                self.warning_selected = 1 - self.warning_selected;
                SetupWizardOutput::None
            }
            KeyCode::Enter => {
                if self.warning_selected == 0 {
                    self.step = WizardStep::DetectingProviders;
                    SetupWizardOutput::NeedsProviderDetection
                } else {
                    self.step = WizardStep::ConfigureKiln;
                    self.resolved_path = None;
                    self.validation = None;
                    SetupWizardOutput::None
                }
            }
            _ => SetupWizardOutput::None,
        }
    }

    fn handle_provider_key(&mut self, key: KeyEvent) -> SetupWizardOutput {
        match key.code {
            KeyCode::Up => {
                self.provider_selected = self.provider_selected.saturating_sub(1);
                SetupWizardOutput::None
            }
            KeyCode::Down => {
                if self.provider_selected + 1 < self.providers.len() {
                    self.provider_selected += 1;
                }
                SetupWizardOutput::None
            }
            KeyCode::Enter => {
                let provider_type = self.providers[self.provider_selected].provider_type.clone();
                self.step = WizardStep::FetchingModels;
                SetupWizardOutput::NeedsModelFetch(provider_type)
            }
            _ => SetupWizardOutput::None,
        }
    }

    fn handle_model_key(&mut self, key: KeyEvent) -> SetupWizardOutput {
        match key.code {
            KeyCode::Up => {
                self.model_selected = self.model_selected.saturating_sub(1);
                SetupWizardOutput::None
            }
            KeyCode::Down => {
                if self.model_selected + 1 < self.models.len() {
                    self.model_selected += 1;
                }
                SetupWizardOutput::None
            }
            KeyCode::Enter => {
                self.step = WizardStep::Complete;
                SetupWizardOutput::None
            }
            _ => SetupWizardOutput::None,
        }
    }

    fn handle_complete_key(&mut self, key: KeyEvent) -> SetupWizardOutput {
        if key.code == KeyCode::Enter {
            let kiln_path = self
                .resolved_path
                .clone()
                .unwrap_or_else(|| expand_tilde(self.path_input.trim()));
            let provider = self
                .providers
                .get(self.provider_selected)
                .map(|p| p.provider_type.clone())
                .unwrap_or_else(|| "ollama".into());
            let model = self
                .models
                .get(self.model_selected)
                .cloned()
                .unwrap_or_else(|| "llama3.2".into());
            return SetupWizardOutput::Complete(WizardConfig {
                kiln_path,
                provider,
                model,
            });
        }
        SetupWizardOutput::None
    }

    pub fn view(&self, theme: &ThemeTokens) -> Node {
        match &self.step {
            WizardStep::Welcome => self.view_welcome(theme),
            WizardStep::ConfigureKiln => self.view_kiln(theme),
            WizardStep::ShowWarning => self.view_warning(theme),
            WizardStep::ConfirmCreate => self.view_confirm_create(theme),
            WizardStep::DetectingProviders => self.view_detecting(theme),
            WizardStep::SelectProvider => self.view_providers(theme),
            WizardStep::FetchingModels => self.view_fetching_models(theme),
            WizardStep::SelectModel => self.view_models(theme),
            WizardStep::Complete => self.view_complete(theme),
        }
    }

    fn view_welcome(&self, theme: &ThemeTokens) -> Node {
        col([
            text(""),
            styled(
                "  Welcome to Crucible",
                Style::new().fg(theme.text_accent).bold(),
            ),
            text(""),
            styled(
                "  Crucible saves your AI conversations as",
                Style::new().fg(theme.text_primary),
            ),
            styled(
                "  searchable markdown notes in a folder",
                Style::new().fg(theme.text_primary),
            ),
            styled("  called a kiln.", Style::new().fg(theme.text_primary)),
            text(""),
            styled("  Let's set one up.", Style::new().fg(theme.text_primary)),
            text(""),
            styled("  Press Enter to continue", Style::new().fg(theme.text_dim)),
        ])
    }

    fn view_kiln(&self, theme: &ThemeTokens) -> Node {
        let mut children = vec![
            text(""),
            styled(
                "  Where should your kiln live?",
                Style::new().fg(theme.text_accent).bold(),
            ),
            text(""),
            styled(
                "  Suggestion: ~/crucible",
                Style::new().fg(theme.text_dim).dim(),
            ),
            text(""),
        ];

        children.push(row([
            styled("  Path: ", Style::new().fg(theme.text_primary)),
            Node::Input(crucible_oil::node::InputNode {
                value: self.path_input.clone(),
                cursor: self.path_cursor,
                placeholder: None,
                style: Style::new().fg(theme.text_primary),
                focused: true,
            }),
        ]));

        if let Some(ref err) = self.error_message {
            children.push(text(""));
            children.push(styled(format!("  ⚠ {}", err), Style::new().fg(theme.error)));
        }

        children.push(text(""));
        children.push(styled(
            "  Enter to submit · Esc to quit",
            Style::new().fg(theme.text_dim),
        ));

        col(children)
    }

    fn view_warning(&self, theme: &ThemeTokens) -> Node {
        let mut children = vec![
            text(""),
            styled("  Warning", Style::new().fg(theme.warning).bold()),
            text(""),
        ];

        if let Some(ref result) = self.validation {
            for finding in result.findings_by_severity(ValidationSeverity::StrongWarning) {
                children.push(styled(
                    format!("  ⚠ {}", finding.message),
                    Style::new().fg(theme.warning),
                ));
                if let Some(ref suggestion) = finding.suggestion {
                    children.push(styled(
                        format!("    {}", suggestion),
                        Style::new().fg(theme.text_dim),
                    ));
                }
            }
        }

        children.push(text(""));

        let proceed_style = if self.warning_selected == 0 {
            Style::new().fg(theme.text_primary).bold().reverse()
        } else {
            Style::new().fg(theme.text_dim)
        };
        let change_style = if self.warning_selected == 1 {
            Style::new().fg(theme.text_primary).bold().reverse()
        } else {
            Style::new().fg(theme.text_dim)
        };

        children.push(row([
            styled("  ", Style::default()),
            styled(" Proceed anyway ", proceed_style),
            styled("  ", Style::default()),
            styled(" Change path ", change_style),
        ]));

        children.push(text(""));
        children.push(styled(
            "  ←/→ to switch · Enter to confirm",
            Style::new().fg(theme.text_dim),
        ));

        col(children)
    }

    fn view_confirm_create(&self, theme: &ThemeTokens) -> Node {
        let path_display = self
            .resolved_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| self.path_input.clone());

        let yes_style = if self.warning_selected == 0 {
            Style::new().fg(theme.text_primary).bold().reverse()
        } else {
            Style::new().fg(theme.text_dim)
        };
        let no_style = if self.warning_selected == 1 {
            Style::new().fg(theme.text_primary).bold().reverse()
        } else {
            Style::new().fg(theme.text_dim)
        };

        col([
            text(""),
            styled(
                format!("  Directory doesn't exist: {}", path_display),
                Style::new().fg(theme.text_primary),
            ),
            text(""),
            styled("  Create it?", Style::new().fg(theme.text_accent).bold()),
            text(""),
            row([
                styled("  ", Style::default()),
                styled(" Yes ", yes_style),
                styled("  ", Style::default()),
                styled(" No ", no_style),
            ]),
            text(""),
            styled(
                "  ←/→ to switch · Enter to confirm",
                Style::new().fg(theme.text_dim),
            ),
        ])
    }

    fn view_detecting(&self, theme: &ThemeTokens) -> Node {
        col([
            text(""),
            styled(
                "  Detecting LLM providers...",
                Style::new().fg(theme.text_accent).bold(),
            ),
            text(""),
            spinner(
                Some("  Checking for Ollama, OpenAI, Anthropic...".into()),
                self.spinner_frame,
            ),
        ])
    }

    fn view_fetching_models(&self, theme: &ThemeTokens) -> Node {
        let provider_name = self
            .providers
            .get(self.provider_selected)
            .map(|p| p.name.as_str())
            .unwrap_or("provider");
        col([
            text(""),
            styled(
                format!("  Fetching models from {}...", provider_name),
                Style::new().fg(theme.text_accent).bold(),
            ),
            text(""),
            spinner(Some("  Loading model list...".into()), self.spinner_frame),
        ])
    }

    fn view_providers(&self, theme: &ThemeTokens) -> Node {
        let mut children = vec![
            text(""),
            styled(
                "  Select LLM Provider",
                Style::new().fg(theme.text_accent).bold(),
            ),
            text(""),
        ];

        for (i, provider) in self.providers.iter().enumerate() {
            let marker = if i == self.provider_selected {
                "▸"
            } else {
                " "
            };
            let name_style = if i == self.provider_selected {
                Style::new().fg(theme.selected).bold()
            } else {
                Style::new().fg(theme.text_primary)
            };
            let reason_style = Style::new().fg(theme.text_dim);

            children.push(row([
                styled(format!("  {} ", marker), name_style),
                styled(provider.name.clone(), name_style),
                styled(format!("  {}", provider.reason), reason_style),
            ]));
        }

        children.push(text(""));
        children.push(styled(
            "  ↑/↓ to select · Enter to confirm · Esc to quit",
            Style::new().fg(theme.text_dim),
        ));

        col(children)
    }

    fn view_models(&self, theme: &ThemeTokens) -> Node {
        let mut children = vec![
            text(""),
            styled("  Select Model", Style::new().fg(theme.text_accent).bold()),
            text(""),
        ];

        let visible_start = self.model_selected.saturating_sub(8);
        let visible_end = (visible_start + 16).min(self.models.len());

        for i in visible_start..visible_end {
            let model = &self.models[i];
            let marker = if i == self.model_selected { "▸" } else { " " };
            let style = if i == self.model_selected {
                Style::new().fg(theme.selected).bold()
            } else {
                Style::new().fg(theme.text_primary)
            };

            children.push(styled(format!("  {} {}", marker, model), style));
        }

        if self.models.len() > 16 {
            children.push(styled(
                format!("  ({} models total)", self.models.len()),
                Style::new().fg(theme.text_dim),
            ));
        }

        children.push(text(""));
        children.push(styled(
            "  ↑/↓ to select · Enter to confirm · Esc to quit",
            Style::new().fg(theme.text_dim),
        ));

        col(children)
    }

    fn view_complete(&self, theme: &ThemeTokens) -> Node {
        let kiln_display = self
            .resolved_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| self.path_input.clone());
        let provider = self
            .providers
            .get(self.provider_selected)
            .map(|p| p.name.as_str())
            .unwrap_or("Ollama");
        let model = self
            .models
            .get(self.model_selected)
            .map(|s| s.as_str())
            .unwrap_or("llama3.2");

        col([
            text(""),
            styled("  Setup Complete!", Style::new().fg(theme.success).bold()),
            text(""),
            row([
                styled("  Kiln:     ", Style::new().fg(theme.text_dim)),
                styled(kiln_display, Style::new().fg(theme.text_primary)),
            ]),
            row([
                styled("  Provider: ", Style::new().fg(theme.text_dim)),
                styled(provider, Style::new().fg(theme.text_primary)),
            ]),
            row([
                styled("  Model:    ", Style::new().fg(theme.text_dim)),
                styled(model, Style::new().fg(theme.text_primary)),
            ]),
            text(""),
            styled(
                "  Press Enter to start chatting",
                Style::new().fg(theme.text_accent),
            ),
        ])
    }
}

impl Default for SetupWizard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> SetupWizardMsg {
        SetupWizardMsg::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn char_key(c: char) -> SetupWizardMsg {
        SetupWizardMsg::Key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE))
    }

    #[test]
    fn initial_state_is_welcome() {
        let wizard = SetupWizard::new();
        assert_eq!(*wizard.step(), WizardStep::Welcome);
    }

    #[test]
    fn enter_on_welcome_goes_to_configure_kiln() {
        let mut wizard = SetupWizard::new();
        wizard.update(key(KeyCode::Enter));
        assert_eq!(*wizard.step(), WizardStep::ConfigureKiln);
    }

    #[test]
    fn typing_in_configure_kiln_updates_path() {
        let mut wizard = SetupWizard::new();
        wizard.update(key(KeyCode::Enter));

        wizard.update(char_key('/'));
        wizard.update(char_key('t'));
        wizard.update(char_key('m'));
        wizard.update(char_key('p'));
        assert_eq!(wizard.path_input, "/tmp");
        assert_eq!(wizard.path_cursor, 4);
    }

    #[test]
    fn empty_path_shows_error() {
        let mut wizard = SetupWizard::new();
        wizard.update(key(KeyCode::Enter));
        wizard.update(key(KeyCode::Enter));
        assert_eq!(*wizard.step(), WizardStep::ConfigureKiln);
        assert!(wizard.error_message.is_some());
    }

    #[test]
    fn backspace_deletes_character() {
        let mut wizard = SetupWizard::new();
        wizard.update(key(KeyCode::Enter));
        wizard.update(char_key('a'));
        wizard.update(char_key('b'));
        wizard.update(key(KeyCode::Backspace));
        assert_eq!(wizard.path_input, "a");
        assert_eq!(wizard.path_cursor, 1);
    }

    #[test]
    fn esc_produces_close() {
        let mut wizard = SetupWizard::new();
        let output = wizard.update(key(KeyCode::Esc));
        assert!(matches!(output, SetupWizardOutput::Close));
    }

    #[test]
    fn esc_closes_from_any_step() {
        let mut wizard = SetupWizard::new();
        wizard.update(key(KeyCode::Enter));
        let output = wizard.update(key(KeyCode::Esc));
        assert!(matches!(output, SetupWizardOutput::Close));
    }

    #[test]
    fn provider_selection_up_down() {
        let mut wizard = SetupWizard::new();
        wizard.step = WizardStep::SelectProvider;
        wizard.providers = vec![
            DetectedProviderInfo {
                name: "Ollama".into(),
                provider_type: "ollama".into(),
                reason: "Running locally".into(),
                default_model: Some("llama3.2".into()),
            },
            DetectedProviderInfo {
                name: "OpenAI".into(),
                provider_type: "openai".into(),
                reason: "API key found".into(),
                default_model: Some("gpt-4o-mini".into()),
            },
        ];

        assert_eq!(wizard.provider_selected, 0);
        wizard.update(key(KeyCode::Down));
        assert_eq!(wizard.provider_selected, 1);
        wizard.update(key(KeyCode::Down));
        assert_eq!(wizard.provider_selected, 1);
        wizard.update(key(KeyCode::Up));
        assert_eq!(wizard.provider_selected, 0);
    }

    #[test]
    fn provider_enter_triggers_model_fetch() {
        let mut wizard = SetupWizard::new();
        wizard.step = WizardStep::SelectProvider;
        wizard.providers = vec![DetectedProviderInfo {
            name: "Ollama".into(),
            provider_type: "ollama".into(),
            reason: "Running locally".into(),
            default_model: Some("llama3.2".into()),
        }];

        let output = wizard.update(key(KeyCode::Enter));
        assert!(matches!(output, SetupWizardOutput::NeedsModelFetch(ref t) if t == "ollama"));
        assert_eq!(*wizard.step(), WizardStep::FetchingModels);
    }

    #[test]
    fn model_selection_up_down() {
        let mut wizard = SetupWizard::new();
        wizard.step = WizardStep::SelectModel;
        wizard.models = vec!["llama3.2".into(), "mistral".into(), "codellama".into()];

        assert_eq!(wizard.model_selected, 0);
        wizard.update(key(KeyCode::Down));
        assert_eq!(wizard.model_selected, 1);
        wizard.update(key(KeyCode::Up));
        assert_eq!(wizard.model_selected, 0);
    }

    #[test]
    fn model_enter_goes_to_complete() {
        let mut wizard = SetupWizard::new();
        wizard.step = WizardStep::SelectModel;
        wizard.models = vec!["llama3.2".into()];

        wizard.update(key(KeyCode::Enter));
        assert_eq!(*wizard.step(), WizardStep::Complete);
    }

    #[test]
    fn complete_enter_produces_wizard_config() {
        let mut wizard = SetupWizard::new();
        wizard.step = WizardStep::Complete;
        wizard.resolved_path = Some(PathBuf::from("/home/user/notes"));
        wizard.providers = vec![DetectedProviderInfo {
            name: "Ollama".into(),
            provider_type: "ollama".into(),
            reason: "Running locally".into(),
            default_model: Some("llama3.2".into()),
        }];
        wizard.models = vec!["llama3.2".into()];

        let output = wizard.update(key(KeyCode::Enter));
        match output {
            SetupWizardOutput::Complete(config) => {
                assert_eq!(config.kiln_path, PathBuf::from("/home/user/notes"));
                assert_eq!(config.provider, "ollama");
                assert_eq!(config.model, "llama3.2");
            }
            other => panic!("Expected Complete, got {:?}", other),
        }
    }

    #[test]
    fn providers_detected_sets_state() {
        let mut wizard = SetupWizard::new();
        wizard.step = WizardStep::DetectingProviders;

        wizard.update(SetupWizardMsg::ProvidersDetected(vec![
            DetectedProviderInfo {
                name: "Ollama".into(),
                provider_type: "ollama".into(),
                reason: "Running".into(),
                default_model: None,
            },
        ]));

        assert_eq!(*wizard.step(), WizardStep::SelectProvider);
        assert_eq!(wizard.providers.len(), 1);
    }

    #[test]
    fn empty_providers_adds_default() {
        let mut wizard = SetupWizard::new();
        wizard.step = WizardStep::DetectingProviders;

        wizard.update(SetupWizardMsg::ProvidersDetected(vec![]));

        assert_eq!(*wizard.step(), WizardStep::SelectProvider);
        assert_eq!(wizard.providers.len(), 1);
        assert_eq!(wizard.providers[0].provider_type, "ollama");
    }

    #[test]
    fn models_loaded_sets_state() {
        let mut wizard = SetupWizard::new();
        wizard.step = WizardStep::FetchingModels;

        wizard.update(SetupWizardMsg::ModelsLoaded(vec![
            "llama3.2".into(),
            "mistral".into(),
        ]));

        assert_eq!(*wizard.step(), WizardStep::SelectModel);
        assert_eq!(wizard.models.len(), 2);
    }

    #[test]
    fn view_produces_nodes_for_each_step() {
        let theme = ThemeTokens::default();

        let mut wizard = SetupWizard::new();
        assert!(!matches!(wizard.view(&theme), Node::Empty));

        wizard.step = WizardStep::ConfigureKiln;
        assert!(!matches!(wizard.view(&theme), Node::Empty));

        wizard.step = WizardStep::DetectingProviders;
        assert!(!matches!(wizard.view(&theme), Node::Empty));

        wizard.step = WizardStep::SelectProvider;
        wizard.providers = vec![DetectedProviderInfo {
            name: "Ollama".into(),
            provider_type: "ollama".into(),
            reason: "Running".into(),
            default_model: None,
        }];
        assert!(!matches!(wizard.view(&theme), Node::Empty));

        wizard.step = WizardStep::SelectModel;
        wizard.models = vec!["llama3.2".into()];
        assert!(!matches!(wizard.view(&theme), Node::Empty));

        wizard.step = WizardStep::Complete;
        assert!(!matches!(wizard.view(&theme), Node::Empty));
    }
}
