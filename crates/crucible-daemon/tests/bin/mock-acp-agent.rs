//! The mock ACP agent as a process, for tests that spawn an agent.
//!
//! It serves the script in `CRU_MOCK_SCRIPT` (see `MockScript`) on stdio.

#[path = "../acp_support/mock_agent.rs"]
mod mock_agent;

#[tokio::main]
async fn main() {
    mock_agent::serve(
        mock_agent::MockScript::from_env(),
        agent_client_protocol::Stdio::new(),
    )
    .await;
    // The stdin reader can hold the runtime open at shutdown.
    std::process::exit(0);
}
