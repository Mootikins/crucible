use super::MockDaemonApi;
use crate::session_api::Session;
use crate::sessions::DaemonSessionApi;
use crate::test_support::TestLuaBuilder;
use mlua::Value;
use std::sync::Arc;

/// A VM with the daemon-backed session module registered against a current
/// session holder, mirroring how the daemon wires a session VM.
fn delegate_vm(api: Arc<dyn DaemonSessionApi>) -> (mlua::Lua, crate::session_api::CurrentSession) {
    let (lua, current) = TestLuaBuilder::new().build_with_current_session();
    crate::sessions::register_sessions_module_with_api_and_current(&lua, api, current.clone())
        .expect("register sessions module with current");
    (lua, current)
}

/// `delegate = true` stamps the parent from the VM's current session — the
/// daemon-side binding, not anything the caller wrote — and the `delegate`
/// key itself never crosses the trait boundary.
#[tokio::test]
async fn a_delegate_create_stamps_the_parent_from_the_current_session() {
    let mock = Arc::new(MockDaemonApi::new());
    let (lua, current) = delegate_vm(Arc::clone(&mock) as Arc<dyn DaemonSessionApi>);
    current.set_current(Session::new("parent-1".to_string()));

    let err: Value = lua
        .load(
            r#"
            local job, err = cru.session.create({
                delegate = true,
                prompt = "summarize the kiln",
                target = "researcher",
            })
            assert(err == nil, "unexpected error: " .. tostring(err))
            return err
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(matches!(err, Value::Nil));
    let params = mock.last_create_params().expect("create reached the api");
    assert_eq!(
        params.get("parent_session_id").and_then(|v| v.as_str()),
        Some("parent-1"),
        "parentage is the daemon's stamp"
    );
    assert!(
        params.get("delegate").is_none(),
        "the delegate switch is consumed, not forwarded"
    );
}

/// A caller-supplied `parent_session_id` never reaches the daemon — on a
/// plain create it is stripped, so borrowing another session's delegation
/// allowlist is not sayable from Lua.
#[tokio::test]
async fn a_forged_parent_session_id_never_reaches_the_daemon() {
    let mock = Arc::new(MockDaemonApi::new());
    let (lua, _current) = delegate_vm(Arc::clone(&mock) as Arc<dyn DaemonSessionApi>);

    let _: Value = lua
        .load(
            r#"
            local s, err = cru.session.create({
                type = "chat",
                parent_session_id = "someone-elses-session",
            })
            assert(err == nil, "unexpected error: " .. tostring(err))
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    let params = mock.last_create_params().expect("create reached the api");
    assert!(
        params.get("parent_session_id").is_none(),
        "a forged parent must be stripped before the boundary: {params}"
    );
}

/// `delegate = true` with no session bound to the VM is an honest refusal,
/// not a delegation attributed to nobody.
#[tokio::test]
async fn delegate_without_a_current_session_is_refused() {
    let mock = Arc::new(MockDaemonApi::new());
    let (lua, _current) = delegate_vm(Arc::clone(&mock) as Arc<dyn DaemonSessionApi>);

    let err: String = lua
        .load(
            r#"
            local job, err = cru.session.create({ delegate = true, prompt = "x" })
            return err
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(
        err.contains("requires a current session"),
        "the refusal should say what is missing: {err}"
    );
    assert!(
        mock.last_create_params().is_none(),
        "a refused delegate must not reach the daemon as a create"
    );
}

/// An ambiguous value on the delegation switch is a type error, not a quiet
/// default — the caller meant something and must say it plainly.
#[tokio::test]
async fn a_non_boolean_delegate_is_refused() {
    let mock = Arc::new(MockDaemonApi::new());
    let (lua, current) = delegate_vm(Arc::clone(&mock) as Arc<dyn DaemonSessionApi>);
    current.set_current(Session::new("parent-1".to_string()));

    let err: String = lua
        .load(
            r#"
            local job, err = cru.session.create({ delegate = "yes", prompt = "x" })
            return err
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(
        err.contains("delegate must be a boolean"),
        "the refusal should name the switch: {err}"
    );
    assert!(mock.last_create_params().is_none());
}

/// The un-bound registration — the plugin-VM shape — refuses a delegate the
/// same way rather than forwarding the flag into a plain create.
#[tokio::test]
async fn the_unbound_registration_refuses_delegate_too() {
    let mock = Arc::new(MockDaemonApi::new());
    let lua = TestLuaBuilder::new()
        .with_sessions_api(Arc::clone(&mock) as Arc<dyn DaemonSessionApi>)
        .build();

    let err: String = lua
        .load(
            r#"
            local job, err = cru.session.create({ delegate = true, prompt = "x" })
            return err
            "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert!(
        err.contains("requires a current session"),
        "the unbound module must refuse, not ignore: {err}"
    );
    assert!(mock.last_create_params().is_none());
}
