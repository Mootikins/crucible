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
//! 3. **It typechecks** — when `luau-analyze` is on PATH. The generated
//!    `cru.d.luau` declarations go in as definitions, so a call into `cru.*`
//!    is checked against the host's own signature table.
//!
//! Step 3 is reported as SKIPPED, never as passed, when the binary is absent.
//! A gate that quietly succeeds because its checker is missing is worse than
//! no gate: it reports the absence of evidence as evidence.

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
    /// `luau-analyze` reported a diagnostic.
    Type { message: String },
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Finding::Syntax { file, message } => {
                write!(f, "{}: {message}", file.display())
            }
            Finding::Declaration { message } | Finding::Type { message } => {
                write!(f, "{message}")
            }
        }
    }
}

/// Whether the typechecker ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypecheckStatus {
    /// `luau-analyze` ran and its diagnostics are in the findings.
    Ran,
    /// `luau-analyze` is not on PATH. Nothing about types was proved.
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
    let init = plugin_dir.join("init.lua");
    if init.is_file() {
        if let Err(e) = load_plugin_spec(&init) {
            if matches!(e, crate::LifecycleError::InvalidDeclaration(_)) {
                findings.push(Finding::Declaration {
                    message: e.to_string(),
                });
            }
        }
    }

    // 3. The types, when there is a checker.
    let typecheck = match analyze(plugin_dir, &files, definitions) {
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

/// Run `luau-analyze`, or answer `None` when it is not installed.
fn analyze(
    plugin_dir: &Path,
    files: &[PathBuf],
    definitions: Option<&Path>,
) -> Option<Vec<Finding>> {
    if files.is_empty() {
        return None;
    }
    let binary = analyzer_binary()?;

    let mut command = Command::new(binary);
    command.current_dir(plugin_dir);
    if let Some(definitions) = definitions {
        command.arg(format!("--defs={}", definitions.display()));
    }
    for file in files {
        command.arg(file);
    }

    let output = command.output().ok()?;
    if output.status.success() {
        return Some(Vec::new());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let errors = String::from_utf8_lossy(&output.stderr);
    Some(
        text.lines()
            .chain(errors.lines())
            .filter(|line| !line.trim().is_empty())
            .map(|line| Finding::Type {
                message: line.to_string(),
            })
            .collect(),
    )
}

/// `luau-analyze` on PATH, or the override an operator set.
///
/// `CRUCIBLE_LUAU_ANALYZE` exists for the case the binary is installed under
/// another name or outside PATH; CI sets it rather than relying on a lookup.
fn analyzer_binary() -> Option<PathBuf> {
    if let Ok(explicit) = std::env::var("CRUCIBLE_LUAU_ANALYZE") {
        let path = PathBuf::from(explicit);
        return path.is_file().then_some(path);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join("luau-analyze"))
        .find(|candidate| candidate.is_file())
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
        let tmp = plugin(&[
            ("init.lua", "return { name = 'nested' }\n"),
            ("lua/helper.lua", "return {}\n"),
            ("tests/init_test.lua", "return {}\n"),
        ]);
        let report = check_plugin(tmp.path(), None).expect("check runs");
        assert_eq!(report.files_checked, 3);
    }

    /// Without the checker installed, the report says SKIPPED. It must never
    /// report a typecheck that did not happen as a pass.
    #[test]
    fn a_missing_analyzer_is_reported_as_skipped() {
        if analyzer_binary().is_some() {
            return;
        }
        let tmp = plugin(&[("init.lua", "return { name = 'ok' }\n")]);
        let report = check_plugin(tmp.path(), None).expect("check runs");
        assert_eq!(report.typecheck, TypecheckStatus::Skipped);
    }
}
