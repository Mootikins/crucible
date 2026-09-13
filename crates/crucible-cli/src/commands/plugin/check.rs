//! `cru plugin check` — parse, declaration and type checks over one plugin.

use anyhow::{Context, Result};
use crucible_lua::{check_plugin_on, find_checker, CheckerChoice, TypecheckStatus};

use super::CheckArgs;
use crate::config::CliConfig;

pub async fn execute(_config: CliConfig, args: CheckArgs) -> Result<()> {
    let plugin_dir = args
        .path
        .canonicalize()
        .with_context(|| format!("no such plugin directory: {}", args.path.display()))?;

    // The checker, resolved once and PROVED before it is trusted. The gates
    // in the daemon do exactly this. Doing less here would let this command
    // print "typecheck: ran" over a binary that reports nothing — the same
    // false green the gates exist to prevent, in the output an author reads.
    let checker = find_checker();
    if let CheckerChoice::Use(found) = &checker {
        if let Err(why) = found.proves_types() {
            eprintln!("✗ {why}");
            std::process::exit(1);
        }
        // WHICH one ran. The pinned `target/tools/luau-lsp` is tried before
        // PATH, so an author who installed a newer `luau-lsp` on purpose would
        // otherwise be overridden by the pinned build with nothing to say so.
        println!("checker: {}", found.path().display());
    }

    // The daemon loader's VM, in this process. It carries every `cru.*`
    // module, so it both generates the declarations AND runs the plugin's
    // `init.luau` for the top-level-effect check — a plugin's top-level
    // `require` of its own modules is correct code the read-only fragment
    // environment cannot run. The check restores the source and clears the
    // plugin's registrations, so the VM is clean of them when it returns.
    // The module roots it set stay in force.
    let loader =
        crucible_daemon::daemon_plugins::DaemonPluginLoader::new(std::collections::HashMap::new())?;

    // The generated declarations. A check WITHOUT them is not a weaker check,
    // it is a wrong one: every host global — `cru`, `io`, `require`, `it`,
    // `expect` — reads as undeclared, and every `os.tmpname` or `mock(...)`
    // return reads as a type error against correct code. So build them when
    // nothing has, from THIS working tree, the same way the daemon's gates do.
    let mut generated: Option<tempfile::TempDir> = None;
    let definitions = match args.definitions.or_else(default_definitions) {
        Some(path) => Some(path),
        None => {
            let dir = tempfile::tempdir().context("a directory for the declarations")?;
            loader.generate_stubs(dir.path())?;
            let path = dir.path().join("cru.d.luau");
            generated = Some(dir);
            println!("declarations: built from this working tree");
            Some(path)
        }
    };

    let report = check_plugin_on(
        &plugin_dir,
        definitions.as_deref(),
        !args.skip_tests,
        &checker,
        loader.executor(),
    )
    .with_context(|| format!("checking {}", plugin_dir.display()))?;
    // The temporary directory outlives the check that reads it.
    drop(generated.take());

    println!("{} file(s) parsed", report.files_checked);
    match report.typecheck {
        TypecheckStatus::Ran if args.skip_tests => {
            println!("typecheck: ran (shipped code only; the suite was skipped)")
        }
        TypecheckStatus::Ran => {
            println!("typecheck: ran (shipped code and suite)")
        }
        // The three places `crucible_lua::find_checker` reads, named in the
        // order it reads them. `target/tools/luau-lsp` is on that list now:
        // the daemon's gates always read it and this command did not, so a
        // checkout where `just luau-lsp` had run typechecked in the gates and
        // printed SKIPPED here over the very same files.
        TypecheckStatus::Skipped => println!(
            "typecheck: SKIPPED — run `just luau-lsp` in the checkout, install \
             `luau-lsp` on PATH, or set CRUCIBLE_LUAU_ANALYZE, to check types."
        ),
    }

    if report.passed() {
        println!("✓ {}", plugin_dir.display());
        return Ok(());
    }

    eprintln!("✗ {}", plugin_dir.display());
    for finding in &report.findings {
        eprintln!("  {finding}");
    }
    std::process::exit(1);
}

/// Where `cru plugin stubs` writes by default.
fn default_definitions() -> Option<std::path::PathBuf> {
    let path = super::stubs::default_stub_dir().ok()?.join("cru.d.luau");
    path.is_file().then_some(path)
}
