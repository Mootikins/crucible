use crucible_core::events::SessionEvent;
use crucible_core::utils::glob_match;
use mlua::{Function, Lua, RegistryKey, Result as LuaResult, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::conversion::session_event_to_lua;
use super::hook_name::HookName;
use super::script_handler::{interpret_handler_result, ScriptHandlerResult};
use crate::plugin_context::{current_owner, set_owner, Owner};

/// Every Lua callback the host holds, in one store.
///
/// Nothing is discovered from the filesystem: a registration exists because
/// Lua called one of the registration APIs.
///
/// # Six stores became one
///
/// `cru.on` kept two parallel collections here; `cru.permissions.on_request`
/// kept a third and a fourth; `cru.on_session_start`, `cru.on_session_end` and
/// `cru.on_provider_auth` kept six Lua tables under two globals. Each repeated
/// the same three things — a monotonic name allocator, an owner tag, and a
/// clear-on-reload — and each got one of them wrong. The auth hooks counted
/// through a Lua global a plugin could assign. The permission hooks derived an
/// id from the list length, which collides after a clear, and had no owner at
/// all, so no reload ever removed one.
///
/// **A store is not a dispatcher.** The six share this store and keep their
/// own fire paths, which differ in argument shape, in budget and in whether
/// they may await. [`execute_permission_hooks`](super::permission::execute_permission_hooks)
/// stays synchronous for the reason recorded there.
///
/// # Example
///
/// ```rust,ignore
/// // Registration happens from Lua, via the APIs this registry backs.
/// register_cru_on_api(&lua, registry.clone())?;
///
/// // Dispatch: select by name, then execute each match by id.
/// for handler in registry.runtime_handlers_for("tool_result", Some(tool_name)) {
///     let outcome = registry
///         .execute_runtime_handler(&lua, handler.id, &event, Some(session_id))
///         .await?;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct LuaScriptHandlerRegistry {
    /// Every registration, in registration order.
    ///
    /// ONE lock, holding the body as well as the row. The two-lock form this
    /// replaces had to document its lock order, because a dispatch racing a
    /// reload could otherwise read a row whose body had already gone.
    registrations: Arc<Mutex<Vec<Registration>>>,
    /// The one id allocator.
    ///
    /// NEVER derive an id from the list length. `clear_owner` shrinks the
    /// list, so after a reload a length-derived id collides with one another
    /// registrant still holds. Dispatch is by id, so the collision rebinds a
    /// survivor's slot to the new body rather than merely duplicating a row —
    /// and with `pre_tool_call` failing closed, a body raising against the
    /// wrong payload denies every matching tool call in every session.
    next_id: Arc<AtomicU64>,
}

/// One Lua callback, and everything the host knows about it.
#[derive(Debug, Clone)]
pub struct Registration {
    /// The hook this callback registered for.
    pub name: HookName,
    /// Who registered it. `clear_owner` matches on exactly this.
    pub owner: Owner,
    /// The dispatch key, from the one monotonic allocator. Never reused.
    pub id: u64,
    /// Lower runs first. Registration order breaks a tie, and that order is
    /// total: search paths rank by `runtime_path::Origin`, and
    /// `lifecycle::discovery` sorts each directory by name.
    pub priority: i64,
    /// Glob over the dispatch identifier — a tool name, for the hooks that
    /// carry one. `None` matches every dispatch.
    pub pattern: Option<String>,
    /// What the registration asked for with `{ timeout_ms = … }`, in
    /// milliseconds. `None` takes the budget of the name it registered for.
    pub timeout_ms: Option<u64>,
    /// Whether a failure refuses the session. Read by the `session:start`
    /// fire path and by nothing else; every other name leaves it `false`.
    ///
    /// Opt-in deliberately. A hook that owns an isolation boundary (`oci` and
    /// its container) must be able to stop a session that would otherwise run
    /// unsandboxed. Making every hook fatal would let one typo in any plugin
    /// refuse every session daemon-wide.
    pub required: bool,
    /// Whether this registration may take a tool call over — return
    /// `{ handled = true, … }` or a transform from `pre_tool_call`.
    ///
    /// [`Owner::may_intercept`] decides it, once, at registration. A handler
    /// firing three turns later still runs as its own owner, which is what
    /// `cru.storage` keys on and what `intercepts_tools` is read from.
    ///
    /// A registration may `cancel` whatever this says: refusing a call can
    /// only narrow.
    pub may_intercept: bool,
    /// The Lua function.
    ///
    /// `Arc` so a selected row clones out of the lock without cloning the
    /// registry slot. The slot is released when the last clone drops, which is
    /// what `expire_registry_values` then reclaims.
    body: Arc<RegistryKey>,
}

