//! The `cru plugin test` runner: discovers a plugin's `*_test.lua` files,
//! loads them into an executor whose module roots mirror the runtime plugin
//! loader, and runs them through the bundled busted-style
//! framework. Includes the CI gates that run every shipped plugin's suite.

use super::*;

pub(crate) async fn handle_lua_run_plugin_tests(req: Request) -> Response {
    let params =
        match crate::rpc_helpers::typed_params::<crate::rpc_client::LuaRunPluginTestsRequest>(&req)
        {
            Ok(p) => p,
            Err(response) => return *response,
        };
    let filter = params.filter;
    let test_path = PathBuf::from(&params.test_path);

    if !test_path.exists() {
        return Response::error(
            req.id,
            INVALID_PARAMS,
            format!("Test path does not exist: {}", test_path.display()),
        );
    }

    // Discover test files
    let test_files = match discover_plugin_test_files(&test_path) {
        Ok(files) => files,
        Err(e) => return internal_error(req.id, e),
    };

    if test_files.is_empty() {
        return Response::success(
            req.id,
            serde_json::json!({
                "passed": 0,
                "failed": 0,
                "load_failures": 0,
                "failures": [],
                "load_failure_details": [],
                "message": "No test files found",
            }),
        );
    }

    let executor = match LuaExecutor::new() {
        Ok(e) => e,
        Err(e) => return internal_error(req.id, e),
    };
    // This VM runs plugin tests, so it gets `describe`, `it`, `run_tests` and
    // the harness `assert`. No other VM does.
    if let Err(e) = executor.install_test_harness() {
        return internal_error(req.id, anyhow::Error::from(e));
    }

    // Mirror the runtime loader exactly: the plugin's siblings are visible by
    // name through the parent root, and the plugin's own `lua/` directory is
    // private to its execution. A suite that passes here therefore proves the
    // plugin loads in the daemon.
    let plugin_root = test_path
        .canonicalize()
        .unwrap_or_else(|_| test_path.clone());
    let plugin_root = if plugin_root.is_file() {
        plugin_root
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(plugin_root)
    } else {
        plugin_root
    };
    let plugin_parent = plugin_root.parent().unwrap_or(&plugin_root).to_path_buf();
    if let Err(e) = executor.configure_module_roots(vec![plugin_parent]) {
        return internal_error(req.id, e);
    }
    // The plugin under test owns its `lua/` directory for the whole run, and
    // nothing else: the runtime grants exactly this. The harness used to add
    // `<plugin_dir>/?.lua` as well, under which `require("lua.container")`
    // passed here and failed in the daemon.
    let _module_scope = match executor.enter_plugin_root(&plugin_root) {
        Ok(guard) => guard,
        Err(e) => return internal_error(req.id, e),
    };

    // Setup test mocks
    if let Err(e) = executor
        .lua()
        .load("test_mocks.setup()")
        .set_name("test_mocks_setup")
        .exec()
    {
        return internal_error(req.id, e);
    }

    // Apply test filter if provided
    if let Some(ref filter_str) = filter {
        if let Err(e) = executor
            .lua()
            .globals()
            .set("__cru_plugin_test_filter", filter_str.clone())
        {
            return internal_error(req.id, e);
        }
        if let Err(e) = executor
            .lua()
            .load(
                r#"
                local _orig_it = it
                local _orig_pending = pending
                local filter = _G.__cru_plugin_test_filter

                it = function(name, fn)
                    if string.find(name, filter, 1, true) then
                        return _orig_it(name, fn)
                    end
                end

                pending = function(name, fn)
                    if string.find(name, filter, 1, true) then
                        return _orig_pending(name, fn)
                    end
                end
                "#,
            )
            .set_name("test_filter")
            .exec()
        {
            return internal_error(req.id, e);
        }
    }

    // Load test files. Failures record *which* file and *why* — a bare count
    // tells a CLI user nothing they can act on.
    let mut load_failure_details: Vec<serde_json::Value> = Vec::new();
    let mut note_load_failure = |file: &Path, error: String| {
        load_failure_details.push(serde_json::json!({
            "file": file.to_string_lossy(),
            "error": error,
        }));
    };

    for file in &test_files {
        let file_contents = match std::fs::read_to_string(file) {
            Ok(contents) => contents,
            Err(e) => {
                note_load_failure(file, e.to_string());
                continue;
            }
        };

        // `@` marks the chunk name as a file path. Without it Lua treats the
        // name as literal source text and renders every location as
        // `[string "/path/to/foo_test.lua"]:3:` instead of
        // `/path/to/foo_test.lua:3:` — which is both noisier to read and not a
        // path a caller can use.
        let chunk_name = format!("@{}", file.to_string_lossy());
        if let Err(e) = executor
            .lua()
            .load(&file_contents)
            .set_name(chunk_name.as_str())
            .exec()
        {
            note_load_failure(file, crucible_lua::format_lua_error(None, &e));
        }
    }
    let load_failures = load_failure_details.len();

    // Run tests
    let results: mlua::Table = match executor
        .lua()
        .load("return run_tests()")
        .set_name("plugin_test_runner")
        .eval()
    {
        Ok(r) => r,
        Err(e) => return internal_error(req.id, e),
    };

    let passed: usize = results.get("passed").unwrap_or(0);
    let failed: usize = results.get("failed").unwrap_or(0);

    // The runner used to report failures by `print`ing them, which reaches only
    // the daemon process's stdout — and an auto-spawned daemon has stdout and
    // stderr on /dev/null, so those results were unrecoverable. They are
    // returned from `run_tests()` now; ship them to whoever asked.
    let mut failures: Vec<serde_json::Value> = Vec::new();
    if let Ok(errors) = results.get::<mlua::Table>("errors") {
        for entry in errors.sequence_values::<mlua::Table>().flatten() {
            failures.push(serde_json::json!({
                "name": entry.get::<String>("name").unwrap_or_default(),
                "suite": entry.get::<String>("suite").ok(),
                "error": entry.get::<String>("error").unwrap_or_default(),
                "file": entry.get::<String>("file").ok(),
                "line": entry.get::<String>("line").ok(),
            }));
        }
    }

    Response::success(
        req.id,
        serde_json::json!({
            "passed": passed,
            "failed": failed,
            "load_failures": load_failures,
            "failures": failures,
            "load_failure_details": load_failure_details,
        }),
    )
}

