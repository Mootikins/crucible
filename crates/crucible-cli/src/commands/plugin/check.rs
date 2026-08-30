//! `cru plugin check` — parse, declaration and type checks over one plugin.

use anyhow::{Context, Result};
use crucible_lua::{check::check_plugin_with, TypecheckStatus};

use super::CheckArgs;
use crate::config::CliConfig;

pub async fn execute(_config: CliConfig, args: CheckArgs) -> Result<()> {
    let plugin_dir = args
        .path
        .canonicalize()
        .with_context(|| format!("no such plugin directory: {}", args.path.display()))?;

    // The generated declarations, when the daemon has written them. Without
    // them a `cru.*` call typechecks as `any`, which is weaker but still
    // catches everything inside the plugin itself.
    let definitions = args.definitions.or_else(default_definitions);

    let report = check_plugin_with(&plugin_dir, definitions.as_deref(), args.include_tests)
        .with_context(|| format!("checking {}", plugin_dir.display()))?;

    println!("{} file(s) parsed", report.files_checked);
    match report.typecheck {
        TypecheckStatus::Ran if args.include_tests => println!("typecheck: ran (suite included)"),
        TypecheckStatus::Ran => {
            println!("typecheck: ran (shipped code; --include-tests for the suite)")
        }
        TypecheckStatus::Skipped => println!(
            "typecheck: SKIPPED — install `luau-lsp` (or set CRUCIBLE_LUAU_ANALYZE) \
             to check types."
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
