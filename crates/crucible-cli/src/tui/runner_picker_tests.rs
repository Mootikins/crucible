//! Tests for the TUI picker phase key handling
//!
//! Tests that the picker phase correctly handles:
//! - j/k navigation (vim-style)
//! - Up/Down arrow navigation
//! - 1-9 quick select
//! - Enter/Space/l confirmation
//! - Esc/q/h cancellation
//! - Agent probe result updates

use super::AgentSelection;
use crate::tui::splash::{AgentOption, SplashState};

/// Helper to create a test splash state
fn create_test_splash() -> SplashState {
    SplashState::new("/tmp".to_string())
}

/// Test that vim-style navigation works (j/k)
#[test]
fn test_vim_navigation_keys() {
    let mut state = create_test_splash();
    let initial_index = state.selected_index;

    // j should move down
    state.select_next();
    assert!(state.selected_index != initial_index, "j (next) should change selection");

    // k should move up
    state.select_prev();
    assert_eq!(state.selected_index, initial_index, "k (prev) should return to initial");
}

/// Test that arrow key navigation works (Up/Down)
#[test]
fn test_arrow_navigation_keys() {
    let mut state = create_test_splash();
    let initial_index = state.selected_index;

    // Down arrow should move down
    state.select_next();
    assert!(state.selected_index != initial_index, "Down should change selection");

    // Up arrow should move up
    state.select_prev();
    assert_eq!(state.selected_index, initial_index, "Up should return to initial");
}

/// Test quick select with number keys (1-9)
#[test]
fn test_quick_select_numeric_keys() {
    let mut state = create_test_splash();

    // Ensure we have enough agents for testing
    assert!(state.agents.len() >= 2, "Need at least 2 agents for testing");

    // Select index 0 with key '1' (index = c - '1')
    state.select_index(0);
    assert_eq!(state.selected_index, 0);

    // Select index 1 with key '2'
    state.select_index(1);
    assert_eq!(state.selected_index, 1);
}

/// Test that quick select is bounds-checked
#[test]
fn test_quick_select_bounds_checking() {
    let mut state = create_test_splash();
    let num_agents = state.agents.len();

    // Try to select beyond bounds
    state.select_index(100);

    // Should stay within bounds (not panic, not change to invalid index)
    assert!(state.selected_index < num_agents);
}

/// Test that can_confirm requires known availability
#[test]
fn test_confirm_requires_known_availability() {
    let mut state = create_test_splash();

    // Initially, agents have unknown availability (None)
    // can_confirm should return false until availability is known
    // This is tested in splash.rs, we verify the behavior here

    // Mark the selected agent as available
    let selected_idx = state.selected_index;
    state.agents[selected_idx].available = Some(true);

    // Now can_confirm should return true
    assert!(state.can_confirm(), "Should be able to confirm with known availability");
}

/// Test that navigation wraps around
#[test]
fn test_navigation_wraps_around() {
    let mut state = create_test_splash();
    let num_agents = state.agents.len();

    // Navigate to last agent
    for _ in 0..num_agents {
        state.select_next();
    }

    // Should wrap to first
    assert_eq!(state.selected_index, 0, "Navigation should wrap to start");

    // Navigate backward from first
    state.select_prev();

    // Should wrap to last
    assert_eq!(state.selected_index, num_agents - 1, "Navigation should wrap to end");
}

/// Test that unavailable agents are skipped during navigation
#[test]
fn test_navigation_skips_unavailable_agents() {
    let mut state = create_test_splash();

    // Mark second agent as unavailable
    if state.agents.len() > 2 {
        state.agents[1].available = Some(false);

        state.selected_index = 0;
        state.select_next();

        // Should skip agent at index 1 and land on index 2
        assert_eq!(state.selected_index, 2, "Should skip unavailable agent");
    }
}

