//! The session that a Lua VM executes for, and `cru.session.current`.

use super::Session;
use crate::error::LuaError;
use mlua::{Lua, Value};
use std::sync::{Arc, Mutex};

/// The one session a Lua VM is currently executing for.
///
/// It was called `SessionManager`, which it is not: it holds an
/// `Option<Session>` and has `set`, `get` and `clear`. It manages nothing, and
/// it shared a name with the daemon's real `SessionManager` — 20 files, session
/// creation, resume, child sessions, storage — and with an ACP trait in
/// `crucible-core`. The three had no method in common.
///
/// TODO: Add config hierarchy (TOML < global Lua < session Lua) once
/// kiln/workspace/session/project nomenclature is clarified.
#[derive(Clone)]
pub struct CurrentSession {
    current: Arc<Mutex<Option<Session>>>,
}

impl CurrentSession {
    pub fn new() -> Self {
        Self {
            current: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_current(&self, session: Session) {
        *self
            .current
            .lock()
            .expect("current_session: poisoned while setting current session") = Some(session);
    }

    pub fn get_current(&self) -> Option<Session> {
        self.current.lock().ok()?.clone()
    }
}

impl Default for CurrentSession {
    fn default() -> Self {
        Self::new()
    }
}

/// The handle of the bound session, or an error when no session is bound.
///
/// `cru.session.current` and `cru.get_session` both call this function, so
/// the two names give the same answer.
fn current_handle(lua: &Lua, current: &CurrentSession) -> mlua::Result<Value> {
    let session = current
        .get_current()
        .ok_or_else(|| mlua::Error::runtime("No active session"))?;
    Ok(Value::UserData(lua.create_userdata(session)?))
}

/// Register `cru.session.current`, `cru.get_session` and the `cru.sessions`
/// alias. The two getters read the holder that this function returns.
pub fn register_session_module(lua: &Lua) -> Result<CurrentSession, LuaError> {
    let manager = CurrentSession::new();
    let cru = crate::lua_util::get_or_create_namespace(lua, "cru")?;

    // `current` is registered here, not with the lifecycle module, because the
    // closure must close over *this* CurrentSession — the same instance the
    // daemon later binds a session into. The lifecycle registrations merge
    // their functions into the same table, so whichever runs first, both
    // surfaces end up on `cru.session`.
    let session_mod = crate::lua_util::get_or_create_module(lua, "session")?;
    let mut session = crate::host_registry::Ns::over(lua, "cru.session", session_mod);

    // It takes NO arguments, and it RAISES when no session is bound — it does
    // not answer nil, and it does not answer the `(value, err)` pair the rest
    // of `cru.session` uses. The handle is userdata, which a declaration has
    // no name for, so the return says `any`; `cru.session.get(id)` says the
    // same for the same reason.
    let mgr = manager.clone();
    session.func("current", "() -> any", move |lua, ()| {
        current_handle(lua, &mgr)
    })?;

    // Deprecated spelling of `current`, kept for one release. Zero production
    // callers when it was deprecated; it stays only so a user's local plugin
    // keeps working. Same declaration, because it is the same closure under
    // an older name.
    let mgr = manager.clone();
    let mut root = crate::host_registry::Ns::over(lua, "cru", cru);
    root.func("get_session", "() -> any", move |lua, ()| {
        current_handle(lua, &mgr)
    })?;

    crate::lua_util::install_sessions_alias(lua)?;

    Ok(manager)
}
