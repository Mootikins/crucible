//! `cru plugin check`: what can be proved about a plugin before it runs.
//!
//! Three things, in increasing strength:
//!
//! 1. **It parses.** Every `.lua` file in the plugin compiles under the Luau
//!    compiler the daemon runs. A syntax error found here is one the loader
//!    would otherwise report at daemon start, in a log nobody is reading.
//! 2. **Its declarations are readable.** Every tool parameter's declared type
//!    parses into [`crate::signature::LuaType`], so the JSON Schema an agent
//!    receives and the Luau declarations an author checks against say the same
//!    thing. This is the same validation the loader applies, run early.
//! 3. **It typechecks** — when a checker is installed. `luau-lsp analyze
//!    --definitions=<cru.d.luau>` is the one that can load the generated
//!    declarations, so it is preferred; upstream `luau-analyze` has no way to
//!    load a definitions file and only checks the plugin's own code.
//!
//! Step 3 is reported as SKIPPED, never as passed, when no checker is
//! installed. A gate that quietly succeeds because its checker is missing is
//! worse than no gate: it reports the absence of evidence as evidence.

use crate::lifecycle::load_plugin_spec;
use std::path::{Path, PathBuf};
use std::process::Command;

/// What one check found.
#[derive(Debug, Clone, PartialEq)]
pub enum Finding {
    /// A file did not compile.
    Syntax { file: PathBuf, message: String },
    /// A declaration the host cannot read.
    Declaration { message: String },
    /// The plugin's `init.lua` does not load — it raises, or its spec cannot
    /// be read.
    Load { message: String },
    /// `luau-analyze` reported a diagnostic.
    Type { message: String },
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finding::Syntax { file, message } => {
                write!(f, "{}: {message}", file.display())
            }
            Finding::Declaration { message }
            | Finding::Load { message }
            | Finding::Type { message } => write!(f, "{message}"),
        }
    }
}

/// Whether the typechecker ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypecheckStatus {
    /// A checker ran and its diagnostics are in the findings.
    Ran,
    /// No checker is installed. Nothing about types was proved.
    Skipped,
}

/// The result of checking one plugin.
#[derive(Debug, Clone)]
pub struct CheckReport {
    pub plugin_dir: PathBuf,
    pub files_checked: usize,
    pub typecheck: TypecheckStatus,
    pub findings: Vec<Finding>,
}

