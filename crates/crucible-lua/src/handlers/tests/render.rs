use crate::handlers::{execute_tool_render, register_cru_on_api, LuaScriptHandlerRegistry};
use crucible_core::turn::TurnOrigin;
use crucible_core::types::CanonicalToolCall;
use mlua::Lua;

/// The render of a finished call reads its result and its error, and its
/// summary says what the result is.
#[tokio::test]
async fn a_render_reads_the_result_of_a_finished_call() {
    let lua = Lua::new();
    let registry = LuaScriptHandlerRegistry::new();
    register_cru_on_api(&lua, registry.clone()).unwrap();
    lua.load(
        r#"
        cru.on("tool:render", { pattern = "search" }, function(ctx, call)
            local summary = call.result and (call.error or ("found " .. call.result))
            return { line = call.query, summary = summary }
        end)
        "#,
    )
    .exec()
    .unwrap();

    let args = serde_json::json!({ "query": "rust" });
    let call = CanonicalToolCall::crucible_tool("web_search", &args);
    let render = |outcome| {
        execute_tool_render(
            &lua,
            &registry,
            Some("s-test"),
            &call,
            &args,
            TurnOrigin::User,
            outcome,
        )
    };

    let before = render(None).await.unwrap().expect("the render answers");
    assert_eq!(before.line.as_deref(), Some("rust"));
    assert_eq!(before.summary, None);

    let after = render(Some(("3", None))).await.unwrap().unwrap();
    assert_eq!(after.summary.as_deref(), Some("found 3"));

    let failed = render(Some(("", Some("boom")))).await.unwrap().unwrap();
    assert_eq!(failed.summary.as_deref(), Some("boom"));
}
