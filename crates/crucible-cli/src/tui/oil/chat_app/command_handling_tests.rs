//! Slash- and REPL-command dispatch tests.
//!
//! Split out of `command_handling.rs` for the 1500-line file-size gate, and
//! attached with `#[path]` rather than moved into `tui/oil/tests/` because the
//! handlers under test are `pub(super)` — reachable from a descendant of
//! `chat_app`, not from the sibling test tree.

use super::*;

// --- suggest_command tests ---

#[test]
fn suggest_command_exact_match() {
    assert_eq!(
        suggest_command("quit", ReplCommand::ALL),
        Some(ReplCommand::Quit)
    );
}

#[test]
fn suggest_command_typo_within_distance_2() {
    let known = &[
        ReplCommand::Quit,
        ReplCommand::Help,
        ReplCommand::Clear,
        ReplCommand::Model,
    ];
    assert_eq!(suggest_command("quiy", known), Some(ReplCommand::Quit));
    assert_eq!(suggest_command("hlep", known), Some(ReplCommand::Help));
    assert_eq!(suggest_command("claer", known), Some(ReplCommand::Clear));
}

#[test]
fn suggest_command_no_match_beyond_distance_2() {
    assert_eq!(suggest_command("xyzzy", ReplCommand::ALL), None);
    assert_eq!(suggest_command("abcdefgh", ReplCommand::ALL), None);
}

/// An alias is a candidate, but the suggestion names the command.
#[test]
fn suggest_command_matches_an_alias() {
    assert_eq!(
        suggest_command("msg", ReplCommand::ALL),
        Some(ReplCommand::Messages)
    );
}

#[test]
fn suggest_command_empty_input() {
    // Empty string is distance 5 from "clear" — beyond threshold of 2
    assert_eq!(
        suggest_command("", &[ReplCommand::Clear, ReplCommand::Model]),
        None
    );
}

// ════════════════════════════════════════════════════════════════
// US-104: `:set` runtime-config dispatch matrix
// ════════════════════════════════════════════════════════════════

use crate::tui::oil::chat_app::ChatAppMsg;
use test_case::test_case;

fn app() -> OilChatApp {
    OilChatApp::default()
}

/// Run a `:set` body (e.g. `"contextbudget=128000"`) through the real
/// command handler and return the resulting action.
fn run_set(app: &mut OilChatApp, body: &str) -> Action<ChatAppMsg> {
    app.handle_set_command(&format!("set {body}"))
}

// Every session-scoped key must emit a daemon-sync `Action::Send` so
// multi-client state stays consistent (see AGENTS.md cross-layer checklist).
#[test_case("model=gpt-4o" ; "model")]
#[test_case("contextbudget=128000" ; "context budget")]
#[test_case("contextstrategy=truncate" ; "context strategy")]
#[test_case("outputvalidation=off" ; "output validation")]
#[test_case("validationretries=2" ; "validation retries")]
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
fn set_model_maps_to_switch_model() {
    let mut app = app();
    assert!(matches!(
        run_set(&mut app, "model=gpt-4o"),
        Action::Send(ChatAppMsg::SwitchModel(m)) if m == "gpt-4o"
    ));
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
    run_set(&mut app, "contextstrategy=truncate");
    let stored = app
        .runtime_config
        .get("contextstrategy")
        .expect("value stored");
    assert_eq!(stored.as_string(), Some("truncate"));
}

// Invalid values surface a warning and do NOT emit a daemon sync.
#[test_case("contextbudget=abc" ; "non-numeric budget")]
#[test_case("contextstrategy=nonsense" ; "unknown strategy")]
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

/// A key the TUI does not own belongs to the daemon app-config store. `:set`
/// sends it there and records nothing locally, so the two stores cannot hold
/// different values for one key.
#[test]
fn set_of_an_app_config_key_records_no_local_value() {
    let mut app = app();
    let action = run_set(&mut app, "myplugin.debug=true");
    assert!(
        matches!(
            action,
            Action::Send(ChatAppMsg::ConfigSet { ref key, ref value })
                if key == "myplugin.debug" && *value == serde_json::json!(true)
        ),
        "an app-config `:set` goes to the daemon store"
    );
    assert!(
        app.runtime_config.get("myplugin.debug").is_none(),
        "the typed value must not become a local answer before the daemon replies"
    );
}

