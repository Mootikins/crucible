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
//!
//! A fourth check runs when the caller supplies a VM ([`check_plugin_on`]):
//! **its module body has no effect.** `init.luau` runs on that VM under the
//! plugin's source, and the check then reads the VM's handler registry, its
//! schedules and its tasks for that source. A registration made there is a
//! top-level effect, reported with the hook's name. Decision 5 of the plugin
//! activation plan says activation runs the module once and then `setup`, so
//! a registration in the body runs before the host calls `setup` and cannot
//! be turned off by a `config` function.

use crate::lifecycle::{
    fragment::{read_fragment, read_only_env, FRAGMENT_FILE},
    spec_from_table,
};
use crate::{LuaExecutor, LuaSource};
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
/// real host — so `mock(...)` marks the deliberate patch and the rest of a
/// test file stays checkable. The gates ask for the suite: excluding it hid
/// 46 diagnostics in the shipped plugins. Both halves have to compile either
/// way, because a test file that does not parse is a test file that never
/// ran, and that check is unconditional.
pub fn check_plugin_with(
    plugin_dir: &Path,
    definitions: Option<&Path>,
    include_tests: bool,
) -> std::io::Result<CheckReport> {
    check_plugin_using(plugin_dir, definitions, include_tests, &find_checker())
}

/// [`check_plugin_with`], against a checker the caller already resolved.
///
/// A caller that must PROVE a checker ran — every gate in the daemon does —
/// resolves one itself with [`find_checker`], proves it with
/// [`Checker::proves_types`], and hands it here. Naming the checker in the
/// call is what let those gates stop writing `CRUCIBLE_LUAU_ANALYZE` on a
/// process they share with every other test.
pub fn check_plugin_using(
    plugin_dir: &Path,
    definitions: Option<&Path>,
    include_tests: bool,
    checker: &CheckerChoice,
) -> std::io::Result<CheckReport> {
    check_plugin_inner(plugin_dir, definitions, include_tests, checker, None)
}

/// [`check_plugin_using`], with `init.luau` run on `vm`.
///
/// The read-only environment a fragment runs in cannot run `init.luau`: a
/// plugin `require`s its own modules at the top level, and that is correct
/// code. So the module runs on a VM the caller built with the real `cru.*`
/// modules, under [`LuaSource::Plugin`] for the plugin's directory name,
/// with the plugin's own `lua/` directory resolvable, as activation runs it.
/// The caller owns the VM and throws it away: the CLI builds the daemon
/// loader's VM in its own process. The check clears the source it entered
/// before it returns, so one VM serves a whole directory of plugins.
///
/// Two findings come from the run. A body that raises, or that returns no
/// table, is [`Finding::Load`]. A registration the body made is
/// [`Finding::Load`] too, with each hook named: the VM's registry, its
/// schedules and its tasks are read for the plugin's source after the body
/// returns, so the finding is a fact the store states and not a reading of
/// an error message. The declarations are read from the returned table.
pub fn check_plugin_on(
    plugin_dir: &Path,
    definitions: Option<&Path>,
    include_tests: bool,
    checker: &CheckerChoice,
    vm: &LuaExecutor,
) -> std::io::Result<CheckReport> {
    check_plugin_inner(plugin_dir, definitions, include_tests, checker, Some(vm))
}