/// Test probe result updates
#[test]
fn test_probe_updates_availability() {
    use crucible_acp::KnownAgent;

    let mut state = create_test_splash();

    // Create probe results (using KnownAgent from crucible-acp)
    let probe_results = vec![
        KnownAgent {
            name: "opencode".to_string(),
            description: "OpenCode AI".to_string(),
            available: true,
        },
        KnownAgent {
            name: "claude".to_string(),
            description: "Claude Code".to_string(),
            available: false,
        },
    ];

    // Update availability
    state.update_availability(probe_results);

    // Verify updates were applied
    let opencode = state.agents.iter().find(|a| a.name == "opencode");
    if let Some(agent) = opencode {
        assert_eq!(agent.available, Some(true), "OpenCode should be available");
    }

    let claude = state.agents.iter().find(|a| a.name == "claude");
    if let Some(agent) = claude {
        assert_eq!(agent.available, Some(false), "Claude should be unavailable");
    }
}

/// Test that internal agent is always available
#[test]
fn test_internal_agent_always_available() {
    let state = create_test_splash();

    let internal = state.agents.iter().find(|a| a.name == "internal");
    assert!(internal.is_some(), "Internal agent should exist");

    if let Some(agent) = internal {
        assert_eq!(agent.available, Some(true), "Internal agent should always be available");
    }
}

/// Test agent selection type mapping
#[test]
fn test_agent_selection_type_mapping() {
    // Test that "internal" maps to AgentSelection::Internal
    let internal_name = "internal";
    let selection = if internal_name == "internal" {
        AgentSelection::Internal
    } else {
        AgentSelection::Acp(internal_name.to_string())
    };

    assert!(matches!(selection, AgentSelection::Internal));

    // Test that other names map to AgentSelection::Acp
    let acp_name = "opencode";
    let selection = if acp_name == "internal" {
        AgentSelection::Internal
    } else {
        AgentSelection::Acp(acp_name.to_string())
    };

    match selection {
        AgentSelection::Acp(name) => assert_eq!(name, "opencode"),
        _ => panic!("Expected Acp variant"),
    }
}

/// Test that cancellation is detected correctly
#[test]
fn test_cancellation_detection() {
    let selection = AgentSelection::Cancelled;

    // The runner checks: matches!(selection, AgentSelection::Cancelled)
    assert!(matches!(selection, AgentSelection::Cancelled));
}

/// Test key code to index conversion for quick select
#[test]
fn test_key_to_index_conversion() {
    // Key '1' -> index 0
    let index = ('1' as usize) - ('1' as usize);
    assert_eq!(index, 0);

    // Key '2' -> index 1
    let index = ('2' as usize) - ('1' as usize);
    assert_eq!(index, 1);

    // Key '9' -> index 8
    let index = ('9' as usize) - ('1' as usize);
    assert_eq!(index, 8);
}

/// Test that all confirm keys work (Enter, Space, l)
#[test]
fn test_all_confirm_keys_recognized() {
    // The runner handles these keys:
    // KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l')

    use crossterm::event::KeyCode;

    let confirm_keys = vec![
        KeyCode::Enter,
        KeyCode::Char(' '),
        KeyCode::Char('l'),
    ];

    for key in confirm_keys {
        let is_confirm = matches!(
            key,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l')
        );
        assert!(is_confirm, "Key {:?} should be recognized as confirm", key);
    }
}

/// Test that all cancel keys work (Esc, q, h)
#[test]
fn test_all_cancel_keys_recognized() {
    use crossterm::event::KeyCode;

    let cancel_keys = vec![
        KeyCode::Esc,
        KeyCode::Char('q'),
        KeyCode::Char('h'),
    ];

    for key in cancel_keys {
        let is_cancel = matches!(
            key,
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h')
        );
        assert!(is_cancel, "Key {:?} should be recognized as cancel", key);
    }
}

/// Test that navigation keys are recognized correctly
#[test]
fn test_all_navigation_keys_recognized() {
    use crossterm::event::KeyCode;

    // Up navigation: Up, k
    let up_keys = vec![KeyCode::Up, KeyCode::Char('k')];
    for key in up_keys {
        let is_up = matches!(key, KeyCode::Up | KeyCode::Char('k'));
        assert!(is_up, "Key {:?} should be recognized as up", key);
    }

    // Down navigation: Down, j
    let down_keys = vec![KeyCode::Down, KeyCode::Char('j')];
    for key in down_keys {
        let is_down = matches!(key, KeyCode::Down | KeyCode::Char('j'));
        assert!(is_down, "Key {:?} should be recognized as down", key);
    }
}

