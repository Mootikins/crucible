use crate::test_support::TestLuaBuilder;

#[test]
fn test_emitter_on_emit() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: i32 = lua
        .load(
            r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function(v) got = v end)
                em:emit("test", 42)
                return got
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, 42);
}

#[test]
fn test_emitter_once() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: i32 = lua
        .load(
            r#"
                local em = cru.emitter.new()
                local count = 0
                em:once("test", function() count = count + 1 end)
                em:emit("test")
                em:emit("test")
                return count
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, 1);
}

#[test]
fn test_emitter_off() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: i32 = lua
        .load(
            r#"
                local em = cru.emitter.new()
                local count = 0
                local id = em:on("test", function() count = count + 1 end)
                em:emit("test")
                em:off("test", id)
                em:emit("test")
                return count
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, 1);
}

#[test]
fn test_emitter_error_handling() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    // Handler errors should not propagate
    let result: i32 = lua
        .load(
            r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function() error("boom") end)
                em:on("test", function() got = 1 end)
                em:emit("test")
                return got
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, 1);
}

#[test]
fn test_emitter_preserves_registration_order() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: String = lua
        .load(
            r#"
                local em = cru.emitter.new()
                local order = ""
                em:on("test", function() order = order .. "a" end)
                em:on("test", function() order = order .. "b" end)
                em:on("test", function() order = order .. "c" end)
                em:emit("test")
                return order
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, "abc");
}

#[test]
fn test_emitter_count() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: (i32, i32, i32) = lua
        .load(
            r#"
                local em = cru.emitter.new()
                -- no listeners yet
                local c0 = em:count("test")
                em:on("test", function() end)
                em:on("test", function() end)
                local c2 = em:count("test")
                -- unknown event
                local c_none = em:count("unknown")
                return c0, c2, c_none
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result.0, 0);
    assert_eq!(result.1, 2);
    assert_eq!(result.2, 0);
}

#[test]
fn test_emitter_count_excludes_removed() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: i32 = lua
        .load(
            r#"
                local em = cru.emitter.new()
                local id1 = em:on("test", function() end)
                em:on("test", function() end)
                em:off("test", id1)
                return em:count("test")
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, 1);
}

#[test]
fn test_emitter_emit_async_fires_listeners() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: i32 = lua
        .load(
            r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function(v) got = v end)
                em:emit_async("test", 99)
                return got
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, 99);
}

#[test]
fn test_emitter_emit_async_swallows_errors() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    // emit_async should not propagate handler errors
    let result: i32 = lua
        .load(
            r#"
                local em = cru.emitter.new()
                local got = 0
                em:on("test", function() error("boom") end)
                em:on("test", function() got = 1 end)
                em:emit_async("test")
                return got
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, 1);
}

#[test]
fn test_emitter_global_returns_same_instance() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: bool = lua
        .load(
            r#"
                local g1 = cru.emitter.global()
                local g2 = cru.emitter.global()
                return g1 == g2
                "#,
        )
        .eval()
        .unwrap();
    assert!(result);
}

#[test]
fn test_emitter_global_is_functional() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: i32 = lua
        .load(
            r#"
                local g = cru.emitter.global()
                local got = 0
                g:on("evt", function(v) got = v end)
                -- Access via a second global() call to prove shared state
                cru.emitter.global():emit("evt", 77)
                return got
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result, 77);
}

#[test]
fn test_emitter_global_independent_from_new() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: (i32, i32) = lua
        .load(
            r#"
                local g = cru.emitter.global()
                local e = cru.emitter.new()
                local g_got = 0
                local e_got = 0
                g:on("test", function() g_got = g_got + 1 end)
                e:on("test", function() e_got = e_got + 1 end)
                g:emit("test")
                return g_got, e_got
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result.0, 1);
    assert_eq!(result.1, 0);
}

#[test]
fn test_emitter_count_after_once_fires() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: (i32, i32) = lua
        .load(
            r#"
                local em = cru.emitter.new()
                em:once("test", function() end)
                local before = em:count("test")
                em:emit("test")
                local after = em:count("test")
                return before, after
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result.0, 1);
    assert_eq!(result.1, 0);
}
