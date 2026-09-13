//! Two live sessions on one daemon VM.
//!
//! Every session shares one VM, so a handler that a plugin scopes to the
//! session it runs in must fire for that session and no other. The registry
//! tests in `crucible-lua` prove the filter. This test proves it across the
//! daemon's own start, turn and end paths.

use super::*;

/// One daemon VM serves every session. A handler a plugin scopes to the
/// session it is running in fires for that session and no other, and ending
/// that session drops its rows and no other's.
#[tokio::test]
async fn a_session_scoped_handler_fires_for_its_session_only() {
    let mut h = ReactorTestHarness::new().await;
    let loader = h.load_daemon_lua(
        r#"
        cru.on_session_start(function(session)
            cru.on("turn:complete", { session = session.id, key = "probe" }, function(ctx, event)
                _G.fired = _G.fired or {}
                table.insert(_G.fired, ctx.session_id)
            end)
        end)
    "#,
    );
    h.attach_lifecycle(loader);

    let a = h.new_session().await;
    let b = h.new_session().await;
    assert_eq!(
        h.registrations_scoped_to(&a),
        1,
        "A's start hook registered one row"
    );
    assert_eq!(
        h.registrations_scoped_to(&b),
        1,
        "B's start hook registered one row"
    );

    h.send_on(&a, "hello").await;
    let fired: Vec<String> = h.lua_eval("return _G.fired or {}");
    assert_eq!(fired, vec![a.clone()], "only A's handler ran for A's turn");

    h.end_session(&a).await;
    assert_eq!(h.registrations_scoped_to(&a), 0, "ending A swept A's rows");
    assert_eq!(
        h.registrations_scoped_to(&b),
        1,
        "ending A left B's row alone"
    );

    h.send_on(&b, "hello").await;
    let fired: Vec<String> = h.lua_eval("return _G.fired or {}");
    assert_eq!(fired, vec![a, b.clone()], "B's turn ran B's handler once");
    assert_eq!(h.registrations_scoped_to(&b), 1);
}
