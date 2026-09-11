//! The type stubs must describe the VM plugins actually run on.
//!
//! This contract lives in `crucible-daemon` and not next to the generator,
//! because the generator's crate cannot build a plugin VM — and that gap is
//! exactly how the two drifted. `crucible-lua`'s own stub tests assert the
//! generator produces *something*; only this one asserts it produces the
//! truth.
//!
//! `stubs.rs` states the rule these enforce: "A stub for a nonexistent
//! function is worse than a stale doc: it looks authoritative."

use std::collections::BTreeSet;

use crucible_daemon::daemon_plugins::DaemonPluginLoader;

/// `---@class cru.X` lines from a freshly generated stub file, top-level only
/// (`cru.notify.messages` is a nested table, not a namespace a user reaches
/// for).
fn stubbed_namespaces(loader: &DaemonPluginLoader) -> BTreeSet<String> {
    let dir = tempfile::tempdir().expect("tempdir");
    loader.generate_stubs(dir.path()).expect("generate stubs");
    let src = std::fs::read_to_string(dir.path().join("cru.lua")).expect("cru.lua");

    src.lines()
        .filter_map(|line| line.strip_prefix("---@class cru."))
        .map(str::trim)
        .filter(|name| !name.contains('.'))
        .map(str::to_string)
        .collect()
}

/// Tables actually hanging off `cru` on the plugin VM.
///
/// `cru.sessions` is excluded by name: it is the deprecated metatable alias
/// over `cru.session`, offers no functions of its own, and must not be
/// advertised by autocomplete. It leaves this list the day the alias is
/// removed.
fn live_namespaces(loader: &DaemonPluginLoader) -> BTreeSet<String> {
    const DEPRECATED_ALIASES: &[&str] = &["sessions"];
    let lua = loader.plugin_lua();
    let cru: mlua::Table = lua.globals().get("cru").expect("cru global");
    cru.pairs::<String, mlua::Value>()
        .filter_map(|pair| {
            let (name, value) = pair.ok()?;
            matches!(value, mlua::Value::Table(_)).then_some(name)
        })
        .filter(|name| !DEPRECATED_ALIASES.contains(&name.as_str()))
        .collect()
}

fn loader() -> DaemonPluginLoader {
    DaemonPluginLoader::new(Default::default()).expect("plugin loader")
}

/// Autocomplete must not offer a namespace that does not exist.
///
/// The generator ran against a throwaway executor and then called
/// `mirror_modules_into_cru`, which *fabricated* `cru.ask`, `cru.graph`,
/// `cru.hooks`, `cru.mcp`, `cru.notify` and `cru.session` out of bare globals
/// and `crucible.*` functions. None of the six are on the plugin VM: a plugin
/// author typing `cru.graph.` got completions for an API that is nil at
/// runtime.
#[test]
fn every_stubbed_namespace_exists_on_the_plugin_vm() {
    let loader = loader();
    let stubbed = stubbed_namespaces(&loader);
    let live = live_namespaces(&loader);

    let fabricated: Vec<_> = stubbed.difference(&live).collect();
    assert!(
        fabricated.is_empty(),
        "stubs advertise {} namespace(s) absent from the plugin VM: {fabricated:?}",
        fabricated.len()
    );
}

/// …and must not hide one that does.
///
/// The walk was a hardcoded `UNIVERSAL_MODULES` list, so every module the
/// daemon registers after it — `check config emitter errors health json log
/// schedule service shell storage ws` — got no stubs at all.
#[test]
fn every_plugin_vm_namespace_is_stubbed() {
    let loader = loader();
    let stubbed = stubbed_namespaces(&loader);
    let live = live_namespaces(&loader);

    let undocumented: Vec<_> = live.difference(&stubbed).collect();
    assert!(
        undocumented.is_empty(),
        "the plugin VM exposes {} namespace(s) with no stubs: {undocumented:?}",
        undocumented.len()
    );
}

