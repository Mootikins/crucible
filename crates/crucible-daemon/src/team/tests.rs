//! Integration tests for the three team patterns.
//!
//! Uses a fake subagent factory that inspects the agent profile name and
//! returns deterministic output per agent. Tests verify orchestration
//! semantics (ordering, parallelism, classifier dispatch, supervisor
//! termination), not the underlying subagent execution loop.

use super::*;
use crate::background_manager::SubagentFactory;
use async_trait::async_trait;
use crucible_core::background::JobResult;
use crucible_core::config::{AgentProfile, BackendType};
use crucible_core::session::{OutputValidation, SessionAgent};
use crucible_core::traits::chat::{AgentHandle, ChatResult};
use crucible_core::turn::{StopReason, TurnEvent};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// Mock subagent whose `turn` emits a single canned `TextDelta` then `Done`.
///
/// `response` is set by the factory based on the agent's `model` field
/// (which is the team-member name when the subagent comes from a profile).
struct CannedAgent {
    response: String,
    delay: Option<Duration>,
}

#[async_trait]
impl crucible_core::turn::Agent for CannedAgent {
    fn capabilities(&self) -> crucible_core::turn::AgentCapabilities {
        crucible_core::turn::AgentCapabilities::default()
    }
    async fn turn<'a>(
        &'a mut self,
        _ctx: crucible_core::turn::TurnContext,
    ) -> Result<
        futures::stream::BoxStream<'a, crucible_core::turn::TurnEvent>,
        crucible_core::turn::AgentError,
    > {
        let response = self.response.clone();
        let delay = self.delay;
        Ok(Box::pin(async_stream::stream! {
            if let Some(d) = delay {
                tokio::time::sleep(d).await;
            }
            yield TurnEvent::TextDelta(response);
            yield TurnEvent::Done { stop_reason: StopReason::EndTurn };
        }))
    }
    async fn cancel(&self) -> Result<(), crucible_core::turn::AgentError> {
        Ok(())
    }
    async fn switch_model(&mut self, _: &str) -> Result<(), crucible_core::turn::NotSupported> {
        Err(crucible_core::turn::NotSupported::new("switch_model"))
    }
}

#[async_trait]
impl AgentHandle for CannedAgent {
    async fn send_message_fire_and_forget(&mut self, _: String) -> ChatResult<()> {
        Ok(())
    }
    async fn set_mode_str(&mut self, _: &str) -> ChatResult<()> {
        Ok(())
    }
}

/// Factory that maps team-member name -> canned response + optional delay.
///
/// The factory inspects `agent_config.model` to figure out which team
/// member it's instantiating (because `target_profile_to_session_agent`
/// stores the profile key in `model`).
fn canned_factory(responses: HashMap<String, (String, Option<Duration>)>) -> SubagentFactory {
    let responses = Arc::new(responses);
    Box::new(move |agent_config, _workspace| {
        let responses = Arc::clone(&responses);
        let model = agent_config.model.clone();
        Box::pin(async move {
            let (response, delay) = responses
                .get(&model)
                .cloned()
                .unwrap_or_else(|| (format!("[no canned response for {model}]"), None));
            Ok(Box::new(CannedAgent { response, delay }) as Box<dyn AgentHandle + Send + Sync>)
        })
    })
}

fn parent_agent() -> SessionAgent {
    SessionAgent {
        agent_type: "internal".to_string(),
        agent_name: Some("team-parent".to_string()),
        provider_key: None,
        provider: BackendType::Custom,
        model: "team-parent".to_string(),
        system_prompt: String::new(),
        temperature: None,
        max_tokens: None,
        max_context_tokens: None,
        thinking_budget: None,
        endpoint: None,
        env_overrides: HashMap::new(),
        mcp_servers: vec![],
        agent_card_name: None,
        capabilities: None,
        agent_description: None,
        delegation_config: None,
        precognition_enabled: false,
        precognition_results: 0,
        max_iterations: None,
        execution_timeout_secs: None,
        context_budget: None,
        context_strategy: Default::default(),
        context_window: None,
        output_validation: OutputValidation::default(),
        validation_retries: 3,
        autocompact_threshold: None,
        grammar: None,
    }
}

