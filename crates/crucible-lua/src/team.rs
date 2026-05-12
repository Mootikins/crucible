//! `cru.team` — team-pattern orchestration of multiple agents.
//!
//! **Requires the `send` feature.** The Lua decider / classifier are
//! invoked from the daemon's async runtime (`spawn_subagent_blocking`
//! futures), which means we need `mlua::Lua` to be `Send + Sync`. Without
//! `send` the bridges below would fail the `LuaSupervisorDecideFn: Send
//! + Sync` bound.
//!
//! Surfaces the three Wave 2 patterns from `crucible-daemon::team`:
//!
//! ```lua
//! -- Supervisor: a Lua decider chooses which worker runs next.
//! local team = cru.team.supervisor({
//!   agents = { "researcher", "writer", "fact_checker" },
//!   decider = function(task, history)
//!     -- history is an array of { agent = "...", output = "..." }
//!     if #history == 0 then return { agent = "researcher", prompt = task } end
//!     if #history == 1 then return { agent = "writer",     prompt = "Write up the research." } end
//!     return { done = true }
//!   end,
//!   rules = "optional human description (informational only)",
//! })
//! local result, err = team:run("Summarize the codebase")
//!
//! -- Router: classifier picks one route → one agent.
//! local router = cru.team.router({
//!   classifier = function(input) return "researcher" end,
//!   routes     = { researcher = "agent_a", writer = "agent_b" },
//! })
//! local response, err = router:run("Some user query")
//!
//! -- Broadcast: parallel fan-out.
//! local responses, err = cru.team.broadcast({ "agent_a", "agent_b" }, "What is your status?")
//! -- responses is an array of strings, one per input agent, in order.
//! ```
//!
//! ## Architecture
//!
//! Like other `cru.*` modules, `team` is wire-only here in crucible-lua —
//! all execution lives behind the [`DaemonTeamApi`] trait that the daemon
//! implements. This keeps the Lua crate free of subagent/factory concerns.

use crate::error::LuaError;
use crate::lua_util::register_in_namespaces;
use async_trait::async_trait;
use mlua::{
    AnyUserData, Function, Lua, MetaMethod, RegistryKey, Table, UserData, UserDataMethods, Value,
};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

/// A single (agent, output) pair from a supervisor run, exposed to the
/// Lua decider as `{ agent = "...", output = "..." }`.
#[derive(Debug, Clone)]
pub struct TeamHistoryEntry {
    pub agent: String,
    pub output: String,
}

/// What the Lua decider returns each turn.
///
/// `done = true` terminates the loop; otherwise both `agent` and `prompt`
/// are required.
#[derive(Debug, Clone)]
pub enum LuaSupervisorDecision {
    Run { agent: String, prompt: String },
    Done,
}

