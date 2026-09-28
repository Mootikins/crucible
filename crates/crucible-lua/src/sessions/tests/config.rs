//! The `Session` handle with a config RPC: property reads and writes,
//! variables, the current-session getters and the unsupported backing.

use crate::sessions::{Session, SessionConfigRpc, UnsupportedSessionRpc};
use crate::test_support::{MockSessionRpc, TestLuaBuilder};

/// A handle is built fresh per fire site and bound once, so a second bind
/// is a double-wired fire site. It is refused, and the FIRST binding stays.
///
/// `Mutex<Option<Box<_>>>` let the second replace the first in silence, and
/// a handle is `Clone` with a shared slot — so a late rebind changed what
/// every already-cloned handle read, with nothing logged.
#[test]
fn a_second_bind_is_refused_and_the_first_rpc_stays() {
    let session = Session::new("s-bind".to_string());
    let first = MockSessionRpc::new();
    first.switch_model("first-model").unwrap();
    session.bind(Box::new(first));

    let clone = session.clone();

    let second = MockSessionRpc::new();
    second.switch_model("second-model").unwrap();
    session.bind(Box::new(second));

    assert_eq!(
        session.model().unwrap().as_deref(),
        Some("first-model"),
        "the first binding must survive the refused second"
    );
    assert_eq!(
        clone.model().unwrap().as_deref(),
        Some("first-model"),
        "and a clone made before the second bind must read the same"
    );
}

#[test]
fn test_get_session_returns_current() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("test-123".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    let id: String = lua.load("return cru.get_session().id").eval().unwrap();
    assert_eq!(id, "test-123");
}

/// `current()` and the deprecated `get_session()` read the same binding —
/// the daemon sets one current session per VM, and both spellings of the
/// getter must see it.
#[test]
fn current_and_the_deprecated_getter_read_the_same_session() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("s-current".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    let (current_id, deprecated_id, model): (String, String, String) = lua
        .load(
            r#"
            local cur = cru.session.current()
            return cur.id, cru.get_session().id, cur.model
            "#,
        )
        .eval()
        .unwrap();
    assert_eq!(current_id, "s-current");
    assert_eq!(deprecated_id, "s-current");
    assert_eq!(model, "test-model");
}

#[test]
fn test_session_property_access() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("s1".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    let model: String = lua.load("return cru.get_session().model").eval().unwrap();
    assert_eq!(model, "test-model");
}

#[test]
fn test_session_property_write() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("s1".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    lua.load(r#"local s = cru.get_session(); s.system_prompt = "rewritten""#)
        .exec()
        .unwrap();

    let prompt: String = lua
        .load("return cru.get_session().system_prompt")
        .eval()
        .unwrap();
    assert_eq!(prompt, "rewritten");
}

/// `session.model = "x"` is how a hook picks the model; it lands in
/// `switch_model` and reads back.
#[test]
fn assigning_model_switches_it() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("s1".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    let model: String = lua
        .load(
            r#"local s = cru.get_session()
               s.model = "new-model"
               return s.model"#,
        )
        .eval()
        .unwrap();
    assert_eq!(model, "new-model");
}

#[test]
fn test_no_session_error() {
    let (lua, _mgr) = TestLuaBuilder::new().build_with_current_session();

    let result: mlua::Result<String> = lua.load("return cru.get_session().id").eval();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("No active session"));
}

#[test]
fn test_session_variable_string() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("s1".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    lua.load("cru.get_session():set_variable('key', 'value')")
        .exec()
        .unwrap();

    let result: String = lua
        .load("return cru.get_session():get_variable('key')")
        .eval()
        .unwrap();
    assert_eq!(result, "value");
}

#[test]
fn test_session_variable_table() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("s1".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    lua.load("cru.get_session():set_variable('config', {nested = true, count = 42})")
        .exec()
        .unwrap();

    let result: mlua::Table = lua
        .load("return cru.get_session():get_variable('config')")
        .eval()
        .unwrap();
    let nested: bool = result.get("nested").unwrap();
    let count: i64 = result.get("count").unwrap();
    assert!(nested);
    assert_eq!(count, 42);
}

#[test]
fn test_session_variable_nil_for_missing() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("s1".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    let result: mlua::Value = lua
        .load("return cru.get_session():get_variable('nonexistent')")
        .eval()
        .unwrap();
    assert!(result.is_nil());
}

#[test]
fn test_session_variable_reject_function() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("s1".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    let result: mlua::Result<()> = lua
        .load("cru.get_session():set_variable('fn', function() end)")
        .exec();
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("JSON-serializable"));
}

#[test]
fn test_session_system_prompt_read() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("test-123".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    let prompt: String = lua
        .load("return cru.get_session().system_prompt")
        .eval()
        .unwrap();
    assert_eq!(prompt, crucible_core::prompts::DEFAULT_SYSTEM_PROMPT);
}

#[test]
fn test_session_system_prompt_write() {
    let (lua, mgr) = TestLuaBuilder::new().build_with_current_session();

    let session = Session::new("s1".to_string());
    session.bind(Box::new(MockSessionRpc::new()));
    mgr.set_current(session);

    lua.load("local s = cru.get_session(); s.system_prompt = 'custom prompt'")
        .exec()
        .unwrap();

    let prompt: String = lua
        .load("return cru.get_session().system_prompt")
        .eval()
        .unwrap();
    assert_eq!(prompt, "custom prompt");
}

/// An unsupported setter must fail, not succeed silently.
///
/// The trait once defaulted every setter to `Ok(())`, and the daemon
/// bound that empty impl at every site, so a plugin that wrote
/// `session.system_prompt = "..."` was told it worked and nothing
/// happened. The methods are required now; the one backing that
/// supports nothing must still say so.
#[test]
fn an_unsupported_setter_reports_that_it_is_unsupported() {
    let rpc = UnsupportedSessionRpc;

    for (name, result) in [
        ("model", rpc.switch_model("gpt-4o")),
        ("mode", rpc.set_mode("plan")),
        ("system_prompt", rpc.set_system_prompt("hi")),
        ("variables", rpc.set_variable("k", serde_json::json!(1))),
    ] {
        let err = result.expect_err("{name}: a no-op setter must not report success");
        assert!(
            err.contains("not supported"),
            "{name}: the error should say why, got: {err}"
        );
    }
}

/// Getters stay silent. The absence of a value is honestly `nil` in
/// Lua, and an error on a read would break `session.x or fallback`.
#[test]
fn unsupported_getters_stay_silent() {
    let rpc = UnsupportedSessionRpc;
    assert_eq!(rpc.get_model(), None);
    assert_eq!(rpc.get_system_prompt(), None);
}