fn profile() -> AgentProfile {
    AgentProfile {
        extends: None,
        command: Some("/bin/true".to_string()),
        args: Some(vec![]),
        env: HashMap::new(),
        description: Some("team member".to_string()),
        capabilities: None,
        delegation: None,
        permissions: None,
    }
}

fn make_team_ctx(
    responses: HashMap<String, (String, Option<Duration>)>,
    members: &[&str],
) -> TeamCtx {
    let (tx, _) = broadcast::channel(64);
    let manager =
        Arc::new(BackgroundJobManager::new(tx).with_subagent_factory(canned_factory(responses)));
    let available_agents: HashMap<String, AgentProfile> =
        members.iter().map(|n| (n.to_string(), profile())).collect();
    TeamCtx {
        manager,
        session_id: "team-session".to_string(),
        parent_agent: parent_agent(),
        available_agents,
        workspace: std::env::temp_dir(),
    }
}

// ---- Supervisor tests ----

/// One call to the decider: the task it was given, plus the history at
/// that point (cumulative `(agent_name, output)` pairs).
type DeciderObservation = (String, Vec<(String, String)>);

/// Decider that returns a queued script of decisions. Test fixture only.
struct ScriptedDecider {
    decisions: std::sync::Mutex<std::collections::VecDeque<SupervisorDecision>>,
    seen: std::sync::Mutex<Vec<DeciderObservation>>,
}

impl ScriptedDecider {
    fn new(decisions: Vec<SupervisorDecision>) -> Self {
        Self {
            decisions: std::sync::Mutex::new(decisions.into()),
            seen: std::sync::Mutex::new(Vec::new()),
        }
    }
    fn observations(&self) -> Vec<DeciderObservation> {
        self.seen.lock().unwrap().clone()
    }
}

#[async_trait]
impl SupervisorDecider for ScriptedDecider {
    async fn decide(
        &self,
        task: &str,
        history: &[(String, String)],
    ) -> Result<SupervisorDecision, String> {
        self.seen
            .lock()
            .unwrap()
            .push((task.to_string(), history.to_vec()));
        Ok(self
            .decisions
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(SupervisorDecision::Done))
    }
}

#[tokio::test]
async fn supervisor_runs_agents_in_order_decided_by_supervisor() {
    let mut responses = HashMap::new();
    responses.insert("a".to_string(), ("a-output".to_string(), None));
    responses.insert("b".to_string(), ("b-output".to_string(), None));
    responses.insert("c".to_string(), ("c-output".to_string(), None));

    let ctx = make_team_ctx(responses, &["a", "b", "c"]);
    let decider = Arc::new(ScriptedDecider::new(vec![
        SupervisorDecision::Run("c".to_string(), "first".to_string()),
        SupervisorDecision::Run("a".to_string(), "second".to_string()),
        SupervisorDecision::Run("b".to_string(), "third".to_string()),
        SupervisorDecision::Done,
    ]));

    let supervisor = Supervisor::new(
        ctx,
        vec!["a".to_string(), "b".to_string(), "c".to_string()],
        Arc::clone(&decider) as Arc<dyn SupervisorDecider>,
    );

    let result = supervisor.run("the task").await.expect("supervisor ok");
    // Final output is the last agent's output.
    assert_eq!(result, "b-output");

    // The decider was called 4 times (3 runs + 1 final Done).
    let obs = decider.observations();
    assert_eq!(obs.len(), 4);

    // Each observation has the cumulative history of (agent, output).
    assert_eq!(obs[0].1, vec![]);
    assert_eq!(obs[1].1, vec![("c".to_string(), "c-output".to_string())]);
    assert_eq!(
        obs[2].1,
        vec![
            ("c".to_string(), "c-output".to_string()),
            ("a".to_string(), "a-output".to_string()),
        ]
    );
    assert_eq!(
        obs[3].1,
        vec![
            ("c".to_string(), "c-output".to_string()),
            ("a".to_string(), "a-output".to_string()),
            ("b".to_string(), "b-output".to_string()),
        ]
    );
}

