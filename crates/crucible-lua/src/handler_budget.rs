//! A wall-clock budget for one Lua handler call.
//!
//! # Why a VM hook and not only a timeout
//!
//! A handler used to run with no budget at all. `tokio::time::timeout` around
//! the call covers a handler that AWAITS — it sleeps, it calls `cru.http`, it
//! shells out — because the future is cancelled at an await point. It covers
//! nothing else: `while true do end` never yields, so the tokio timer never
//! gets the runtime back, the worker thread stays captive, and every other
//! plugin on that VM queues behind it. The synchronous permission path has no
//! await point at all, so a timeout there is inert by construction.
//!
//! This module is the other half. Lua's own instruction hook runs every
//! [`CHECK_EVERY_N_INSTRUCTIONS`] VM instructions, asks the clock, and raises
//! when the deadline has passed. It interrupts mid-execution, on both paths,
//! and it needs no cooperation from the handler.
//!
//! # What it does not cover
//!
//! Two residual blind spots, stated rather than hidden:
//!
//! 1. **A hook blocked inside a C call.** The instruction hook runs between
//!    Lua instructions, so a call that blocks in Rust or in a C library — a
//!    synchronous socket read, say — is not interrupted until it returns. The
//!    timer this replaces had exactly the same hole.
//! 2. **A handler that swallows the error.** The deadline arrives as an
//!    ordinary Lua error, so a body wrapped in `pcall` can catch it. It is
//!    raised again after the next [`CHECK_EVERY_N_INSTRUCTIONS`], so the
//!    handler makes almost no progress, but it does not stop.
//!
//! # Nesting and interleaving
//!
//! The deadline is one slot per VM. [`enter`] returns the previous value and
//! [`BudgetGuard`] restores it, so nesting is correct. Two handler calls
//! interleaved on one shared VM — two sessions dispatching into the plugin VM
//! — can still see each other's deadline, so [`enter`] keeps whichever is
//! EARLIER. Interleaving can then only make a budget stricter. It can never
//! leave a handler running unbounded, which is the property that matters.

use std::time::{Duration, Instant};

#[cfg(not(feature = "luau"))]
use mlua::HookTriggers;
use mlua::{Lua, VmState};

/// How often the VM asks the clock.
///
/// Small enough that a 1 s permission budget is honoured closely, large enough
/// that the check is not measurable against ordinary handler work.
pub const CHECK_EVERY_N_INSTRUCTIONS: u32 = 10_000;

/// The budget for a turn-loop stage.
///
/// Matches the turn loop's own tool-dispatch timeout, so a handler cannot be
/// the reason a dispatch outlives it.
pub const TURN_STAGE_BUDGET: Duration = Duration::from_secs(30);

/// The budget for `session_start` and `session_end`.
///
/// Deliberately long: the `oci` plugin pulls container images in
/// `on_session_start`, and a short default would break a shipped plugin.
pub const LIFECYCLE_BUDGET: Duration = Duration::from_secs(120);

/// The budget for a permission hook.
///
/// One second, which is what the timer this replaces intended. A permission
/// decision runs while a person waits for a prompt, and it may not call async
/// APIs, so there is nothing legitimate for it to be slow about.
pub const PERMISSION_BUDGET: Duration = Duration::from_secs(1);

/// The deadline in force on a VM. A newtype so the `Option` is the whole
/// stored value: `remove_app_data` cannot express "present but empty".
struct CurrentDeadline(Option<Deadline>);

/// When the running Lua must stop, and what to call it in the error.
#[derive(Clone)]
struct Deadline {
    at: Instant,
    budget: Duration,
    label: String,
}

/// Install the execution interrupt that enforces deadlines on this VM.
///
/// Idempotent, and cheap while no deadline is set: the hook reads one app-data
/// slot and returns. Every VM that runs a handler needs it — the plugin VM and
/// each session VM — because the deadline is a property of the VM the handler
/// runs in.
///
/// Luau's interrupt callback reaches loops and function calls in every
/// execution thread. PUC Lua needs a global instruction hook because an async
/// handler runs in an mlua-created coroutine.
pub fn install_deadline_hook(lua: &Lua) -> mlua::Result<()> {
    #[cfg(feature = "luau")]
    {
        lua.set_interrupt(deadline_state);
        Ok(())
    }

    #[cfg(not(feature = "luau"))]
    lua.set_global_hook(
        HookTriggers::new().every_nth_instruction(CHECK_EVERY_N_INSTRUCTIONS),
        |lua, _debug| deadline_state(lua),
    )
}

fn deadline_state(lua: &Lua) -> mlua::Result<VmState> {
    // Cloned out, and the borrow dropped, before anything else runs: an error
    // raised while the app-data borrow is live would block a guard's restore.
    let expired = {
        let slot = lua.app_data_ref::<CurrentDeadline>();
        slot.and_then(|slot| slot.0.clone())
            .filter(|deadline| Instant::now() >= deadline.at)
    };
    match expired {
        Some(deadline) => Err(mlua::Error::runtime(deadline.message(lua))),
        None => Ok(VmState::Continue),
    }
}

