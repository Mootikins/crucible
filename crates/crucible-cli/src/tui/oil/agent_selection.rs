#[derive(Debug, Clone)]
pub enum AgentSelection {
    Acp(String),
    Internal,
    Cancelled,
}
