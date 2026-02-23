use anyhow::{Context, Result};
use colored::Colorize;
use crucible_lua::LuaExecutor;
use mlua::{Function, Table};
use std::path::{Path, PathBuf};

use super::TestArgs;
use crate::config::CliConfig;

pub async fn execute(_config: CliConfig, args: TestArgs) -> Result<()> {
    if !args.path.exists() {
        eprintln!("{} Path does not exist: {}", "✗".red(), args.path.display());
        std::process::exit(2);
    }

    let test_files = discover_test_files(&args.path)?;
    if test_files.is_empty() {
        println!("No test files found in {}", args.path.display());
        return Ok(());
    }

    let executor = LuaExecutor::new().context("failed to initialize Lua runtime")?;

    // Set package.path to include the plugin root so test files can `require("init")` etc.
    let plugin_root = args
        .path
        .canonicalize()
        .unwrap_or_else(|_| args.path.clone());
    let plugin_root_str = plugin_root.to_string_lossy();
    executor
        .lua()
        .load(format!(
            r#"
local plugin_root = {plugin_root_str:?}
local entries = {{
    plugin_root .. "/?.lua",
    plugin_root .. "/?/init.lua",
}}
for _, entry in ipairs(entries) do
    if not package.path:find(entry, 1, true) then
        package.path = entry .. ";" .. package.path
    end
end
"#
        ))
        .set_name("plugin_package_path")
        .exec()
        .context("failed to configure plugin package path")?;

    executor
        .lua()
        .load("test_mocks.setup()")
        .set_name("test_mocks_setup")
        .exec()
        .context("failed to setup test mocks")?;

    if let Some(filter) = &args.filter {
        executor
            .lua()
            .globals()
            .set("__cru_plugin_test_filter", filter.clone())?;
        executor
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
            .context("failed to apply test filter")?;
    }

    let mut load_failures = 0usize;

    for file in &test_files {
        let file_contents = match std::fs::read_to_string(file) {
            Ok(contents) => contents,
            Err(err) => {
                eprintln!("{} Failed to read {}: {}", "✗".red(), file.display(), err);
                load_failures += 1;
                continue;
            }
        };

        let chunk = if file.extension().is_some_and(|ext| ext == "fnl") {
            match compile_fennel(executor.lua(), &file_contents) {
                Ok(Some(compiled)) => compiled,
                Ok(None) => {
                    println!(
                        "{} Skipping {} (Fennel compiler not available)",
                        "⚠".yellow(),
                        file.display()
                    );
                    continue;
                }
                Err(err) => {
                    eprintln!(
                        "{} Failed to compile {}: {}",
                        "✗".red(),
                        file.display(),
                        err
                    );
                    load_failures += 1;
                    continue;
                }
            }
        } else {
            file_contents
        };

        let chunk_name = file.to_string_lossy();
        if let Err(err) = executor
            .lua()
            .load(&chunk)
            .set_name(chunk_name.as_ref())
            .exec()
        {
            eprintln!("{} Failed to load {}: {}", "✗".red(), file.display(), err);
            load_failures += 1;
        }
    }

    let results: Table = executor
        .lua()
        .load("return run_tests()")
        .set_name("plugin_test_runner")
        .eval()
        .context("failed to execute test runner")?;

    let passed: usize = results.get("passed")?;
    let failed: usize = results.get("failed")?;

    println!(
        "{}, {}",
        format!("{} passed", passed).green(),
        format!("{} failed", failed).red()
    );

    if load_failures > 0 {
        eprintln!(
            "{} {} test file(s) failed to load",
            "✗".red(),
            load_failures
        );
        std::process::exit(2);
    }

    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn is_test_file(path: &Path) -> bool {
    let stem = path.file_stem().and_then(|name| name.to_str());
    let ext = path.extension().and_then(|e| e.to_str());
    matches!((stem, ext), (Some(s), Some("lua" | "fnl")) if s.ends_with("_test"))
}

fn collect_test_files_from(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in
        std::fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?
    {
        let path = entry?.path();
        if path.is_file() && is_test_file(&path) {
            out.push(path);
        }
    }
    Ok(())
}

fn discover_test_files(path: &Path) -> Result<Vec<PathBuf>> {
    if path.is_file() {
        return Ok(vec![path.to_path_buf()]);
    }

    let mut files = Vec::new();

    let tests_dir = path.join("tests");
    if tests_dir.is_dir() {
        collect_test_files_from(&tests_dir, &mut files)?;
    }

    collect_test_files_from(path, &mut files)?;

    files.sort();
    files.dedup();
    Ok(files)
}

fn compile_fennel(lua: &mlua::Lua, source: &str) -> Result<Option<String>> {
    let globals = lua.globals();
    let fennel: Table = match globals.get("fennel") {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    let compile_string: Function = match fennel.get("compileString") {
        Ok(function) => function,
        Err(_) => return Ok(None),
    };

    let compiled: String = compile_string
        .call(source)
        .map_err(|e| anyhow::anyhow!("Fennel compilation error: {}", e))?;
    Ok(Some(compiled))
}
