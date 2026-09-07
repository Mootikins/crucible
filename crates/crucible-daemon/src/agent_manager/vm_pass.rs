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
//! The break value can have its own type `B`, so a stage that cancels can
//! return `None` while its accumulator stays a plain value. The fold
//! converts the accumulator with `Into<B>` when no pass breaks.
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

/// The daemon VM's permission hooks, their bodies, and the state those bodies
/// live in.
///
/// The `Lua` travels with the registry rather than being taken from
/// `plugin_lua`: that handle is bound with the VALIDATOR registry, so reading
/// it here made permission dispatch depend on whether validators happened to
/// be wired.
pub type DaemonPermissions = (
    Arc<std::sync::Mutex<Vec<crucible_lua::PermissionHook>>>,
    Arc<std::sync::Mutex<std::collections::HashMap<String, mlua::RegistryKey>>>,
    Arc<Lua>,
);

/// One stage's pass over a VM. `T` flows in as the accumulator; `B` is the
/// early result of a `Break`.
pub(crate) trait VmPass<'a, T, B>:
    FnMut(Vm, LuaScriptHandlerRegistry, Lua, T) -> BoxFuture<'a, ControlFlow<B, T>>
{
}

impl<'a, T, B, F> VmPass<'a, T, B> for F where
    F: FnMut(Vm, LuaScriptHandlerRegistry, Lua, T) -> BoxFuture<'a, ControlFlow<B, T>>
{
}

/// Which VM a pass is running against.
///
/// One variant: every Lua file runs on the daemon VM. Kept as a type rather
/// than dropped from the pass signature because a second VM is a design
/// change, and this is where it would have to be named.
#[derive(Clone, Copy)]
pub(crate) enum Vm {
    Plugin,
}

impl fmt::Display for Vm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Vm::Plugin => "plugin",
        })
    }
}

/// Run `pass` against the session VM under the state lock, then against
/// the plugin VM with the lock released.
///
/// `Break` after the session pass returns its value without a plugin pass.
pub(crate) async fn fold_vms<'a, T: Into<B>, B>(
    session_state: &Mutex<SessionEventState>,
    plugin_handlers: Option<&PluginHandlers>,
    init: T,
    mut pass: impl VmPass<'a, T, B>,
) -> B {
    let _ = session_state;
    plugin_pass(plugin_handlers, init, &mut pass).await
}

/// The same two passes over a session state the caller already locked.
///
/// Only for stages whose handlers run pre-turn and return quickly, where the
/// caller keeps one lock across both passes on purpose.
pub(crate) async fn fold_vms_locked<'a, T: Into<B>, B>(
    state: &SessionEventState,
    plugin_handlers: Option<&PluginHandlers>,
    init: T,
    mut pass: impl VmPass<'a, T, B>,
) -> B {
    let _ = state;
    plugin_pass(plugin_handlers, init, &mut pass).await
}

async fn plugin_pass<'a, T: Into<B>, B>(
    plugin_handlers: Option<&PluginHandlers>,
    acc: T,
    pass: &mut impl VmPass<'a, T, B>,
) -> B {
    match plugin_handlers {
        Some((registry, lua)) => {
            match pass(Vm::Plugin, (**registry).clone(), (**lua).clone(), acc).await {
                ControlFlow::Break(done) => done,
                ControlFlow::Continue(done) => done.into(),
            }
        }
        None => acc.into(),
    }
}
