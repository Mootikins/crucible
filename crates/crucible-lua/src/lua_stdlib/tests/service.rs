use crate::test_support::TestLuaBuilder;

#[test]
fn test_service_define_returns_descriptor() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let result: (String, bool) = lua
        .load(
            r#"
                local started = false
                local svc = cru.service.define({
                    name = "test",
                    desc = "Test service",
                    start = function() started = true end,
                })
                return svc.desc, type(svc.fn) == "function"
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(result.0, "Test service");
    assert!(result.1);
}

#[test]
fn test_service_define_validates_required_fields() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    // Missing start fn
    assert!(lua
        .load(r#"cru.service.define({ name = "x", desc = "x" })"#)
        .exec()
        .is_err());
    // Missing name
    assert!(lua
        .load(r#"cru.service.define({ desc = "x", start = function() end })"#)
        .exec()
        .is_err());
}

#[test]
fn test_service_list_and_status() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let (count, name, running): (i32, String, bool) = lua
        .load(
            r#"
                cru.service.define({
                    name = "svc1",
                    desc = "Service One",
                    start = function() end,
                    health = function() return true end,
                })
                local list = cru.service.list()
                local st = cru.service.status("svc1")
                return #list, st.name, st.running
                "#,
        )
        .eval()
        .unwrap();
    assert!(count >= 1);
    assert_eq!(name, "svc1");
    assert!(!running); // Not started yet, just defined
}

#[test]
fn test_service_stop() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let stopped: bool = lua
        .load(
            r#"
                local was_stopped = false
                cru.service.define({
                    name = "stoppable",
                    desc = "Can stop",
                    start = function() end,
                    stop = function() was_stopped = true end,
                })
                cru.service.stop("stoppable")
                return was_stopped
                "#,
        )
        .eval()
        .unwrap();
    assert!(stopped);
}

#[test]
fn test_service_config_resolution() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    // Mock crucible.config.get to return nil (no config file)
    lua.load(
        r#"
            crucible = crucible or {}
            crucible.config = { get = function() return nil end }
            "#,
    )
    .exec()
    .unwrap();

    let val: i32 = lua
        .load(
            r#"
                cru.service.define({
                    name = "cfgtest",
                    desc = "Config test",
                    start = function() end,
                    config = {
                        port = { type = "number", default = 8080 },
                    },
                })
                local entry = cru.service._services["cfgtest"]
                return entry.config.port
                "#,
        )
        .eval()
        .unwrap();
    assert_eq!(val, 8080);
}