/// The `cru.fs` surface, held in both directions, derived from the running VM
/// and the stub file the generator renders from it — never from source text,
/// which a change can satisfy without doing the work.
///
/// `read` and `write` came BACK, and why is what this pins. They were removed
/// as thin wrappers over `io.open`; they return as the scoped alternative to
/// it, confined to the roots the host binds per plugin. `append` and `rename`
/// stay removed, because `io` and `os` answer those and neither needs a scope
/// `io.open` would defeat anyway.
#[test]
fn cru_fs_offers_a_scoped_read_and_write_and_nothing_io_already_does() {
    let loader = loader();
    let lua = loader.plugin_lua();
    let cru: mlua::Table = lua.globals().get("cru").expect("cru global");
    let fs: mlua::Table = cru.get("fs").expect("cru.fs");

    // Still removed: `io` and `os` answer these, and a wrapper buys nothing.
    for name in ["append", "rename"] {
        let value: mlua::Value = fs.get(name).expect("table get");
        assert!(
            value.is_nil(),
            "cru.fs.{name} is removed and must be nil, got {value:?}"
        );
    }

    // Present names are functions — `remove` among them, as the raising
    // data-loss guard rather than a delete.
    for name in [
        "read",
        "write",
        "exists",
        "is_file",
        "is_dir",
        "list",
        "mkdir",
        "copy",
        "remove_all",
        "remove",
    ] {
        let value: mlua::Value = fs.get(name).expect("table get");
        assert!(
            value.is_function(),
            "cru.fs.{name} must be a function, got {value:?}"
        );
    }
    let err = lua
        .load(r#"cru.fs.remove("nowhere")"#)
        .exec()
        .expect_err("the remove guard must raise");
    let text = err.to_string();
    assert!(
        text.contains("remove_all") && text.contains("os.remove"),
        "the remove guard must name both replacements: {text}"
    );

    // And the generated stubs agree with the VM they were rendered from.
    let dir = tempfile::tempdir().expect("tempdir");
    loader.generate_stubs(dir.path()).expect("generate stubs");
    let src = std::fs::read_to_string(dir.path().join("cru.lua")).expect("cru.lua");
    for name in ["append", "rename"] {
        assert!(
            !src.contains(&format!("function cru.fs.{name}(")),
            "the stubs still advertise cru.fs.{name}"
        );
    }
    for name in ["read", "write", "exists", "mkdir", "remove_all"] {
        assert!(
            src.contains(&format!("function cru.fs.{name}(")),
            "the stubs must document cru.fs.{name}"
        );
    }
}

/// The M1 hard removals hold on the running VM — each removed name is nil,
/// and its replacement answers. Derived from the VM, never from source text.
#[test]
fn removed_root_names_are_gone_and_their_replacements_answer() {
    let loader = loader();
    let lua = loader.plugin_lua();

    // Removed name -> the Lua expression that must be nil.
    let removed = [
        "cru.fmt",
        "cru.spawn",
        "cru.oq.json",
        "cru.oq.json_pretty",
        "cru.paths.join",
        "cru.paths.kiln",
        "cru.oil.if_else",
        "cru.oil.hr",
        "cru.oil.maybe",
        "_G.inspect",
        "cru.graph",
        // The six bare globals: one namespace, `cru`.
        "_G.fs",
        "_G.shell",
        "_G.paths",
        "_G.http",
        "_G.graph",
        "_G.mcp",
    ];
    for name in removed {
        let value: mlua::Value = lua
            .load(format!("return {name}"))
            .eval()
            .expect("evaluating a name never raises");
        assert!(
            value.is_nil(),
            "{name} is removed and must be nil, got {value:?}"
        );
    }

    // Each replacement answers, so the removal is a rename, not a hole.
    let replacements = [
        ("cru.timer.spawn", "function"), // was cru.spawn
        ("cru.inspect", "function"),     // was _G.inspect
        ("cru.oil.either", "function"),  // was if_else
        ("cru.oil.divider", "function"), // was hr
        ("cru.fs.mkdir", "function"),    // was _G.fs.mkdir
        ("cru.shell.exec", "function"),  // was _G.shell.exec
        ("cru.paths.state", "function"), // was _G.paths.state
        ("cru.kiln.path", "function"),   // was cru.paths.kiln (name-addressed now)
        ("cru.http.get", "function"),    // was _G.http.get
    ];
    // (cru.mcp is not on the daemon plugin VM at all — the mcp stub module is
    // registered only on the stub generator's own VM, where its bare global
    // is likewise removed.)
    for (name, expected) in replacements {
        let type_name: String = lua
            .load(format!("return type({name})"))
            .eval()
            .expect("type() never raises");
        assert_eq!(
            type_name, expected,
            "{name} must answer for its removed form"
        );
    }

    // The pretty option replaces oq.json_pretty in place.
    let pretty: String = lua
        .load(r#"return cru.json.encode({ a = 1 }, { pretty = true })"#)
        .eval()
        .expect("cru.json.encode with pretty must answer");
    assert!(
        pretty.contains('\n'),
        "pretty output must actually be pretty-printed: {pretty:?}"
    );
}

/// `cru.kiln.path` resolves a name against the DAEMON's registry.
///
/// The assertion runs against a real registry over a tempdir, not against the
/// source text of the closure: the point of the seam is that Lua reaches the
/// registry, and only a live registry proves it does.
#[test]
fn cru_kiln_path_resolves_a_registered_name_through_the_registry() {
    use crucible_daemon::kiln_registry::{KilnRegistry, KilnRegistryContext};

    let tmp = tempfile::TempDir::new().expect("tempdir");
    let root = tmp.path().join("notes");
    std::fs::create_dir_all(&root).expect("kiln root");

    let ctx = KilnRegistryContext::new(
        tmp.path().join("cwd"),
        Some(tmp.path().join("home")),
        tmp.path().join("home").join(".crucible"),
    );
    let config = serde_json::json!({ "kilns": { "notes": root.to_string_lossy() } });
    let registry = KilnRegistry::from_app_config(ctx, Some(&config)).expect("registry must build");

    let loader = loader()
        .with_kiln_path_resolver(std::sync::Arc::new(registry))
        .expect("kiln path resolver");
    let lua = loader.plugin_lua();

    let resolved: String = lua
        .load(r#"return cru.kiln.path("notes", "Inbox/today.md")"#)
        .eval()
        .expect("a registered name must resolve");
    assert_eq!(
        std::path::Path::new(&resolved),
        std::fs::canonicalize(&root)
            .expect("canonical root")
            .join("Inbox/today.md")
    );

    let err = lua
        .load(r#"return cru.kiln.path("absent")"#)
        .eval::<String>()
        .expect_err("an unregistered name must be refused");
    assert!(err.to_string().contains("absent"), "unhelpful: {err}");
}

/// Every signature the host declares must name a function the plugin VM
/// really has — matched by PATH, not by a substring of the rendered file.
///
/// A declaration is read as authoritative: an author who sees
/// `cru.shell.exec(command: string, ...)` writes the call and expects it to
/// exist. This test's first form asked whether the rendered text contained
/// `"{leaf}: ("`, which for `cru.on` is `on: (` — a needle that matches
/// `option: (` and `session: (`. It passed while `cru.on` was
/// absent from the file entirely.
#[tokio::test]
async fn every_declared_signature_exists_on_the_vm() {
    let loader =
        crucible_daemon::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
            .expect("loader");
    let registered: std::collections::BTreeSet<String> =
        crucible_lua::stubs::function_paths(&loader.executor().lua().clone())
            .expect("walk the plugin VM")
            .into_iter()
            .collect();

    let members: std::collections::BTreeSet<String> =
        crucible_lua::stubs::value_members(&loader.executor().lua().clone())
            .expect("walk the plugin VM")
            .into_iter()
            .map(|member| member.path)
            .collect();

    // A declared FUNCTION must be a registered function. A declared FIELD —
    // `cru.kiln.active` is a string, absent when no kiln is active — is
    // satisfied by the member existing, or by its namespace existing.
    // A function bound to the loading plugin cannot be seen from an idle VM;
    // the declaration says so, and only those are exempt.
    let bound_at_load = crucible_lua::host_api::bound_at_load();

    let missing: Vec<&str> = crucible_lua::host_api::declared_signatures()
        .iter()
        .filter(|(path, _)| !bound_at_load.contains_key(**path))
        .filter(|(path, ty)| {
            if crucible_lua::host_api::is_callable_declaration(ty) {
                return !registered.contains(**path);
            }
            let namespace = path.rsplit_once('.').map(|(head, _)| head).unwrap_or(path);
            !members.contains(**path)
                && !registered.iter().any(|known| known.starts_with(namespace))
        })
        .map(|(path, _)| *path)
        .collect();

    assert!(
        missing.is_empty(),
        "declared signatures name functions the plugin VM does not have: {missing:?}\n\
         registered paths: {registered:#?}"
    );
}

/// The declared signature reaches the generated file, at its own path.
#[tokio::test]
async fn a_declared_signature_reaches_the_generated_declarations() {
    let loader =
        crucible_daemon::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
            .expect("loader");
    let dir = tempfile::TempDir::new().expect("tempdir");
    loader.generate_stubs(dir.path()).expect("generate stubs");
    let declarations = std::fs::read_to_string(dir.path().join("cru.d.luau")).expect("cru.d.luau");

    // `cru.on` is the one that was silently absent: a top-level function, so
    // the module walk skipped it, and the substring gate did not notice.
    assert!(
        declarations.contains("    on: ((event: string"),
        "cru.on must be declared at the top level: {declarations}"
    );
    assert!(
        declarations.contains("exec: (command: string"),
        "a nested signature must reach the file: {declarations}"
    );
    // No unsigned function may render Luau's variadic with a name.
    assert!(
        !declarations.contains("...: any"),
        "`(...: any)` is a parse error; the variadic carries no name: {declarations}"
    );
}

/// The declarations describe the same VM the stubs do, so a plugin author
/// checking against them is checking against what the daemon runs.
#[tokio::test]
async fn the_luau_declarations_cover_the_vm_namespaces() {
    let loader =
        crucible_daemon::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
            .expect("loader");
    let dir = tempfile::TempDir::new().expect("tempdir");
    loader.generate_stubs(dir.path()).expect("generate stubs");
    let declarations = std::fs::read_to_string(dir.path().join("cru.d.luau")).expect("cru.d.luau");

    assert!(declarations.starts_with("--!strict"), "{declarations}");
    for namespace in ["fs", "json", "shell", "timer"] {
        assert!(
            declarations.contains(&format!("{namespace}: {{")),
            "the declarations must cover cru.{namespace}: {declarations}"
        );
    }
}

/// Every function the plugin VM exposes is either signed or listed as
/// unsigned — so a NEW function cannot ship undescribed.
///
/// The declarations are read as authoritative by anyone running
/// `cru plugin check`, and a function that is not in them is
/// `(...any) -> any`: no argument checked, no result checked. That is a fine
/// state for a function nobody has got to yet, and a bad state to reach by
/// accident. This test makes reaching it a deliberate line in
/// `host_api::UNSIGNED`, written in the same diff as the function.
///
/// It fails in both directions. An unlisted, unsigned function fails it, and
/// so does a listed path the VM no longer has — otherwise the list would rot
/// behind a rename and quietly stop covering anything.
/// Every PROFILE, not just the daemon one.
///
/// This built `DaemonPluginLoader` alone, so a function registered only on the
/// daemon VM — `cru.permissions.on_request` and its neighbours — could go
/// undeclared with nothing to say so, and `cru-session.d.luau` would render it
/// `(...any) -> any` while the header still counted it as unsigned. That is
/// the same false green the profiles were built to end, one VM short of the
/// end.
#[tokio::test]
async fn every_function_is_signed_or_listed() {
    use crucible_daemon::vm_profiles::VmProfile;

    let loader =
        crucible_daemon::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
            .expect("loader");

    // Each profile's VM, and the signatures IT recorded. A path is described
    // if any profile that has it declares it — a function on two VMs is
    // declared once.
    let mut registered: std::collections::BTreeSet<String> = Default::default();
    let mut per_vm_signed: std::collections::BTreeSet<String> = Default::default();
    for profile in VmProfile::all() {
        let lua = match profile {
            VmProfile::Daemon => loader.executor().lua().clone(),
            VmProfile::Statusline => {
                crucible_daemon::vm_profiles::statusline_vm().expect("statusline vm")
            }
            // No `cru` at all, so nothing to walk.
            VmProfile::Theme => continue,
        };
        registered.extend(
            crucible_lua::stubs::function_paths(&lua)
                .unwrap_or_else(|e| panic!("walk the {} VM: {e}", profile.name())),
        );
        per_vm_signed.extend(crucible_lua::HostSignatures::of(&lua).paths());
    }
    // Signed either way: beside its registration (`host_registry::Ns`, the
    // form that is checked against the Rust types) or in the static table
    // that has not moved yet.
    let mut signed: std::collections::BTreeSet<String> =
        crucible_lua::host_api::declared_signatures()
            .keys()
            .map(|path| path.to_string())
            .collect();
    signed.extend(per_vm_signed);
    let listed: std::collections::BTreeSet<&str> =
        crucible_lua::host_api::UNSIGNED.iter().copied().collect();

    let undescribed: Vec<&String> = registered
        .iter()
        .filter(|path| !signed.contains(path.as_str()) && !listed.contains(path.as_str()))
        .collect();
    assert!(
        undescribed.is_empty(),
        "these functions are neither signed nor listed as unsigned. Write a \
         signature in `host_api::DECLARED`, or add the path to \
         `host_api::UNSIGNED`:\n{undescribed:#?}"
    );

    let stale: Vec<&&str> = listed
        .iter()
        .filter(|path| !registered.contains(**path))
        .collect();
    assert!(
        stale.is_empty(),
        "`host_api::UNSIGNED` names functions the plugin VM does not have. \
         Remove them:\n{stale:#?}"
    );

    // A path that is BOTH signed and listed leaves the list overstating the
    // gap, which is how it stops being an inventory and becomes decoration.
    // Signing a function is therefore also deleting its line here.
    let signed_but_listed: Vec<&&str> = listed
        .iter()
        .filter(|path| signed.contains(**path))
        .collect();
    assert!(
        signed_but_listed.is_empty(),
        "these functions carry a signature AND are listed as unsigned. \
         Delete them from `host_api::UNSIGNED`:\n{signed_but_listed:#?}"
    );
}
