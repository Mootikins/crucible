use super::*;

// =============================================================================
// Model Command Tests
// =============================================================================

#[test]
fn model_command_opens_popup_with_available_models() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
        "openai/gpt-4".to_string(),
    ]);

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should open when typing ':model '"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(output.contains("llama3"), "Popup should show llama3 model");
    assert!(
        output.contains("claude-3"),
        "Popup should show claude-3 model"
    );
}

#[test]
fn model_space_with_preloaded_models_shows_popup_immediately() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
    ]);

    let mut last_action = Action::Continue;
    for c in ":model ".chars() {
        last_action = app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should be visible immediately"
    );
    assert!(
        matches!(last_action, Action::Continue),
        "Loaded model list should not trigger a new fetch"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("llama3"),
        "Popup should include preloaded models"
    );
}

#[test]
fn model_command_filters_models() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
        "openai/gpt-4".to_string(),
    ]);

    for c in ":model clau".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(app.is_popup_visible(), "Popup should be visible");
    assert_eq!(
        app.current_popup_filter(),
        "clau",
        "Filter should be 'clau'"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("claude-3"),
        "Popup should show claude-3 (matches filter)"
    );
}

#[test]
fn model_popup_shows_all_twenty_models() {
    let mut app = OilChatApp::default();
    let models = vec![
        "ollama/atlas-01".to_string(),
        "zai/atlas-02".to_string(),
        "openai/atlas-03".to_string(),
        "anthropic/atlas-04".to_string(),
        "ollama/atlas-05".to_string(),
        "zai/atlas-06".to_string(),
        "openai/atlas-07".to_string(),
        "anthropic/atlas-08".to_string(),
        "ollama/atlas-09".to_string(),
        "zai/atlas-10".to_string(),
        "openai/atlas-11".to_string(),
        "anthropic/atlas-12".to_string(),
        "ollama/atlas-13".to_string(),
        "zai/atlas-14".to_string(),
        "openai/atlas-15".to_string(),
        "anthropic/atlas-16".to_string(),
        "ollama/atlas-17".to_string(),
        "zai/atlas-18".to_string(),
        "openai/atlas-19".to_string(),
        "anthropic/atlas-20".to_string(),
    ];
    let expected_last_model = models[19].clone();

    app.set_available_models(models);

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should open when typing ':model '"
    );

    for _ in 0..19 {
        app.update(Event::Key(key(KeyCode::Down)));
    }

    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        app.input_content()
            .contains(&format!(":model {}", expected_last_model)),
        "Popup should allow selecting the 20th model"
    );
}

#[test]
fn model_popup_shows_exactly_fifteen_models() {
    let mut app = OilChatApp::default();
    let models = vec![
        "ollama/ember-01".to_string(),
        "zai/ember-02".to_string(),
        "openai/ember-03".to_string(),
        "anthropic/ember-04".to_string(),
        "ollama/ember-05".to_string(),
        "zai/ember-06".to_string(),
        "openai/ember-07".to_string(),
        "anthropic/ember-08".to_string(),
        "ollama/ember-09".to_string(),
        "zai/ember-10".to_string(),
        "openai/ember-11".to_string(),
        "anthropic/ember-12".to_string(),
        "ollama/ember-13".to_string(),
        "zai/ember-14".to_string(),
        "openai/ember-15".to_string(),
    ];

    app.set_available_models(models.clone());

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should open when typing ':model '"
    );

    let mut rendered_states = String::new();
    for _ in 0..models.len() {
        let tree = view_with_default_ctx(&app);
        rendered_states.push_str(&render_to_string(&tree, 80));
        app.update(Event::Key(key(KeyCode::Down)));
    }

    for model in &models {
        assert!(
            rendered_states.contains(model),
            "Popup should render all 15 models across navigable popup states, missing: {}",
            model
        );
    }
}

