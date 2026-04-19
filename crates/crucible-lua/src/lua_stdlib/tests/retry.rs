use super::super::register_lua_stdlib;
use crate::test_support::TestLuaBuilder;
use mlua::Lua;

#[test]
fn test_retry_succeeds() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: (String, i32) = lua
        .load(
            r#"
                local attempts = 0
                local result = cru.retry(function()
                    attempts = attempts + 1
                    if attempts < 3 then error({ retryable = true }) end
                    return "ok"
                end, { max_retries = 5, base_delay = 0.001 })
                return result, attempts
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result.0, "ok");
    assert_eq!(result.1, 3);
}

#[test]
fn test_retry_exhausted() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result = lua
        .load(
            r#"
                cru.retry(function()
                    error("always fails")
                end, { max_retries = 2, base_delay = 0.001 })
                "#,
        )
        .exec();
    assert!(result.is_err());
}

#[test]
fn test_retry_non_retryable() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: i32 = lua
        .load(
            r#"
                local attempts = 0
                pcall(cru.retry, function()
                    attempts = attempts + 1
                    error("fatal")
                end, {
                    max_retries = 5,
                    base_delay = 0.001,
                    retryable = function() return false end,
                })
                return attempts
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, 1);
}

#[tokio::test]
async fn test_retry_with_real_timer() {
    let lua = Lua::new();
    lua.load("cru = cru or {}").exec().unwrap();
    lua.load(r#"cru.log = function() end"#).exec().unwrap();
    crate::timer::register_timer_module(&lua).unwrap();
    register_lua_stdlib(&lua).unwrap();

    let start = std::time::Instant::now();
    let result: (String, i32) = lua
        .load(
            r#"
                local attempts = 0
                local result = cru.retry(function()
                    attempts = attempts + 1
                    if attempts < 3 then error({ retryable = true }) end
                    return "ok"
                end, { max_retries = 5, base_delay = 0.01 })
                return result, attempts
                "#,
        )
        .eval_async()
        .await
        .unwrap();

    assert_eq!(result.0, "ok");
    assert_eq!(result.1, 3);
    // Verify real async sleep was used (at least some time passed)
    assert!(start.elapsed().as_millis() >= 10);
}