impl CheckReport {
    /// Whether everything that RAN passed. A skipped typecheck does not make
    /// this false — and does not make it a proof of type correctness either,
    /// which is why `typecheck` is reported alongside.
    pub fn passed(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Check one plugin directory.
///
/// `definitions` is the generated `cru.d.luau`, when the caller has one;
/// without it the typecheck still runs, but a `cru.*` call is unconstrained.
pub fn check_plugin(plugin_dir: &Path, definitions: Option<&Path>) -> std::io::Result<CheckReport> {
    check_plugin_with(plugin_dir, definitions, false)
}

/// [`check_plugin`], with the choice of typechecking the suite too.
///
/// A suite monkey-patches the host on purpose — `cru.log` becomes a plain
/// function to capture warnings, `cru.plugin` becomes an empty table — and
/// every one of those is a type error against declarations that describe the
/// real host. So the typechecker sees the plugin's SHIPPED code by default,
/// and the suite only when asked. Both halves still have to compile: a test
/// file that does not parse is a test file that never ran, and that check is
/// unconditional.
pub fn check_plugin_with(
    plugin_dir: &Path,
    definitions: Option<&Path>,
    include_tests: bool,
) -> std::io::Result<CheckReport> {
    // Absolute from here down: `analyze` runs the checker with the plugin
    // directory as its working directory, so a relative definitions path or a
    // relative plugin path would resolve against the wrong root.
    let plugin_dir =
        &std::fs::canonicalize(plugin_dir).unwrap_or_else(|_| plugin_dir.to_path_buf());
    let definitions =
        definitions.map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
    let definitions = definitions.as_deref();
    let files = lua_files(plugin_dir)?;
    let mut findings = Vec::new();

    // 1. Every file compiles.
    let lua = mlua::Lua::new();
    for file in &files {
        let source = std::fs::read_to_string(file)?;
        if let Err(e) = lua
            .load(&source)
            .set_name(format!("@{}", file.display()))
            .into_function()
        {
            findings.push(Finding::Syntax {
                file: file.clone(),
                message: e.to_string(),
            });
        }
    }

    // 2. The spec's declarations are readable. A plugin with no `init.lua` —
    // a bare module directory — declares nothing, and that is not a failure.
    // Every spec failure is reported, not only the unreadable declarations.
    // The loader FAILS OPEN on the rest — a plugin whose spec cannot be read
    // still loads and merely exports nothing — but a check that stayed silent
    // about it would print a tick for a plugin the daemon cannot use.
    let init = plugin_dir.join("init.lua");
    if init.is_file() {
        if let Err(e) = load_plugin_spec(&init) {
            let message = e.to_string();
            findings.push(match e {
                crate::LifecycleError::InvalidDeclaration(_) => Finding::Declaration { message },
                _ => Finding::Load { message },
            });
        }
    }

    // A checker an operator NAMED and that is not there is a failure, not a
    // skip: they asked for the check, so silence would answer a question they
    // did not ask.
    if let Some(configured) = std::env::var_os("CRUCIBLE_LUAU_ANALYZE") {
        let path = PathBuf::from(&configured);
        if !path.is_file() {
            findings.push(Finding::Type {
                message: format!(
                    "CRUCIBLE_LUAU_ANALYZE names {}, which is not a file",
                    path.display()
                ),
            });
        }
    }

    // A directory with no Lua in it is not a plugin, and answering "checked"
    // for it is the same false green as a skipped typecheck reported as a
    // pass.
    if files.is_empty() {
        findings.push(Finding::Load {
            message: format!("no .lua files under {}", plugin_dir.display()),
        });
    }

    // 3. The types, when there is a checker.
    let typechecked: Vec<PathBuf> = files
        .iter()
        .filter(|file| include_tests || !is_test_file(plugin_dir, file))
        .cloned()
        .collect();
    let typecheck = match analyze(plugin_dir, &typechecked, definitions) {
        Some(diagnostics) => {
            findings.extend(diagnostics);
            TypecheckStatus::Ran
        }
        None => TypecheckStatus::Skipped,
    };

    Ok(CheckReport {
        plugin_dir: plugin_dir.to_path_buf(),
        files_checked: files.len(),
        typecheck,
        findings,
    })
}

/// Check ONE Lua file that is not part of a plugin directory.
///
/// Crucible ships Lua that is not a plugin: the shipped defaults, the themes,
/// the statusline, the prelude's pure-Lua half. [`check_plugin`] takes a
/// directory and reads an `init.lua` spec out of it, and neither fits a loose
/// file whose neighbours may run on a different VM — `runtime/themes/` and
/// `runtime/defaults/` sit two directories apart and use two different
/// `cru.*` surfaces.
///
/// Same two checks as a plugin, minus the spec: the file compiles, and it
/// typechecks against `definitions`. A file that does not exist is a finding,
/// not a silent pass.
pub fn check_file(file: &Path, definitions: Option<&Path>) -> std::io::Result<CheckReport> {
    let file = std::fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let definitions =
        definitions.map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
    let definitions = definitions.as_deref();
    let parent = file.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut findings = Vec::new();

    if !file.is_file() {
        findings.push(Finding::Load {
            message: format!("{} is not a file", file.display()),
        });
        return Ok(CheckReport {
            plugin_dir: parent,
            files_checked: 0,
            typecheck: TypecheckStatus::Skipped,
            findings,
        });
    }

    let source = std::fs::read_to_string(&file)?;
    let lua = mlua::Lua::new();
    if let Err(e) = lua
        .load(&source)
        .set_name(format!("@{}", file.display()))
        .into_function()
    {
        findings.push(Finding::Syntax {
            file: file.clone(),
            message: e.to_string(),
        });
    }

    // Same reasoning as `check_plugin_with`: a checker an operator NAMED and
    // that is not there is a failure, not a skip.
    if let Some(configured) = std::env::var_os("CRUCIBLE_LUAU_ANALYZE") {
        let path = PathBuf::from(&configured);
        if !path.is_file() {
            findings.push(Finding::Type {
                message: format!(
                    "CRUCIBLE_LUAU_ANALYZE names {}, which is not a file",
                    path.display()
                ),
            });
        }
    }

    let typecheck = match analyze(&parent, std::slice::from_ref(&file), definitions) {
        Some(diagnostics) => {
            findings.extend(diagnostics);
            TypecheckStatus::Ran
        }
        None => TypecheckStatus::Skipped,
    };

    Ok(CheckReport {
        plugin_dir: parent,
        files_checked: 1,
        typecheck,
        findings,
    })
}

/// Which checker is in use. They are not interchangeable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Analyzer {
    /// `luau-lsp analyze --definitions=…`. The only one that can load a
    /// definitions file, so the only one that checks a call into `cru.*`.
    LuauLsp,
    /// Upstream `luau-analyze`. Typechecks the plugin's own code; its CLI has
    /// no way to load definitions, so every `cru` reference is an unknown
    /// global to it and those diagnostics are dropped.
    LuauAnalyze,
}

/// Run the checker, or answer `None` when none is installed.
///
/// The output is read whatever the exit status says, because the status alone
/// answers neither question:
///
/// - `luau-lsp` exits 0 for a LINT — an unused local, a duplicate table key —
///   and 1 only for a type error. Trusting the status threw every lint away,
///   including `TableLiteral: Table field 'a' is a duplicate`, which is a real
///   bug the checker had already found.
/// - A non-zero exit whose every line falls to a filter used to leave no
///   findings at all, so a checker that failed for a reason of its own was
///   reported as a pass.
fn analyze(
    plugin_dir: &Path,
    files: &[PathBuf],
    definitions: Option<&Path>,
) -> Option<Vec<Finding>> {
    if files.is_empty() {
        return None;
    }
    let (binary, kind) = analyzer_binary()?;

    let mut command = Command::new(&binary);
    command.current_dir(plugin_dir);
    if kind == Analyzer::LuauLsp {
        command.arg("analyze");
        if let Some(definitions) = definitions {
            command.arg(format!("--definitions={}", definitions.display()));
        }
    }
    for file in files {
        command.arg(file);
    }

    // A checker that cannot be RUN is a failure, not a skip: something named
    // it, and silence would answer a question nobody asked.
    let output = match command.output() {
        Ok(output) => output,
        Err(e) => {
            return Some(vec![Finding::Type {
                message: format!("could not run {}: {e}", binary.display()),
            }])
        }
    };

    let text = String::from_utf8_lossy(&output.stdout);
    let errors = String::from_utf8_lossy(&output.stderr);
    let findings: Vec<Finding> = text
        .lines()
        .chain(errors.lines())
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        // `luau-lsp` narrates what it loaded on stdout; only diagnostics are
        // findings.
        .filter(|line| !line.starts_with("[INFO]"))
        // Without definitions every `cru` is an unknown global. Reporting
        // that against correct code trains an author to ignore the tool.
        .filter(|line| kind == Analyzer::LuauLsp || !line.contains("Unknown global 'cru'"))
        .map(|line| Finding::Type {
            message: line.to_string(),
        })
        .collect();

    if findings.is_empty() && !output.status.success() {
        return Some(vec![Finding::Type {
            message: format!(
                "{} exited with {} and said nothing this check could read",
                binary.display(),
                output.status
            ),
        }]);
    }
    Some(findings)
}

/// The checker to run: `luau-lsp` first, because it is the one that can load
/// the generated `cru.d.luau`.
///
/// `CRUCIBLE_LUAU_ANALYZE` overrides the lookup, for a binary installed under
/// another name or outside PATH; CI sets it rather than relying on a lookup.
/// A path whose file name mentions `lsp` is driven as `luau-lsp analyze`.
fn analyzer_binary() -> Option<(PathBuf, Analyzer)> {
    if let Ok(explicit) = std::env::var("CRUCIBLE_LUAU_ANALYZE") {
        let path = PathBuf::from(explicit);
        if !path.is_file() {
            return None;
        }
        let kind = if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains("lsp"))
        {
            Analyzer::LuauLsp
        } else {
            Analyzer::LuauAnalyze
        };
        return Some((path, kind));
    }

