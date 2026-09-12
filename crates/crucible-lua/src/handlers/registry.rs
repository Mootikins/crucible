use crucible_core::events::SessionEvent;
use crucible_core::utils::glob_match;
use mlua::{Function, Lua, RegistryKey, Result as LuaResult, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::conversion::session_event_to_lua;
use super::hook_name::HookName;
use super::script_handler::{interpret_handler_result, ScriptHandlerResult};
use crate::plugin_context::{current_source, enter_session, set_source, LuaSource};

/// Which sessions a registration fires for.
///
/// A third concern beside the source and the pattern, and independent of both:
/// the source says whose registration it is, the pattern says which tool name,
/// this says which session.
///
/// # Activation REGISTERS
///
/// A workflow plugin must fire for the sessions a user turned it on for and
/// for no others — a loop that re-prompts a model would otherwise take over
/// turns nobody asked it to. The set of sessions a handler serves is
/// therefore the set of registrations that exist: a plugin turned on for one
/// session registers a handler scoped to it, at the moment it is turned on.
/// Nothing is looked up while a turn runs.
///
/// The earlier draft of this had a third variant, where sessions joined a
/// list and a handler registered once read the list at fire time. Two
/// variants cost less: no tag vocabulary to typo, no per-session state on the
/// hot path, and no second mechanism for "which sessions" beside this one.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SessionScope {
    /// Every session, and every dispatch that carries none.
    ///
    /// `Global` rather than `Any`, after Neovim's
    /// `OptScope { kOptScopeGlobal, … }`: the name states the fact, where
    /// `Any` stated the matching rule that follows from it.
    Global,
    /// One session, by id.
    ///
    /// Registration is IDEMPOTENT for these — see
    /// [`LuaScriptHandlerRegistry::register`].
    Session(String),
}

/// The session a dispatch belongs to.
///
/// Its own type rather than a second `Option<&str>` beside the pattern
/// identifier: two adjacent options of one type let a caller swap them, and
/// the swap compiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Firing<'a> {
    /// The dispatch names no session — a file event, a webhook, an index
    /// pass. Only [`SessionScope::Global`] fires.
    Sessionless,
    /// The dispatch belongs to this session.
    InSession(&'a str),
}

impl<'a> Firing<'a> {
    /// The dispatch for a site that holds `Option<&str>`, as most do.
    #[must_use]
    pub const fn of(session: Option<&'a str>) -> Self {
        match session {
            Some(id) => Self::InSession(id),
            None => Self::Sessionless,
        }
    }

    /// The session id, or `None` when the dispatch names none.
    #[must_use]
    pub const fn session(self) -> Option<&'a str> {
        match self {
            Self::InSession(id) => Some(id),
            Self::Sessionless => None,
        }
    }
}

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
/// the same three things — a monotonic name allocator, an source tag, and a
/// clear-on-reload — and each got one of them wrong. The auth hooks counted
/// through a Lua global a plugin could assign. The permission hooks derived an
/// id from the list length, which collides after a clear, and had no source at
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
/// // Dispatch: select by name, pattern and session, then execute each
/// // match by id.
/// let firing = Firing::InSession(session_id);
/// for handler in registry.runtime_handlers_for("tool_result", Some(tool_name), firing) {
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
    /// NEVER derive an id from the list length. `clear_source` shrinks the
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
    /// Who registered it. `clear_source` matches on exactly this.
    pub source: LuaSource,
    /// The dispatch key, from the one monotonic allocator. Never reused.
    pub id: u64,
    /// Glob over the dispatch identifier — a tool name, for the hooks that
    /// carry one. `None` matches every dispatch.
    pub pattern: Option<String>,
    /// Which sessions this fires for. [`SessionScope::Global`] is every one.
    pub scope: SessionScope,
    /// What the registration called itself with `{ key = … }`.
    ///
    /// Part of the replacement key, and there for one reason: without it a
    /// plugin could not register two scoped handlers on one hook for one
    /// session, which is a legal thing to want. Lua spells it `key` rather
    /// than `name` because `name` above is already the hook.
    ///
    /// Neovim names no row and keys its coarse delete on three parts
    /// (`autocmd.c:952`), so this axis has no prior art — but Neovim also
    /// ALWAYS appends, and Crucible replaces a scoped row. Under replacement
    /// this is the only thing that tells two such rows apart.
    pub key: Option<String>,
    /// Whether this registration retires itself after it runs once.
    ///
    /// Neovim's `AutoCmd.once`, and the only one of its three retirement
    /// paths that asks the author for no state. The row leaves the store
    /// BEFORE its body runs; see [`Registration::take_body`](Self::take_body)
    /// for why the order matters, and for why that is the only way to obtain
    /// a body.
    pub once: bool,
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
    /// Glob over the dispatch identifier.
    pub pattern: Option<String>,
    /// Which sessions to fire for.
    pub scope: SessionScope,
    /// What the registration calls itself. See [`Registration::key`].
    pub key: Option<String>,
    /// Whether to retire the registration after it runs once. See
    /// [`Registration::once`].
    pub once: bool,
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
            pattern: None,
            scope: SessionScope::Global,
            key: None,
            once: false,
            timeout_ms: None,
            required: false,
        }
    }
}