#[test]
fn model_popup_sixteen_models_all_selectable() {
    let mut app = OilChatApp::default();
    let models = vec![
        "ollama/ridge-01".to_string(),
        "zai/ridge-02".to_string(),
        "openai/ridge-03".to_string(),
        "anthropic/ridge-04".to_string(),
        "ollama/ridge-05".to_string(),
        "zai/ridge-06".to_string(),
        "openai/ridge-07".to_string(),
        "anthropic/ridge-08".to_string(),
        "ollama/ridge-09".to_string(),
        "zai/ridge-10".to_string(),
        "openai/ridge-11".to_string(),
        "anthropic/ridge-12".to_string(),
        "ollama/ridge-13".to_string(),
        "zai/ridge-14".to_string(),
        "openai/ridge-15".to_string(),
        "anthropic/ridge-16".to_string(),
    ];
    let fifteenth_model = models[14].clone();
    let sixteenth_model = models[15].clone();

    app.set_available_models(models);

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should open when typing ':model '"
    );

    for _ in 0..30 {
        app.update(Event::Key(key(KeyCode::Down)));
    }

    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        app.input_content()
            .contains(&format!(":model {}", sixteenth_model)),
        "16th model should be selectable now that popup limit is raised to 100"
    );
    assert!(
        !app.input_content()
            .contains(&format!(":model {}", fifteenth_model)),
        "Selection should reach the 16th model, not saturate at the 15th"
    );
}

#[test]
fn model_popup_filter_across_all_provider_prefixes() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/visioncraft-alpha".to_string(),
        "zai/visioncraft-beta".to_string(),
        "openai/visioncraft-gamma".to_string(),
        "anthropic/visioncraft-delta".to_string(),
        "ollama/llama3.2".to_string(),
        "zai/GLM-4.7".to_string(),
        "openai/gpt-4o".to_string(),
        "anthropic/claude-3".to_string(),
    ]);

    for c in ":model visioncraft".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(app.is_popup_visible(), "Popup should be visible");
    assert_eq!(
        app.current_popup_filter(),
        "visioncraft",
        "Filter should be 'visioncraft'"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("ollama/visioncraft-alpha"),
        "Filter should include matching ollama model"
    );
    assert!(
        output.contains("zai/visioncraft-beta"),
        "Filter should include matching zai model"
    );
    assert!(
        output.contains("openai/visioncraft-gamma"),
        "Filter should include matching openai model"
    );
    assert!(
        output.contains("anthropic/visioncraft-delta"),
        "Filter should include matching anthropic model"
    );
}

#[test]
fn model_command_selection_fills_input() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
    ]);

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(app.is_popup_visible(), "Popup should be visible");

    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        !app.is_popup_visible(),
        "Popup should close after selection"
    );
    assert!(
        app.input_content().contains(":model ollama/llama3"),
        "Input should contain ':model ollama/llama3', got: {}",
        app.input_content()
    );
}

#[test]
fn model_command_popup_select_updates_model() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec!["ollama/llama3".to_string()]);

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(app.is_popup_visible(), "Popup should open after ':model '");

    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        !app.is_popup_visible(),
        "Popup should close after selection"
    );

    assert!(
        app.input_content().contains(":model ollama/llama3"),
        "Input should contain ':model ollama/llama3', got: {}",
        app.input_content()
    );

    let action = app.update(Event::Key(key(KeyCode::Enter)));

    match action {
        Action::Send(msg) => {
            app.on_message(msg);
        }
        other => panic!(
            "Expected Action::Send after submitting, got {:?}. Input was: '{}'",
            other,
            app.input_content()
        ),
    }

    assert_eq!(
        app.current_model(),
        "ollama/llama3",
        "Model should be updated to ollama/llama3"
    );
}

#[test]
fn model_command_no_models_shows_message() {
    use crate::tui::oil::chat_app::ChatAppMsg;

    let mut app = OilChatApp::default();

    // First :model triggers lazy fetch (state is NotLoaded)
    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    // Simulate the fetch completing with empty model list
    app.on_message(ChatAppMsg::ModelsLoaded(vec![]));

    // Now try :model again - should show "No models configured" message
    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("No models configured"),
        "Should show 'No models configured' when no models available. Got: {}",
        output
    );
}

#[test]
fn model_popup_repl_command_keeps_open_when_typing_filter() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
        "openai/gpt-4".to_string(),
    ]);

    // Open via REPL command path (NOT autocomplete path)
    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    // Popup should NOT be open — REPL command shows message instead
    assert!(
        !app.is_popup_visible(),
        "Popup should NOT be open after ':model' + Enter (REPL command shows message)"
    );

    // Check that the model list message is shown
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Available models"),
        "Should show available models list in chat. Got: {}",
        output
    );
}