/// The value a later read answers with is the daemon's, not the one that was
/// typed. The two differ here on purpose: only the store's answer can produce
/// 7 from a typed 3.
#[test]
fn an_app_config_read_answers_with_the_daemon_value() {
    let mut app = app();
    run_set(&mut app, "myplugin.retries=3");
    app.on_message(ChatAppMsg::ConfigSetResolved {
        key: "myplugin.retries".to_string(),
        value: serde_json::json!(7),
    });
    assert_eq!(
        app.runtime_config
            .get("myplugin.retries")
            .and_then(|v| v.as_int()),
        Some(7),
        "the store's answer is what `:set key?` reads back"
    );
}

/// A write the daemon refuses leaves no value behind, and says so. Swallowing
/// it into a local value is what let the TUI report a setting the daemon
/// never took.
#[test]
fn a_refused_app_config_write_surfaces_and_leaves_no_value() {
    let mut app = app();
    run_set(&mut app, "kiln_path=/nowhere");
    assert!(
        app.runtime_config.get("kiln_path").is_none(),
        "nothing is recorded before the daemon answers"
    );

    app.on_message(ChatAppMsg::Error(
        "set kiln_path: 'kiln_path' names where the daemon acts".to_string(),
    ));
    assert!(
        app.has_notifications(),
        "a refused config write must reach the user"
    );
    assert!(
        app.runtime_config.get("kiln_path").is_none(),
        "a refused write must not leave a local value the daemon disagrees with"
    );
}

/// The value-less spellings write the same store as `key=value` does. A
/// local-only `:set nofoo` after a daemon-bound `foo=true` is two answers for
/// one key.
#[test_case(":set myplugin.debug", serde_json::json!(true) ; "enable")]
#[test_case(":set nomyplugin.debug", serde_json::json!(false) ; "disable")]
fn a_value_less_app_config_set_goes_to_the_daemon(input: &str, expected: serde_json::Value) {
    let mut app = app();
    let action = app.handle_set_command(input);
    assert!(
        matches!(
            action,
            Action::Send(ChatAppMsg::ConfigSet { ref key, ref value })
                if key == "myplugin.debug" && *value == expected
        ),
        "`{input}` must reach the daemon store, got {action:?}"
    );
    assert!(
        app.runtime_config.get("myplugin.debug").is_none(),
        "`{input}` must not record a second local answer"
    );
}