/// Daemon-side team executor. Implemented by `crucible-daemon` so the Lua
/// crate stays free of subagent/factory dependencies.
///
/// The Lua wrapper here is responsible for turning the user's Lua
/// callbacks into Rust traits that the implementation can consume; the
/// implementation owns the actual `BackgroundJobManager` and `TeamCtx`.
pub trait DaemonTeamApi: Send + Sync {
    /// Run a supervisor loop. `decide` is invoked once per step.
    fn supervisor(
        &self,
        agents: Vec<String>,
        decide: Arc<dyn LuaSupervisorDecideFn>,
        task: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

    /// Run a single-shot router. `classify` is invoked once.
    fn router(
        &self,
        routes: HashMap<String, String>,
        classify: Arc<dyn LuaClassifyFn>,
        input: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

    /// Fan-out broadcast.
    fn broadcast(
        &self,
        agents: Vec<String>,
        prompt: String,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, String>> + Send>>;
}

/// Decider closure handed to the daemon-side supervisor.
///
/// Trait, not `Box<dyn FnMut>`, because we want it `Send + Sync` to live
/// inside an `Arc<dyn SupervisorDecider>` on the daemon side.
#[async_trait]
pub trait LuaSupervisorDecideFn: Send + Sync {
    async fn call(
        &self,
        task: &str,
        history: &[TeamHistoryEntry],
    ) -> Result<LuaSupervisorDecision, String>;
}

#[async_trait]
pub trait LuaClassifyFn: Send + Sync {
    async fn call(&self, input: &str) -> Result<String, String>;
}

/// Stub: register `cru.team.*` with placeholders that return
/// `(nil, "no daemon connected")`. Lets the stub generator and pre-daemon
/// init phase reference `cru.team` without crashing.
///
/// Each stub also emits a `tracing::warn!` when fired so that running
/// against the stub at runtime is loud — the daemon MUST call
/// `upgrade_with_team` after construction to replace these. If the warn
/// fires in production, `Server::run` regressed (e.g. someone deleted the
/// `upgrade_with_team` call again).
pub fn register_team_module_stub(lua: &Lua) -> Result<(), LuaError> {
    let team = lua.create_table()?;

    let supervisor = lua.create_function(|lua, _opts: Table| {
        tracing::warn!(
            "cru.team.supervisor called but daemon never upgraded the team module \
             (Server::run is missing upgrade_with_team)"
        );
        let err = lua.create_string("no daemon connected")?;
        Ok((Value::Nil, Value::String(err)))
    })?;
    team.set("supervisor", supervisor)?;

    let router = lua.create_function(|lua, _opts: Table| {
        tracing::warn!(
            "cru.team.router called but daemon never upgraded the team module \
             (Server::run is missing upgrade_with_team)"
        );
        let err = lua.create_string("no daemon connected")?;
        Ok((Value::Nil, Value::String(err)))
    })?;
    team.set("router", router)?;

    let broadcast = lua.create_async_function(|lua, _args: (Value, String)| async move {
        tracing::warn!(
            "cru.team.broadcast called but daemon never upgraded the team module \
             (Server::run is missing upgrade_with_team)"
        );
        let err = lua.create_string("no daemon connected")?;
        Ok((Value::Nil, Value::String(err)))
    })?;
    team.set("broadcast", broadcast)?;

    register_in_namespaces(lua, "team", team)?;
    Ok(())
}

/// Register `cru.team.*` backed by a real [`DaemonTeamApi`].
pub fn register_team_module(lua: &Lua, api: Arc<dyn DaemonTeamApi>) -> Result<(), LuaError> {
    let team = lua.create_table()?;

    // supervisor({ agents, decider, rules? }) -> Team userdata
    //
    // Returns (userdata, nil) on success or (nil, err_string) on validation
    // failure (missing agents / non-function decider / etc.). The userdata
    // has a `:run(task)` method that returns (string, nil) | (nil, err).
    {
        let api = Arc::clone(&api);
        let supervisor = lua.create_function(move |lua, opts: Table| {
            let agents = read_string_list(&opts, "agents")?;
            if agents.is_empty() {
                let err = lua.create_string("supervisor: 'agents' must be a non-empty array")?;
                return Ok((Value::Nil, Value::String(err)));
            }
            let decider_fn: Function = match opts.get("decider") {
                Ok(f) => f,
                Err(_) => {
                    let err = lua.create_string("supervisor: 'decider' must be a function")?;
                    return Ok((Value::Nil, Value::String(err)));
                }
            };
            let key = lua.create_registry_value(decider_fn)?;
            // `rules` is kept for documentation / future use but does not
            // drive behaviour today — the Lua decider does.
            let rules: Option<String> = opts.get("rules").ok();

            let ud = lua.create_userdata(SupervisorHandle {
                api: Arc::clone(&api),
                agents,
                decider_key: Arc::new(key),
                _rules: rules,
            })?;
            Ok((Value::UserData(ud), Value::Nil))
        })?;
        team.set("supervisor", supervisor)?;
    }

    // router({ classifier = fn, routes = { ... } }) -> Router userdata
    {
        let api = Arc::clone(&api);
        let router = lua.create_function(move |lua, opts: Table| {
            let classifier: Function = match opts.get("classifier") {
                Ok(f) => f,
                Err(_) => {
                    let err = lua.create_string("router: 'classifier' must be a function")?;
                    return Ok((Value::Nil, Value::String(err)));
                }
            };
            let routes_tbl: Table = match opts.get("routes") {
                Ok(t) => t,
                Err(_) => {
                    let err = lua.create_string("router: 'routes' must be a table")?;
                    return Ok((Value::Nil, Value::String(err)));
                }
            };
            let mut routes: HashMap<String, String> = HashMap::new();
            for pair in routes_tbl.pairs::<String, String>() {
                let (k, v) = pair?;
                routes.insert(k, v);
            }
            if routes.is_empty() {
                let err = lua.create_string("router: 'routes' must be non-empty")?;
                return Ok((Value::Nil, Value::String(err)));
            }
            let key = lua.create_registry_value(classifier)?;
            let ud = lua.create_userdata(RouterHandle {
                api: Arc::clone(&api),
                routes,
                classifier_key: Arc::new(key),
            })?;
            Ok((Value::UserData(ud), Value::Nil))
        })?;
        team.set("router", router)?;
    }

    // broadcast(agents, prompt) -> ({ output_1, ..., output_n }, nil) | (nil, err)
    {
        let api = Arc::clone(&api);
        let broadcast =
            lua.create_async_function(move |lua, (agents, prompt): (Table, String)| {
                let api = Arc::clone(&api);
                async move {
                    let agents: Vec<String> = agents
                        .sequence_values::<String>()
                        .collect::<mlua::Result<_>>()?;
                    match api.broadcast(agents, prompt).await {
                        Ok(results) => {
                            let table = lua.create_table()?;
                            for (i, s) in results.into_iter().enumerate() {
                                table.set(i + 1, s)?;
                            }
                            Ok((Value::Table(table), Value::Nil))
                        }
                        Err(e) => {
                            let err = lua.create_string(&e)?;
                            Ok((Value::Nil, Value::String(err)))
                        }
                    }
                }
            })?;
        team.set("broadcast", broadcast)?;
    }

    register_in_namespaces(lua, "team", team)?;
    Ok(())
}

fn read_string_list(opts: &Table, field: &str) -> mlua::Result<Vec<String>> {
    let Ok(tbl): mlua::Result<Table> = opts.get(field) else {
        return Ok(Vec::new());
    };
    tbl.sequence_values::<String>().collect()
}

// ---- userdata for the fluent .run(task) API ----

struct SupervisorHandle {
    api: Arc<dyn DaemonTeamApi>,
    agents: Vec<String>,
    decider_key: Arc<RegistryKey>,
    _rules: Option<String>,
}

impl UserData for SupervisorHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("run", |lua, this, task: String| async move {
            let bridge = LuaDecideBridge::new(lua.clone(), Arc::clone(&this.decider_key));
            match this
                .api
                .supervisor(this.agents.clone(), Arc::new(bridge), task)
                .await
            {
                Ok(out) => Ok((Value::String(lua.create_string(&out)?), Value::Nil)),
                Err(e) => Ok((Value::Nil, Value::String(lua.create_string(&e)?))),
            }
        });

        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| {
            Ok("cru.team.supervisor".to_string())
        });
    }
}