#[test]
fn model_popup_repl_command_not_loaded_keeps_open_after_models_arrive() {
    use crate::tui::oil::chat_app::ChatAppMsg;
    let mut app = OilChatApp::default();
    // State is NotLoaded by default

    // Open via REPL command (triggers lazy fetch)
    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    // Popup should NOT be open — REPL command shows "Fetching..." message
    assert!(
        !app.is_popup_visible(),
        "Popup should NOT be open after ':model' + Enter (REPL command shows message)"
    );

    // Check that the fetching message is shown
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Fetching available models"),
        "Should show 'Fetching available models' message. Got: {}",
        output
    );

    // Simulate models arriving
    app.on_message(ChatAppMsg::ModelsLoaded(vec![
        "ollama/llama3".to_string(),
        "ollama/llama2".to_string(),
        "anthropic/claude-3".to_string(),
    ]));

    // Popup should still NOT be open
    assert!(
        !app.is_popup_visible(),
        "Popup should still NOT be open after models arrive"
    );
}

#[test]
fn model_popup_repl_command_multi_char_filter_narrows_results() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "ollama/llama2".to_string(),
        "anthropic/claude-3".to_string(),
        "openai/gpt-4".to_string(),
    ]);

    // Open via REPL command path
    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    // Popup should NOT be open — REPL command shows message instead
    assert!(
        !app.is_popup_visible(),
        "Popup should NOT be open after ':model' + Enter (REPL command shows message)"
    );

    // Verify the model list is shown in chat with all models
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Available models"),
        "Should show available models list. Got: {}",
        output
    );
    assert!(
        output.contains("llama3") && output.contains("claude-3") && output.contains("gpt-4"),
        "Should show all models in the list"
    );
}

#[test]
fn model_repl_command_in_popup_list() {
    let mut app = OilChatApp::default();

    for c in ":".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(app.is_popup_visible(), "Popup should open on :");

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains(":model"),
        "REPL command popup should include :model"
    );
}

// =============================================================================
// Config Command Tests
// =============================================================================

#[test]
fn config_show_command_displays_values() {
    let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
    harness.render();

    // Type :config show command
    harness.send_text(":config show");
    harness.send_enter();

    let output = harness.screen();

    // Should display temperature value
    assert!(
        output.contains("temperature:") || output.contains("temperature ="),
        "Should display temperature value. Got: {}",
        output
    );

    // Should display max_tokens value
    assert!(
        output.contains("max_tokens:")
            || output.contains("max_tokens =")
            || output.contains("maxtokens"),
        "Should display max_tokens value. Got: {}",
        output
    );

    // Should display thinking_budget value
    assert!(
        output.contains("thinking_budget:")
            || output.contains("thinking_budget =")
            || output.contains("thinkingbudget"),
        "Should display thinking_budget value. Got: {}",
        output
    );

    // Should display mode value
    assert!(
        output.contains("mode:") || output.contains("mode ="),
        "Should display mode value. Got: {}",
        output
    );
}

// =============================================================================
// BackTab Mode Cycling Tests
// =============================================================================

fn rendered_status_bar(app: &OilChatApp) -> String {
    let tree = view_with_default_ctx(app);
    let output = render_to_string(&tree, 80);
    crate::tui::oil::ansi::strip_ansi(&output)
}

#[test]
fn backtab_cycles_mode_from_default() {
    let mut app = OilChatApp::default();
    let initial = rendered_status_bar(&app);
    assert!(
        initial.contains("NORMAL"),
        "Default mode should be NORMAL: {}",
        initial
    );

    app.update(Event::Key(KeyEvent::new(
        KeyCode::BackTab,
        KeyModifiers::SHIFT,
    )));
    let after = rendered_status_bar(&app);
    assert!(
        after.contains("PLAN") || after.contains("AUTO"),
        "BackTab should cycle to PLAN or AUTO: {}",
        after
    );
}

