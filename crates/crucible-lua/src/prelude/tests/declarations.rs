//! The prelude's hand-written declarations must at least be Luau.
//!
//! `Ns::declare_only` cannot check a declaration against a Rust type, because
//! a function written in Lua has none. That leaves one property a machine can
//! still hold: the type it renders must be one Luau can READ. A declaration
//! that does not parse is not a weak description — it is a syntax error in
//! the generated `cru.d.luau`, and one of those takes the whole file down,
//! so every `cru.*` call in every plugin becomes an unknown global at once.
//!
//! What it does not check stays human care: whether the declaration matches
//! the Lua body. Read `stdlib.rs`, `qol.rs` and `health.rs` for that.

use crate::host_registry::HostSignatures;
use crate::test_support::TestLuaBuilder;

#[test]
fn every_prelude_declaration_parses_as_luau() {
    let lua = TestLuaBuilder::new().with_stdlib().build();
    let signatures = HostSignatures::of(&lua);

    let mut paths = signatures.paths();
    paths.sort();
    assert!(
        paths.len() >= 21,
        "the prelude declares 21 paths; the walk found {}: {paths:?}",
        paths.len()
    );

    // A fresh VM, so a type alias cannot collide with the prelude's own
    // globals.
    let probe = mlua::Lua::new();
    for path in paths {
        let rendered = signatures
            .get(&path)
            .expect("a path the registry listed has a type")
            .to_luau();
        probe
            .load(format!("type T = {rendered}\nreturn 1"))
            .exec()
            .unwrap_or_else(|e| panic!("{path} renders `{rendered}`, which Luau cannot read: {e}"));
    }
}