struct RouterHandle {
    api: Arc<dyn DaemonTeamApi>,
    routes: HashMap<String, String>,
    classifier_key: Arc<RegistryKey>,
}

impl UserData for RouterHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("run", |lua, this, input: String| async move {
            let bridge = LuaClassifyBridge::new(lua.clone(), Arc::clone(&this.classifier_key));
            match this
                .api
                .router(this.routes.clone(), Arc::new(bridge), input)
                .await
            {
                Ok(out) => Ok((Value::String(lua.create_string(&out)?), Value::Nil)),
                Err(e) => Ok((Value::Nil, Value::String(lua.create_string(&e)?))),
            }
        });

        methods.add_meta_method(MetaMethod::ToString, |_, _, ()| {
            Ok("cru.team.router".to_string())
        });
    }
}

/// Adapter that turns a Lua-side decider function into a [`LuaSupervisorDecideFn`]
/// the daemon can call from async Rust.
///
/// The trick: `RegistryKey` is `Send + Sync` (it's just an integer handle
/// into the Lua-owned registry), so we can store it inside an `Arc` and
/// resolve back to an `mlua::Function` whenever the daemon asks us to
/// decide. The `Lua` we cloned shares state with the original via mlua's
/// internal refcount.
struct LuaDecideBridge {
    lua: Lua,
    key: Arc<RegistryKey>,
    inflight: Mutex<()>,
}

impl LuaDecideBridge {
    fn new(lua: Lua, key: Arc<RegistryKey>) -> Self {
        Self {
            lua,
            key,
            inflight: Mutex::new(()),
        }
    }
}

#[async_trait]
impl LuaSupervisorDecideFn for LuaDecideBridge {
    async fn call(
        &self,
        task: &str,
        history: &[TeamHistoryEntry],
    ) -> Result<LuaSupervisorDecision, String> {
        // Serialise calls into Lua. mlua::Lua is internally synchronous;
        // even with the `send` feature on, concurrent calls would
        // interleave registry lookups. The supervisor itself is sequential,
        // so this lock is functionally a no-op — kept for safety if a
        // future variant calls deciders concurrently.
        let _guard = self.inflight.lock().map_err(|e| e.to_string())?;

        let f: Function = self
            .lua
            .registry_value(&self.key)
            .map_err(|e| format!("decider lookup: {e}"))?;
        let history_tbl = self
            .lua
            .create_table()
            .map_err(|e| format!("history table: {e}"))?;
        for (i, entry) in history.iter().enumerate() {
            let row = self
                .lua
                .create_table()
                .map_err(|e| format!("history row: {e}"))?;
            row.set("agent", entry.agent.as_str())
                .map_err(|e| e.to_string())?;
            row.set("output", entry.output.as_str())
                .map_err(|e| e.to_string())?;
            history_tbl
                .set(i + 1, row)
                .map_err(|e| format!("history set: {e}"))?;
        }

        let ret: Value = f
            .call((task, history_tbl))
            .map_err(|e| format!("decider call: {e}"))?;
        decode_decision(ret)
    }
}