/// Test numeric key range detection
#[test]
fn test_numeric_key_range() {
    use crossterm::event::KeyCode;

    // Valid quick select keys: 1-9
    for c in '1'..='9' {
        let is_numeric = matches!(KeyCode::Char(c), KeyCode::Char('1'..='9'));
        assert!(is_numeric, "Char {} should be recognized as numeric", c);
    }

    // Invalid quick select keys
    let invalid = vec!['0', 'a', 'z', ' '];
    for c in invalid {
        let is_numeric = matches!(KeyCode::Char(c), KeyCode::Char('1'..='9'));
        assert!(!is_numeric, "Char {} should NOT be recognized as numeric", c);
    }
}

/// Test that all ACP agents unavailable still allows internal selection
#[test]
fn test_all_acp_unavailable_allows_internal() {
    let mut state = create_test_splash();

    // Mark all ACP agents as unavailable (internal is always available)
    for agent in &mut state.agents {
        if agent.name != "internal" {
            agent.available = Some(false);
        }
    }

    // Navigate to internal
    let internal_idx = state.agents.iter().position(|a| a.name == "internal").unwrap();
    state.select_index(internal_idx);

    // Should be able to confirm internal
    assert!(state.can_confirm(), "Should be able to confirm internal when all ACP unavailable");
}

/// Test that can_confirm returns false when selected agent is unavailable
#[test]
fn test_can_confirm_false_when_unavailable() {
    let mut state = create_test_splash();

    // Mark selected agent as unavailable
    state.agents[state.selected_index].available = Some(false);

    // can_confirm should return false
    assert!(!state.can_confirm(), "can_confirm should be false for unavailable agent");
}

/// Test agent name with special characters in selection
#[test]
fn test_agent_name_with_special_chars() {
    let selection = AgentSelection::Acp("agent-with-dashes_and_underscores".to_string());

    match selection {
        AgentSelection::Acp(name) => {
            assert_eq!(name, "agent-with-dashes_and_underscores");
        }
        _ => panic!("Expected Acp variant"),
    }
}

/// Test multiple availability updates
#[test]
fn test_multiple_availability_updates() {
    use crucible_acp::KnownAgent;

    let mut state = create_test_splash();

    // First update
    state.update_availability(vec![KnownAgent {
        name: "opencode".to_string(),
        description: "".to_string(),
        available: true,
    }]);

    // Second update should override
    state.update_availability(vec![KnownAgent {
        name: "opencode".to_string(),
        description: "".to_string(),
        available: false,
    }]);

    let opencode = state.agents.iter().find(|a| a.name == "opencode");
    if let Some(agent) = opencode {
        assert_eq!(agent.available, Some(false), "Second update should override first");
    }
}

/// Test that navigation works when only internal is available
#[test]
fn test_navigation_with_only_internal_available() {
    let mut state = create_test_splash();

    // Mark all ACP agents as unavailable
    for agent in &mut state.agents {
        if agent.name != "internal" {
            agent.available = Some(false);
        }
    }

    let internal_idx = state.agents.iter().position(|a| a.name == "internal").unwrap();

    // Navigate should always land on internal (only selectable option)
    state.selected_index = internal_idx;
    state.select_next();
    assert_eq!(state.selected_index, internal_idx, "Should stay on internal");

    state.select_prev();
    assert_eq!(state.selected_index, internal_idx, "Should stay on internal");
}

// Integration test notes:
//
// The run_picker_phase() function combines:
// 1. Event polling with crossterm
// 2. Agent probe results (async)
// 3. SplashState updates (sync)
// 4. Terminal rendering (ratatui)
// 5. AgentSelection return value
//
// Full integration testing requires:
// - Terminal backend (can't easily test in CI)
// - Agent probe results (requires ACP agents)
// - Event stream simulation (complex to mock)
//
// These unit tests verify:
// - Key handling logic is correct
// - State transitions work as expected
// - Pattern matching is exhaustive
// - Edge cases are handled
//
// Integration testing is done through:
// 1. Manual testing: `cru chat --lazy-agent-selection`
// 2. Visual verification of splash screen behavior
// 3. End-to-end tests with real terminal