/// What a registration asks for, beside its body.
///
/// One struct so the four registration APIs pass the same shape and cannot
/// disagree about a default.
#[derive(Debug, Clone)]
pub struct RegistrationSpec {
    /// The hook to register for.
    pub name: HookName,
    /// Lower runs first. 100 is the documented default.
    pub priority: i64,
    /// Glob over the dispatch identifier.
    pub pattern: Option<String>,
    /// An explicit time budget, in milliseconds.
    pub timeout_ms: Option<u64>,
    /// Whether a failure refuses the session; `session:start` only.
    pub required: bool,
}

impl RegistrationSpec {
    /// A registration for `name` with every option at its default.
    #[must_use]
    pub fn new(name: HookName) -> Self {
        Self {
            name,
            priority: DEFAULT_PRIORITY,
            pattern: None,
            timeout_ms: None,
            required: false,
        }
    }
}

/// The priority a registration takes when it names none.
pub const DEFAULT_PRIORITY: i64 = 100;

impl Registration {
    /// Whether this registration may take a tool call over.
    ///
    /// A registration without the grant may observe and may `cancel`; its
    /// `handled` and transform results are refused, because `handled` returns
    /// before the permission gate.
    #[must_use]
    pub fn may_intercept(&self) -> bool {
        self.may_intercept
    }
}

