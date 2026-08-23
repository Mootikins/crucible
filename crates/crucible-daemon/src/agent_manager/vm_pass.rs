//! One pass over the two Lua VMs a turn can reach.
//!
//! Every turn-loop stage runs its handlers in the same order: the session
//! VM first, then the plugin VM. The two VMs keep separate registries
//! because a `RegistryKey` is only valid against the `Lua` state that made
//! it. The pass over the session VM holds the session-state lock. The pass
//! over the plugin VM runs with that lock released, because plugin Lua can
//! call `cru.shell` or `cru.http` for seconds, and the lock would starve
//! every other operation on the session.
//!
//! Each stage supplies one closure and an accumulator. The closure returns
//! `ControlFlow::Break` to stop after the session VM (first result wins),
//! or `ControlFlow::Continue` to carry the accumulator into the plugin VM.
//!
//! The closure gets owned handles, because both are cheap `Arc` clones, and
//! it returns a boxed future. An `AsyncFnMut` bound cannot promise `Send`
//! for every call lifetime, so a `tokio::spawn` around the caller rejects
//! it; a `BoxFuture` carries the `Send` bound itself.

use std::fmt;
use std::ops::ControlFlow;
use std::sync::Arc;

use crucible_lua::LuaScriptHandlerRegistry;
use futures::future::BoxFuture;
use mlua::Lua;
use tokio::sync::Mutex;

use super::SessionEventState;

/// A plugin registry with the `Lua` state it belongs to.
pub(crate) type PluginHandlers = (Arc<LuaScriptHandlerRegistry>, Arc<Lua>);

/// One stage's pass over a VM. `T` flows in as the accumulator and out as
/// the result.
pub(crate) trait VmPass<'a, T>:
    FnMut(Vm, LuaScriptHandlerRegistry, Lua, T) -> BoxFuture<'a, ControlFlow<T, T>>
{
}

impl<'a, T, F> VmPass<'a, T> for F where
    F: FnMut(Vm, LuaScriptHandlerRegistry, Lua, T) -> BoxFuture<'a, ControlFlow<T, T>>
{
}

/// Which VM a pass is running against. Logs name it so a handler error
/// points at the right registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Vm {
    Session,
    Plugin,
}

impl fmt::Display for Vm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Vm::Session => "session",
            Vm::Plugin => "plugin",
        })
    }
}

/// Run `pass` against the session VM under the state lock, then against
/// the plugin VM with the lock released.
///
/// `Break` after the session pass returns its value without a plugin pass.
pub(crate) async fn fold_vms<'a, T>(
    session_state: &Mutex<SessionEventState>,
    plugin_handlers: Option<&PluginHandlers>,
    init: T,
    mut pass: impl VmPass<'a, T>,
) -> T {
    let acc = {
        let state = session_state.lock().await;
        match pass(Vm::Session, state.registry.clone(), state.lua.clone(), init).await {
            ControlFlow::Break(done) => return done,
            ControlFlow::Continue(acc) => acc,
        }
    };
    plugin_pass(plugin_handlers, acc, &mut pass).await
}

/// The same two passes over a session state the caller already locked.
///
/// Only for stages whose handlers run pre-turn and return quickly, where the
/// caller keeps one lock across both passes on purpose.
pub(crate) async fn fold_vms_locked<'a, T>(
    state: &SessionEventState,
    plugin_handlers: Option<&PluginHandlers>,
    init: T,
    mut pass: impl VmPass<'a, T>,
) -> T {
    let acc = match pass(Vm::Session, state.registry.clone(), state.lua.clone(), init).await {
        ControlFlow::Break(done) => return done,
        ControlFlow::Continue(acc) => acc,
    };
    plugin_pass(plugin_handlers, acc, &mut pass).await
}

async fn plugin_pass<'a, T>(
    plugin_handlers: Option<&PluginHandlers>,
    acc: T,
    pass: &mut impl VmPass<'a, T>,
) -> T {
    match plugin_handlers {
        Some((registry, lua)) => {
            match pass(Vm::Plugin, (**registry).clone(), (**lua).clone(), acc).await {
                ControlFlow::Break(done) | ControlFlow::Continue(done) => done,
            }
        }
        None => acc,
    }
}
