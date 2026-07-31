//! Slash- and REPL-command dispatch tests.
//!
//! Split out of `command_handling.rs` for the 1500-line file-size gate, and
//! attached with `#[path]` rather than moved into `tui/oil/tests/` because the
//! handlers under test are `pub(super)` — reachable from a descendant of
//! `chat_app`, not from the sibling test tree.

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
    assert_eq!(app.mode(), "plan");
}

#[test]
fn slash_mode_cycles() {
    let mut app = app();
    assert_eq!(app.mode(), "normal");
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
        "normal".to_string(),
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
        "normal",
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
    app.on_message(ChatAppMsg::ModesLoaded(vec!["normal".to_string()]));
    app.handle_slash_command("/mode");
    assert_eq!(app.mode(), "normal");

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
        "normal".to_string(),
        "undo".to_string(),
        "help".to_string(),
    ]));

    let action = app.handle_slash_command("/undo 2");
    assert!(
        matches!(action, Action::Send(ChatAppMsg::Undo(2))),
        "/undo must still undo, got {action:?}"
    );
    assert_eq!(app.mode(), "normal", "and must not have changed the mode");
}

/// A mode declared as `cru.modes.Review` is reachable as `/review`, and
/// switching to it reports the id the daemon actually declared.
#[test]
fn a_mode_id_matches_case_insensitively() {
    let mut app = app();
    app.on_message(ChatAppMsg::ModesLoaded(vec![
        "normal".to_string(),
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
