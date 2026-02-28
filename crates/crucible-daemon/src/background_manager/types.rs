use super::*;
use thiserror::Error;

pub(super) mod events {
    pub const BASH_SPAWNED: &str = "bash_job_spawned";
    pub const BASH_COMPLETED: &str = "bash_job_completed";
    pub const BASH_FAILED: &str = "bash_job_failed";
    pub const SUBAGENT_SPAWNED: &str = "subagent_spawned";
    pub const SUBAGENT_COMPLETED: &str = "subagent_completed";
    pub const SUBAGENT_FAILED: &str = "subagent_failed";
    pub const BACKGROUND_COMPLETED: &str = "background_job_completed";
    pub const DELEGATION_SPAWNED: &str = "delegation_spawned";
    pub const DELEGATION_COMPLETED: &str = "delegation_completed";
    pub const DELEGATION_FAILED: &str = "delegation_failed";
}

#[derive(Error, Debug)]
pub enum BackgroundError {
    #[error("Job error: {0}")]
    Job(#[from] JobError),

    #[error("Failed to spawn job: {0}")]
    SpawnFailed(String),

    #[error("No subagent factory configured")]
    NoSubagentFactory,
}

pub type SubagentFactory = Box<
    dyn Fn(
            &SessionAgent,
            &Path,
        ) -> Pin<
            Box<dyn Future<Output = Result<Box<dyn AgentHandle + Send + Sync>, String>> + Send>,
        > + Send
        + Sync,
>;

pub(super) struct RunningJob {
    pub(crate) info: JobInfo,
    pub(crate) is_delegation: bool,
    pub(crate) parent_session_id: Option<String>,
    pub(crate) cancel_tx: oneshot::Sender<()>,
    #[allow(dead_code)]
    pub(crate) task_handle: JoinHandle<()>,
}

pub struct SubagentContext {
    pub agent: SessionAgent,
    pub available_agents: HashMap<String, AgentProfile>,
    pub workspace: PathBuf,
    pub parent_session_id: Option<String>,
    /// Parent session directory for creating subagent session files
    pub parent_session_dir: Option<PathBuf>,
    pub delegator_agent_name: Option<String>,
    pub target_agent_name: Option<String>,
    /// Delegation depth counter (0 = root, 1 = first delegation, etc.)
    pub delegation_depth: u32,
}

pub(super) struct PreparedSubagentExecution {
    pub(crate) info: JobInfo,
    pub(crate) prompt: String,
    pub(crate) context: Option<String>,
    pub(crate) session_id: String,
    pub(crate) session_link: String,
    pub(crate) agent: Box<dyn AgentHandle + Send + Sync>,
    pub(crate) subagent_writer: Option<Arc<Mutex<SessionWriter>>>,
    pub(crate) is_delegation: bool,
    pub(crate) parent_session_id: Option<String>,
}

pub(super) struct SubagentExecutionOptions {
    pub(crate) max_turns: usize,
    pub(crate) max_output_bytes: usize,
    pub(crate) timeout: Option<Duration>,
}

pub(super) fn parse_target_agent_name(context: Option<&str>) -> Option<String> {
    const TARGET_PREFIX: &str = "Target agent: ";
    context.and_then(|ctx| {
        ctx.lines().find_map(|line| {
            line.strip_prefix(TARGET_PREFIX)
                .map(str::trim)
                .filter(|target| !target.is_empty())
                .map(ToString::to_string)
        })
    })
}

pub(super) fn target_profile_to_session_agent(
    target_name: &str,
    available_agents: &HashMap<String, AgentProfile>,
) -> Result<SessionAgent, BackgroundError> {
    let profile = available_agents.get(target_name).ok_or_else(|| {
        let mut available: Vec<_> = available_agents.keys().cloned().collect();
        available.sort();
        let available_list = if available.is_empty() {
            "(none)".to_string()
        } else {
            available.join(", ")
        };
        BackgroundError::SpawnFailed(format!(
            "Delegation target '{target_name}' not found. Available agents: {available_list}"
        ))
    })?;

    Ok(SessionAgent::from_profile(profile, target_name))
}

pub(super) enum SubagentError {
    Cancelled,
    Failed(String),
    Timeout,
}

pub(super) enum BashError {
    Cancelled,
    Timeout,
    Failed {
        message: String,
        exit_code: Option<i32>,
    },
}