fn decode_decision(value: Value) -> Result<LuaSupervisorDecision, String> {
    let Value::Table(t) = value else {
        return Err("decider must return a table { agent=, prompt= } or { done = true }".into());
    };
    let done: Option<bool> = t.get("done").ok();
    if done.unwrap_or(false) {
        return Ok(LuaSupervisorDecision::Done);
    }
    let agent: String = t
        .get("agent")
        .map_err(|_| "decider table missing 'agent' (or set done=true)".to_string())?;
    let prompt: String = t
        .get("prompt")
        .map_err(|_| "decider table missing 'prompt' (or set done=true)".to_string())?;
    Ok(LuaSupervisorDecision::Run { agent, prompt })
}

struct LuaClassifyBridge {
    lua: Lua,
    key: Arc<RegistryKey>,
    inflight: Mutex<()>,
}

impl LuaClassifyBridge {
    fn new(lua: Lua, key: Arc<RegistryKey>) -> Self {
        Self {
            lua,
            key,
            inflight: Mutex::new(()),
        }
    }
}

#[async_trait]
impl LuaClassifyFn for LuaClassifyBridge {
    async fn call(&self, input: &str) -> Result<String, String> {
        let _guard = self.inflight.lock().map_err(|e| e.to_string())?;
        let f: Function = self
            .lua
            .registry_value(&self.key)
            .map_err(|e| format!("classifier lookup: {e}"))?;
        let route: String = f
            .call(input.to_string())
            .map_err(|e| format!("classifier call: {e}"))?;
        Ok(route)
    }
}