/// Which of one source's registrations [`LuaScriptHandlerRegistry::clear_matching`]
/// removes.
///
/// Every field NARROWS, and `None` is no constraint — so every field absent
/// clears everything the calling source registered, which is what
/// `nvim_del_augroup_by_name` does for a group. The source itself is not a
/// field: it is the caller.
#[derive(Debug, Clone, Default)]
pub struct ClearFilter {
    /// The hook to clear, or every hook the source registered for.
    pub name: Option<HookName>,
    /// The pattern to clear, compared as EXACT TEXT and never evaluated as a
    /// glob.
    ///
    /// `cru.clear{ pattern = "bash" }` removes a row registered with
    /// `pattern = "bash"` and leaves one registered with `pattern = "b*"`,
    /// even though that row fires for `bash`. Neovim states the same rule for
    /// `nvim_clear_autocmds` (`api/autocmd.c:538`), and the reason is that a
    /// glob evaluated here would remove rows the caller never named.
    pub pattern: Option<String>,
    /// The scope to clear. `None` removes a row whatever its scope.
    pub scope: Option<SessionScope>,
}

impl ClearFilter {
    /// Whether `row` passes every filter this names.
    ///
    /// The source is checked by the caller, which is the only place that
    /// knows it.
    fn selects(&self, row: &Registration) -> bool {
        self.name.is_none_or(|name| row.name == name)
            && self
                .pattern
                .as_deref()
                .is_none_or(|pattern| row.pattern.as_deref() == Some(pattern))
            && self.scope.as_ref().is_none_or(|scope| &row.scope == scope)
    }
}

/// Read `{ session = …, key = … }` from a registration's options table.
///
/// One function so every registration API reads the two options the same way
/// and refuses the same things. `api` names the caller in the messages.
///
/// Three refusals, all at registration:
///
/// 1. `session` on a hook that dispatches without one. Nothing would ever
///    fire, and [`HookName::carries_session`] knows which names those are.
/// 2. `session` where the host is in no session. There is nothing to resolve
///    the id against.
/// 3. `session` naming a session other than the one the host is in. The host
///    resolves the id; a caller never writes one it chose. See
///    [`crate::plugin_context::current_session`] for the reason.
pub fn scope_from_opts(
    lua: &Lua,
    api: &str,
    name: HookName,
    opts: &mlua::Table,
) -> LuaResult<(SessionScope, Option<String>)> {
    let key = string_option(api, opts, "key")?;
    // Read the option first, so `{ session = <a table> }` raises here rather
    // than passing the `carries_session` gate as an absent scope.
    let asked = string_option(api, opts, "session")?;
    if asked.is_some() && !name.carries_session() {
        return Err(mlua::Error::RuntimeError(format!(
            "{api}: `{name}` is dispatched without a session, so a \
             `session` scope on it could never fire"
        )));
    }
    match session_from_opts(lua, api, opts)? {
        Some(session) => Ok((SessionScope::Session(session), key)),
        None => Ok((SessionScope::Global, key)),
    }
}