/// Discover test files in a plugin directory (files ending with `_test.lua`).
pub(super) fn discover_plugin_test_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut files = Vec::new();

    // Check tests/ subdirectory
    let tests_dir = path.join("tests");
    if tests_dir.is_dir() {
        collect_plugin_test_files(&tests_dir, &mut files)?;
    }

    // Check root directory
    collect_plugin_test_files(path, &mut files)?;

    files.sort();
    files.dedup();
    Ok(files)
}

fn collect_plugin_test_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() {
            let stem = path.file_stem().and_then(|name| name.to_str());
            if crucible_lua::source_files::is_lua_source(&path)
                && stem.is_some_and(|s| s.ends_with("_test"))
            {
                out.push(path);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod shipped_plugin_tests {
    use crucible_core::protocol::Request;
    use std::path::PathBuf;
    use test_case::test_case;

    fn shipped_plugins_dir() -> PathBuf {
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../runtime/plugins"
        ))
    }

    /// Shipped plugins that deliberately have no Lua suite, and why.
    ///
    /// Empty, and that is the point: every shipped plugin is gated. The list
    /// is the escape hatch that keeps
    /// `every_shipped_plugin_with_a_suite_is_gated` usable — an experimental
    /// plugin may sit in `runtime/plugins/` untested, but only by saying so
    /// here, with a reason. Silence is what let `web-search` (148 assertions)
    /// and `worktree` (42) go unrun by any in-process gate for months.
    const NO_LUA_SUITE: &[(&str, &str)] = &[];

    fn run_plugin_tests(plugin_dir: &str) -> serde_json::Value {
        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "lua.run_plugin_tests".to_string(),
            params: serde_json::json!({ "test_path": plugin_dir }),
        };
        let resp = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(super::handle_lua_run_plugin_tests(req));
        assert!(resp.error.is_none(), "handler errored: {:?}", resp.error);
        resp.result.expect("result present")
    }

    /// Render the failure diagnostics the handler returns, so a red CI run
    /// says *which* assertion broke rather than only how many did.
    fn describe_failures(result: &serde_json::Value) -> String {
        let mut out = String::new();
        for f in result["load_failure_details"]
            .as_array()
            .into_iter()
            .flatten()
        {
            out.push_str(&format!(
                "\n  load error in {}: {}",
                f["file"].as_str().unwrap_or("?"),
                f["error"].as_str().unwrap_or("?")
            ));
        }
        for f in result["failures"].as_array().into_iter().flatten() {
            out.push_str(&format!(
                "\n  ✗ {} (line {})\n      {}",
                f["name"].as_str().unwrap_or("?"),
                f["line"].as_str().unwrap_or("?"),
                f["error"].as_str().unwrap_or("?")
            ));
        }
        out
    }

    /// Runs each shipped plugin's busted-style Lua suite in-process through the
    /// same handler `cru plugin test` uses — no daemon, so it runs under
    /// nextest alongside everything else. Also exercises the package.path
    /// Every shipped plugin passes `cru plugin check`: it parses, and every
    /// tool parameter's declared type is one the host can read.
    ///
    /// The type check is NOT optional here. This test used to run it only
    /// where a checker happened to be installed and pass otherwise, with a
    /// comment arguing that was honest because "the report says which
    /// happened". The report went nowhere: the test passed either way, and in
    /// the CI `test` job — which sets no `CRUCIBLE_LUAU_ANALYZE` and installs
    /// no `luau-lsp` — it passed having checked nothing.
    #[test]
    fn every_shipped_plugin_typechecks() {
        // Fails when no checker exists, rather than skipping.
        let checker = required_checker();

        let stubs = tempfile::TempDir::new().expect("tempdir");
        let definitions = stubs.path().join("cru.d.luau");
        // The DAEMON's VM, not `StubGenerator::generate`, which builds the
        // `crucible-lua` subset: it has no `cru.shell`, `cru.storage`,
        // `cru.ws`, `cru.isolation` or `cru.plugin.set_status`, and invents a
        // `cru.mcp` the plugin VM does not have. Checked against the subset,
        // this gate reported "Key 'shell' not found" against `oci`, `discord`
        // and `worktree` the moment a checker existed — so it passed only
        // while no machine could run it.
        let loader =
            crate::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
                .expect("loader");
        loader
            .generate_stubs(stubs.path())
            .expect("generate declarations");

        let mut failures: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(shipped_plugins_dir()).expect("shipped plugins") {
            let dir = entry.expect("entry").path();
            if !dir.is_dir() {
                continue;
            }
            // The suite too. Excluding it hid 46 diagnostics in the shipped
            // plugins, and `mock(...)` now marks the deliberate monkey-patch
            // so the rest of a test file stays checkable.
            let report = crucible_lua::check_plugin_using(&dir, Some(&definitions), true, &checker)
                .expect("check");
            assert_eq!(
                report.typecheck,
                crucible_lua::TypecheckStatus::Ran,
                "{}: the typecheck did not run, so a pass here would prove nothing",
                dir.display()
            );
            if !report.passed() {
                failures.push(format!(
                    "{}: {}",
                    dir.file_name().unwrap_or_default().to_string_lossy(),
                    report
                        .findings
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }

        assert!(
            failures.is_empty(),
            "shipped plugins failed `cru plugin check`:\n{}",
            failures.join("\n")
        );
    }

    /// The type checker, or a failure telling the caller how to get one.
    ///
    /// Every gate below RUNS a checker; a gate that skipped when none was
    /// installed reported the absence of evidence as evidence, which is the
    /// exact failure these gates exist to prevent. Two of them did that until
    /// 2026-08-31: `just plugin-check` exports `CRUCIBLE_LUAU_ANALYZE`, but the
    /// CI `test` job runs nextest with no such variable and no `luau-lsp`, so
    /// both ran green having checked nothing at all.
    ///
    /// `crucible_lua::find_checker` looks in the three places — the variable an
    /// operator set, the pinned binary `just luau-lsp` writes, the PATH — and
    /// the shipped `cru plugin check` reads exactly the same three. This gate
    /// used to search that list itself and reach `target/tools`, which the CLI
    /// did not: the gate typechecked, the CLI printed SKIPPED over the same
    /// files, and a defect the gate should have caught stayed invisible.
    ///
    /// Nothing here writes the environment. The variable is the OPERATOR's
    /// channel; the gates pass the checker they resolved, so a test cannot
    /// remove a variable another test is reading on the way out.
    fn required_checker() -> crucible_lua::CheckerChoice {
        let choice = crucible_lua::find_checker();
        let checker = match &choice {
            crucible_lua::CheckerChoice::Use(checker) => checker,
            crucible_lua::CheckerChoice::Missing(path) => panic!(
                "CRUCIBLE_LUAU_ANALYZE names {}, which is not a file",
                path.display()
            ),
            crucible_lua::CheckerChoice::None => panic!(
                "no Luau type checker. These gates check types; without one they \
                 would pass having proved nothing. Run `just luau-lsp` to fetch \
                 the pinned build, or set CRUCIBLE_LUAU_ANALYZE to your own."
            ),
        };
        // Whichever of the three it came from, it must PROVE it checks types.
        // The probe used to run on the env-named binary alone, so the pinned
        // build and anything called `luau-lsp` on PATH were trusted on their
        // file name.
        if let Err(why) = checker.proves_types() {
            panic!("{why}");
        }
        choice
    }

    /// The binary behind a resolved choice.
    ///
    /// One gate runs the checker itself, to prove a definitions file LOADS
    /// rather than to check a file. `required_checker` answers `Use` or
    /// panics, so no other variant can reach here.
    fn checker_binary(choice: &crucible_lua::CheckerChoice) -> &std::path::Path {
        match choice {
            crucible_lua::CheckerChoice::Use(checker) => checker.path(),
            _ => unreachable!("required_checker answers Use or panics"),
        }
    }

    /// Every profile's definitions file must LOAD.
    ///
    /// A definitions file that names an undefined type is rejected WHOLE by
    /// `luau-lsp`, which then reports "Unknown global 'cru'" for every line of
    /// every plugin. That is loud, but it is loud in a way that reads like a
    /// hundred plugin bugs rather than one host bug, and the message that says
    /// what really happened goes to stderr with an `[ERROR]` tag that no gate
    /// looked at.
    ///
    /// It has happened twice. Once from rendering `(...: any) -> any`, which is
    /// not valid Luau, and once from declaring a `StatusItem` return before
    /// exporting the type. Both times a plugin author would have seen a
    /// definitions file that appeared to do nothing.
    #[test]
    fn every_profile_definitions_file_loads() {
        let stubs = tempfile::TempDir::new().expect("tempdir");
        let loader =
            crate::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
                .expect("loader");
        loader
            .generate_stubs(stubs.path())
            .expect("generate declarations");

        let checker = required_checker();

        // A file that is syntactically fine but references an undefined type
        // still loads; the reference has to be USED. An empty Lua file is
        // enough — the loader parses the whole definitions file first.
        let probe = stubs.path().join("probe.lua");
        std::fs::write(&probe, "return {}\n").expect("probe");

        let mut broken = Vec::new();
        for profile in crate::vm_profiles::VmProfile::all() {
            let definitions = stubs.path().join(profile.definitions_file());
            assert!(
                definitions.is_file(),
                "{} renders no definitions file",
                profile.name()
            );
            let output = std::process::Command::new(checker_binary(&checker))
                .arg("analyze")
                .arg(format!("--definitions={}", definitions.display()))
                .arg(&probe)
                .output()
                .expect("run the checker");
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stderr.contains("Failed to read definitions")
                || stdout.contains("Failed to read definitions")
            {
                broken.push(format!(
                    "{} ({}): {}{}",
                    profile.name(),
                    profile.definitions_file(),
                    stdout.trim(),
                    stderr.trim()
                ));
            }
        }

        assert!(
            broken.is_empty(),
            "a definitions file did not load, so every `cru.*` in every plugin \
             checked against it reads as an unknown global:\n{}",
            broken.join("\n")
        );
    }

    /// Every OTHER `.lua` file Crucible ships, checked against the profile of
    /// the VM it actually runs on.
    ///
    /// `every_shipped_plugin_typechecks` above walks `runtime/plugins/` only,
    /// which is what `just plugin-check` iterates too. The shipped files
    /// outside it had never been checked by any machine — including
    /// `plugin/templates/init.luau`, the scaffold `cru plugin new` copies,
    /// whose `health.luau` did not pass `cru plugin check`.
    ///
    /// No count here. This paragraph said "eleven" for about a day, and then
    /// the prelude's Lua half was deleted; a tally in prose that no test reads
    /// drifts every time the set changes. `PROFILES` below is the real list,
    /// and the unmapped-file assertion is what keeps it complete.
    ///
    /// The profile matters as much as the coverage. `runtime/defaults/init.lua`
    /// runs on the SESSION VM and a theme on a BARE VM with no `cru` at all;
    /// checked against the daemon definitions they report type errors for
    /// working API,
    /// which is exactly the false failure that made a wider gate look
    /// impossible.
    ///
    /// The mapping is exhaustive by assertion: a new `.lua` anywhere in the
    /// repository fails this test until someone says which VM runs it. That is
    /// the property `just plugin-check`'s glob never had.
    #[test]
    fn every_shipped_lua_file_typechecks() {
        use crate::vm_profiles::VmProfile;

        // (path prefix relative to the repo root, the VM that runs it).
        // Ordered longest-prefix-first is unnecessary: no prefix here contains
        // another.
        const PROFILES: &[(&str, VmProfile)] = &[
            // The shipped defaults and a workspace `init.lua`: session VM.
            ("runtime/defaults/", VmProfile::Daemon),
            // A statusline layout evaluates on a VM with `cru.statusline` and
            // NOTHING else; a theme evaluates on a bare VM with no `cru` at
            // all. Both were mapped to the config profile, which is strictly
            // more permissive than either — so the gate proved a property of a
            // stand-in, and a theme calling `cru.hl.set` passed the check and
            // raised "attempt to index nil with 'hl'" at load.
            ("runtime/statusline/", VmProfile::Statusline),
            ("runtime/themes/", VmProfile::Theme),
            // Ordinary plugins, and the scaffold for writing one.
            ("runtime/crucible-help/", VmProfile::Daemon),
            ("examples/plugins/", VmProfile::Daemon),
            (
                "crates/crucible-cli/src/commands/plugin/templates/",
                VmProfile::Daemon,
            ),
        ];

        let checker = required_checker();

        let root = repo_root();
        let stubs = tempfile::TempDir::new().expect("tempdir");
        let loader =
            crate::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())
                .expect("loader");
        loader
            .generate_stubs(stubs.path())
            .expect("generate declarations");

        let mut unmapped: Vec<String> = Vec::new();
        let mut failures: Vec<String> = Vec::new();

        for file in lua_files_outside_plugins(&root) {
            let relative = file
                .strip_prefix(&root)
                .expect("under the repo root")
                .to_string_lossy()
                .replace('\\', "/");
            let Some((_, profile)) = PROFILES
                .iter()
                .find(|(prefix, _)| relative.starts_with(prefix))
            else {
                unmapped.push(relative);
                continue;
            };
            let definitions = stubs.path().join(profile.definitions_file());
            // One file at a time: `check_plugin` takes a directory, and these
            // are loose files whose neighbours may run on a different VM.
            let report =
                crucible_lua::check_file_using(&file, Some(&definitions), &checker).expect("check");
            assert_eq!(
                report.typecheck,
                crucible_lua::TypecheckStatus::Ran,
                "{relative}: the typecheck did not run, so a pass here would prove nothing"
            );
            if !report.passed() {
                failures.push(format!(
                    "{relative} (profile {}): {}",
                    profile.name(),
                    report
                        .findings
                        .iter()
                        .map(|f| f.to_string())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
            }
        }

        assert!(
            unmapped.is_empty(),
            "these shipped .lua files name no VM profile, so nothing knows which \
             definitions to check them against. Add each to PROFILES:\n{}",
            unmapped.join("\n")
        );
        assert!(
            failures.is_empty(),
            "shipped Lua outside runtime/plugins/ failed its typecheck:\n{}",
            failures.join("\n")
        );
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .canonicalize()
            .expect("repo root")
    }

    /// Every Lua source file git tracks, except the plugins the test above
    /// already walks.
    ///
    /// The set is what git tracks, not what a directory walk finds: a nested
    /// checkout under `.worktrees/`, build output, and a stray untracked file
    /// are outside it by definition, so no name list has to keep up with them.
    /// A new shipped Lua file is tracked, so it still reaches the profile gate.
    fn lua_files_outside_plugins(root: &std::path::Path) -> Vec<PathBuf> {
        let output = std::process::Command::new("git")
            .args([
                "-C",
                root.to_str().expect("utf-8 repo root"),
                "ls-files",
                "-z",
            ])
            .output()
            .expect("git ls-files runs");
        assert!(
            output.status.success(),
            "git ls-files failed under {}",
            root.display()
        );
        let mut out: Vec<PathBuf> = String::from_utf8_lossy(&output.stdout)
            .split('\0')
            .filter(|rel| !rel.is_empty())
            .filter(|rel| !rel.starts_with("runtime/plugins/"))
            .map(|rel| root.join(rel))
            .filter(|path| crucible_lua::source_files::is_lua_source(path))
            .collect();
        out.sort();
        out
    }

    #[test]
    fn the_typecheck_walk_sees_tracked_lua_files_only() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let root = tmp.path();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .status()
                .expect("git runs");
            assert!(status.success(), "git {args:?}");
        };
        git(&["init", "-q"]);
        std::fs::write(root.join(".gitignore"), "ignored/\n").unwrap();
        std::fs::write(root.join("tracked.luau"), "return 1\n").unwrap();
        std::fs::write(root.join("untracked.luau"), "return 2\n").unwrap();
        std::fs::create_dir_all(root.join("ignored")).unwrap();
        std::fs::write(root.join("ignored/nested.luau"), "return 3\n").unwrap();
        std::fs::create_dir_all(root.join("runtime/plugins/x")).unwrap();
        std::fs::write(root.join("runtime/plugins/x/init.luau"), "return 4\n").unwrap();
        git(&[
            "add",
            ".gitignore",
            "tracked.luau",
            "runtime/plugins/x/init.luau",
        ]);

        let found = lua_files_outside_plugins(&root.canonicalize().unwrap());
        let names: Vec<String> = found
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["tracked.luau"], "{found:?}");
    }

    /// setup that lets `require("config")` resolve a plugin's lua/ submodule.
    //
    // Arms are listed explicitly so that a plugin can sit in `runtime/plugins/`
    // while still experimental without being forced into CI. What makes that
    // safe is `every_shipped_plugin_with_a_suite_is_gated` below: the omission
    // has to be declared in `NO_LUA_SUITE` with a reason, so the list cannot
    // drift behind the directory the way it did for `web-search` and
    // `worktree`. `every_shipped_plugin_is_discovered` and
    // `every_shipped_plugin_executes` still walk the real directory, so a
    // broken plugin in the tree fails CI regardless of this list.
    #[test_case("auto-title")]
    #[test_case("consolidation")]
    #[test_case("daily-notes")]
    #[test_case("discord")]
    #[test_case("oci")]
    #[test_case("reflection")]
    #[test_case("retrieval-lab")]
    #[test_case("review")]
    #[test_case("todo-list")]
    #[test_case("web-search")]
    #[test_case("worktree")]
    fn shipped_plugin_lua_suite_passes(plugin: &str) {
        let plugin_dir = shipped_plugins_dir().join(plugin);
        let result = run_plugin_tests(&plugin_dir.to_string_lossy());

        let passed = result["passed"].as_u64().unwrap_or(0);
        let failed = result["failed"].as_u64().unwrap_or(u64::MAX);
        let load_failures = result["load_failures"].as_u64().unwrap_or(u64::MAX);

        assert_eq!(
            load_failures,
            0,
            "{plugin}: test files should load:{}",
            describe_failures(&result)
        );
        assert_eq!(
            failed,
            0,
            "{plugin}: {failed} Lua test(s) failed:{}",
            describe_failures(&result)
        );
        assert!(passed > 0, "{plugin}: expected passing assertions");
    }

    /// The arms of `shipped_plugin_lua_suite_passes`, read out of this file.
    ///
    /// Parsing the source is ugly, and it is still the only way to compare the
    /// gated set against the directory: `#[test_case]` arms are consumed by a
    /// proc macro and leave nothing to enumerate at runtime.
    fn gated_plugins() -> Vec<String> {
        let source = include_str!("lua_plugin_suite.rs");
        let body = source
            .split_once("fn shipped_plugin_lua_suite_passes")
            .map(|(before, _)| before)
            .expect("the gate function must exist");
        body.lines()
            .filter_map(|line| {
                line.trim()
                    .strip_prefix("#[test_case(\"")?
                    .strip_suffix("\")]")
                    .map(str::to_string)
            })
            .collect()
    }

    /// Every shipped plugin either has its suite gated or says why it does not.
    ///
    /// A plugin that registers a hook at body level must claim its own
    /// `package.loaded` entry.
    ///
    /// The daemon executes `init.lua` BY PATH (`daemon_plugins::execute_plugin`
    /// → `lua.load(source).eval_async()`), never through `require`. So a
    /// documented `require("<plugin>").setup{…}` in a user's `init.lua` loads a
    /// SECOND copy of the file: new upvalues, and every body-level
    /// `cru.on_*` call runs again. The handler is then registered twice
    /// and fires twice per event — for `reflection` that is two forked
    /// cheap-model reviews and two sets of staged proposals per session end.
    ///
    /// This exists because fixing it once was not enough. `auto-title` was
    /// repaired on 2026-08-18 and the enumeration stopped there; an independent
    /// reviewer then found `reflection` and `oci` carrying the identical shape,
    /// with `require("reflection").setup` documented in five shipped places.
    /// That is the third instance this repo has recorded of "fixed one call
    /// site of many", so the fix is a sweep over every plugin rather than a
    /// third patch.
    ///
    /// Textual on purpose: it must fail for a plugin whose Lua the test harness
    /// cannot construct, and the property is a property of the source.
    #[test]
    fn a_plugin_registering_a_hook_at_body_level_guards_its_require() {
        let mut unguarded = Vec::new();

        for entry in std::fs::read_dir(shipped_plugins_dir()).expect("runtime/plugins must exist") {
            let dir = entry.expect("readable dir entry").path();
            if !dir.is_dir() {
                continue;
            }
            let init = dir.join("init.lua");
            let Ok(src) = std::fs::read_to_string(&init) else {
                continue;
            };
            let name = dir
                .file_name()
                .and_then(|n| n.to_str())
                .expect("plugin dir name is UTF-8")
                .to_string();

            // Body level means column zero: an `on_*` call indented inside a
            // function runs when that function does, not on re-execution.
            //
            // ANY receiver, not just `crucible`/`cru`. Narrowing it to those
            // two let `discord` through: it registers with `gateway.on(...)` on
            // a `require`d module whose `Emitter:on` appends, so a second
            // execution handled every Discord message twice — two agent turns,
            // two replies, two quota charges — and `tests/service_test.lua`
            // already does `require("discord")`. The property is "a handler is
            // attached when this file runs", and the receiver's name has
            // nothing to do with it.
            let body_level_hook = src.lines().any(|l| {
                let Some((receiver, rest)) = l.split_once(['.', ':']) else {
                    return false;
                };
                !receiver.is_empty()
                    && receiver
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !receiver.starts_with(|c: char| c.is_ascii_digit())
                    && (rest.starts_with("on(") || rest.starts_with("on_"))
            });
            // The ASSIGNMENT, not the substring. Checking `contains` matched
            // the guard's own explanatory comment, so this test passed with
            // both guards deleted — decorative in exactly the way the review
            // has been catching all week. Found by red-proofing it.
            let guarded = src.lines().any(|l| {
                let l = l.trim_start();
                !l.starts_with("--") && l.starts_with("package.loaded[") && l.contains("] =")
            });
            if body_level_hook && !guarded {
                unguarded.push(name);
            }
        }

        assert!(
            unguarded.is_empty(),
            "these plugins register a hook at body level but never set \
             `package.loaded`, so the documented `require(\"<name>\").setup{{…}}` \
             loads a second copy and registers the hook twice:\n  - {}\n\
             Add `package.loaded[\"<name>\"] = plugin` before the final return, \
             as `auto-title`, `web-search`, `reflection` and `oci` do.",
            unguarded.join("\n  - ")
        );
    }

    /// The gate that was missing. `shipped_plugin_lua_suite_passes` listed four
    /// plugins while the directory held seven, so `web-search` (148 assertions)
    /// and `worktree` (42) ran only under `just test plugins` — a heavier tier
    /// needing a built binary and a live daemon — and nothing said so.
    #[test]
    fn every_shipped_plugin_with_a_suite_is_gated() {
        let gated = gated_plugins();
        assert!(!gated.is_empty(), "failed to parse the #[test_case] arms");

        let mut ungated = Vec::new();
        let mut stale_exemptions = Vec::new();

        for entry in std::fs::read_dir(shipped_plugins_dir()).expect("runtime/plugins must exist") {
            let path = entry.expect("readable dir entry").path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("plugin dir name is UTF-8")
                .to_string();

            let has_suite = super::discover_plugin_test_files(&path)
                .map(|files| !files.is_empty())
                .unwrap_or(false);
            let exempt = NO_LUA_SUITE.iter().any(|(n, _)| *n == name);
            let is_gated = gated.contains(&name);

            if has_suite && !is_gated && !exempt {
                ungated.push(name);
            } else if !has_suite && !exempt && !is_gated {
                ungated.push(format!("{name} (no tests/ at all)"));
            } else if exempt && has_suite {
                stale_exemptions.push(name);
            }
        }

        assert!(
            ungated.is_empty(),
            "these shipped plugins are not in the CI gate: {ungated:?}\n\
             Add a #[test_case(\"<name>\")] arm to shipped_plugin_lua_suite_passes, \
             or an entry to NO_LUA_SUITE saying why not."
        );
        assert!(
            stale_exemptions.is_empty(),
            "these plugins are in NO_LUA_SUITE but DO have a suite now: {stale_exemptions:?}\n\
             Drop the exemption and add a #[test_case] arm."
        );
    }

    /// No arm names a plugin that is no longer there — a stale arm fails with
    /// "0 passed" and reads as a broken suite rather than a deleted plugin.
    #[test]
    fn every_gated_plugin_still_exists() {
        for name in gated_plugins() {
            assert!(
                shipped_plugins_dir().join(&name).is_dir(),
                "gate names '{name}', which is not a directory under runtime/plugins"
            );
        }
    }
}