fn check_plugin_inner(
    plugin_dir: &Path,
    definitions: Option<&Path>,
    include_tests: bool,
    checker: &CheckerChoice,
    vm: Option<&LuaExecutor>,
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
    // Two extensions of ONE name, anywhere under the plugin. The entry point
    // was reported below and a submodule was not, so `helper.luau` beside
    // `helper.lua` passed the check and then resolved to nothing at run time.
    // Collected from the file list rather than by walking again, so it sees
    // exactly what the checker sees.
    let mut colliding: Vec<PathBuf> = files
        .iter()
        .filter(|file| {
            crate::source_files::is_lua_source(file)
                && file.extension().is_some_and(|ext| ext == "luau")
                && file.with_extension("lua").is_file()
        })
        .cloned()
        .collect();
    colliding.sort();
    // `read_fragment` refuses `spec.luau` beside `spec.lua` with the same
    // wording the sweep uses, so that pair is reported once, by the sweep.
    let fragment_pair_reported = colliding
        .iter()
        .any(|luau| luau.file_name().is_some_and(|name| name == FRAGMENT_FILE));
    for luau in colliding {
        // The wording lives in `Ambiguous`, not here. A second copy of it is
        // a second thing to keep in step with the rule it describes.
        findings.push(Finding::Load {
            message: crate::source_files::Ambiguous(vec![luau.clone(), luau.with_extension("lua")])
                .to_string(),
        });
    }

    // The init file, both extensions. A directory holding both is refused
    // rather than resolved — see `source_files`.
    //
    // The refusal is NOT reported here: `init.luau` beside `init.lua` is a
    // `.luau` with a `.lua` sibling, so the sweep above already named that
    // pair. Reporting it here too printed one mistake twice.
    let init = crate::source_files::init_file(plugin_dir).ok().flatten();
    if let Some(init) = &init {
        // Declarations, and — on a VM — the top-level effect. The read-only
        // environment cannot run `init.luau` (a plugin `require`s its own
        // modules at the top level), so a caller that supplied a VM runs the
        // body there under the plugin's source and reads the registry for it.
        match vm {
            Some(vm) => findings.extend(effect_findings(vm, plugin_dir, init)),
            None => findings.extend(declaration_findings(&lua, init)),
        }
    }

    // The fragment's own fields. `read_fragment` refuses `spec.luau` beside
    // `spec.lua` with the wording the sweep uses, so a pair the sweep already
    // named is not read again — reporting it twice is one mistake twice.
    if !fragment_pair_reported {
        if let Err(e) = read_fragment(&lua, plugin_dir) {
            findings.push(Finding::Load {
                message: e.to_string(),
            });
        }
    }

    findings.extend(checker.finding());

    // A directory with no Lua in it is not a plugin, and answering "checked"
    // for it is the same false green as a skipped typecheck reported as a
    // pass.
    if files.is_empty() {
        findings.push(Finding::Load {
            message: format!("no Lua source under {}", plugin_dir.display()),
        });
    }

    // 3. The types, when there is a checker.
    let typechecked: Vec<PathBuf> = files
        .iter()
        .filter(|file| include_tests || !is_test_file(plugin_dir, file))
        .cloned()
        .collect();
    let typecheck = match analyze(plugin_dir, &typechecked, definitions, checker.checker()) {
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

/// Evaluate `init` in the read-only environment and read its declarations.
///
/// The environment is the one a fragment runs in: no `cru`, no `require`,
/// no `io`. A plugin whose `init.luau` acts at the top level raises there
/// before it returns its table. This step reports nothing for that raise
/// yet, and reads no declarations from such a plugin: the shipped plugins
/// `require` their own modules at the top level, and the wording of that
/// finding is a later change. A table that comes back is read with
/// `spec_from_table`, so an unreadable declaration is reported.
fn declaration_findings(lua: &mlua::Lua, init: &Path) -> Vec<Finding> {
    let Ok(source) = std::fs::read_to_string(init) else {
        return Vec::new();
    };
    let Ok(env) = read_only_env(lua) else {
        return Vec::new();
    };
    let Ok(mlua::Value::Table(table)) = lua
        .load(&source)
        .set_name(format!("@{}", init.display()))
        .set_environment(env)
        .eval::<mlua::Value>()
    else {
        return Vec::new();
    };
    match spec_from_table(&table, init) {
        Ok(_) => Vec::new(),
        Err(e) => {
            let message = e.to_string();
            vec![match e {
                crate::LifecycleError::InvalidDeclaration(_) => Finding::Declaration { message },
                _ => Finding::Load { message },
            }]
        }
    }
}

/// Run `init` on `vm` and read what the module body did, and what it declared.
///
/// The body runs under [`LuaSource::Plugin`] for the plugin's directory name,
/// with the plugin's own `lua/` directory resolvable — the same context
/// activation runs it in — so a top-level `require` of the plugin's own
/// module works and a top-level `cru.on` lands under the plugin's source.
///
/// After the run the VM's registry, schedules and tasks are read for that
/// source. A registration there is a top-level effect: it ran before the
/// host called `setup`, so a `config` function cannot turn it off. The
/// finding names each hook, so an author sees what to move.
///
/// The source is restored and the plugin's registrations are cleared on
/// every path, so one VM serves a whole directory of plugins and leaves none
/// of them behind. The registration count is read BEFORE the clear.
fn effect_findings(vm: &LuaExecutor, plugin_dir: &Path, init: &Path) -> Vec<Finding> {
    let lua = vm.lua();
    let name = plugin_dir
        .file_name()
        .map(|part| part.to_string_lossy().into_owned())
        .unwrap_or_default();
    let source = match std::fs::read_to_string(init) {
        Ok(source) => source,
        Err(e) => {
            return vec![Finding::Load {
                message: format!("{}: {e}", init.display()),
            }]
        }
    };
    let registry = match crate::handlers::registry_of(lua) {
        Ok(registry) => registry,
        Err(e) => {
            return vec![Finding::Load {
                message: format!("{}: {e}", init.display()),
            }]
        }
    };
    let plugin_source = LuaSource::Plugin(name.clone());

    // Start clean: a plugin checked earlier on this VM must not be counted
    // against this one.
    crate::clear_source(lua, &registry, &plugin_source);

    // The plugin's own `lua/` directory, resolvable for the run and popped
    // after, as activation resolves it. The parent is the public root a
    // sibling `require` would use.
    if let Some(parent) = plugin_dir.parent() {
        let _ = vm.configure_module_roots(vec![parent.to_path_buf()]);
    }
    let module_scope = vm.enter_plugin_root(plugin_dir);

    let previous = crate::enter_plugin(lua, &name);
    let evaluated: mlua::Result<mlua::Value> = lua
        .load(&source)
        .set_name(format!("@{}", init.display()))
        .eval();
    crate::set_source(lua, previous);
    drop(module_scope);

    let mut findings = Vec::new();

    // The effect, read before the clear.
    let registrations = registry.for_source(&plugin_source);
    let schedules = crate::schedule::count_source(lua, &plugin_source);
    let tasks = crate::timer::count_source(lua, &plugin_source);
    let total = registrations.len() + schedules + tasks;
    if total > 0 {
        let mut hooks: Vec<String> = registrations.iter().map(|r| r.name.to_string()).collect();
        hooks.extend(std::iter::repeat_n("cru.schedule".to_string(), schedules));
        hooks.extend(std::iter::repeat_n("cru.timer.spawn".to_string(), tasks));
        findings.push(Finding::Load {
            message: format!(
                "{}: {total} registration(s) at top level ({}); move them into setup()",
                init.display(),
                hooks.join(", "),
            ),
        });
    }

    // Leave the VM as clean as this check found it.
    crate::clear_source(lua, &registry, &plugin_source);

    // The declarations, from the table the body returned.
    match evaluated {
        Ok(mlua::Value::Table(table)) => {
            if let Err(e) = spec_from_table(&table, init) {
                let message = e.to_string();
                findings.push(match e {
                    crate::LifecycleError::InvalidDeclaration(_) => {
                        Finding::Declaration { message }
                    }
                    _ => Finding::Load { message },
                });
            }
        }
        Ok(other) => findings.push(Finding::Load {
            message: format!(
                "{}: init.luau returned {}, not a table",
                init.display(),
                other.type_name()
            ),
        }),
        Err(e) => findings.push(Finding::Load {
            message: format!("{}: {e}", init.display()),
        }),
    }
    findings
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
    check_file_using(file, definitions, &find_checker())
}

/// [`check_file`], against a checker the caller already resolved. Same reason
/// as [`check_plugin_using`].
pub fn check_file_using(
    file: &Path,
    definitions: Option<&Path>,
    checker: &CheckerChoice,
) -> std::io::Result<CheckReport> {
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

    findings.extend(checker.finding());

    let typecheck = match analyze(
        &parent,
        std::slice::from_ref(&file),
        definitions,
        checker.checker(),
    ) {
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
    checker: Option<&Checker>,
) -> Option<Vec<Finding>> {
    if files.is_empty() {
        return None;
    }
    let checker = checker?;
    let binary = &checker.path;

    let mut command = Command::new(binary);
    command.current_dir(plugin_dir);
    if checker.kind == Analyzer::LuauLsp {
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
        // `luau-lsp` narrates what it loaded and what it is missing; only
        // DIAGNOSTICS are findings. Its two narration lines when no
        // definitions file is given —
        //   WARNING: --platform is set to 'roblox' but no definitions ...
        //   [WARN] No definitions file provided by client
        // — were reported as type errors against correct code, so
        // `cru plugin check` on a well-formed plugin failed with 8 findings
        // wherever a checker was installed.
        .filter(|line| {
            !line.starts_with("[INFO]")
                && !line.starts_with("[WARN]")
                && !line.starts_with("WARNING:")
        })
        // Without a definitions file EVERY host global is unknown — `cru`,
        // and `io`, `require`, `describe`, `it`, `expect` with it. Reporting
        // that against correct code trains an author to ignore the tool. WITH
        // definitions it means they failed to load, which is the one thing
        // that must never be swallowed — so the test is on the definitions,
        // not on which analyzer ran. It used to read `kind == LuauLsp || ...`,
        // which kept the noise for the analyzer that produces it and filtered
        // it for the one that does not; then it named `cru` alone, so
        // `cru plugin check` with no definitions buried its real findings under
        // one line per `it(` in the suite.
        .filter(|line| definitions.is_some() || !is_unknown_host_global(line))
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

/// A resolved type checker: the binary, and how a check drives it.
///
/// The point of naming it is that ONE piece of code decides what a checker is
/// and where it comes from. The gates used to resolve their own — env var,
/// then `target/tools/luau-lsp`, then PATH — while the shipped CLI read only
/// the env var and PATH. On a machine where `just luau-lsp` had run, the gates
/// checked types and `cru plugin check` printed `typecheck: SKIPPED` over the
/// same files. Both now call [`find_checker`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checker {
    path: PathBuf,
    kind: Analyzer,
}

impl Checker {
    /// Drive the binary at `path`, classified by its file name: a name that
    /// mentions `lsp` is driven as `luau-lsp analyze`.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let kind = if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.contains("lsp"))
        {
            Analyzer::LuauLsp
        } else {
            Analyzer::LuauAnalyze
        };
        Self { path, kind }
    }

    /// The binary this checker runs.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Prove the binary CHECKS TYPES, by requiring it to find one that is
    /// wrong.
    ///
    /// [`Checker::at`] classifies by file name alone, so any executable was a
    /// checker to it: `CRUCIBLE_LUAU_ANALYZE=/bin/true` ran, exited 0,
    /// produced no diagnostics, and every gate reported
    /// [`TypecheckStatus::Ran`] over a deliberately broken file. "Something
    /// ran" is not "a type checker ran", and that distinction is the whole
    /// reason the gates exist.
    ///
    /// Not a `--version` probe: `luau-lsp --version` prints a bare `1.69.0`
    /// with the word "luau" nowhere in it, so a string match REJECTS the
    /// pinned build. Handing it a file that cannot typecheck tests the
    /// property a check actually relies on.
    pub fn proves_types(&self) -> Result<(), String> {
        let probe = tempfile::TempDir::new().map_err(|e| e.to_string())?;
        let file = probe.path().join("checker_probe.luau");
        std::fs::write(
            &file,
            "--!strict\nlocal n: number = \"not a number\"\nreturn n\n",
        )
        .map_err(|e| e.to_string())?;

        let choice = CheckerChoice::Use(self.clone());
        let report = check_file_using(&file, None, &choice).map_err(|e| e.to_string())?;
        if report.passed() {
            return Err(format!(
                "{} reported no problem with a file that assigns a string to a \
                 `number`, so it is not checking types. Anything that trusts \
                 it reports a pass having checked nothing.",
                self.path.display()
            ));
        }
        Ok(())
    }
}

/// What the search for a checker found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckerChoice {
    /// Run this one.
    Use(Checker),
    /// None is installed. The typecheck is [`TypecheckStatus::Skipped`], and
    /// nothing about types is proved.
    None,
    /// `CRUCIBLE_LUAU_ANALYZE` names a path that is not a file. A checker an
    /// operator NAMED and that is not there is a failure, not a skip: they
    /// asked for the check, so silence would answer a question they did not
    /// ask.
    Missing(PathBuf),
}

impl CheckerChoice {
    fn checker(&self) -> Option<&Checker> {
        match self {
            CheckerChoice::Use(checker) => Some(checker),
            _ => None,
        }
    }

    /// The finding this choice adds to every report, if any.
    fn finding(&self) -> Option<Finding> {
        match self {
            CheckerChoice::Missing(path) => Some(Finding::Type {
                message: format!(
                    "CRUCIBLE_LUAU_ANALYZE names {}, which is not a file",
                    path.display()
                ),
            }),
            _ => None,
        }
    }
}

/// The checker to run. THE resolver: three places, in this order.
///
/// 1. `CRUCIBLE_LUAU_ANALYZE`, for a binary installed under another name or
///    outside PATH. An operator who names one gets that one or a failure.
/// 2. `target/tools/luau-lsp`, the pinned build `just luau-lsp` writes, found
///    relative to THIS BINARY. A `cru` built from the checkout lives in
///    `target/debug/`, so the pinned checker is its sibling; an installed
///    `cru` finds nothing. Reading it here is what makes `cru plugin check`
///    agree with the gates.
/// 3. PATH: `luau-lsp` first, because it is the only one that can load the
///    generated `cru.d.luau`.
pub fn find_checker() -> CheckerChoice {
    if let Some(configured) = std::env::var_os("CRUCIBLE_LUAU_ANALYZE") {
        let path = PathBuf::from(configured);
        return if path.is_file() {
            CheckerChoice::Use(Checker::at(path))
        } else {
            CheckerChoice::Missing(path)
        };
    }
    if let Some(pinned) = pinned_checker() {
        return CheckerChoice::Use(Checker::at(pinned));
    }
    if let Some(found) = checker_on_path() {
        return CheckerChoice::Use(Checker::at(found));
    }
    CheckerChoice::None
}

/// The pinned build beside the running binary.
fn pinned_checker() -> Option<PathBuf> {
    pinned_beside(&std::env::current_exe().ok()?)
}

/// `target/tools/luau-lsp` in the cargo target directory that holds `binary`.
///
/// Resolved from the BINARY, never from the working directory. `cru plugin
/// check` is a command an author points at code they downloaded, and running
/// it from inside that tree is reasonable — so a walk up from the working
/// directory would let anyone who can write `<any ancestor>/target/tools/
/// luau-lsp` choose the executable this process then runs.
/// [`Checker::proves_types`] does not close that: it is a correctness probe,
/// and a hostile binary prints one plausible diagnostic and then does as it
/// likes.
///
/// The ancestor must be NAMED `target`, so the search cannot wander into a
/// home directory when `cru` is installed outside a checkout. A test binary
/// sits in `target/debug/deps/`, a `cargo run` binary in `target/debug/`, and
/// both find the same file.
fn pinned_beside(binary: &Path) -> Option<PathBuf> {
    binary
        .ancestors()
        .filter(|dir| dir.file_name().is_some_and(|name| name == "target"))
        .map(|target| target.join("tools").join("luau-lsp"))
        .find(|candidate| candidate.is_file())
}

/// The first checker on PATH.
fn checker_on_path() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    ["luau-lsp", "luau-analyze"].into_iter().find_map(|name| {
        dirs.iter()
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

/// Whether a diagnostic says a name the HOST declares is an unknown global.
///
/// The names come from the declarations themselves
/// ([`crate::host_api::host_global_names`]), so a global added there cannot
/// leave this filter behind.
fn is_unknown_host_global(line: &str) -> bool {
    crate::host_api::host_global_names()
        .iter()
        .any(|name| line.contains(&format!("Unknown global '{name}'")))
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
        if crate::source_files::is_lua_source(dir) {
            out.push(dir.to_path_buf());
        }
        return Ok(());
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            collect(&path, out)?;
        } else if crate::source_files::is_lua_source(&path) {
            out.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::LuaScriptHandlerRegistry;
    use tempfile::TempDir;

    /// `CRUCIBLE_LUAU_ANALYZE` is process-global, so a test that sets it
    /// changes what every OTHER test in this module sees. The tests of
    /// [`find_checker`]'s environment branch take this first; nothing else in
    /// this module reads or writes the variable any more, because a check now
    /// takes the checker as an argument.
    static ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn env_lock() -> std::sync::MutexGuard<'static, ()> {
        ENV.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// A check that must NOT typecheck, whatever this machine has installed.
    ///
    /// Naming the absence is the point: these tests used to call
    /// `check_plugin` and get whatever the machine happened to have, so half
    /// of them proved one thing on a developer's box and another in CI.
    const NO_CHECKER: CheckerChoice = CheckerChoice::None;

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
        let report = check_plugin_using(tmp.path(), None, false, &NO_CHECKER).expect("check runs");
        assert!(report.passed(), "{:?}", report.findings);
        assert_eq!(report.files_checked, 1);

        // And it must pass WITH a checker, which is the only configuration
        // where this test proves anything. It used to pass only without one:
        // `luau-lsp` narrates two WARNING lines when no definitions file is
        // given, both became findings, and a well-formed plugin failed with
        // eight of them on any machine that had the checker installed.
        if let Some(checker) = installed_checker() {
            let checked =
                check_plugin_using(tmp.path(), None, false, &checker).expect("check runs");
            assert_eq!(
                checked.typecheck,
                TypecheckStatus::Ran,
                "the checker was named, so it must have run"
            );
            assert!(
                checked.passed(),
                "a well-formed plugin must pass WITH a checker too: {:?}",
                checked.findings
            );
        }
    }

    /// The pinned checker, when this working tree has one.
    fn installed_checker() -> Option<CheckerChoice> {
        let pinned = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../target/tools/luau-lsp"
        ));
        pinned
            .is_file()
            .then(|| CheckerChoice::Use(Checker::at(pinned)))
    }

    /// A strict plugin calls the two process functions, and the checker
    /// agrees that it may.
    ///
    /// `HOST_ENVIRONMENT` is what `cru plugin check` and `.luarc.json` read.
    /// A function the host provides and does not declare turns correct code
    /// into a type error, so the declarations must move with the runtime.
    #[test]
    fn a_strict_plugin_may_call_the_process_functions() {
        let Some(checker) = installed_checker() else {
            return;
        };
        let tmp = tempfile::TempDir::new().expect("tempdir");
        crate::stubs::StubGenerator::generate(tmp.path()).expect("declarations");
        let definitions = tmp.path().join("cru.d.luau");

        let file = tmp.path().join("uses_process.luau");
        std::fs::write(
            &file,
            "--!strict\n\
             local ran, reason, code = os.execute(\"exit 0\")\n\
             print(ran, reason, code)\n\
             local pipe = io.popen(\"printf hi\", \"r\")\n\
             if pipe then\n\
             \tprint(pipe:read(\"a\"))\n\
             \tprint(pipe:close())\n\
             end\n\
             print(os.execute())\n\
             return {}\n",
        )
        .expect("write the plugin file");

        let report = check_file_using(&file, Some(&definitions), &checker).expect("check runs");
        assert!(
            report.passed(),
            "a plugin that calls io.popen and os.execute must typecheck: {:?}",
            report.findings
        );
    }

    #[test]
    fn a_syntax_error_is_reported_with_its_file() {
        let tmp = plugin(&[("init.lua", "return {\n")]);
        let report = check_plugin_using(tmp.path(), None, false, &NO_CHECKER).expect("check runs");
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
        let report = check_plugin_using(tmp.path(), None, false, &NO_CHECKER).expect("check runs");
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
        let report = check_plugin_using(tmp.path(), None, false, &NO_CHECKER).expect("check runs");
        assert_eq!(report.files_checked, 3);
    }

    /// A checker that fails for a reason of its own must not read as a pass.
    ///
    /// Every line it printed fell to a filter, so there were no findings, so
    /// `passed()` was true and the CLI printed a tick with exit 0.
    #[cfg(unix)]
    #[test]
    fn a_checker_that_exits_non_zero_saying_nothing_is_a_finding() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = plugin(&[("init.lua", "return { name = 'x' }\n")]);
        let fake = tmp.path().join("fake-lsp");
        std::fs::write(&fake, "#!/bin/sh\necho \"[INFO] loading\"\nexit 1\n").unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

        let checker = CheckerChoice::Use(Checker::at(&fake));
        let report = check_plugin_using(tmp.path(), None, false, &checker).expect("check runs");

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
        let tmp = TempDir::new().unwrap();
        let report = check_plugin_using(tmp.path(), None, false, &NO_CHECKER).expect("check runs");
        assert!(!report.passed(), "{report:?}");
        assert!(
            report.findings.iter().any(
                |f| matches!(f, Finding::Load { message } if message.contains("no Lua source"))
            ),
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

    /// Without a checker, the report says SKIPPED. It must never report a
    /// typecheck that did not happen as a pass.
    ///
    /// This used to return early wherever a checker WAS installed, so on a
    /// developer's machine and in the CI job that installs one it proved
    /// nothing. The choice is an argument now, so the absence is a case this
    /// test can state.
    #[test]
    fn a_missing_analyzer_is_reported_as_skipped() {
        let tmp = plugin(&[("init.lua", "return { name = 'ok' }\n")]);
        let report = check_plugin_using(tmp.path(), None, false, &NO_CHECKER).expect("check runs");
        assert_eq!(report.typecheck, TypecheckStatus::Skipped);
        assert!(report.passed(), "{:?}", report.findings);
    }

    /// The variable an operator sets wins over the pinned build.
    ///
    /// It is the OPERATOR's channel, and it is the only one of the three that
    /// can name a binary under another name or outside PATH.
    #[test]
    fn a_named_checker_wins_over_the_pinned_build() {
        let _env = env_lock();
        let tmp = TempDir::new().unwrap();
        let named = tmp.path().join("my-lsp");
        std::fs::write(&named, "#!/bin/sh\nexit 0\n").unwrap();

        let restore = std::env::var_os("CRUCIBLE_LUAU_ANALYZE");
        // SAFETY: `env_lock()` serialises every test that touches this.
        unsafe { std::env::set_var("CRUCIBLE_LUAU_ANALYZE", &named) };
        let choice = find_checker();
        restore_env(restore);

        assert_eq!(
            choice,
            CheckerChoice::Use(Checker::at(&named)),
            "{choice:?}"
        );
    }

    /// A checker an operator NAMED and that is not there is a failure, not a
    /// skip — and not a silent fall through to the pinned build either.
    #[test]
    fn a_named_checker_that_is_not_there_is_a_finding() {
        let _env = env_lock();
        let missing = PathBuf::from("/nonexistent/luau-lsp");

        let restore = std::env::var_os("CRUCIBLE_LUAU_ANALYZE");
        // SAFETY: as above.
        unsafe { std::env::set_var("CRUCIBLE_LUAU_ANALYZE", &missing) };
        let choice = find_checker();
        restore_env(restore);

        assert_eq!(choice, CheckerChoice::Missing(missing.clone()));

        let tmp = plugin(&[("init.lua", "return { name = 'x' }\n")]);
        let report = check_plugin_using(tmp.path(), None, false, &choice).expect("check runs");
        assert_eq!(report.typecheck, TypecheckStatus::Skipped);
        assert!(
            report.findings.iter().any(|f| matches!(
                f,
                Finding::Type { message } if message.contains("is not a file")
            )),
            "{:?}",
            report.findings
        );
    }

    /// The pinned build is found beside the binary that looks for it.
    ///
    /// `just luau-lsp` writes `target/tools/luau-lsp`. A `cru` built from the
    /// checkout and a test binary both sit under that same `target/`, so both
    /// find it; an installed `cru` does not, and neither does anything that
    /// merely RUNS in a directory someone else can write.
    #[test]
    fn the_pinned_build_is_found_beside_the_binary() {
        let root = TempDir::new().unwrap();
        let tools = root.path().join("target/tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join("luau-lsp"), "#!/bin/sh\nexit 0\n").unwrap();
        let pinned = Some(tools.join("luau-lsp"));

        // `cargo run`, and a test binary.
        assert_eq!(pinned_beside(&root.path().join("target/debug/cru")), pinned);
        assert_eq!(
            pinned_beside(&root.path().join("target/debug/deps/crucible_lua-abc")),
            pinned
        );

        // An installed binary finds nothing, even with the same tree present.
        assert_eq!(pinned_beside(&root.path().join(".local/bin/cru")), None);
    }

    /// A `target/tools/luau-lsp` the BINARY does not live under is not a
    /// checker.
    ///
    /// `cru plugin check` is pointed at code an author downloaded, and running
    /// it inside that tree is reasonable. Resolving from the working directory
    /// would let whoever wrote the tree choose the executable this process
    /// runs; the function takes no working directory at all, and this states
    /// the consequence.
    #[test]
    fn a_target_tools_the_binary_does_not_live_under_is_not_a_checker() {
        let hostile = TempDir::new().unwrap();
        let tools = hostile.path().join("target/tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join("luau-lsp"), "#!/bin/sh\nexit 0\n").unwrap();

        // A binary that lives somewhere else entirely, as an installed `cru`
        // does. Nothing under the hostile tree can be reached from it.
        assert_eq!(
            pinned_beside(&hostile.path().join("downloaded/plugin/init.luau")),
            None,
            "only a directory named `target` on the BINARY's path counts"
        );
    }

    /// A binary that runs, exits 0 and says nothing is NOT a checker.
    ///
    /// `Checker::at` classifies on the file name alone, so
    /// `CRUCIBLE_LUAU_ANALYZE=/bin/true` used to satisfy every gate: it ran, it
    /// exited 0, it produced no diagnostics, and the report said the typecheck
    /// RAN over a deliberately broken file.
    #[cfg(unix)]
    #[test]
    fn a_binary_that_finds_nothing_does_not_prove_it_checks_types() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let liar = tmp.path().join("liar-lsp");
        std::fs::write(&liar, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&liar, std::fs::Permissions::from_mode(0o755)).unwrap();

        let why = Checker::at(&liar)
            .proves_types()
            .expect_err("a binary that reports nothing must not pass the probe");
        assert!(why.contains("not checking types"), "{why}");
    }

    /// And the real one does prove it, so the probe is not merely strict.
    #[test]
    fn the_installed_checker_proves_it_checks_types() {
        let Some(CheckerChoice::Use(checker)) = installed_checker() else {
            return;
        };
        checker
            .proves_types()
            .expect("the pinned build checks types");
    }

    fn restore_env(restore: Option<std::ffi::OsString>) {
        // SAFETY: the caller holds `env_lock()`.
        match restore {
            Some(v) => unsafe { std::env::set_var("CRUCIBLE_LUAU_ANALYZE", v) },
            None => unsafe { std::env::remove_var("CRUCIBLE_LUAU_ANALYZE") },
        }
    }

    /// A VM with `require`, `cru.on` and the two session hooks, and no
    /// daemon: what this crate alone builds. The CLI hands the check the
    /// daemon loader's VM instead, which carries every `cru.*` module.
    fn check_vm() -> crate::LuaExecutor {
        let vm = crate::LuaExecutor::new().expect("a VM");
        crate::handlers::register_cru_on_api(vm.lua(), LuaScriptHandlerRegistry::new())
            .expect("cru.on");
        vm
    }

    /// A plugin directory named `name`, under a fresh temporary root, with
    /// `init.luau` and, when given, `spec.luau`. The name matters: the check
    /// enters `LuaSource::Plugin(<directory name>)`, as activation does.
    fn plugin_dir(name: &str, fragment: Option<&str>, init: &str) -> (TempDir, PathBuf) {
        plugin_dir_with_module(name, None, fragment, init)
    }

    /// [`plugin_dir`], with one module under the plugin's own `lua/`.
    fn plugin_dir_with_module(
        name: &str,
        module: Option<(&str, &str)>,
        fragment: Option<&str>,
        init: &str,
    ) -> (TempDir, PathBuf) {
        let root = TempDir::new().unwrap();
        let dir = root.path().join(name);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("init.luau"), init).unwrap();
        if let Some(fragment) = fragment {
            std::fs::write(dir.join("spec.luau"), fragment).unwrap();
        }
        if let Some((module_name, source)) = module {
            let lua_dir = dir.join("lua");
            std::fs::create_dir_all(&lua_dir).unwrap();
            std::fs::write(lua_dir.join(format!("{module_name}.luau")), source).unwrap();
        }
        (root, dir)
    }

    fn load_findings(report: &CheckReport) -> Vec<&str> {
        report
            .findings
            .iter()
            .filter_map(|f| match f {
                Finding::Load { message } => Some(message.as_str()),
                _ => None,
            })
            .collect()
    }

    /// A registration in the module body is a finding that names each hook,
    /// and it fails the check.
    #[test]
    fn check_reports_a_top_level_effect() {
        let (_root, dir) = plugin_dir(
            "noisy",
            None,
            "cru.on(\"turn:complete\", function() end)\n\
             cru.on_session_end(function() end)\n\
             return {}\n",
        );
        let vm = check_vm();
        let report = check_plugin_on(&dir, None, false, &NO_CHECKER, &vm).expect("check runs");
        let loads = load_findings(&report);
        assert!(
            loads.iter().any(|m| m.contains("2 registration(s) at top level")
                && m.contains("turn:complete")
                && m.contains("session:end")
                && m.contains("setup()")),
            "{:?}",
            report.findings
        );
        assert!(!report.passed(), "a top-level effect fails the check");
    }

    /// The same registration made from `setup()` is not an effect of the
    /// module body, and a top-level `require` of the plugin's own module is
    /// correct code.
    #[test]
    fn check_accepts_a_top_level_require_of_the_plugins_own_module() {
        let (_root, dir) = plugin_dir_with_module(
            "tidy",
            Some(("helper", "return { n = 1 }\n")),
            None,
            "local h = require(\"helper\")\n\
             assert(h.n == 1)\n\
             return { setup = function() cru.on(\"turn:complete\", function() end) end }\n",
        );
        let vm = check_vm();
        let report = check_plugin_on(&dir, None, false, &NO_CHECKER, &vm).expect("check runs");
        assert!(
            load_findings(&report).is_empty(),
            "{:?}",
            report.findings
        );
        assert!(report.passed(), "{:?}", report.findings);
    }

    /// The fragment is read, and the module's declarations are read from the
    /// table the body returned on the VM.
    #[test]
    fn check_reads_the_fragment_and_the_module_declarations() {
        let (_root, dir) = plugin_dir(
            "typed",
            Some("return { version = \"1.0.0\" }\n"),
            "return { tools = { t = { desc = \"t\", \
             params = { { name = \"p\", type = \"not a type\", desc = \"\" } }, \
             fn = function() end } } }\n",
        );
        let vm = check_vm();
        let report = check_plugin_on(&dir, None, false, &NO_CHECKER, &vm).expect("check runs");
        assert!(
            report
                .findings
                .iter()
                .any(|f| matches!(f, Finding::Declaration { .. })),
            "{:?}",
            report.findings
        );

        // And a fragment the host cannot read is a finding that names it.
        let (_root, dir) = plugin_dir("badspec", Some("return { version = 3 }\n"), "return {}\n");
        let report = check_plugin_on(&dir, None, false, &NO_CHECKER, &vm).expect("check runs");
        assert!(
            load_findings(&report)
                .iter()
                .any(|m| m.contains("spec.luau")),
            "{:?}",
            report.findings
        );
    }

    /// A body that raises is a finding, and the VM is left as the check
    /// found it: no source in force, no registrations under the plugin's
    /// name.
    #[test]
    fn check_leaves_the_vm_clean() {
        let (_root, dir) = plugin_dir(
            "messy",
            None,
            "cru.on(\"turn:complete\", function() end)\n\
             error(\"boom\")\n",
        );
        let vm = check_vm();
        let report = check_plugin_on(&dir, None, false, &NO_CHECKER, &vm).expect("check runs");
        assert!(
            load_findings(&report).iter().any(|m| m.contains("boom")),
            "{:?}",
            report.findings
        );
        assert!(
            load_findings(&report)
                .iter()
                .any(|m| m.contains("at top level")),
            "the registration before the raise still counts: {:?}",
            report.findings
        );
        assert_eq!(
            crate::plugin_context::current_source(vm.lua()),
            crate::LuaSource::UserLua
        );
        let registry = crate::handlers::registry_of(vm.lua()).unwrap();
        assert!(registry
            .for_source(&crate::LuaSource::Plugin("messy".into()))
            .is_empty());
    }
}