/// Resolve `{ session = … }` to the session the caller is running in.
///
/// `None` when the option is absent. The two refusals here are the ones that
/// hold for every caller, so `cru.clear` reads the option through this
/// function and `scope_from_opts` adds only the registration-time gate that
/// needs a [`HookName`].
///
/// 1. `session` where the host is in no session. There is nothing to resolve
///    the id against.
/// 2. `session` naming a session other than the one the host is in. The host
///    resolves the id; a caller never writes one it chose. See
///    [`crate::plugin_context::current_session`] for the reason.
pub fn session_from_opts(lua: &Lua, api: &str, opts: &mlua::Table) -> LuaResult<Option<String>> {
    let Some(asked) = string_option(api, opts, "session")? else {
        return Ok(None);
    };
    let Some(current) = crate::plugin_context::current_session(lua) else {
        return Err(mlua::Error::RuntimeError(format!(
            "{api}: a `session` scope names the session this code is running \
             in, and nothing is running in one here. Register from \
             `cru.on_session_start` or from a handler, where the host holds \
             the session"
        )));
    };
    if asked != current {
        return Err(mlua::Error::RuntimeError(format!(
            "{api}: this code runs in session `{current}`, so it may not \
             name session `{asked}`"
        )));
    }
    Ok(Some(current))
}

/// One string option off a registration's table, or `None` when it is absent.
///
/// A value of the wrong type RAISES rather than reading as absent. That
/// matters most for `session`: an absent scope means every session, so
/// swallowing `{ session = session }` — the handle instead of its id, which
/// is the mistake an author will make — would silently widen a handler from
/// one session to all of them.
///
/// `pattern` is the same harm on the other axis. An absent pattern matches
/// every dispatch identifier, so `cru.on("pre_tool_call", { pattern = tool },
/// h)` where `tool` is a table would swallow to `None` and put the handler
/// against EVERY tool call in every session — on the one hook that fails
/// closed.
///
/// **Every option on every registration table is read through this function
/// or one of its two siblings**, [`integer_option`] and [`bool_option`].
/// Three readers, not one per option, and none of them is `.ok()`: a wrong
/// type is an author's mistake and it is reported where it was made.
pub(crate) fn string_option(
    api: &str,
    opts: &mlua::Table,
    field: &str,
) -> LuaResult<Option<String>> {
    match opts.get::<Value>(field) {
        Ok(Value::Nil) => Ok(None),
        Ok(Value::String(s)) => Ok(Some(s.to_str()?.to_string())),
        Ok(other) => Err(mlua::Error::RuntimeError(format!(
            "{api}: `{field}` must be a string, not a {}",
            other.type_name()
        ))),
        Err(e) => Err(e),
    }
}

/// One non-negative integer option, or `None` when it is absent.
///
/// See [`string_option`] for why the wrong type raises. Here the swallowed
/// value would drop a handler back to its hook's default time budget, so a
/// handler that asked for a long one would be cancelled mid-call with nothing
/// naming the reason.
pub(crate) fn integer_option(api: &str, opts: &mlua::Table, field: &str) -> LuaResult<Option<u64>> {
    let wrong = |found: &str| {
        Err(mlua::Error::RuntimeError(format!(
            "{api}: `{field}` must be a non-negative integer, not {found}"
        )))
    };
    match opts.get::<Value>(field) {
        Ok(Value::Nil) => Ok(None),
        Ok(Value::Integer(n)) => match u64::try_from(n) {
            Ok(n) => Ok(Some(n)),
            Err(_) => wrong(&format!("{n}")),
        },
        // Luau numbers are doubles, so `{ timeout_ms = 5000 }` can arrive as
        // one. An integral value is the same option; a fractional one is not.
        Ok(Value::Number(n)) if n >= 0.0 && n.fract() == 0.0 && n <= u64::MAX as f64 => {
            Ok(Some(n as u64))
        }
        Ok(other) => wrong(&format!("a {}", other.type_name())),
        Err(e) => Err(e),
    }
}

