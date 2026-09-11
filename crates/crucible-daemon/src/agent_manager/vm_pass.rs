//! One pass over the VM a turn can reach.
//!
//! Every turn-loop stage runs its handlers the same way: against the daemon
//! VM, which is the only VM that runs Lua files. It used to be two passes —
//! a per-session VM first, then the plugin VM, because a `RegistryKey` is only
//! valid against the `Lua` state that made it, and each session had its own.
//! Sessions no longer do.
//!
//! Each stage supplies one closure and an accumulator. `ControlFlow` survives
//! the collapse because a stage still stops on the first decisive answer; with
//! one VM, `Break` and `Continue` differ only in whether the value needs
//! `Into<B>`.
//!
//! The closure gets owned handles, because both are cheap `Arc` clones, and
//! it returns a boxed future. An `AsyncFnMut` bound cannot promise `Send`
//! for every call lifetime, so a `tokio::spawn` around the caller rejects
//! it; a `BoxFuture` carries the `Send` bound itself.

use std::ops::ControlFlow;
use std::sync::Arc;

use crucible_lua::LuaScriptHandlerRegistry;
use futures::future::BoxFuture;
use mlua::Lua;

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

/// Run `pass` against the handler VM, if one is bound.
///
/// `None` is a manager with no daemon VM — every handler-free test — and
/// yields the accumulator untouched.
pub(crate) async fn run_handlers<'a, T: Into<B>, B>(
    plugin_handlers: Option<&PluginHandlers>,
    init: T,
    mut pass: impl FnMut(LuaScriptHandlerRegistry, Lua, T) -> BoxFuture<'a, ControlFlow<B, T>>,
) -> B {
    match plugin_handlers {
        Some((registry, lua)) => match pass((**registry).clone(), (**lua).clone(), init).await {
            ControlFlow::Break(done) => done,
            ControlFlow::Continue(done) => done.into(),
        },
        None => init.into(),
    }
}