#[tokio::test]
async fn supervisor_terminates_when_supervisor_says_done() {
    let mut responses = HashMap::new();
    responses.insert("a".to_string(), ("a-output".to_string(), None));
    let ctx = make_team_ctx(responses, &["a"]);
    let decider = Arc::new(ScriptedDecider::new(vec![SupervisorDecision::Done]));

    let supervisor = Supervisor::new(
        ctx,
        vec!["a".to_string()],
        Arc::clone(&decider) as Arc<dyn SupervisorDecider>,
    );
    let result = supervisor.run("noop").await.expect("supervisor ok");
    // No agents ran, so we return the empty string per documented contract.
    assert_eq!(result, "");
    // Only one decision was requested.
    assert_eq!(decider.observations().len(), 1);
}

#[tokio::test]
async fn supervisor_rejects_unknown_agent_from_decider() {
    let responses = HashMap::new();
    let ctx = make_team_ctx(responses, &["a"]);
    let decider = Arc::new(ScriptedDecider::new(vec![SupervisorDecision::Run(
        "ghost".to_string(),
        "go".to_string(),
    )]));
    let supervisor = Supervisor::new(
        ctx,
        vec!["a".to_string()],
        Arc::clone(&decider) as Arc<dyn SupervisorDecider>,
    );
    let err = supervisor.run("task").await.expect_err("should fail");
    assert!(matches!(err, SupervisorError::UnknownAgent(ref n) if n == "ghost"));
}

// ---- Router tests ----

struct FixedClassifier(String);

