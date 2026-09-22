use super::super::*;
use super::MockDaemonApi;
use crate::test_support::TestLuaBuilder;
use std::sync::Arc;
use strum::IntoEnumIterator;

/// Each `cru.diff` function passes its params whole to the operation of
/// its name, and a JSON null in the reply reaches Lua as nil.
#[tokio::test]
async fn every_diff_function_calls_the_operation_of_its_name() {
    let mock = Arc::new(MockDaemonApi::new());
    let api: Arc<dyn DaemonSessionApi> = mock.clone();
    let lua = TestLuaBuilder::new().with_sessions_api(api).build();

    for op in DiffOp::iter() {
        let (name, base_is_nil): (String, bool) = lua
            .load(format!(
                r#"
                local reply, err = cru.diff.{}({{ source = {{ kind = "session_record", session = "child-1" }} }})
                assert(err == nil, "unexpected error: " .. tostring(err))
                return reply.op, reply.base_text == nil
                "#,
                op.name()
            ))
            .eval_async()
            .await
            .unwrap();
        assert_eq!(name, op.name());
        assert!(
            base_is_nil,
            "{}: a JSON null must reach Lua as nil",
            op.name()
        );
    }

    let calls = mock.diff_calls();
    assert_eq!(
        calls.iter().map(|(op, _)| *op).collect::<Vec<_>>(),
        DiffOp::iter().collect::<Vec<_>>()
    );
    assert_eq!(
        calls[0].1,
        serde_json::json!({ "source": { "kind": "session_record", "session": "child-1" } })
    );
}

#[tokio::test]
async fn the_diff_stubs_answer_no_daemon() {
    let lua = mlua::Lua::new();
    register_sessions_module(&lua).expect("stub module");

    for op in DiffOp::iter() {
        let (reply, err): (mlua::Value, Option<String>) = lua
            .load(format!(
                r#"return cru.diff.{}({{ source = {{ kind = "proposal", id = "p1" }} }})"#,
                op.name()
            ))
            .eval_async()
            .await
            .unwrap();
        assert!(reply.is_nil(), "{}: {reply:?}", op.name());
        assert_eq!(err.as_deref(), Some("no daemon connected"));
    }
}