/// A key this client owns keeps answering locally: no daemon round trip, and
/// no app-config entry for a display setting.
#[test]
fn a_display_key_stays_local() {
    let mut app = app();
    let action = run_set(&mut app, "show_diffs=false");
    assert!(
        matches!(action, Action::Continue),
        "`show_diffs` is display state, not app config"
    );
    assert!(!app.show_diffs);
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
fn set_syntax_theme_updates_the_active_highlighter() {
    let _guard = crate::formatting::syntax::ACTIVE_STATE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut app = app();
    let action = run_set(&mut app, "syntax_theme=Solarized (dark)");
    assert!(matches!(action, Action::Continue));
    assert_eq!(
        crate::formatting::syntax::active_theme_name(),
        "Solarized (dark)"
    );
    let stored = app.runtime_config.get("syntax_theme").expect("stored");
    assert_eq!(stored.as_string(), Some("Solarized (dark)"));
}

/// `:set syntax_theme&` must revert the RENDERED theme, not just the stored
/// value — otherwise the query reports the default while diffs/code
/// blocks keep highlighting with the old override.
#[test]
fn set_syntax_theme_reset_reverts_active_theme_to_seed() {
    let _guard = crate::formatting::syntax::ACTIVE_STATE_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    crate::formatting::syntax::seed_from_config(&crucible_core::config::HighlightingConfig {
        enabled: true,
        theme: "base16-eighties.dark".to_string(),
    });
    let mut app = app();
    run_set(&mut app, "syntax_theme=InspiredGitHub");
    assert_eq!(
        crate::formatting::syntax::active_theme_name(),
        "InspiredGitHub"
    );
    run_set(&mut app, "syntax_theme&");
    assert_eq!(
        crate::formatting::syntax::active_theme_name(),
        "base16-eighties.dark",
        "reset must revert rendering to the config-seeded theme"
    );
}

#[test]
fn set_syntax_theme_invalid_value_warns_and_leaves_state() {
    let mut app = app();
    let action = run_set(&mut app, "syntax_theme=no-such-theme");
    assert!(matches!(action, Action::Continue));
    assert!(
        app.has_notifications(),
        "invalid theme should surface a warning listing valid themes"
    );
    assert!(
        app.runtime_config.get("syntax_theme").is_none(),
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
    let action = app.handle_set_command("set contextbudget?");
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
    assert_eq!(app.mode(), "plan");
}

#[test]
fn slash_mode_cycles() {
    let mut app = app();
    assert_eq!(app.mode(), "ask");
    app.handle_slash_command("/mode");
    assert_eq!(app.mode(), "plan");
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
fn registered_plugin_command_dispatches_to_run_plugin_command() {
    let mut app = app();
    app.set_plugin_commands(vec![(
        "reflect".to_string(),
        "Run a reflection pass".into(),
    )]);
    let action = app.handle_slash_command("/reflect last 3 turns");
    assert!(
        matches!(
            action,
            Action::Send(ChatAppMsg::RunPluginCommand { ref name, ref args })
                if name == "reflect" && args == "last 3 turns"
        ),
        "a plugin command is an invocation, not chat text — got {action:?}"
    );
}

#[test]
fn plugin_command_cannot_shadow_a_builtin_slash() {
    let mut app = app();
    app.set_plugin_commands(vec![("plan".to_string(), "impostor".into())]);
    app.handle_slash_command("/plan");
    assert_eq!(
        app.mode(),
        "plan",
        "built-ins dispatch before plugin names, so /plan stays mode-switching"
    );
}

#[test]
fn plugin_commands_join_slash_autocomplete() {
    let mut app = app();
    app.set_plugin_commands(vec![(
        "reflect".to_string(),
        "Run a reflection pass".into(),
    )]);
    assert!(
        app.slash_commands
            .iter()
            .any(|(n, d)| n == "reflect" && d.contains("(plugin)")),
        "plugin commands must be discoverable, not just callable"
    );
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

/// A mode the TUI has never heard of is its own slash command, and `/mode`
/// walks the daemon's list rather than a fixed Normal → Plan → Auto ring.
#[test]
fn a_lua_declared_mode_is_selectable_and_cyclable() {
    let mut app = app();
    app.on_message(ChatAppMsg::ModesLoaded(vec![
        "ask".to_string(),
        "review".to_string(),
    ]));

    app.handle_slash_command("/review");
    assert_eq!(
        app.mode(),
        "review",
        "a declared mode is its own slash command"
    );

    app.handle_slash_command("/mode");
    assert_eq!(
        app.mode(),
        "ask",
        "cycling wraps within the daemon's list, not the built-in ring"
    );
    app.handle_slash_command("/mode");
    assert_eq!(
        app.mode(),
        "review",
        "and reaches the declared mode, which the built-in ring never would"
    );
}

/// A mode whose declaration is gone must not advance into another one:
/// `set_mode` would reject it, leaving the badge and the daemon disagreeing.
#[test]
fn a_mode_the_daemon_no_longer_offers_cycles_nowhere() {
    let mut app = app();
    app.on_message(ChatAppMsg::ModesLoaded(vec!["ask".to_string()]));
    app.handle_slash_command("/mode");
    assert_eq!(app.mode(), "ask");

    app.on_message(ChatAppMsg::ModeSynced("review".into()));
    app.handle_slash_command("/mode");
    assert_eq!(
        app.mode(),
        "review",
        "an undeclared mode stays put rather than jumping to the first declared one"
    );
}

/// A mode may not shadow a built-in slash command. Arms are tried in order,
/// so a mode declared as `undo` used to make `/undo` switch modes and leave
/// no way to reach the real command.
#[test]
fn a_mode_named_after_a_builtin_does_not_shadow_it() {
    let mut app = app();
    app.on_message(ChatAppMsg::ModesLoaded(vec![
        "ask".to_string(),
        "undo".to_string(),
        "help".to_string(),
    ]));

    let action = app.handle_slash_command("/undo 2");
    assert!(
        matches!(action, Action::Send(ChatAppMsg::Undo(2))),
        "/undo must still undo, got {action:?}"
    );
    assert_eq!(app.mode(), "ask", "and must not have changed the mode");
}

/// A mode declared as `cru.modes.Review` is reachable as `/review`, and
/// switching to it reports the id the daemon actually declared.
#[test]
fn a_mode_id_matches_case_insensitively() {
    let mut app = app();
    app.on_message(ChatAppMsg::ModesLoaded(vec![
        "ask".to_string(),
        "Review".to_string(),
    ]));

    app.handle_slash_command("/review");
    assert_eq!(
        app.mode(),
        "Review",
        "the daemon's own spelling wins — it is what set_mode validates against"
    );
}

/// `:set precognition` / `:set noprecognition` / `:set precognition!` are the
/// spellings `:help` advertises, and they never reach `classify_set_value` —
/// they go straight to the runtime-config enable/disable/toggle handlers.
/// Those handlers wrote a local bool and returned `Continue`, which made the
/// three documented spellings the *most* broken of the four: the readout
/// changed, the daemon kept injecting.
#[test]
fn value_less_precognition_spellings_carry_the_value_to_the_daemon() {
    let mut app = app();

    let off = app.handle_set_command("set noprecognition");
    assert!(
        matches!(off, Action::Send(ChatAppMsg::SetPrecognition(false))),
        "`:set noprecognition` must sync precognition=false, got {off:?}"
    );

    let on = app.handle_set_command("set precognition");
    assert!(
        matches!(on, Action::Send(ChatAppMsg::SetPrecognition(true))),
        "`:set precognition` must sync precognition=true, got {on:?}"
    );

    // Toggle reads the value the previous line left behind, so this also
    // pins that the enable path stored a real bool rather than a string.
    let toggled = app.handle_set_command("set precognition!");
    assert!(
        matches!(toggled, Action::Send(ChatAppMsg::SetPrecognition(false))),
        "`:set precognition!` must sync the flipped value, got {toggled:?}"
    );
}

/// `thinking` shares the `:set` arm precognition used to sit in, and it is
/// staying there: it hides or shows reasoning blocks in this client's
/// transcript and changes nothing the daemon does. Pinning that so the next
/// person to read the two side by side does not "fix" it into an RPC.
#[test]
fn thinking_stays_a_local_display_toggle() {
    let mut app = app();
    let action = run_set(&mut app, "thinking=false");
    assert!(
        matches!(action, Action::Continue),
        "`:set thinking` is display state; it must not emit a daemon-sync message, got {action:?}"
    );
}

// ════════════════════════════════════════════════════════════════
// T5-31: exhaustive `ReplCommand` dispatch
// ════════════════════════════════════════════════════════════════

/// Every variant the compiler knows reaches its own arm. A bare command
/// name must never fall through to the "Unknown REPL command" branch.
#[test]
fn every_repl_command_dispatches_without_unknown_warning() {
    use strum::IntoEnumIterator;
    for cmd in ReplCommand::iter() {
        let mut app = app();
        let line = format!(":{}", cmd.name());
        let _ = app.handle_repl_command(&line);
        let unknown = app
            .notification_area
            .history()
            .iter()
            .any(|(n, _)| n.message.contains("Unknown REPL command"));
        assert!(!unknown, "{:?} fell through to the unknown branch", cmd);
    }
}

/// An alias resolves to the same variant as the name.
#[test]
fn repl_aliases_resolve_to_their_command() {
    use strum::IntoEnumIterator;
    for cmd in ReplCommand::iter() {
        assert_eq!(ReplCommand::parse(cmd.name()), Some(cmd));
        for alias in cmd.aliases() {
            assert_eq!(ReplCommand::parse(alias), Some(cmd), "{alias}");
        }
    }
    assert_eq!(ReplCommand::parse("nope"), None);
}

/// `:set perm.show_diff=y` and `=n` are accepted bool tokens.
#[test]
fn perm_set_accepts_y_and_n() {
    let mut app = app();
    app.handle_perm_set("perm.show_diff", "n");
    assert!(!app.permission.perm_show_diff, "n is a valid bool token");
    app.handle_perm_set("perm.show_diff", "y");
    assert!(app.permission.perm_show_diff, "y is a valid bool token");
}

/// A rejected bool value names the tokens the user can type.
#[test]
fn perm_set_rejection_lists_accepted_tokens() {
    let mut app = app();
    app.handle_perm_set("perm.show_diff", "maybe");
    let msgs: Vec<String> = app
        .notification_area
        .history()
        .iter()
        .map(|(n, _)| n.message.clone())
        .collect();
    let text = msgs.join("\n");
    for token in [
        "true", "false", "yes", "no", "on", "off", "y", "n", "1", "0",
    ] {
        assert!(text.contains(token), "missing {token} in: {text}");
    }
}

// ════════════════════════════════════════════════════════════════
// Every `:set` spelling of one key answers from one store
// ════════════════════════════════════════════════════════════════

/// The `:set` input text for one [`SetCommand`] variant and one key.
///
/// `None` for the two variants that carry no key. The match is exhaustive on
/// purpose: a new spelling must be given input text here before the
/// completeness tests below can walk it.
fn spelling_for(command: &SetCommand, key: &str) -> Option<String> {
    match command {
        SetCommand::ShowModified | SetCommand::ShowAll => None,
        SetCommand::Query { .. } => Some(format!("{key}?")),
        SetCommand::QueryHistory { .. } => Some(format!("{key}??")),
        SetCommand::Enable { .. } => Some(key.to_string()),
        SetCommand::Disable { .. } => Some(format!("no{key}")),
        SetCommand::Toggle { .. } => Some(format!("{key}!")),
        SetCommand::Reset { .. } => Some(format!("{key}&")),
        SetCommand::Pop { .. } => Some(format!("{key}^")),
        SetCommand::Unset { .. } => Some(format!("{key}=")),
        SetCommand::Set { .. } => Some(format!("{key}=1")),
    }
}

/// `:set key?` on an app-config key must ask the daemon. This client keeps no
/// copy of app config, so a local answer is "not set" for every key
/// `init.lua` wrote.
#[test]
fn an_app_config_query_asks_the_daemon() {
    let mut app = app();
    let action = app.handle_set_command("set myplugin.debug?");
    assert!(
        matches!(action, Action::Send(_)),
        "`:set myplugin.debug?` answered locally: {action:?}"
    );
}

/// `:set key??` on an app-config key must ask the daemon too. The daemon
/// records where each leaf came from; this client records nothing.
#[test]
fn an_app_config_history_query_asks_the_daemon() {
    let mut app = app();
    let action = app.handle_set_command("set myplugin.debug??");
    assert!(
        matches!(action, Action::Send(_)),
        "`:set myplugin.debug??` answered locally: {action:?}"
    );
}

/// `:set key&` must not drop this client's copy of a value the daemon still
/// holds. Dropping it locally, and sending nothing, is two stores that
/// disagree about one key.
#[test]
fn an_app_config_reset_does_not_diverge_from_the_daemon() {
    let mut app = app();
    app.on_message(ChatAppMsg::ConfigSetResolved {
        key: "myplugin.retries".to_string(),
        value: serde_json::json!(9),
    });
    app.handle_set_command("set myplugin.retries&");
    assert_eq!(
        app.runtime_config
            .get("myplugin.retries")
            .and_then(|v| v.as_int()),
        Some(9),
        "`:set key&` cleared the local copy while the daemon store still holds 9"
    );
}

/// `:set key&` and `:set key^` on an app-config key ask the daemon, which is
/// the only process that keeps the layers. The two spellings are different
/// verbs there — `config.reset` drops the ephemeral layer, `config.pop` drops
/// the highest one — so the message has to carry which.
#[test]
fn an_app_config_reset_and_pop_ask_the_daemon_for_the_right_verb() {
    let mut app = app();
    assert!(
        matches!(
            app.handle_set_command("set myplugin.retries&"),
            Action::Send(ChatAppMsg::ConfigDrop { kind: DropKind::Reset, ref key })
                if key == "myplugin.retries"
        ),
        "`:set key&` must send config.reset"
    );
    assert!(
        matches!(
            app.handle_set_command("set myplugin.retries^"),
            Action::Send(ChatAppMsg::ConfigDrop { kind: DropKind::Pop, ref key })
                if key == "myplugin.retries"
        ),
        "`:set key^` must send config.pop"
    );
}

/// `:set key=` — an assignment with nothing after it — removes the key.
///
/// Vim has no such spelling because its option set is fixed and nothing can
/// be removed from it. Crucible's is not: `llm.providers.<name>` is
/// user-named, so a stale one has to go somewhere, and an empty assignment is
/// where a Vim user would reach. It must send `config.unset`, NOT
/// `config.reset` — a reset would put back whatever a file declares, which is
/// the opposite of removing the key.
#[test]
fn an_empty_assignment_asks_the_daemon_to_unset() {
    let mut app = app();
    assert!(
        matches!(
            app.handle_set_command("set llm.providers.stale="),
            Action::Send(ChatAppMsg::ConfigDrop { kind: DropKind::Unset, ref key })
                if key == "llm.providers.stale"
        ),
        "`:set key=` must send config.unset"
    );
}

/// The three drop spellings are three verbs, and each names its own.
///
/// Derived from the enum rather than written out, so a fourth spelling cannot
/// be added without deciding which verb it maps to.
#[test]
fn every_drop_spelling_names_a_distinct_verb() {
    use strum::IntoEnumIterator;
    let verbs: Vec<&str> = DropKind::iter().map(DropKind::method).collect();
    let mut unique = verbs.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(
        verbs.len(),
        unique.len(),
        "two DropKind variants share one RPC verb: {verbs:?}"
    );
}

/// The daemon's answer for a `&` or a `^` is printed with the layers it
/// dropped and the origin the leaf now has, and records nothing locally.
///
/// The origin is the point of the print: a pop that did not say which layer
/// now holds the key would leave the user guessing how many more to press.
#[test]
fn an_app_config_drop_answer_names_the_layer_it_revealed() {
    let mut app = app();
    app.on_message(ChatAppMsg::ConfigDropResolved {
        key: "chat.model".to_string(),
        dropped: vec!["lua".to_string()],
        value: serde_json::json!("from-settings"),
        origin: serde_json::json!({ "source": "settings" }),
    });

    assert!(
        app.runtime_config.get("chat.model").is_none(),
        "a drop answer must not write this client's store"
    );
    let tree = crate::tui::oil::tests::helpers::view_with_default_ctx(&app);
    let output = crucible_oil::ansi::strip_ansi(&crucible_oil::render_to_string(&tree, 80));
    assert!(
        output.contains("Dropped lua"),
        "the answer must name the layer that went: {output}"
    );
    assert!(
        output.contains("chat.model=from-settings"),
        "and the value that showed: {output}"
    );
    assert!(
        output.contains("from settings"),
        "and where that value comes from: {output}"
    );
}

/// A key with no layer left to drop says so, rather than printing a drop
/// that did not happen.
#[test]
fn an_app_config_drop_that_dropped_nothing_says_so() {
    let mut app = app();
    app.on_message(ChatAppMsg::ConfigDropResolved {
        key: "chat.model".to_string(),
        dropped: Vec::new(),
        value: serde_json::Value::Null,
        origin: serde_json::json!({ "source": "default" }),
    });
    let tree = crate::tui::oil::tests::helpers::view_with_default_ctx(&app);
    let output = crucible_oil::ansi::strip_ansi(&crucible_oil::render_to_string(&tree, 80));
    assert!(
        output.contains("no layer left to drop"),
        "an untouched key must not read as a drop: {output}"
    );
}

/// No `:set` spelling of an app-config key answers from — or writes — this
/// client's own store. The spellings walk [`SetCommand`] through `EnumIter`,
/// so a spelling added later cannot pass this test by being absent from it.
#[test]
fn every_set_spelling_of_an_app_config_key_leaves_the_local_store_alone() {
    use strum::IntoEnumIterator;

    const KEY: &str = "myplugin.retries";

    for command in SetCommand::iter() {
        let Some(spelling) = spelling_for(&command, KEY) else {
            continue;
        };
        let parsed = SetCommand::parse(&spelling).expect("the spelling parses");
        assert_eq!(
            std::mem::discriminant(&parsed),
            std::mem::discriminant(&command),
            "`:set {spelling}` does not spell {command:?}"
        );

        let mut app = app();
        // What the daemon answered for this key before the spelling runs.
        app.on_message(ChatAppMsg::ConfigSetResolved {
            key: KEY.to_string(),
            value: serde_json::json!(9),
        });
        let action = app.handle_set_command(&format!("set {spelling}"));

        assert_eq!(
            app.runtime_config.get(KEY).and_then(|v| v.as_int()),
            Some(9),
            "`:set {spelling}` changed this client's copy of a key the daemon owns"
        );
        assert!(
            matches!(action, Action::Send(_)) || app.has_notifications(),
            "`:set {spelling}` answered from the local store: {action:?}"
        );
    }
}

/// Every declared `:set` target, under every spelling, is answered by this
/// client. A spelling that fell through to the daemon app-config store would
/// give one declared key two homes.
///
/// The targets walk `SHORTCUTS` and the spellings walk [`SetCommand`], so
/// neither a new target nor a new spelling can pass by being absent here.
#[test]
fn every_declared_target_answers_locally_under_every_spelling() {
    use crate::tui::oil::config::SHORTCUTS;
    use strum::IntoEnumIterator;

    for shortcut in SHORTCUTS {
        for command in SetCommand::iter() {
            let Some(spelling) = spelling_for(&command, shortcut.short) else {
                continue;
            };
            let mut app = app();
            let action = app.handle_set_command(&format!("set {spelling}"));
            assert!(
                !matches!(
                    action,
                    Action::Send(ChatAppMsg::ConfigSet { .. })
                        | Action::Send(ChatAppMsg::ConfigQuery { .. })
                        | Action::Send(ChatAppMsg::ConfigDrop { .. })
                ),
                "`:set {spelling}` sent a declared target to the daemon app-config store"
            );
        }
    }
}

/// `:set key?` on an app-config key prints the daemon's answer and records
/// nothing. A recorded answer is the second copy the write path stopped
/// making.
#[test]
fn an_app_config_query_answer_is_printed_and_not_recorded() {
    let mut app = app();
    app.on_message(ChatAppMsg::ConfigQueryResolved {
        key: "myplugin.retries".to_string(),
        value: serde_json::json!(7),
        origin: None,
    });
    assert!(
        app.runtime_config.get("myplugin.retries").is_none(),
        "a read must not write this client's store"
    );

    let tree = crate::tui::oil::tests::helpers::view_with_default_ctx(&app);
    let output = crucible_oil::ansi::strip_ansi(&crucible_oil::render_to_string(&tree, 80));
    assert!(
        output.contains("myplugin.retries=7"),
        "the daemon's answer must reach the transcript: {output}"
    );
}
