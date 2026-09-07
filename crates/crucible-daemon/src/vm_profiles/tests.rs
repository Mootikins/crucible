//! Each profile must describe the VM production actually builds.

use super::*;

/// Every `cru.*` path on a VM, one level deep per table, sorted.
///
/// Depth matters: a namespace present but empty is not the same surface as one
/// carrying six functions, and a definitions file states both.
pub(crate) fn surface(lua: &Lua) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(cru) = lua.globals().get::<mlua::Table>("cru") else {
        return out;
    };
    for pair in cru.pairs::<String, mlua::Value>().flatten() {
        let (key, value) = pair;
        out.push(format!("cru.{key}"));
        if let mlua::Value::Table(child) = value {
            for inner in child.pairs::<String, mlua::Value>().flatten() {
                out.push(format!("cru.{key}.{}", inner.0));
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// A statusline layout is evaluated with `cru.statusline` and nothing else.
///
/// It used to be checked against the config profile, which also carried
/// `cru.colorscheme`, `cru.hl` and `cru.syntax`: the gate proved a property of
/// a strictly more permissive VM, so a layout reaching for `cru.hl` passed the
/// check and failed to load.
#[test]
fn the_statusline_profile_carries_only_the_statusline() {
    let lua = statusline_vm().expect("the statusline profile must build");
    let paths = surface(&lua);
    assert!(
        paths.iter().all(|p| p.starts_with("cru.statusline")),
        "the statusline VM must carry nothing else: {paths:?}"
    );
    assert!(
        paths.len() > 1,
        "and it must carry the item constructors: {paths:?}"
    );
}

/// No two profiles may render to the same file, or one silently overwrites
/// another and a shape goes undescribed.
#[test]
fn every_profile_renders_to_its_own_file() {
    let mut seen = std::collections::HashSet::new();
    for profile in VmProfile::all() {
        assert!(
            seen.insert(profile.definitions_file()),
            "{} reuses a definitions file name",
            profile.name()
        );
    }
    assert_eq!(seen.len(), VmProfile::all().len());
}