impl Deadline {
    /// The error text, attributed to the plugin whose handler is running.
    fn message(&self, lua: &Lua) -> String {
        let what = &self.label;
        let ms = self.budget.as_millis();
        match crate::plugin_context::current_plugin_name(lua) {
            Some(plugin) => {
                format!("[{plugin}] {what} exceeded its {ms} ms time budget and was stopped")
            }
            None => format!("{what} exceeded its {ms} ms time budget and was stopped"),
        }
    }
}

/// Restores the deadline that was in force before [`enter`].
///
/// Held by the caller for exactly as long as the handler runs. `Drop` and not
/// an explicit call, so an error path cannot leave a stale deadline behind for
/// whatever runs next.
#[must_use = "the budget ends when the guard is dropped"]
pub struct BudgetGuard<'lua> {
    lua: &'lua Lua,
    previous: Option<Deadline>,
}

impl Drop for BudgetGuard<'_> {
    fn drop(&mut self) {
        self.lua.set_app_data(CurrentDeadline(self.previous.take()));
    }
}

/// Give the Lua that runs next `budget` of wall-clock time.
///
/// `label` names what is being budgeted, for the error a handler that overruns
/// receives — "the `pre_tool_call` handler", "the permission hook".
pub fn enter<'lua>(
    lua: &'lua Lua,
    budget: Duration,
    label: impl Into<String>,
) -> BudgetGuard<'lua> {
    let deadline = Deadline {
        at: Instant::now() + budget,
        budget,
        label: label.into(),
    };
    let previous = lua
        .set_app_data(CurrentDeadline(Some(deadline.clone())))
        .and_then(|slot: CurrentDeadline| slot.0);
    // The earlier of the two wins, so an interleaved call on a shared VM can
    // only tighten the budget in force.
    if let Some(outer) = previous.clone().filter(|outer| outer.at < deadline.at) {
        lua.set_app_data(CurrentDeadline(Some(outer)));
    }
    BudgetGuard { lua, previous }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vm() -> Lua {
        let lua = Lua::new();
        install_deadline_hook(&lua).expect("install the hook");
        lua
    }

    /// The case no timeout can reach: Lua that never yields.
    #[test]
    fn a_cpu_bound_chunk_is_stopped_at_its_deadline() {
        let lua = vm();
        let _guard = enter(&lua, Duration::from_millis(200), "the test chunk");

        let started = Instant::now();
        let error = lua
            .load("while true do end")
            .exec()
            .expect_err("a spinning chunk must be stopped");

        assert!(
            error.to_string().contains("time budget"),
            "the error must say what happened: {error}"
        );
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the deadline must interrupt, not measure afterwards"
        );
    }

    /// The error names the plugin, so an operator knows whom to uninstall.
    #[test]
    fn the_error_names_the_plugin_whose_handler_overran() {
        let lua = vm();
        crate::plugin_context::enter_plugin(&lua, "grabby", crate::manifest::CapabilitySet::none());
        let _guard = enter(&lua, Duration::from_millis(50), "the pre_tool_call handler");

        let error = lua
            .load("while true do end")
            .exec()
            .expect_err("a spinning chunk must be stopped");
        assert!(
            error.to_string().contains("[grabby]"),
            "the error must name the plugin: {error}"
        );
    }

    /// No deadline in force is the ordinary state, and it costs nothing.
    #[test]
    fn a_chunk_with_no_deadline_runs_to_completion() {
        let lua = vm();
        let sum: i64 = lua
            .load("local s = 0 for i = 1, 200000 do s = s + i end return s")
            .eval()
            .expect("a chunk with no deadline must finish");
        assert_eq!(sum, 20_000_100_000);
    }

    /// The selected backend accepts Luau's gradual-type syntax. Runtime
    /// execution deliberately erases the annotation; `luau-analyze` remains
    /// the separate gate that proves a declared type is correct.
    #[cfg(feature = "luau")]
    #[test]
    fn a_strict_luau_chunk_with_type_annotations_executes() {
        let lua = vm();
        let total: i64 = lua
            .load(
                r#"
                --!strict
                type Totals = { left: number, right: number }
                local function add(values: Totals): number
                    return values.left + values.right
                end
                return add({ left = 20, right = 22 })
                "#,
            )
            .eval()
            .expect("Luau annotations must parse in the plugin VM");
        assert_eq!(total, 42);
    }

    /// The guard restores what it replaced, so one overrun does not poison the
    /// next call on the same VM.
    #[test]
    fn the_deadline_is_gone_once_the_guard_drops() {
        let lua = vm();
        {
            let _guard = enter(&lua, Duration::from_millis(50), "the first handler");
            let _ = lua.load("while true do end").exec();
        }
        lua.load("local s = 0 for i = 1, 200000 do s = s + i end")
            .exec()
            .expect("the next call must not inherit a spent deadline");
    }

    /// Interleaved calls on one VM keep the EARLIER deadline, so a long budget
    /// entered second cannot loosen a short one already in force.
    #[test]
    fn the_earlier_deadline_wins() {
        let lua = vm();
        let _outer = enter(&lua, Duration::from_millis(100), "the short handler");
        let _inner = enter(&lua, Duration::from_secs(60), "the long handler");

        let started = Instant::now();
        lua.load("while true do end")
            .exec()
            .expect_err("the short deadline must still stop this");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the later deadline replaced the earlier one"
        );
    }
}
