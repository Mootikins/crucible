//! The `cru plugin test` runner: discovers a plugin's `*_test.lua` files,
//! loads them into an executor whose `package.path` mirrors the runtime
//! plugin loader exactly, and runs them through the bundled busted-style
//! framework. Includes the CI gates that run every shipped plugin's suite.

use super::*;

pub(crate) async fn handle_lua_run_plugin_tests(req: Request) -> Response {
    let test_path_str = require_param!(req, "test_path", as_str).to_string();
    let filter = optional_param!(req, "filter", as_str).map(|s| s.to_string());
    let test_path = PathBuf::from(&test_path_str);

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

    // Mirror the runtime plugin loader's package.path EXACTLY. The loader
    // gives a plugin `<plugins_parent>/?.lua`, `<plugins_parent>/?/init.lua`
    // (configure_runtime_path) and `<plugin_dir>/lua/?.lua` (execute_plugin) —
    // nothing else. The harness previously also granted `<plugin_dir>/?.lua`,
    // under which `require("lua.container")` resolved in tests while failing
    // in the daemon: a suite could be green against a plugin that could not
    // load. Tests load their plugin the way the runtime does:
    // `require("<plugin-name>")` via the parent's `?/init.lua` entry.
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
    let plugin_root_str = plugin_root.to_string_lossy();
    if let Err(e) = executor
        .lua()
        .load(format!(
            r#"
local plugin_root = {plugin_root_str:?}
local plugin_parent = plugin_root:match("^(.*)/[^/]+$") or plugin_root
local entries = {{
    plugin_parent .. "/?.lua",
    plugin_parent .. "/?/init.lua",
    plugin_root .. "/lua/?.lua",
}}
for _, entry in ipairs(entries) do
    if not ((";" .. package.path .. ";"):find(";" .. entry .. ";", 1, true)) then
        package.path = entry .. ";" .. package.path
    end
end
"#
        ))
        .set_name("plugin_package_path")
        .exec()
    {
        return internal_error(req.id, e);
    }

    // Make a Fennel plugin requirable by name, the way a Lua one already is.
    //
    // `package.path` has no `.fnl` templates and Lua has no Fennel searcher —
    // the daemon compiles a plugin's `.fnl` main by path and never goes
    // through `require` — so `(require :graph-view)` from that plugin's own
    // suite failed with "module not found". A Fennel plugin was therefore
    // untestable: the runner would compile its *test* files and then have no
    // way to load the thing under test. Preloading the compiled main closes
    // that without inventing a searcher the daemon does not have.
    if let Err(e) = preload_fennel_main(&executor, &plugin_root) {
        return internal_error(req.id, e);
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

        // `.fnl` suites compile to Lua first — discovery has always accepted
        // them, but they were loaded raw, so every Fennel test file counted
        // as a load failure with a Lua parse error that never said why.
        let is_fennel = file.extension().is_some_and(|e| e == "fnl");
        let file_contents = if is_fennel {
            match executor.compile_fennel_source(&file_contents) {
                Ok(lua_source) => lua_source,
                Err(e) => {
                    note_load_failure(file, format!("fennel compile: {e}"));
                    continue;
                }
            }
        } else {
            file_contents
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

/// Put `<plugin_root>/init.fnl`, compiled, into `package.preload` under the
/// plugin's directory name.
///
/// A no-op for a Lua plugin, or for a path that is a single file rather than a
/// plugin directory. A compile error is returned rather than swallowed: a
/// Fennel plugin whose main does not compile should fail the run loudly, not
/// look like a plugin with no tests.
fn preload_fennel_main(executor: &LuaExecutor, plugin_root: &Path) -> Result<()> {
    let main = plugin_root.join("init.fnl");
    if !main.is_file() {
        return Ok(());
    }
    let Some(name) = plugin_root.file_name().and_then(|n| n.to_str()) else {
        return Ok(());
    };

    let source = std::fs::read_to_string(&main)?;
    let lua_source =
        anyhow::Context::with_context(executor.compile_fennel_source(&source), || {
            format!("compiling {}", main.display())
        })?;

    let chunk = executor
        .lua()
        .load(&lua_source)
        .set_name(format!("@{}", main.display()))
        .into_function()?;

    let preload: mlua::Table = executor
        .lua()
        .globals()
        .get::<mlua::Table>("package")?
        .get("preload")?;
    preload.set(name, chunk)?;
    Ok(())
}

/// Discover test files in a plugin directory (files ending with _test.lua or _test.fnl)
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

pub(super) fn collect_plugin_test_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_file() {
            let stem = path.file_stem().and_then(|name| name.to_str());
            let ext = path.extension().and_then(|e| e.to_str());
            if matches!((stem, ext), (Some(s), Some("lua" | "fnl")) if s.ends_with("_test")) {
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
    #[test_case("daily-notes")]
    #[test_case("discord")]
    #[test_case("graph-view")]
    #[test_case("kiln-expert")]
    #[test_case("oci")]
    #[test_case("reflection")]
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

    /// `.fnl` suites have always been *discovered*; until the compile step
    /// they were loaded as raw Lua and every one counted as a load failure.
    #[test]
    fn a_fennel_test_file_compiles_and_runs() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("arith_test.fnl"),
            "(describe \"fennel math\" (fn []\n\
               (it \"adds two numbers\" (fn []\n\
                 (assert.equal 2 (+ 1 1))))))\n",
        )
        .unwrap();

        let result = run(tmp.path());
        assert_eq!(
            result["load_failures"].as_u64(),
            Some(0),
            "fennel suite should compile, not fail to load: {result:#}"
        );
        assert_eq!(result["passed"].as_u64(), Some(1), "{result:#}");
        assert_eq!(result["failed"].as_u64(), Some(0), "{result:#}");
    }

    /// A Fennel syntax error is reported as a compile failure naming the
    /// file, not a baffling Lua parse error.
    #[test]
    fn a_broken_fennel_file_reports_a_compile_error() {
        let tmp = TempDir::new().unwrap();
        fs::write(
            tmp.path().join("broken_test.fnl"),
            "(describe \"unclosed\"\n",
        )
        .unwrap();

        let result = run(tmp.path());
        assert_eq!(result["load_failures"].as_u64(), Some(1), "{result:#}");
        let detail = &result["load_failure_details"][0];
        assert!(
            detail["error"]
                .as_str()
                .is_some_and(|e| e.contains("fennel compile")),
            "failure should say it was a fennel compile error: {result:#}"
        );
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
                 assert.equal(3, 1 + 1)\n\
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
                 it('fails', function() assert.truthy(false) end)\n\
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
