//! The `cru plugin test` runner: discovers a plugin's `*_test.lua` files,
//! activates the plugin on a fresh `DaemonPluginLoader` with the body the
//! daemon uses, then runs the files through the bundled busted-style
//! framework on that VM. Includes the CI gates that run every shipped
//! plugin's suite.

use super::*;
use crucible_lua::manifest::PluginSource;
use std::collections::HashMap;

/// Activate the plugin at `plugin_root` the way the daemon does, on a
/// loader of its own.
///
/// The loader is `DaemonPluginLoader::new` with no config, so the plugin
/// meets the real `cru.*` modules, the real `require` searcher and the real
/// activation body: discovery, the private module root, `setup(opts)`, the
/// tool registration and the handler store. A `require("<name>")` from a
/// test file resolves through the parent directory the loader searches,
/// and a `require("<private>")` resolves through the plugin directory the
/// activation entered, from the file that asked.
///
/// A directory with no entry file is not a plugin. The suite then runs on a
/// bare loader VM, and the answer names no plugin.
pub(super) async fn activate_plugin_under_test(
    plugin_root: &Path,
) -> anyhow::Result<(DaemonPluginLoader, Option<String>)> {
    let mut loader = DaemonPluginLoader::new(HashMap::new())?;
    let entry = crucible_lua::source_files::init_file(plugin_root)
        .map_err(|ambiguous| anyhow::anyhow!("{}: {ambiguous}", plugin_root.display()))?;
    if entry.is_none() {
        return Ok((loader, None));
    }
    let name = plugin_root
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("{}: not a plugin directory", plugin_root.display()))?
        .to_string();
    // The daemon searches a directory of plugins, so the parent goes on the
    // path. The source is a label for `plugin.list`, which the runner never
    // answers.
    let parent = plugin_root
        .parent()
        .ok_or_else(|| anyhow::anyhow!("{}: has no parent", plugin_root.display()))?
        .to_path_buf();
    loader.add_plugin_paths(&[(parent, PluginSource::Runtime)])?;
    loader
        .activate_plugin(&name)
        .await
        .map_err(|e| anyhow::anyhow!("plugin '{name}' did not activate: {e}"))?;
    Ok((loader, Some(name)))
}

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
    // Canonical from here on. `require` resolves a plugin's private module
    // from the FILE that asked, and that file must sit under the plugin
    // directory the loader recorded. A `..` component in either path breaks
    // the match.
    let test_path = test_path
        .canonicalize()
        .unwrap_or_else(|_| test_path.clone());

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

    let plugin_root = if test_path.is_file() {
        test_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| test_path.clone())
    } else {
        test_path.clone()
    };

    // The plugin activates BEFORE the harness goes in, on the loader the
    // daemon uses. So `setup` runs against the real `cru.*` modules, the
    // tools land in the plugin registry and the handlers land in the handler
    // store, as they do at daemon boot. The runner used to build a bare
    // `LuaExecutor` and reproduce the module roots by hand. That VM ran no
    // activation, so a plugin whose `setup` raised in the daemon passed its
    // suite here. An activation failure is now the answer to the request.
    let (loader, _) = match activate_plugin_under_test(&plugin_root).await {
        Ok(outcome) => outcome,
        Err(e) => return internal_error(req.id, e),
    };
    let executor = loader.executor();
    // This VM runs plugin tests, so it gets `describe`, `it`, `run_tests` and
    // the harness `assert`. No other VM does. The harness chunk captures the
    // real `cru.fs` functions here, before `test_mocks.setup()` replaces the
    // namespaces a suite stubs.
    if let Err(e) = executor.install_test_harness() {
        return internal_error(req.id, anyhow::Error::from(e));
    }

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
    /// Every shipped plugin's entry file is the file that is really there.
    ///
    /// The gate B3 needed and did not have. `crucible-help` declared
    /// `main: init.lua` beside an `init.luau` and silently did not load,
    /// because the sweep that renamed the other eleven walked
    /// `runtime/plugins/` and it sat outside. The field is gone now, so this
    /// asserts the property the field used to be able to violate: the
    /// resolver finds exactly one entry file per plugin.
    ///
    /// It derives its list from the directory rather than a maintained one,
    /// which is the difference between this and the sweep that missed.
    #[test]
    fn every_shipped_plugin_resolves_exactly_one_entry_file() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../runtime/plugins")
            .canonicalize()
            .expect("the shipped plugin tree");

        let mut checked = 0;
        for entry in std::fs::read_dir(&root).expect("read runtime/plugins") {
            let dir = entry.expect("dir entry").path();
            if !dir.is_dir() {
                continue;
            }
            let found = crucible_lua::source_files::init_file(&dir);
            match found {
                Ok(Some(path)) => assert!(
                    path.is_file(),
                    "{} resolved to a file that is not there: {}",
                    dir.display(),
                    path.display()
                ),
                Ok(None) => panic!("{} ships no init.luau or init.lua", dir.display()),
                Err(ambiguous) => {
                    panic!("{} ships both entry spellings: {ambiguous}", dir.display())
                }
            }
            checked += 1;
        }
        assert!(checked >= 12, "expected the shipped plugins, saw {checked}");
    }

    const NO_LUA_SUITE: &[(&str, &str)] = &[(
        "crucible-help",
        "It registers the shipped documentation as skill context and declares \
         no tools, commands or handlers, so there is no behaviour a Lua suite \
         could assert. The skills it ships are covered by \
         `skills_extracted_from_the_binary_are_discovered`, and its own \
         loading by `every_shipped_plugin_executes`.",
    )];

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
            //
            // On the loader's VM, so the module body runs and a top-level
            // registration is a finding. `check_plugin_using` reads the
            // declarations only, so this gate did not see such an effect in a
            // shipped plugin.
            let report = crucible_lua::check_plugin_on(
                &dir,
                Some(&definitions),
                true,
                &checker,
                loader.executor(),
            )
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
    /// runs on the DAEMON VM and a theme on a BARE VM with no `cru` at all;
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
            // The shipped defaults file: it runs on the daemon VM.
            ("runtime/defaults/", VmProfile::Daemon),
            // A statusline layout evaluates on a VM with `cru.statusline` and
            // NOTHING else; a theme evaluates on a bare VM with no `cru` at
            // all. Both were mapped to the config profile, which is strictly
            // more permissive than either — so the gate proved a property of a
            // stand-in, and a theme calling `cru.hl.set` passed the check and
            // raised "attempt to index nil with 'hl'" at load.
            ("runtime/statusline/", VmProfile::Statusline),
            ("runtime/themes/", VmProfile::Theme),
            // The reference config and its example fragments. A user copies
            // these into `init.lua`, which the daemon VM evaluates, so they
            // are checked against the same definitions that file gets.
            ("docs/init.lua", VmProfile::Daemon),
            ("docs/examples/config/", VmProfile::Daemon),
            // The demo configs the recording recipes pass to `--config`. A
            // config root's `init.lua` runs on the daemon VM, the same as the
            // reference config above.
            ("assets/demo-config/", VmProfile::Daemon),
            ("assets/demo-acp-config/", VmProfile::Daemon),
            // Ordinary plugins, and the scaffold for writing one.
            ("runtime/plugins/crucible-help/", VmProfile::Daemon),
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
    #[test_case("kanban")]
    #[test_case("consolidation")]
    #[test_case("daily-notes")]
    #[test_case("discord")]
    #[test_case("graph")]
    #[test_case("oci")]
    #[test_case("reflection")]
    #[test_case("retrieval-lab")]
    #[test_case("review")]
    #[test_case("session-board")]
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

    /// The suite runs the plugin the daemon activated, not a copy the runner
    /// built by hand.
    ///
    /// `activate_plugin_under_test` is the one function the runner and this
    /// gate share, so what this gate proves about the loader holds for every
    /// suite `shipped_plugin_lua_suite_passes` runs. Three facts, each read
    /// from the loader and none from source text: the plugin is `Active`,
    /// every tool its spec declares is in the plugin registry, and a
    /// `setup` that calls `cru.on` left its registrations in the handler
    /// store under the plugin's own source.
    ///
    /// The tool count comes from the running loader. One plugin activates
    /// per loader, so the registry holds that plugin's tools and no others,
    /// and the count must equal the spec's.
    ///
    /// This replaces a gate that read each `init.lua` for a `package.loaded`
    /// assignment. Its own comment recorded that it passed with both guards
    /// deleted, because the substring it looked for matched a comment.
    #[tokio::test]
    async fn the_suite_runs_the_plugin_the_daemon_activated() {
        let mut checked = 0;
        for entry in std::fs::read_dir(shipped_plugins_dir()).expect("runtime/plugins must exist") {
            let dir = entry.expect("readable dir entry").path();
            if !dir.is_dir() {
                continue;
            }
            let dir = dir.canonicalize().expect("plugin dir");
            let (loader, activated) = super::activate_plugin_under_test(&dir)
                .await
                .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
            let name = activated.unwrap_or_else(|| panic!("{} is not a plugin", dir.display()));

            assert_eq!(
                loader.plugin_state(&name),
                Some(crucible_lua::manifest::PluginState::Active),
                "{name}: the runner activates a plugin before it runs the suite"
            );

            let info = loader
                .loaded_plugin_info()
                .into_iter()
                .find(|p| p["name"].as_str() == Some(name.as_str()))
                .unwrap_or_else(|| panic!("{name}: missing from plugin info"));
            let declared_tools = info["tools"].as_u64().expect("tool count") as usize;
            let registered = loader.plugin_registry().tool_names();
            assert_eq!(
                registered.len(),
                declared_tools,
                "{name}: declares {declared_tools} tool(s), the registry holds {registered:?}"
            );

            let handlers = loader
                .plugin_handlers()
                .for_source(&crucible_lua::LuaSource::Plugin(name.clone()));
            if name == "session-board" {
                // Its `setup` subscribes to `session:created` and
                // `session:ended`, and its own suite asserts the two calls.
                assert_eq!(
                    handlers.len(),
                    2,
                    "{name}: setup() registered {} handler(s) under its source",
                    handlers.len()
                );
            }
            checked += 1;
        }
        assert!(checked >= 12, "expected the shipped plugins, saw {checked}");
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
