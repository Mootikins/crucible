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

/// The config profile must not reach the network or the filesystem. `cru config`
/// and `cru doctor` evaluate a config file to READ values out of it.
#[test]
fn the_config_profile_has_no_network_or_filesystem() {
    let lua = config_vm().expect("the config profile must build");
    let paths = surface(&lua);
    for forbidden in ["cru.http", "cru.fs", "cru.shell", "cru.ws", "cru.storage"] {
        assert!(
            !paths.contains(&forbidden.to_string()),
            "{forbidden} must not be on the config VM: {paths:?}"
        );
    }
}

/// The three profiles must not render to the same file, or one silently
/// overwrites another and two of the three shapes go undescribed.
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
    assert_eq!(seen.len(), 3);
}