/// One boolean option, or `None` when it is absent.
///
/// See [`string_option`] for why the wrong type raises. `required` and `once`
/// are both "opt in to something unusual", so a swallowed value reads as the
/// ordinary case and the author sees a registration that succeeded.
pub(crate) fn bool_option(api: &str, opts: &mlua::Table, field: &str) -> LuaResult<Option<bool>> {
    match opts.get::<Value>(field) {
        Ok(Value::Nil) => Ok(None),
        Ok(Value::Boolean(b)) => Ok(Some(b)),
        Ok(other) => Err(mlua::Error::RuntimeError(format!(
            "{api}: `{field}` must be a boolean, not a {}",
            other.type_name()
        ))),
        Err(e) => Err(e),
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

    /// The row list, and the ONE poison policy: a poisoned lock is ENTERED.
    ///
    /// **The guarded value cannot be half-updated, so the poison flag carries
    /// nothing a reader needs.** Every mutation here is one `Vec` operation —
    /// a `push`, a `retain`, or a single index assignment — and a `Vec` that
    /// survives a panic mid-operation is still structurally sound. There is no
    /// multi-step invariant across two fields for a panic to break.
    ///
    /// Nine call sites read this lock and they used to answer FOUR ways: a
    /// `map_err` to a Lua error, a `.map(…).unwrap_or(0)`, an `.expect` panic,
    /// and a `let Ok(..) else { return 0 }`. The panic was the worst of them,
    /// and it was on `for_hook` — the `pre_tool_call` selection — so a poison
    /// that merely failed one registration would abort a turn the next time
    /// anything read the store. The swallows were the second worst: an empty
    /// answer from `for_hook` silently disables every guard registered on a
    /// hook that fails closed.
    ///
    /// Entering is therefore both the least destructive and the honest one.
    /// It is not "ignore an error": there is no error, because the state a
    /// panicking thread left behind is a valid `Vec` of valid rows.
    fn rows(&self) -> MutexGuard<'_, Vec<Registration>> {
        self.registrations
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    /// Store `handler` under `spec`, owned by whoever is running now.
    ///
    /// The source comes from the VM's own app data, never from an argument: a
    /// plugin must not be able to register under another plugin's name.
    ///
    /// The registration records no authority. Whether a `pre_tool_call`
    /// handler may take the call over is read at the seam that gates it, from
    /// the declaration the loader recorded for the source's plugin — see
    /// `crate::plugin_context`.
    ///
    /// Answers the id, which is the dispatch key.
    ///
    /// # A scoped registration REPLACES, and this is not optional
    ///
    /// A [`SessionScope::Session`] registration is keyed by
    /// `(source, name, pattern, scope, key)` and overwrites a row that carries
    /// the same key. A second registration of the same key is the SAME
    /// registration.
    ///
    /// `runtime/plugins/oci/init.luau` records the bug this closes, because
    /// it had to avoid the seam entirely to escape it: `on_session_start`
    /// fires on create, on resume AND on `resume_from_storage`, and a web
    /// history fetch calls `resume_from_storage` on every request, while
    /// `on_session_end` fires once. So "activation registers" would append one
    /// handler per history fetch, and the list has no unregister — leaving one
    /// stale copy per fetch, firing for the life of the daemon.
    ///
    /// Every part of the key carries weight. Without the source, two plugins
    /// registering `cru.on("pre_tool_call", { session = id }, h)` would
    /// silently overwrite each other. Without the `key`, one plugin could not
    /// register two handlers on one hook for one session.
    ///
    /// The `key` is the part with no Neovim counterpart, and it is the part a
    /// later step should retire. Neovim names no row: it keys its coarse
    /// delete on group, event and pattern (`autocmd.c:952`) and tells two
    /// otherwise identical rows apart by ALWAYS appending. Crucible cannot
    /// simply copy that, because a scoped row is registered from
    /// `on_session_start`, which fires again on every resume and on every
    /// `resume_from_storage` — so appending accumulates.
    ///
    /// What would retire `key` is a host-derived identity for the definition
    /// SITE, which is Neovim's third axis (`AutoCmd.script_ctx`) and the one
    /// Crucible does not yet record. Two activations of one line are the same
    /// registration; two different lines are two. Until something supplies
    /// that, `key` is the only axis separating two scoped rows on one hook
    /// and one pattern.
    ///
    /// A [`SessionScope::Global`] registration still APPENDS. Two identical unscoped
    /// registrations are two handlers, as they have always been: an unscoped
    /// registration is made once at load, so nothing accumulates, and
    /// collapsing them would change what every existing plugin does.
    pub fn register(&self, lua: &Lua, spec: RegistrationSpec, handler: Function) -> LuaResult<u64> {
        let source = current_source(lua);
        // The body is in hand before anything is pushed: nothing lands in the
        // list without a function, which `pre_tool_call` would otherwise turn
        // into a denied tool call.
        let body = Arc::new(lua.create_registry_value(handler)?);
        // `Relaxed` suffices: the list mutex taken below brackets the whole
        // allocate-then-push sequence, so it supplies the ordering.
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let registration = Registration {
            name: spec.name,
            source,
            id,
            pattern: spec.pattern,
            scope: spec.scope,
            key: spec.key,
            once: spec.once,
            timeout_ms: spec.timeout_ms,
            required: spec.required,
            body,
        };
        let mut rows = self.rows();
        match registration.replaces(&rows) {
            // In place, so a re-registration on every history fetch does not
            // churn the tie-break order of the rows around it.
            Some(index) => rows[index] = registration,
            None => rows.push(registration),
        }
        Ok(id)
    }

    /// How many registrations `plugin` holds.
    ///
    /// This is the count `plugin.list` reports. It used to come from the
    /// spec-table `handlers` field — which is parsed but never dispatched —
    /// so plugins using the real API showed 0 and plugins using the dead one
    /// showed a number that meant nothing.
    pub fn plugin_handler_count(&self, plugin: &str) -> usize {
        self.rows()
            .iter()
            .filter(|r| r.source.plugin_name() == Some(plugin))
            .count()
    }

    /// Every registration for `name` matching `identifier` and serving
    /// `firing`, in REGISTRATION ORDER.
    ///
    /// # Nothing reorders, and registration order is total
    ///
    /// There was a `priority` field, and it is gone. Neovim orders no
    /// autocommand — `nvim_create_autocmd` takes eight option fields and none
    /// of them ranks a handler — and an author who must run last writes into
    /// `after/` rather than negotiating a number with strangers. Composition
    /// is a plugin's concern; a store is not a scheduler.
    ///
    /// So the answer to "which handler runs first" is "the one that
    /// registered first", and three steps give that a total order:
    ///
    /// 1. `runtime/defaults/init.luau` runs first, as
    ///    [`LuaSource::Builtin`](crate::plugin_context::LuaSource::Builtin)
    ///    (`daemon_plugins::boot::load_shipped_defaults`).
    /// 2. Then the user's `init.lua`, as
    ///    [`LuaSource::UserLua`](crate::plugin_context::LuaSource::UserLua).
    ///    The boot inversion puts it BEFORE plugin loading.
    /// 3. Then the plugins, and `PluginManager::load_all`
    ///    (`crate::lifecycle::loading`) sorts its whole key set by name, so
    ///    they run alphabetically and the daemon executes them in that
    ///    order.
    ///
    /// **Step 3 is the mechanism, and it is the plugin NAME.** Neither the
    /// search-path rank nor `lifecycle::discovery` supplies it: the rank
    /// decides which plugin wins a name clash, and the directory sort only
    /// keeps `discover` itself repeatable. A plugin found on any root loads
    /// in the same place, which is the name's place.
    ///
    /// **This closure is the ONE place a scope is read.** Every fire path
    /// comes through here, the synchronous
    /// [`execute_permission_hooks`](super::permission::execute_permission_hooks)
    /// included — it calls this method and reads no scope of its own. A
    /// dispatch site never checks one either: it says which session it is in
    /// and gets the handlers for it. The pattern has worked this way since it
    /// was added, and the scope sits beside it for the same reason — a
    /// per-site check is a per-site chance to omit the check.
    pub fn for_hook(
        &self,
        name: HookName,
        identifier: Option<&str>,
        firing: Firing<'_>,
    ) -> Vec<Registration> {
        let rows = self.rows();
        // No sort. `registrations` is already in registration order, which is
        // the whole of the ordering contract — see this method's doc.
        rows.iter()
            .filter(|r| r.name == name && r.matches(identifier) && r.serves(firing))
            .cloned()
            .collect()
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
        firing: Firing<'_>,
    ) -> Vec<Registration> {
        match HookName::parse(event_type) {
            Some(name) => self.for_hook(name, identifier, firing),
            None => Vec::new(),
        }
    }

    /// Drop every registration `source` made, and release their bodies.
    ///
    /// Answers how many it removed, so a caller can log the change.
    pub fn clear_source(&self, source: &LuaSource) -> usize {
        let mut rows = self.rows();
        let before = rows.len();
        rows.retain(|r| &r.source != source);
        before - rows.len()
    }

    /// Drop every registration scoped to `session`, and release their bodies.
    ///
    /// **This is what makes activation-registers legal, not an optimisation.**
    /// A plugin turned on for a session registers a row for it, and the list
    /// has no unregister, so without this sweep every session that ever
    /// enabled a plugin leaves a row behind for the life of the daemon.
    ///
    /// [`SessionScope::Global`] rows are left alone: they belong to a plugin load, not
    /// to a session, and a reload clears them by source.
    ///
    /// A scope is a field, so a sweep is a `retain`. A per-session UNLOAD of
    /// the BODY is a different thing and stays unaffordable: a `RegistryKey`
    /// is valid only against the VM that made it, and the per-session VMs
    /// were deliberately deleted.
    ///
    /// Answers how many it removed.
    pub fn clear_session(&self, session: &str) -> usize {
        let mut rows = self.rows();
        let before = rows.len();
        rows.retain(|r| r.scope != SessionScope::Session(session.to_string()));
        before - rows.len()
    }

    /// Drop every registration of `source` that `filter` names.
    ///
    /// Answers how many it removed, which is what `cru.clear` returns.
    ///
    /// The filtered middle ground between [`Self::clear_source`], which takes
    /// all of one source's rows, and [`Self::clear_session`], which takes
    /// every source's rows for one session. `nvim_clear_autocmds` is the most
    /// used retirement call in Neovim's shipped runtime, and its dominant
    /// form — `{ group = g, buf = b }` — is exactly "my own rows, for this
    /// container".
    ///
    /// `source` is the CALLER, not a filter: a plugin clears its own rows and
    /// reaches no other plugin's. `clear_source` already matched on exactly
    /// this, so the gate is the one that was already here.
    pub fn clear_matching(&self, source: &LuaSource, filter: &ClearFilter) -> usize {
        let mut rows = self.rows();
        let before = rows.len();
        rows.retain(|row| !(&row.source == source && filter.selects(row)));
        before - rows.len()
    }

    /// Retire `registration` when it asked to run once. Answers whether it
    /// removed a row.
    ///
    /// # The row leaves the store BEFORE the body runs, not after
    ///
    /// Neovim retires a one-shot "in anticipation of its execution"
    /// (`autocmd.c:2240`) and marks the row dead before it invokes the body.
    /// Upstream #25526 records that order as a FIX, not a nicety: a nested
    /// dispatch of the same hook re-enters a one-shot that is still visible.
    /// A Crucible turn loop re-enters the same way — a `pre_tool_call`
    /// handler can run a tool — so the same order applies here.
    ///
    /// Neovim then has to restore the pattern pointer, because its row owns a
    /// refcounted `AutoPat`. Crucible needs no such dance: the body is an
    /// `Arc<RegistryKey>` the caller already holds, so the row can simply go
    /// while the clone in hand stays callable.
    ///
    /// A body that RAISES still retires the row. `once` says how many times
    /// the host CALLS a handler, not how many times the handler succeeds.
    fn retire_if_once(&self, registration: &Registration) -> bool {
        if !registration.once {
            return false;
        }
        let mut rows = self.rows();
        // By id, never by a held index: `register` may have replaced or
        // pushed rows since the dispatch snapshot, so an index would name
        // some other registration.
        let before = rows.len();
        rows.retain(|row| row.id != registration.id);
        before != rows.len()
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
        // The source recorded when the registration was made. A deferred call
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
        let handler: Function = registration.take_body(lua)?;

        let ctx_table = lua.create_table()?;
        if let Some(session) = session_id {
            ctx_table.set("session_id", session)?;
        }

        let previous = set_source(lua, registration.source.clone());
        // The session this dispatch belongs to, so a handler that registers
        // ANOTHER handler for the session it is running in resolves the id
        // from the host rather than naming one. See
        // `plugin_context::current_session`.
        let _session = enter_session(lua, session_id);
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
        // Restored before the `?`: an source left behind would attribute the
        // next registration to the wrong author.
        set_source(lua, previous);

        interpret_handler_result(&call?)
    }

    /// Every registration, in registration order.
    ///
    /// For a caller that wants the whole store rather than one name: a test
    /// counting what a load left behind, and the boot rollback check.
    pub fn all(&self) -> Vec<Registration> {
        self.rows().clone()
    }

    /// One registration by id, cloned out of the lock.
    pub fn by_id(&self, id: u64) -> Option<Registration> {
        self.rows().iter().find(|r| r.id == id).cloned()
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

    /// Whether this registration fires for `firing`.
    ///
    /// A [`SessionScope::Session`] registration does not fire for a dispatch that
    /// names no session. There is nothing to compare it against, and firing
    /// would be firing for every session at once — the harm the scope
    /// exists to stop. [`HookName::carries_session`] refuses the combination
    /// at registration, so this arm covers the sites that legitimately
    /// dispatch a session-carrying name with no session in hand.
    #[must_use]
    pub fn serves(&self, firing: Firing<'_>) -> bool {
        match (&self.scope, firing) {
            (SessionScope::Global, _) => true,
            (SessionScope::Session(wanted), Firing::InSession(id)) => wanted == id,
            (SessionScope::Session(_), Firing::Sessionless) => false,
        }
    }

    /// The index in `rows` this registration replaces, if any.
    ///
    /// `None` for a [`SessionScope::Global`] registration, which appends. See
    /// [`LuaScriptHandlerRegistry::register`] for why a scoped one replaces.
    fn replaces(&self, rows: &[Self]) -> Option<usize> {
        if self.scope == SessionScope::Global {
            return None;
        }
        rows.iter().position(|row| {
            row.source == self.source
                && row.name == self.name
                && row.pattern == self.pattern
                && row.scope == self.scope
                && row.key == self.key
        })
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

    /// The callable to run, and the retirement of a one-shot row.
    ///
    /// # This is the ONE choke point, and it is why there is no `body()`
    ///
    /// Four fire paths run a registration's body, and three of them do not go
    /// through `execute_handler_with_payload`: the
    /// synchronous permission gate
    /// ([`execute_permission_hooks`](super::permission::execute_permission_hooks)),
    /// the two session-lifecycle paths (`executor.rs`) and the provider-auth
    /// gate (`auth_plugin.rs`). Each selected a `Vec<Registration>` and
    /// loaded `body()` itself.
    ///
    /// So `once` gets no per-path retire call. The accessor that handed out
    /// the slot is GONE, and `body` is private, so the only way to obtain the
    /// callable is this method — a path cannot run a body without passing the
    /// retirement. That is the same fault, and the same fix, as the four
    /// partial clear paths that became one `clear_source`: a rule four callers
    /// must remember is a rule one of them will not.
    ///
    /// Two properties follow from loading the body HERE rather than at
    /// selection:
    ///
    /// - The permission gate and the auth gate answer on the FIRST hook that
    ///   decides, so a `once` row further down the list is selected and never
    ///   reached. It keeps its registration, because a row retires when its
    ///   body RUNS and not when it is selected.
    /// - `retrieval_stage::has_handlers` asks only whether a handler exists.
    ///   It loads no body, so it retires nothing.
    ///
    /// A row that fails to load retires nothing either: `once` counts the
    /// calls the host makes, and a callable it could not produce is no call.
    pub fn take_body(&self, lua: &Lua) -> LuaResult<Function> {
        let handler: Function = lua.registry_value(&self.body)?;
        if self.once {
            super::registry_of(lua)?.retire_if_once(self);
        }
        Ok(handler)
    }
}

/// The poison policy, tested where the policy lives.
///
/// Inline rather than in `handlers/tests/`, because poisoning a `Mutex`
/// requires panicking while the GUARD is held and `rows` is private. Every
/// public method drops the guard before it returns, which is exactly why a
/// poison here is so rare — and exactly why nothing noticed that the store
/// answered a poison four different ways.
#[cfg(test)]
mod poison {
    use super::*;
    use crate::handlers::{register_cru_on_api, StageId};

    /// A poisoned lock must not take a turn down, and must not silently
    /// answer "no handlers" either.
    ///
    /// `for_hook` is the `pre_tool_call` selection, so the `.expect` this
    /// replaces turned a poison that merely failed one registration into an
    /// aborted turn the next time anything read the store. A swallowed
    /// `Vec::new()` would be worse still: it disables every guard on a hook
    /// that fails closed, with nothing logged.
    #[test]
    fn every_reader_and_writer_still_works_on_a_poisoned_lock() {
        let lua = Lua::new();
        let registry = LuaScriptHandlerRegistry::new();
        register_cru_on_api(&lua, registry.clone()).expect("register cru.on");
        lua.load(r#"cru.on("pre_tool_call", function() end)"#)
            .exec()
            .expect("registers");

        // The only way a `Mutex` is poisoned: panic while the guard is live.
        let poisoner = registry.clone();
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = poisoner.rows();
            panic!("a path panicked while holding the registration lock");
        }));
        assert!(panicked.is_err(), "the poisoning panic must have happened");
        assert!(
            registry.registrations.is_poisoned(),
            "and it must have left the lock poisoned"
        );

        // Every reader keeps working, and keeps telling the truth.
        assert_eq!(
            registry
                .for_hook(
                    StageId::PreToolCall.into(),
                    Some("bash"),
                    crate::handlers::Firing::Sessionless
                )
                .len(),
            1,
            "`for_hook` must neither panic nor answer with an empty list"
        );
        assert_eq!(registry.all().len(), 1);
        assert!(registry.by_id(0).is_some());
        assert_eq!(registry.plugin_handler_count("nobody"), 0);

        // And a write still lands, so the store is not left read-only.
        lua.load(r#"cru.on("turn:complete", function() end)"#)
            .exec()
            .expect("registration must still succeed on a poisoned lock");
        assert_eq!(registry.all().len(), 2);
        assert_eq!(registry.clear_source(&LuaSource::UserLua), 2);
    }
}

impl Default for LuaScriptHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Remove every registration `source` made, wherever the host keeps one.
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
/// **"Registered" it makes true at once; "running" it makes true at the next
/// yield.** A schedule and a spawned task are the two things of an source's
/// that RUN, and neither can be interrupted mid-call: a tick already in its
/// callback finishes it, and a stretch of Luau that awaits nothing runs to its
/// end. See [`crate::schedule::cancel_source`] and
/// [`crate::timer::abort_source`], each of which states its own half. What this
/// call does guarantee is that no further body of `source` starts.
///
/// # Why the three bodies stay three, and are not one helper
///
/// They look alike from here — reach a store, take a `std::sync::Mutex`,
/// select by source equality, stop, count — and a reviewer reasonably reads
/// that as a pattern one step from a fourth copy. It is not one shape. Four
/// things differ, and each difference is the point of its own store:
///
/// - **What is stopped.** A registration needs no stop action: dropping the
///   row IS the stop. A schedule needs `tx.send(())` on a oneshot. A task
///   needs `JoinHandle::abort`.
/// - **The container and the element.** A `Vec<Registration>`, a
///   `HashMap<u64, (LuaSource, Sender)>` keyed by the handle
///   `cru.schedule.cancel` takes, and a `Vec<(LuaSource, JoinHandle)>` with no
///   key because `cru.timer.spawn` answers nothing a caller could name one
///   with.
/// - **Where the store lives.** This one is a field on the registry the host
///   already holds. The other two are VM app data under two different types,
///   so each has an "absent store" arm that this one cannot have.
/// - **Pruning, which is the divergence a review flagged.** `abort_source`
///   also drops OTHER sources' finished handles; the other two drop nothing
///   they did not select. That asymmetry is load-bearing rather than drift:
///   only the task store holds handles the RUNTIME may retire on its own, so
///   only it accumulates dead entries that nothing else would ever remove. A
///   schedule's canceller leaves its map when it is cancelled and by nothing
///   else, and a registration leaves this list when it is cleared.
///
/// A shared helper would have to be generic over the container, the element,
/// the owner projection, the stop action and the store lookup — five knobs
/// for three call sites, which buys nothing and hides each store's own
/// reason. So they stay three, and this comment is the thing that was
/// missing: the record of why.
///
/// Answers how many registrations it removed.
pub fn clear_source(lua: &Lua, registry: &LuaScriptHandlerRegistry, source: &LuaSource) -> usize {
    let dropped = registry.clear_source(source);
    let schedules = crate::schedule::cancel_source(lua, source);
    let tasks = crate::timer::abort_source(lua, source);
    let total = dropped + schedules + tasks;
    if total > 0 {
        tracing::debug!(
            %source,
            dropped,
            schedules,
            tasks,
            "cleared source registrations"
        );
    }
    total
}