/// `cru plugin test` reported bare counts: the runner's per-test `✗ name /
/// error / line` output went to the *daemon's* stdout, so a CLI user saw
/// "0 passed, 9 failed" and nothing actionable.
#[cfg(test)]
mod plugin_test_diagnostics_tests {
    use crucible_core::protocol::Request;
    use std::fs;
    use tempfile::TempDir;

    fn run(dir: &std::path::Path) -> serde_json::Value {
        let req = Request {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: "lua.run_plugin_tests".to_string(),
            params: serde_json::json!({ "test_path": dir.to_string_lossy() }),
        };
        let resp = tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(super::handle_lua_run_plugin_tests(req));
        assert!(resp.error.is_none(), "handler errored: {:?}", resp.error);
        resp.result.expect("result present")
    }

    /// The runner grants exactly what the runtime grants, and nothing more.
    ///
    /// A plugin's own module is `require("<module>")`, resolved under the
    /// plugin's `lua/` directory. `require("lua.<module>")` is NOT a thing the
    /// daemon can resolve — and the harness used to grant `<plugin_dir>/?.lua`
    /// on top of the runtime's roots, under which that spelling worked here
    /// and failed in the daemon. A suite could be green against a plugin that
    /// could not load.
    #[test]
    fn the_runner_refuses_a_require_the_runtime_cannot_resolve() {
        let tmp = TempDir::new().unwrap();
        let plugin = tmp.path().join("parity-probe");
        fs::create_dir_all(plugin.join("lua")).unwrap();
        fs::write(plugin.join("lua/container.lua"), "return { ok = true }").unwrap();
        fs::write(
            plugin.join("init.lua"),
            "return { name = \"parity-probe\", version = \"0.1.0\" }",
        )
        .unwrap();
        fs::create_dir_all(plugin.join("tests")).unwrap();
        fs::write(
            plugin.join("tests/private_test.lua"),
            r#"
describe("module resolution", function()
    it("resolves the plugin's own module the way the daemon does", function()
        expect.truthy(require("container").ok)
    end)

    it("does not resolve it through a lua. prefix", function()
        expect.truthy(not pcall(require, "lua.container"))
    end)
end)
"#,
        )
        .unwrap();

        let result = run(&plugin);
        assert_eq!(result["failed"].as_u64(), Some(0), "{result:#}");
        assert_eq!(result["passed"].as_u64(), Some(2), "{result:#}");
        assert_eq!(result["load_failures"].as_u64(), Some(0), "{result:#}");
    }

