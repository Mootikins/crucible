//! Forwarding syntax owns cloning/boxing, never RPC argument or result semantics.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ReplayPolicy {
    Safe,
    Once,
}

// Every row explicitly decides whether an ambiguous failure permits replay.
// Wire labels come from the daemon's RpcMethod, not a second string catalog.
macro_rules! forward_rpc {
    (
        $(#[$attr:meta])*
        $policy:ident $method:ident =>
        $name:ident($($arg:ident: $ty:ty $(=> $owned:expr)?),* $(,)?)
        -> $ret:ty = $client_method:ident($($value:expr),* $(,)?);
    ) => {
        $(#[$attr])*
        pub async fn $name(&self, $($arg: $ty),*) -> anyhow::Result<$ret> {
            $(let $arg = forward_rpc!(@own $arg $(, $owned)?);)*
            self.forward_rpc(
                $crate::services::forwarding::ReplayPolicy::$policy,
                crucible_daemon::rpc::RpcMethod::$method,
                move |daemon| {
                    $(let $arg = $arg.to_owned();)*
                    Box::pin(async move { daemon.$client_method($($value),*).await })
                },
            ).await
        }
    };
    (@own $arg:ident, $owned:expr) => { $owned };
    (@own $arg:ident) => { $arg.to_owned() };
}