    let path = std::env::var_os("PATH")?;
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    for (name, kind) in [
        ("luau-lsp", Analyzer::LuauLsp),
        ("luau-analyze", Analyzer::LuauAnalyze),
    ] {
        if let Some(found) = dirs
            .iter()
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
        {
            return Some((found, kind));
        }
    }
    None
}

/// Whether a file belongs to the plugin's suite rather than its shipped code.
///
/// Judged on the path RELATIVE to the plugin. `plugin_dir` is absolute, so
/// walking the whole path asked about every ancestor too: a plugin developed
/// under any directory named `tests` had its entire typecheck skipped, and
/// the report blamed a missing checker.
fn is_test_file(plugin_dir: &Path, path: &Path) -> bool {
    let relative = path.strip_prefix(plugin_dir).unwrap_or(path);
    relative
        .components()
        .any(|part| part.as_os_str() == "tests")
        || relative
            .file_stem()
            .and_then(|stem| stem.to_str())
            .is_some_and(|stem| stem.ends_with("_test"))
}

/// Every `.lua` file under the plugin, tests included: a suite that does not
/// compile is a suite that never ran.
fn lua_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    if !dir.is_dir() {
        if dir.extension().is_some_and(|ext| ext == "lua") {
            out.push(dir.to_path_buf());
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "lua") {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// `CRUCIBLE_LUAU_ANALYZE` is process-global, so a test that sets it
    /// changes what every OTHER test in this module sees. Every test that
    /// runs a check takes this first.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn plugin(files: &[(&str, &str)]) -> TempDir {
        let tmp = TempDir::new().unwrap();
        for (name, source) in files {
            let path = tmp.path().join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, source).unwrap();
        }
        tmp
    }

    #[test]
    fn a_well_formed_plugin_passes() {
        let _env = env_lock();
        let tmp = plugin(&[(
            "init.lua",
            "--!strict\nreturn { name = 'ok', version = '0.1.0' }\n",
        )]);
        let report = check_plugin(tmp.path(), None).expect("check runs");
        assert!(report.passed(), "{:?}", report.findings);
        assert_eq!(report.files_checked, 1);
    }

    #[test]
    fn a_syntax_error_is_reported_with_its_file() {
        let _env = env_lock();
        let tmp = plugin(&[("init.lua", "return {\n")]);
        let report = check_plugin(tmp.path(), None).expect("check runs");
        assert!(!report.passed());
        assert!(
            matches!(&report.findings[0], Finding::Syntax { file, .. } if file.ends_with("init.lua")),
            "{:?}",
            report.findings
        );
    }

    #[test]
    fn an_unreadable_declaration_is_reported() {
        let _env = env_lock();
        let tmp = plugin(&[(
            "init.lua",
            r#"
            return {
                name = "bad",
                tools = {
                    search = {
                        desc = "search",
                        params = { { name = "tags", type = "array<", desc = "" } },
                        fn = function() end,
                    },
                },
            }
            "#,
        )]);
        let report = check_plugin(tmp.path(), None).expect("check runs");
        assert!(
            report
                .findings
                .iter()
                .any(|f| matches!(f, Finding::Declaration { .. })),
            "{:?}",
            report.findings
        );
    }

    /// Nested files count: a plugin's `lua/` modules and its test suite are
    /// part of what must compile.
    #[test]
    fn every_lua_file_under_the_plugin_is_checked() {
        let _env = env_lock();
        let tmp = plugin(&[
            ("init.lua", "return { name = 'nested' }\n"),
            ("lua/helper.lua", "return {}\n"),
            ("tests/init_test.lua", "return {}\n"),
        ]);
        let report = check_plugin(tmp.path(), None).expect("check runs");
        assert_eq!(report.files_checked, 3);
    }

    /// A checker that fails for a reason of its own must not read as a pass.
    ///
    /// Every line it printed fell to a filter, so there were no findings, so
    /// `passed()` was true and the CLI printed a tick with exit 0.
    #[cfg(unix)]
    #[test]
    fn a_checker_that_exits_non_zero_saying_nothing_is_a_finding() {
        let _env = env_lock();
        use std::os::unix::fs::PermissionsExt;
        let tmp = plugin(&[("init.lua", "return { name = 'x' }\n")]);
        let fake = tmp.path().join("fake-lsp");
        std::fs::write(&fake, "#!/bin/sh\necho \"[INFO] loading\"\nexit 1\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        std::env::set_var("CRUCIBLE_LUAU_ANALYZE", &fake);
        let report = check_plugin(tmp.path(), None).expect("check runs");
        std::env::remove_var("CRUCIBLE_LUAU_ANALYZE");

        assert!(
            !report.passed(),
            "a failing checker must not pass: {report:?}"
        );
        assert_eq!(report.typecheck, TypecheckStatus::Ran);
    }

    /// A directory with no Lua in it is not a plugin. It used to report
    /// SKIPPED — blaming a missing checker that was installed — and pass.
    #[test]
    fn a_directory_with_no_lua_is_a_finding() {
        let _env = env_lock();
        let tmp = TempDir::new().unwrap();
        let report = check_plugin(tmp.path(), None).expect("check runs");
        assert!(!report.passed(), "{report:?}");
        assert!(
            report
                .findings
                .iter()
                .any(|f| matches!(f, Finding::Load { message } if message.contains("no .lua"))),
            "{:?}",
            report.findings
        );
    }

    /// The suite predicate reads the path RELATIVE to the plugin. Judged on
    /// the absolute path, a plugin developed under any directory named
    /// `tests` had its whole typecheck skipped.
    #[test]
    fn a_tests_ancestor_does_not_make_every_file_a_test() {
        let outer = TempDir::new().unwrap();
        let plugin_dir = outer.path().join("tests").join("myplugin");
        std::fs::create_dir_all(plugin_dir.join("tests")).unwrap();
        std::fs::write(plugin_dir.join("init.lua"), "return {}\n").unwrap();
        std::fs::write(plugin_dir.join("tests/init_test.lua"), "return {}\n").unwrap();

        assert!(
            !is_test_file(&plugin_dir, &plugin_dir.join("init.lua")),
            "the plugin's own file is not a test because an ancestor is named tests"
        );
        assert!(
            is_test_file(&plugin_dir, &plugin_dir.join("tests/init_test.lua")),
            "the plugin's own tests/ directory still counts"
        );
    }

    /// Without the checker installed, the report says SKIPPED. It must never
    /// report a typecheck that did not happen as a pass.
    #[test]
    fn a_missing_analyzer_is_reported_as_skipped() {
        let _env = env_lock();
        if analyzer_binary().is_some() {
            return;
        }
        let tmp = plugin(&[("init.lua", "return { name = 'ok' }\n")]);
        let report = check_plugin(tmp.path(), None).expect("check runs");
        assert_eq!(report.typecheck, TypecheckStatus::Skipped);
    }
}