    #[test]
    fn a_failing_test_returns_its_name_error_and_line() {
        let tmp = TempDir::new().unwrap();
        // The failing assertion is on line 3 of this file, deliberately not
        // line 1 — a location scraped from `debug.traceback()` points inside
        // the runner and would not match.
        fs::write(
            tmp.path().join("arithmetic_test.lua"),
            "describe('math', function()\n\
               it('adds two numbers', function()\n\
                 expect.equal(3, 1 + 1)\n\
               end)\n\
             end)\n",
        )
        .unwrap();

        let result = run(tmp.path());
        assert_eq!(result["failed"], 1, "{result:#}");

        let failures = result["failures"]
            .as_array()
            .unwrap_or_else(|| panic!("expected a `failures` array: {result:#}"));
        assert_eq!(failures.len(), 1, "{result:#}");
        assert_eq!(failures[0]["name"], "adds two numbers");
        assert!(
            failures[0]["error"]
                .as_str()
                .is_some_and(|e| e.contains("Expected")),
            "failure should carry the assertion message: {result:#}"
        );
        assert_eq!(
            failures[0]["line"], "3",
            "line must point at the assertion in the test file, not at a frame \
             inside the runner: {result:#}"
        );
        assert!(
            failures[0]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("arithmetic_test.lua")),
            "failure should name the test file: {result:#}"
        );
    }

    /// Two suites can share an `it` name — several shipped plugins have their
    /// own "returns nil on non-JSON". Reporting the bare name makes a red run
    /// ambiguous about which suite broke.
    #[test]
    fn a_failure_carries_its_full_describe_path() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("nested_test.lua"),
            "describe('outer', function()\n\
               describe('inner', function()\n\
                 it('fails', function() expect.truthy(false) end)\n\
               end)\n\
             end)\n",
        )
        .unwrap();

        let result = run(tmp.path());
        let failures = result["failures"].as_array().expect("failures array");
        assert_eq!(failures.len(), 1, "{result:#}");
        assert_eq!(failures[0]["suite"], "outer / inner", "{result:#}");
    }

    #[test]
    fn a_test_file_that_does_not_load_reports_which_file_and_why() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("broken_test.lua"),
            "describe('unclosed', function()\n",
        )
        .unwrap();

        let result = run(tmp.path());
        assert_eq!(result["load_failures"], 1, "{result:#}");

        let details = result["load_failure_details"]
            .as_array()
            .unwrap_or_else(|| panic!("expected `load_failure_details`: {result:#}"));
        assert_eq!(details.len(), 1, "{result:#}");
        assert!(
            details[0]["file"]
                .as_str()
                .is_some_and(|f| f.ends_with("broken_test.lua")),
            "{result:#}"
        );
        assert!(
            details[0]["error"].as_str().is_some_and(|e| !e.is_empty()),
            "load failure must say why: {result:#}"
        );
    }
}