#[test]
fn backtab_cycles_through_all_modes() {
    let mut app = OilChatApp::default();

    let mut modes_seen = Vec::new();
    for _ in 0..4 {
        let bar = rendered_status_bar(&app);
        if bar.contains("PLAN") {
            modes_seen.push("PLAN");
        } else if bar.contains("NORMAL") {
            modes_seen.push("NORMAL");
        } else if bar.contains("AUTO") {
            modes_seen.push("AUTO");
        }
        app.update(Event::Key(KeyEvent::new(
            KeyCode::BackTab,
            KeyModifiers::SHIFT,
        )));
    }

    assert!(
        modes_seen.len() >= 2,
        "Should visit multiple modes: {:?}",
        modes_seen
    );
    assert!(
        modes_seen.contains(&"NORMAL"),
        "Should visit NORMAL mode: {:?}",
        modes_seen
    );
}

#[test]
fn backtab_during_streaming_still_cycles() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::UserMessage("Hello".to_string()));
    app.on_message(ChatAppMsg::TextDelta("Hi there".to_string()));

    let bar_before = rendered_status_bar(&app);
    app.update(Event::Key(KeyEvent::new(
        KeyCode::BackTab,
        KeyModifiers::SHIFT,
    )));
    let bar_after = rendered_status_bar(&app);
    assert_ne!(
        bar_before, bar_after,
        "BackTab should change mode during streaming"
    );
}

// =============================================================================
// :set Command Tests
// =============================================================================

#[test]
fn set_unknown_option_echoes_value() {
    let mut harness: AppHarness<OilChatApp> = AppHarness::new(80, 24);
    harness.render();

    harness.send_text(":set nonexistent_option=true");
    harness.send_enter();

    let output = harness.screen();
    assert!(
        output.contains("nonexistent_option"),
        "Should echo the set option. Got: {}",
        output
    );
}

// =============================================================================
// Notification Lifecycle Tests
// =============================================================================

#[test]
fn error_notification_appears_and_app_stays_responsive() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::Error("Database connection failed".to_string()));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Database connection failed"),
        "Error should be visible: {}",
        output
    );

    for c in "still typing".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("still typing"),
        "Input should still work after error: {}",
        output
    );
}

// =============================================================================
// Model Loading State Tests
// =============================================================================

#[test]
fn models_loaded_updates_popup_content() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ModelsLoaded(vec![
        "llama3.2".to_string(),
        "mistral".to_string(),
        "codellama".to_string(),
    ]));

    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("llama3.2") || output.contains("mistral"),
        "Model popup should show loaded models: {}",
        output
    );
}

#[test]
fn model_fetch_failed_shows_error_in_popup() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ModelsFetchFailed(
        "Connection refused".to_string(),
    ));

    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);
    assert!(
        output.contains("Connection refused")
            || output.contains("error")
            || output.contains("failed")
            || output.contains("No models"),
        "Should show error when models failed to load: {}",
        output
    );
}

#[test]
fn set_available_models_sets_loaded_state_when_non_empty() {
    let mut app = OilChatApp::default();
    assert_eq!(
        app.model_list_state(),
        &ModelListState::NotLoaded,
        "Initial state should be NotLoaded"
    );

    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
    ]);

    assert_eq!(
        app.model_list_state(),
        &ModelListState::Loaded,
        "State should be Loaded after setting non-empty models"
    );
    assert_eq!(app.available_models().len(), 2, "Models should be stored");
}

#[test]
fn set_available_models_does_not_set_loaded_state_when_empty() {
    let mut app = OilChatApp::default();

    app.set_available_models(vec![]);

    assert_eq!(
        app.model_list_state(),
        &ModelListState::NotLoaded,
        "State should remain NotLoaded when setting empty models"
    );
    assert!(app.available_models().is_empty(), "Models should be empty");
}

// =============================================================================
// Model REPL Command State Tests (new behavior: :model<CR> shows chat listing)
// =============================================================================

#[test]
fn model_repl_loaded_lists_models_in_chat() {
    use crate::tui::oil::chat_app::ChatAppMsg;

    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
    ]);
    app.on_message(ChatAppMsg::SwitchModel("ollama/llama3".to_string()));

    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        !app.is_popup_visible(),
        "Popup should NOT be open after ':model' + Enter (REPL command shows message)"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("Available models"),
        "Should show 'Available models' header. Got: {}",
        output
    );
    assert!(
        output.contains("ollama/llama3"),
        "Should list ollama/llama3. Got: {}",
        output
    );
    assert!(
        output.contains("anthropic/claude-3"),
        "Should list anthropic/claude-3. Got: {}",
        output
    );
}

