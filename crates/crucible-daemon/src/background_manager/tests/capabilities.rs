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
fn enforce_delegation_capabilities_rejects_depth_at_hard_cap() {
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
        3,
        "session-1",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delegation depth limit exceeded"));
}

#[test]
fn enforce_delegation_capabilities_rejects_depth_above_hard_cap() {
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
        5,
        "session-1",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delegation depth limit exceeded"));
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
fn enforce_delegation_capabilities_hard_cap_checked_before_enabled_check() {
    let manager = create_manager();
    let agent = test_session_agent(Some(DelegationConfig {
        enabled: false,
        max_depth: 10,
        allowed_targets: None,
        result_max_bytes: 51200,
        max_concurrent_delegations: 3,
    }));

    let result = manager.enforce_delegation_capabilities(
        &agent,
        Some("parent"),
        Some("child"),
        3,
        "session-1",
    );

    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Delegation depth limit exceeded"));
}