#[async_trait]
impl RouterClassifier for FixedClassifier {
    async fn classify(&self, _input: &str) -> Result<String, String> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
async fn router_dispatches_based_on_classifier() {
    let mut responses = HashMap::new();
    responses.insert(
        "research_agent".to_string(),
        ("researched!".to_string(), None),
    );
    responses.insert("write_agent".to_string(), ("written!".to_string(), None));
    let ctx = make_team_ctx(responses, &["research_agent", "write_agent"]);

    let mut routes = HashMap::new();
    routes.insert("researcher".to_string(), "research_agent".to_string());
    routes.insert("writer".to_string(), "write_agent".to_string());

    let router = Router::new(
        ctx.clone(),
        routes.clone(),
        Arc::new(FixedClassifier("researcher".to_string())),
    );
    let result = router.run("anything").await.expect("router ok");
    assert_eq!(result, "researched!");

    let router2 = Router::new(ctx, routes, Arc::new(FixedClassifier("writer".to_string())));
    let result2 = router2.run("anything").await.expect("router ok");
    assert_eq!(result2, "written!");
}

#[tokio::test]
async fn router_returns_classifier_error_when_route_missing() {
    let responses = HashMap::new();
    let ctx = make_team_ctx(responses, &["x"]);
    let routes = HashMap::new();
    let router = Router::new(ctx, routes, Arc::new(FixedClassifier("nope".to_string())));
    let err = router.run("anything").await.expect_err("should fail");
    assert!(matches!(err, RouterError::UnknownRoute(ref r) if r == "nope"));
}

#[tokio::test]
async fn router_propagates_classifier_failures() {
    struct FailingClassifier;
    #[async_trait]
    impl RouterClassifier for FailingClassifier {
        async fn classify(&self, _: &str) -> Result<String, String> {
            Err("classifier exploded".to_string())
        }
    }
    let responses = HashMap::new();
    let ctx = make_team_ctx(responses, &["x"]);
    let router = Router::new(ctx, HashMap::new(), Arc::new(FailingClassifier));
    let err = router.run("anything").await.expect_err("should fail");
    assert!(matches!(err, RouterError::Classifier(ref m) if m.contains("exploded")));
}

// ---- Broadcast tests ----

#[tokio::test]
async fn broadcast_runs_agents_in_parallel() {
    // Three agents each take 100ms. If they run in parallel, wall time
    // should be well under 200ms; sequential would be ~300ms.
    let delay = Duration::from_millis(100);
    let mut responses = HashMap::new();
    responses.insert("a".to_string(), ("ra".to_string(), Some(delay)));
    responses.insert("b".to_string(), ("rb".to_string(), Some(delay)));
    responses.insert("c".to_string(), ("rc".to_string(), Some(delay)));
    let ctx = make_team_ctx(responses, &["a", "b", "c"]);

    let bc = Broadcast::new(ctx, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    let start = Instant::now();
    let results = bc.run("ping").await.expect("broadcast ok");
    let elapsed = start.elapsed();

    assert_eq!(
        results,
        vec!["ra".to_string(), "rb".to_string(), "rc".to_string()]
    );
    assert!(
        elapsed < Duration::from_millis(250),
        "broadcast took {elapsed:?}, expected <250ms with parallel execution"
    );
}

#[tokio::test]
async fn broadcast_preserves_input_order_in_results() {
    // Give agents inverse delays so a slow agent's result would arrive
    // last. If we sorted by completion time we'd get [c, b, a]; we
    // should get [a, b, c] because we map over input order.
    let mut responses = HashMap::new();
    responses.insert(
        "a".to_string(),
        ("first".to_string(), Some(Duration::from_millis(120))),
    );
    responses.insert(
        "b".to_string(),
        ("second".to_string(), Some(Duration::from_millis(80))),
    );
    responses.insert(
        "c".to_string(),
        ("third".to_string(), Some(Duration::from_millis(40))),
    );
    let ctx = make_team_ctx(responses, &["a", "b", "c"]);

    let bc = Broadcast::new(ctx, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    let results = bc.run("ping").await.expect("broadcast ok");
    assert_eq!(
        results,
        vec![
            "first".to_string(),
            "second".to_string(),
            "third".to_string()
        ]
    );
}

#[tokio::test]
async fn broadcast_with_empty_agent_list_returns_empty_results() {
    let ctx = make_team_ctx(HashMap::new(), &[]);
    let bc = Broadcast::new(ctx, vec![]);
    let results = bc.run("ping").await.expect("broadcast ok");
    assert!(results.is_empty());
}

#[tokio::test]
async fn broadcast_collects_per_agent_failures() {
    // One agent has no canned response so the factory returns a placeholder
    // string — but the test asserts the structural shape: each input slot
    // gets *some* result, even if it's a failure marker.
    let mut responses = HashMap::new();
    responses.insert("a".to_string(), ("aaa".to_string(), None));
    let ctx = make_team_ctx(responses, &["a", "ghost"]);
    let bc = Broadcast::new(ctx, vec!["a".to_string(), "ghost".to_string()]);
    let results = bc.run("ping").await.expect("broadcast ok");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0], "aaa");
    // ghost has no profile entry — actually it does (we listed it) but no
    // canned response. The factory placeholder kicks in.
    assert!(results[1].contains("no canned response"));
}

// Sanity: ensure the underlying BackgroundJobManager actually returned
// success — guards against a regression where we silently swallow errors.
#[tokio::test]
async fn run_member_returns_underlying_output() {
    let mut responses = HashMap::new();
    responses.insert("solo".to_string(), ("HELLO".to_string(), None));
    let ctx = make_team_ctx(responses, &["solo"]);
    ctx.register_context(1).expect("register");
    let out = ctx
        .run_member("solo", "hi".to_string(), None)
        .await
        .expect("ok");
    assert_eq!(out, "HELLO");
}

// Type assertion: JobResult is what `spawn_subagent_blocking` returns and
// what we drop into `run_member`. If this signature changes we want a
// compile-time failure here.
#[allow(dead_code)]
fn _job_result_typecheck(_: JobResult) {}

// ---- End-to-end: Lua → DaemonTeamBridge → BackgroundJobManager ----

/// Builds a [`crate::team_bridge::DaemonTeamBridge`] over the same fake
/// factory/profile machinery the Rust-side team tests use. This is the
/// minimum needed to exercise `cru.team.*` Lua surface against real
/// orchestration logic.
fn make_bridge(
    responses: HashMap<String, (String, Option<Duration>)>,
    members: &[&str],
) -> crate::team_bridge::DaemonTeamBridge {
    let (tx, _) = broadcast::channel(64);
    let manager =
        Arc::new(BackgroundJobManager::new(tx).with_subagent_factory(canned_factory(responses)));
    let available_agents: HashMap<String, AgentProfile> =
        members.iter().map(|n| (n.to_string(), profile())).collect();
    crate::team_bridge::DaemonTeamBridge::new(
        manager,
        "team-session".to_string(),
        parent_agent(),
        available_agents,
        std::env::temp_dir(),
    )
}

#[tokio::test]
async fn lua_broadcast_returns_outputs_in_order() {
    let mut responses = HashMap::new();
    responses.insert("a".to_string(), ("ra".to_string(), None));
    responses.insert("b".to_string(), ("rb".to_string(), None));
    let bridge = make_bridge(responses, &["a", "b"]);
    let api: Arc<dyn crucible_lua::DaemonTeamApi> = Arc::new(bridge);

    let lua = mlua::Lua::new();
    crucible_lua::register_team_module(&lua, api).expect("register");

    let out: Vec<String> = lua
        .load(
            r#"
            local r, err = cru.team.broadcast({ "a", "b" }, "go")
            assert(err == nil, "err: " .. tostring(err))
            return r
            "#,
        )
        .eval_async()
        .await
        .unwrap();
    assert_eq!(out, vec!["ra".to_string(), "rb".to_string()]);
}

#[tokio::test]
async fn lua_router_dispatches_through_real_bridge() {
    let mut responses = HashMap::new();
    responses.insert(
        "researcher_impl".to_string(),
        ("researched".to_string(), None),
    );
    responses.insert("writer_impl".to_string(), ("written".to_string(), None));
    let bridge = make_bridge(responses, &["researcher_impl", "writer_impl"]);
    let api: Arc<dyn crucible_lua::DaemonTeamApi> = Arc::new(bridge);

    let lua = mlua::Lua::new();
    crucible_lua::register_team_module(&lua, api).expect("register");

    let out: String = lua
        .load(
            r#"
            local r = cru.team.router({
                classifier = function(input)
                    if input:find("write") then return "writer" else return "researcher" end
                end,
                routes = { researcher = "researcher_impl", writer = "writer_impl" }
            })
            local result, err = r:run("please write a summary")
            assert(err == nil, "err: " .. tostring(err))
            return result
            "#,
        )
        .eval_async()
        .await
        .unwrap();
    assert_eq!(out, "written");
}

#[tokio::test]
async fn lua_supervisor_drives_real_bridge_to_completion() {
    let mut responses = HashMap::new();
    responses.insert("a".to_string(), ("alpha".to_string(), None));
    responses.insert("b".to_string(), ("beta".to_string(), None));
    let bridge = make_bridge(responses, &["a", "b"]);
    let api: Arc<dyn crucible_lua::DaemonTeamApi> = Arc::new(bridge);

    let lua = mlua::Lua::new();
    crucible_lua::register_team_module(&lua, api).expect("register");

    let out: String = lua
        .load(
            r#"
            local t = cru.team.supervisor({
                agents = { "a", "b" },
                decider = function(task, history)
                    if #history == 0 then return { agent = "a", prompt = "first" } end
                    if #history == 1 then return { agent = "b", prompt = "second" } end
                    return { done = true }
                end
            })
            local r, err = t:run("the task")
            assert(err == nil, "err: " .. tostring(err))
            return r
            "#,
        )
        .eval_async()
        .await
        .unwrap();
    // Last worker's output is what we return.
    assert_eq!(out, "beta");
}