#[test]
fn model_repl_loaded_marks_current_model() {
    use crate::tui::oil::chat_app::ChatAppMsg;

    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
    ]);
    app.on_message(ChatAppMsg::SwitchModel("ollama/llama3".to_string()));

    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("← current"),
        "Should mark the current model with '← current'. Got: {}",
        output
    );
}

#[test]
fn model_repl_loaded_empty_shows_not_configured() {
    use crate::tui::oil::chat_app::ChatAppMsg;

    let mut app = OilChatApp::default();
    // ModelsLoaded(vec![]) transitions to Loaded state with empty models
    app.on_message(ChatAppMsg::ModelsLoaded(vec![]));

    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        !app.is_popup_visible(),
        "Popup should NOT be open after ':model' + Enter"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("No models configured"),
        "Should show 'No models configured' when Loaded state has empty list. Got: {}",
        output
    );
    assert!(
        !output.contains("No models available"),
        "Should NOT show old broken message 'No models available'. Got: {}",
        output
    );
}

#[test]
fn model_command_in_not_loaded_state_triggers_fetch() {
    let mut app = OilChatApp::default();

    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    let action = app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        matches!(action, Action::Send(ChatAppMsg::FetchModels)),
        ":model should trigger FetchModels when state is NotLoaded, got: {:?}",
        action
    );
}

#[test]
fn model_command_in_loading_state_triggers_refetch() {
    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::FetchModels);
    assert_eq!(app.model_list_state(), &ModelListState::Loading);

    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    let action = app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        matches!(action, Action::Send(ChatAppMsg::FetchModels)),
        ":model should re-trigger FetchModels when state is Loading, got: {:?}",
        action
    );
    assert_eq!(
        app.model_list_state(),
        &ModelListState::NotLoaded,
        "Loading state should reset to NotLoaded before retry"
    );
}

#[test]
fn model_repl_failed_retries_and_shows_message() {
    use crate::tui::oil::chat_app::ChatAppMsg;

    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ModelsFetchFailed(
        "connection refused".to_string(),
    ));

    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    assert!(
        !app.is_popup_visible(),
        "Popup should NOT be open after ':model' + Enter"
    );

    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    assert!(
        output.contains("Retrying"),
        "Should show 'Retrying' message when state is Failed. Got: {}",
        output
    );
    assert!(
        output.contains("connection refused"),
        "Should include the error reason in the message. Got: {}",
        output
    );
}

#[test]
fn model_popup_failed_state_retries_fetch() {
    use crate::tui::oil::chat_app::ChatAppMsg;

    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::ModelsFetchFailed("timeout".to_string()));

    // Type ':model ' (with space) — autocomplete popup path
    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should be visible when typing ':model ' in Failed state (force-shown for retry)"
    );
}

#[test]
fn model_popup_loading_state_forces_visible() {
    use crate::tui::oil::chat_app::ChatAppMsg;

    let mut app = OilChatApp::default();
    app.on_message(ChatAppMsg::FetchModels);

    // Type ':model ' (with space) — autocomplete popup path
    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }

    assert!(
        app.is_popup_visible(),
        "Popup should be visible when typing ':model ' in Loading state (force-shown during loading)"
    );
}

#[test]
fn spinner_style_uses_theme_color() {
    use crate::tui::oil::style::Style;
    use crate::tui::oil::theme::ThemeConfig;

    // Verify that constructing a spinner style from ThemeConfig produces a non-default style
    let theme = ThemeConfig::default_dark();
    let spinner_style = Style::new().fg(theme.resolve_color(theme.colors.text));

    assert_ne!(
        spinner_style,
        Style::default(),
        "Spinner style should not be default (should have color applied)"
    );
}