// `AnyUserData` re-export so doc tests / users can refer to the return
// type of `cru.team.supervisor()` without poking into mlua directly.
pub type TeamHandle = AnyUserData;

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake daemon-side API that records what was asked of it and returns
    /// canned answers — lets us exercise the Lua surface end-to-end
    /// without spinning up `BackgroundJobManager`.
    struct StubTeamApi {
        broadcast_returns: Mutex<Vec<String>>,
        supervisor_log: Mutex<Vec<String>>,
        router_answer: Mutex<String>,
    }

    impl StubTeamApi {
        fn new() -> Self {
            Self {
                broadcast_returns: Mutex::new(Vec::new()),
                supervisor_log: Mutex::new(Vec::new()),
                router_answer: Mutex::new("router-fired".to_string()),
            }
        }
    }

    impl DaemonTeamApi for StubTeamApi {
        fn supervisor(
            &self,
            agents: Vec<String>,
            decide: Arc<dyn LuaSupervisorDecideFn>,
            task: String,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            let log = self.supervisor_log.lock().unwrap().clone();
            let _ = log;
            Box::pin(async move {
                // Walk the decider just like the real supervisor would.
                let mut history: Vec<TeamHistoryEntry> = Vec::new();
                let mut last = String::new();
                for _ in 0..(agents.len() * 2 + 1) {
                    let dec = decide.call(&task, &history).await?;
                    match dec {
                        LuaSupervisorDecision::Done => break,
                        LuaSupervisorDecision::Run { agent, prompt } => {
                            // "Run" simulator: output is `<agent>:<prompt>`.
                            let output = format!("{agent}:{prompt}");
                            history.push(TeamHistoryEntry {
                                agent: agent.clone(),
                                output: output.clone(),
                            });
                            last = output;
                        }
                    }
                }
                Ok(last)
            })
        }

        fn router(
            &self,
            routes: HashMap<String, String>,
            classify: Arc<dyn LuaClassifyFn>,
            input: String,
        ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send>> {
            let canned = self.router_answer.lock().unwrap().clone();
            Box::pin(async move {
                let route = classify.call(&input).await?;
                let _agent = routes
                    .get(&route)
                    .cloned()
                    .ok_or_else(|| format!("unknown route: {route}"))?;
                Ok(format!("{canned}:{route}"))
            })
        }

        fn broadcast(
            &self,
            agents: Vec<String>,
            _prompt: String,
        ) -> Pin<Box<dyn Future<Output = Result<Vec<String>, String>> + Send>> {
            let mut canned = self.broadcast_returns.lock().unwrap().clone();
            Box::pin(async move {
                if canned.len() != agents.len() {
                    // default: echo agent name
                    canned = agents.iter().map(|a| format!("from:{a}")).collect();
                }
                Ok(canned)
            })
        }
    }

    fn make_lua_with_api(api: Arc<dyn DaemonTeamApi>) -> Lua {
        let lua = Lua::new();
        register_team_module(&lua, api).expect("register");
        lua
    }

    #[tokio::test]
    async fn broadcast_returns_array_per_agent() {
        let api = Arc::new(StubTeamApi::new());
        let lua = make_lua_with_api(api);

        let out: Vec<String> = lua
            .load(
                r#"
                local r, err = cru.team.broadcast({ "a", "b", "c" }, "status?")
                assert(err == nil, "err: " .. tostring(err))
                return r
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(
            out,
            vec![
                "from:a".to_string(),
                "from:b".to_string(),
                "from:c".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn router_uses_lua_classifier() {
        let api = Arc::new(StubTeamApi::new());
        let lua = make_lua_with_api(api);

        let out: String = lua
            .load(
                r#"
                local r = cru.team.router({
                    classifier = function(input)
                        if input == "x" then return "writer" else return "researcher" end
                    end,
                    routes = { researcher = "agent_a", writer = "agent_b" }
                })
                local result, err = r:run("anything")
                assert(err == nil, "err: " .. tostring(err))
                return result
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(out, "router-fired:researcher");
    }

    #[tokio::test]
    async fn router_surfaces_classifier_route_miss() {
        let api = Arc::new(StubTeamApi::new());
        let lua = make_lua_with_api(api);

        let err: String = lua
            .load(
                r#"
                local r = cru.team.router({
                    classifier = function(_) return "ghost" end,
                    routes = { researcher = "agent_a" }
                })
                local _, err = r:run("anything")
                return err
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(err.contains("unknown route"), "got: {err}");
    }

    #[tokio::test]
    async fn supervisor_invokes_decider_until_done() {
        let api = Arc::new(StubTeamApi::new());
        let lua = make_lua_with_api(api);

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
        // Stub returns "<agent>:<prompt>" per turn; final output is the
        // last one ("b:second").
        assert_eq!(out, "b:second");
    }

    #[tokio::test]
    async fn supervisor_decider_sees_growing_history() {
        let api = Arc::new(StubTeamApi::new());
        let lua = make_lua_with_api(api);

        // The decider records the cumulative history lengths it saw.
        let lengths: Vec<i64> = lua
            .load(
                r#"
                local seen = {}
                local t = cru.team.supervisor({
                    agents = { "x" },
                    decider = function(task, history)
                        seen[#seen + 1] = #history
                        if #history < 2 then
                            return { agent = "x", prompt = "go " .. tostring(#history) }
                        end
                        return { done = true }
                    end
                })
                t:run("t")
                return seen
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert_eq!(lengths, vec![0, 1, 2]);
    }

    #[tokio::test]
    async fn supervisor_rejects_empty_agents() {
        let api = Arc::new(StubTeamApi::new());
        let lua = make_lua_with_api(api);

        let err: String = lua
            .load(
                r#"
                local t, err = cru.team.supervisor({
                    agents = {},
                    decider = function() return { done = true } end,
                })
                assert(t == nil)
                return err
                "#,
            )
            .eval_async()
            .await
            .unwrap();
        assert!(err.contains("non-empty"), "got: {err}");
    }

    #[tokio::test]
    async fn registers_in_both_namespaces() {
        let api = Arc::new(StubTeamApi::new());
        let lua = make_lua_with_api(api);

        let ok: bool = lua
            .load(
                r#"
                return type(cru.team.broadcast) == "function"
                   and type(crucible.team.broadcast) == "function"
                "#,
            )
            .eval()
            .unwrap();
        assert!(ok);
    }

    #[test]
    fn stub_registration_returns_no_daemon_error() {
        let lua = Lua::new();
        register_team_module_stub(&lua).unwrap();
        let err: String = lua
            .load(
                r#"
                local _, err = cru.team.supervisor({ agents = {"a"}, decider = function() end })
                return err
                "#,
            )
            .eval()
            .unwrap();
        assert!(err.contains("no daemon connected"));
    }
}
