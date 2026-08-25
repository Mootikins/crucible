use crate::test_support::TestLuaBuilder;
use mlua::Table;

#[test]
fn test_check_string() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    // Valid
    lua.load(r#"cru.check.string("hello", "name")"#)
        .exec()
        .unwrap();
    // Invalid
    assert!(lua.load(r#"cru.check.string(42, "name")"#).exec().is_err());
    // Optional nil
    lua.load(r#"cru.check.string(nil, "name", { optional = true })"#)
        .exec()
        .unwrap();
    // Optional non-nil wrong type
    assert!(lua
        .load(r#"cru.check.string(42, "name", { optional = true })"#)
        .exec()
        .is_err());
}

#[test]
fn test_check_number_with_range() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    lua.load(r#"cru.check.number(5, "count", { min = 1, max = 10 })"#)
        .exec()
        .unwrap();
    assert!(lua
        .load(r#"cru.check.number(0, "count", { min = 1 })"#)
        .exec()
        .is_err());
    assert!(lua
        .load(r#"cru.check.number(11, "count", { max = 10 })"#)
        .exec()
        .is_err());
}

#[test]
fn test_check_one_of() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    lua.load(r#"cru.check.one_of("json", {"json", "text"}, "format")"#)
        .exec()
        .unwrap();
    assert!(lua
        .load(r#"cru.check.one_of("xml", {"json", "text"}, "format")"#)
        .exec()
        .is_err());
}

#[test]
fn test_check_table() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    lua.load(r#"cru.check.table({}, "opts")"#).exec().unwrap();
    assert!(lua
        .load(r#"cru.check.table("string", "opts")"#)
        .exec()
        .is_err());
}

#[test]
fn test_check_modules_exist() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let cru: Table = lua.globals().get("cru").unwrap();

    assert!(cru.get::<Table>("emitter").is_ok());
    let emitter: Table = cru.get("emitter").unwrap();
    assert!(emitter.get::<mlua::Function>("new").is_ok());
    assert!(emitter.get::<mlua::Function>("global").is_ok());
    assert!(cru.get::<Table>("check").is_ok());
    assert!(cru.get::<mlua::Function>("retry").is_ok());
    assert!(cru.get::<Table>("health").is_ok());
}