impl LuaScriptHandlerRegistry {
    /// Create an empty registry
    #[must_use]
    pub fn new() -> Self {
        Self {
            registrations: Arc::new(Mutex::new(Vec::new())),
            next_id: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Store `handler` under `spec`, owned by whoever is running now.
    ///
    /// The owner comes from the VM's own app data, never from an argument: a
    /// plugin must not be able to register under another plugin's name, nor
    /// grant itself the interception right read here.
    ///
    /// Answers the id, which is the dispatch key.
    pub fn register(&self, lua: &Lua, spec: RegistrationSpec, handler: Function) -> LuaResult<u64> {
        let owner = current_owner(lua);
        let may_intercept = owner.may_intercept(lua);
        // The body is in hand before anything is pushed: nothing lands in the
        // list without a function, which `pre_tool_call` would otherwise turn
        // into a denied tool call.
        let body = Arc::new(lua.create_registry_value(handler)?);
        // `Relaxed` suffices: the list mutex taken below brackets the whole
        // allocate-then-push sequence, so it supplies the ordering.
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let registration = Registration {
            name: spec.name,
            owner,
            id,
            priority: spec.priority,
            pattern: spec.pattern,
            timeout_ms: spec.timeout_ms,
            required: spec.required,
            may_intercept,
            body,
        };
        self.registrations
            .lock()
            .map_err(|e| mlua::Error::RuntimeError(format!("Failed to lock registrations: {e}")))?
            .push(registration);
        Ok(id)
    }

    /// How many registrations `plugin` holds.
    ///
    /// This is the count `plugin.list` reports. It used to come from the
    /// spec-table `handlers` field — which is parsed but never dispatched —
    /// so plugins using the real API showed 0 and plugins using the dead one
    /// showed a number that meant nothing.
    pub fn plugin_handler_count(&self, plugin: &str) -> usize {
        self.registrations
            .lock()
            .map(|rows| {
                rows.iter()
                    .filter(|r| r.owner.plugin_name() == Some(plugin))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Every registration for `name`, priority first, matching `identifier`.
    ///
    /// The sort is stable, so two registrations of equal priority run in
    /// registration order.
    pub fn for_hook(&self, name: HookName, identifier: Option<&str>) -> Vec<Registration> {
        let rows = self
            .registrations
            .lock()
            .expect("registrations: poisoned while selecting handlers");
        let mut matching: Vec<Registration> = rows
            .iter()
            .filter(|r| r.name == name && r.matches(identifier))
            .cloned()
            .collect();
        matching.sort_by_key(|r| r.priority);
        matching
    }

    /// [`Self::for_hook`] by registered name.
    ///
    /// The dispatch sites hold a [`HookName`], a [`StageId`](super::hook_name::StageId)
    /// or an [`EventName`](super::hook_name::EventName), and the string is the
    /// one spelling all three agree on. A name nothing can register answers
    /// with no handlers rather than raising: `cru.on` already refused it at
    /// registration, so an empty answer is the truth.
    pub fn runtime_handlers_for(
        &self,
        event_type: &str,
        identifier: Option<&str>,
    ) -> Vec<Registration> {
        match HookName::parse(event_type) {
            Some(name) => self.for_hook(name, identifier),
            None => Vec::new(),
        }
    }

    /// Drop every registration `owner` made, and release their bodies.
    ///
    /// Answers how many it removed, so a caller can log the change.
    pub fn clear_owner(&self, owner: &Owner) -> usize {
        let Ok(mut rows) = self.registrations.lock() else {
            return 0;
        };
        let before = rows.len();
        rows.retain(|r| &r.owner != owner);
        before - rows.len()
    }

    /// Execute a registration by id.
    ///
    /// The handler receives `(ctx, event)`; `ctx.session_id` carries the
    /// session the event belongs to when the dispatch site knows it.
    ///
    /// That field is what lets a handler registered once at plugin load serve
    /// many sessions (`oci` keys its containers by it).
    ///
    /// # Returns
    ///
    /// Returns `Ok(ScriptHandlerResult)` on success, or `Err` if execution
    /// fails. An unknown `id` is NOT an error: the registration went away
    /// between the dispatch snapshot and execution (a plugin reload mid-call)
    /// and has no opinion — `PassThrough` is returned.
    pub async fn execute_runtime_handler(
        &self,
        lua: &Lua,
        id: u64,
        event: &SessionEvent,
        session_id: Option<&str>,
    ) -> LuaResult<ScriptHandlerResult> {
        let event_table = session_event_to_lua(lua, event)?;
        self.execute_handler_with_payload(lua, id, Value::Table(event_table), session_id)
            .await
    }

    /// Run the registration `id` with `payload` as its event argument. This is
    /// the one body behind [`Self::execute_runtime_handler`] and the
    /// JSON-payload stages in `before_execute.rs`.
    pub(super) async fn execute_handler_with_payload(
        &self,
        lua: &Lua,
        id: u64,
        payload: Value,
        session_id: Option<&str>,
    ) -> LuaResult<ScriptHandlerResult> {
        // The owner recorded when the registration was made. A deferred call
        // keeps that identity fixed, so a handler calling `cru.storage` from a
        // later turn still reaches its own plugin's namespace and holds no
        // more authority than its plugin does.
        let Some(registration) = self.by_id(id) else {
            // Unregistered between the dispatch snapshot and execution — a
            // plugin reload clears its rows while a call is in flight. An
            // absent handler has no opinion; erroring instead lands in
            // `pre_tool_call`'s fail-closed arm and denies the tool call on
            // behalf of a handler that no longer exists.
            tracing::debug!(
                handler = id,
                "handler unregistered mid-dispatch; passing through"
            );
            return Ok(ScriptHandlerResult::PassThrough);
        };
        let budget = registration.budget();
        let handler: Function = lua.registry_value(registration.body())?;

        let ctx_table = lua.create_table()?;
        if let Some(session) = session_id {
            ctx_table.set("session_id", session)?;
        }

        let previous = set_owner(lua, registration.owner.clone());
        // Two mechanisms, because one is not enough. The tokio timeout ends a
        // handler that AWAITS — a sleep, an http call, a shell command — by
        // cancelling the future at an await point. It cannot end
        // `while true do end`, which never yields and never gives the runtime
        // back; the VM deadline does that, from inside Lua's own instruction
        // hook. Neither covers the other's case.
        let call = {
            let _budget = crate::handler_budget::enter(
                lua,
                budget,
                format!("the `{}` handler", registration.name),
            );
            match tokio::time::timeout(budget, handler.call_async::<Value>((ctx_table, payload)))
                .await
            {
                Ok(call) => call,
                Err(_elapsed) => Err(mlua::Error::runtime(format!(
                    "the `{}` handler exceeded its {} ms time budget and was cancelled",
                    registration.name,
                    budget.as_millis()
                ))),
            }
        };
        // Restored before the `?`: an owner left behind would attribute the
        // next registration to the wrong author.
        set_owner(lua, previous);

        interpret_handler_result(&call?)
    }

    /// Every registration, in registration order.
    ///
    /// For a caller that wants the whole store rather than one name: a test
    /// counting what a load left behind, and the boot rollback check.
    pub fn all(&self) -> Vec<Registration> {
        self.registrations
            .lock()
            .expect("registrations: poisoned while listing handlers")
            .clone()
    }

    /// One registration by id, cloned out of the lock.
    pub fn by_id(&self, id: u64) -> Option<Registration> {
        self.registrations
            .lock()
            .expect("registrations: poisoned while looking a handler up")
            .iter()
            .find(|r| r.id == id)
            .cloned()
    }
}

impl Registration {
    /// Whether `identifier` passes this registration's pattern.
    ///
    /// A pattern with no identifier does not match: the registration asked to
    /// be filtered and the dispatch site cannot filter it.
    #[must_use]
    pub fn matches(&self, identifier: Option<&str>) -> bool {
        match (&self.pattern, identifier) {
            (Some(pattern), Some(id)) => glob_match(pattern, id),
            (Some(_), None) => false,
            (None, _) => true,
        }
    }

    /// How long this registration may run: what it asked for, else the budget
    /// of the name it registered for.
    #[must_use]
    pub fn budget(&self) -> std::time::Duration {
        match self.timeout_ms {
            Some(ms) => std::time::Duration::from_millis(ms),
            None => self.name.budget(),
        }
    }

    /// The Lua registry slot holding the body.
    #[must_use]
    pub fn body(&self) -> &RegistryKey {
        &self.body
    }
}

impl Default for LuaScriptHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Remove every registration `owner` made, wherever the host keeps one.
///
/// One free function rather than a trait: the stores it reaches have nothing
/// else in common, and a trait would ask each of them to grow a method it has
/// no other use for.
///
/// `make_plugin_inert` states the invariant this makes true — "Not Active"
/// must imply "nothing of this plugin's is registered or running". Before
/// this, it cleared five stores and missed the permission hooks, the schedules
/// and both session-hook maps, so a plugin marked Not Active still held live
/// registrations and leaked one more copy on every reload.
///
/// Answers how many registrations it removed.
pub fn clear_owner(lua: &Lua, registry: &LuaScriptHandlerRegistry, owner: &Owner) -> usize {
    let dropped = registry.clear_owner(owner);
    let schedules = crate::schedule::cancel_owner(lua, owner);
    if dropped + schedules > 0 {
        tracing::debug!(%owner, dropped, schedules, "cleared owner registrations");
    }
    dropped + schedules
}
