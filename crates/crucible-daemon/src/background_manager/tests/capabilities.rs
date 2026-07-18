use super::*;

#[test]
fn subagent_context_default_delegation_depth_is_zero() {
    let ctx = SubagentContext {
        agent: test_session_agent(None),
        available_agents: HashMap::new(),
        workspace: std::env::temp_dir(),
        parent_session_id: Some("session-1".to_string()),
        parent_session_dir: None,
        delegator_agent_name: None,
        target_agent_name: None,
        delegation_depth: 0,
    };

    assert_eq!(ctx.delegation_depth, 0);
}

#[test]
fn enforce_delegation_capabilities_rejects_depth_above_max_depth() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 1,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    // A child at depth 2 exceeds the configured max_depth = 1.
    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        2,
        "session-1",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delegation depth limit exceeded"));
}

#[test]
fn enforce_delegation_capabilities_max_depth_zero_disables_delegation() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 0,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    // Even a first-level child (depth 1) exceeds max_depth = 0.
    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        1,
        "session-1",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delegation depth limit exceeded"));
}

#[test]
fn enforce_delegation_capabilities_allows_first_level_at_default_max_depth() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 1,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    // Default max_depth = 1 must still allow a first-level delegation (depth 1).
    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        1,
        "session-1",
    );

    assert!(result.is_ok(), "first-level delegation must be allowed at max_depth=1");
}

#[test]
fn enforce_delegation_capabilities_allows_depth_below_hard_cap() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        0,
        "session-1",
    );

    assert!(result.is_ok());
}

#[test]
fn enforce_delegation_capabilities_allows_depth_one() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        1,
        "session-1",
    );

    assert!(result.is_ok());
}

#[test]
fn enforce_delegation_capabilities_allows_depth_two() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: true,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        2,
        "session-1",
    );

    assert!(result.is_ok());
}

#[test]
fn enforce_delegation_capabilities_checks_enabled_before_depth() {
    // The depth cap is now read from the (enabled) delegation config, so the
    // enabled check must run first: a disabled agent gets the "disabled" error
    // regardless of depth.
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: false,
        max_depth: 0,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        5,
        "session-1",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delegation is disabled"));
}
