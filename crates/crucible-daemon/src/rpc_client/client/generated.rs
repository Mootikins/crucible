//! One typed method per `rpc_methods!` row, bound to that row's own
//! params/reply types.
//!
//! [`DaemonClient::call`] lets a caller name any `Req`/`Resp` it likes for a
//! given [`RpcMethod`] — that is gap 1 of step 19 (see
//! `docs/Meta/Architecture/Simplification Plan.md`): a call site that
//! disagrees with its row's declared types still compiles. The methods
//! generated here close it for a call site that switches to one: each
//! method's signature IS its row's `Req`/`Resp` pair, generated from the same
//! [`crucible_core::for_each_rpc_method!`] callback the row itself feeds, so
//! the method and the row cannot drift apart — a caller that hands the wrong
//! params type, or reads the reply as the wrong type, fails to compile here
//! rather than at a runtime `serde_json::from_value`.
//!
//! `crucible-core` cannot generate these methods itself — it cannot name
//! [`DaemonClient`], since the daemon depends on core, not the reverse. This
//! module is the seam: `for_each_rpc_method!` hands every row to
//! [`gen_rpc_methods`], a `macro_rules!` callback in this crate that expands
//! them into one inherent method per row.
//!
//! Each method is named `rpc_<variant in snake_case>`
//! (`SessionGet` → `rpc_session_get`) — the row's wire name with its dots
//! turned to underscores, prefixed. The prefix, not a facade type
//! (`client.rpc().session_get(...)`) or a curated name clash with an
//! existing hand-written method, is what keeps the struct/enum count at
//! zero growth (`rg -c -t rust '^\s*(pub(\([a-z:]+\))? )?(struct|enum)
//! [A-Z]' crates/*/src`, per "How a step is accepted" in the plan): a row's
//! generated method exists unconditionally, with no per-row curation to
//! keep in sync, and a hand-written method that turns an ergonomic
//! argument list into a wire body (`kiln_forget(name: &str)`, and the
//! like) keeps its own name free.

use super::DaemonClient;
use anyhow::Result;
use crucible_core::protocol::RpcMethod;

/// The callback [`crucible_core::for_each_rpc_method!`] invokes with every
/// row, as `Variant, "wire.name", ReqTy, RespTy;` repeated. Expands to one
/// `impl DaemonClient` block holding one method per row.
macro_rules! gen_rpc_methods {
    ($( $variant:ident, $name:literal, $req:ty, $resp:ty ; )*) => {
        impl DaemonClient {
            ::pastey::paste! {
                $(
                    #[doc = concat!(
                        "The `", $name, "` row of `rpc_methods!`: `",
                        stringify!($req), "` in, `", stringify!($resp), "` out."
                    )]
                    pub async fn [<rpc_ $variant:snake>](&self, params: $req) -> Result<$resp> {
                        self.call(RpcMethod::$variant, params).await
                    }
                )*
            }
        }
    };
}

crucible_core::for_each_rpc_method!(gen_rpc_methods);
