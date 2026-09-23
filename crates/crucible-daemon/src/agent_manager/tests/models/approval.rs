use crate::agent_manager::tests::create_test_agent_manager;
use crate::test_support::temp_session_manager;
use crucible_core::session::{PluginApproval, SessionType};

#[tokio::test]
async fn plugin_approval_can_be_read_and_changed_after_session_eviction() {
    let sessions = temp_session_manager();
    let session = sessions
        .create_session(SessionType::Chat, vec![], None, None)
        .await
        .unwrap();
    let id = session.id.to_string();
    let agents = create_test_agent_manager(sessions.clone());
    agents
        .set_plugin_approval(&id, "alpha", PluginApproval::Ask, None)
        .await
        .unwrap();
    sessions.end_session(&id).await.unwrap();
    sessions.remove_session(&id).unwrap();

    assert_eq!(
        agents.get_plugin_approval(&id, "alpha").await.unwrap(),
        PluginApproval::Ask
    );
    assert_eq!(agents.list_plugin_approvals(&id).await.unwrap().len(), 1);
    agents
        .set_plugin_approval(&id, "alpha", PluginApproval::Stop, None)
        .await
        .unwrap();
    assert_eq!(
        agents.get_plugin_approval(&id, "alpha").await.unwrap(),
        PluginApproval::Stop
    );
}