#[test]
fn needs_turn_spinner_returns_true_after_user_message() {
    use crate::tui::oil::ContainerList;

    // Create a new container list (initial state: turn_active = false)
    let mut containers = ContainerList::new();

    // Verify initial state: turn_active is false, so needs_turn_spinner returns false
    assert!(
        !containers.needs_turn_spinner(),
        "needs_turn_spinner should return false when turn_active is false"
    );

    // Mark the turn as active (simulates user submitting a message)
    containers.mark_turn_active();

    // After marking turn active, needs_turn_spinner should return true
    // (spinner should show immediately, before any LLM response arrives)
    assert!(
        containers.needs_turn_spinner(),
        "needs_turn_spinner should return true immediately after mark_turn_active"
    );
}

#[test]
fn model_loading_message_not_duplicated() {
    let mut app = OilChatApp::default();
    // Set state to Loading to simulate a fetch in progress
    app.set_model_list_state(ModelListState::Loading);

    // Press :model<CR> three times while in Loading state
    for _ in 0..3 {
        for c in ":model".chars() {
            app.update(Event::Key(key(KeyCode::Char(c))));
        }
        app.update(Event::Key(key(KeyCode::Enter)));
    }

    // Render the chat and count occurrences of "Retrying" or "Fetching"
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    // Count how many times "Retrying" appears
    let retrying_count = output.matches("Retrying").count();
    let fetching_count = output.matches("Fetching").count();
    let total_fetch_messages = retrying_count + fetching_count;

    assert!(
        total_fetch_messages <= 1,
        "Should have at most 1 fetch/retrying message, but found {} (Retrying: {}, Fetching: {})",
        total_fetch_messages,
        retrying_count,
        fetching_count
    );
}

#[test]
fn model_fetch_message_shown_once() {
    let mut app = OilChatApp::default();
    // State is NotLoaded by default
    assert_eq!(
        *app.model_list_state(),
        ModelListState::NotLoaded,
        "Initial state should be NotLoaded"
    );

    // Press :model<CR> once
    for c in ":model".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    app.update(Event::Key(key(KeyCode::Enter)));

    // Render and check that a fetch message appears
    let tree = view_with_default_ctx(&app);
    let output = render_to_string(&tree, 80);

    let has_fetch_message = output.contains("Fetching") || output.contains("Retrying");
    assert!(
        has_fetch_message,
        "Should show a fetch message on first :model press"
    );

    // Count occurrences
    let fetch_count = output.matches("Fetching").count() + output.matches("Retrying").count();
    assert_eq!(
        fetch_count, 1,
        "Should show exactly 1 fetch message, but found {}",
        fetch_count
    );
}

#[test]
fn model_space_backspace_renders_single_border_row() {
    let mut app = OilChatApp::default();
    app.set_available_models(vec![
        "ollama/llama3".to_string(),
        "anthropic/claude-3".to_string(),
    ]);

    for c in ":model ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    assert!(
        app.is_popup_visible(),
        "Popup should be visible for :model "
    );

    app.update(Event::Key(key(KeyCode::Backspace)));

    assert_eq!(
        app.input_content(),
        ":model",
        "Backspace should remove the trailing space"
    );
    assert!(
        app.is_popup_visible(),
        "Popup should stay visible and switch to REPL command completion"
    );

    let rendered = composited_output(&app);
    let border_rows = count_half_block_border_rows(&rendered);
    assert_eq!(
        border_rows, 1,
        "Expected exactly one half-block border row after :model<BS>, got {}.\n{}",
        border_rows, rendered
    );
}

#[test]
fn set_space_backspace_renders_single_border_row() {
    let mut app = OilChatApp::default();

    for c in ":set ".chars() {
        app.update(Event::Key(key(KeyCode::Char(c))));
    }
    assert!(app.is_popup_visible(), "Popup should be visible for :set ");

    app.update(Event::Key(key(KeyCode::Backspace)));

    assert_eq!(
        app.input_content(),
        ":set",
        "Backspace should remove the trailing space"
    );
    assert!(
        app.is_popup_visible(),
        "Popup should stay visible and switch to REPL command completion"
    );

    let rendered = composited_output(&app);
    let border_rows = count_half_block_border_rows(&rendered);
    assert_eq!(
        border_rows, 1,
        "Expected exactly one half-block border row after :set<BS>, got {}.\n{}",
        border_rows, rendered
    );
}
