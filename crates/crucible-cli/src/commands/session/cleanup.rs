use super::io::sessions_dir;
use crate::common::daemon_client;
use crate::config::CliConfig;
use anyhow::Result;

/// Delete sessions older than `older_than` days.
///
/// Sessions live in one flat root now, so what used to be bounded by the
/// kiln's own directory has to be bounded by the request instead. The default
/// is the invoking kiln's scope — only sessions that share it are eligible —
/// and `--all-kilns` is the deliberate widening to every session on the
/// machine. Both the flag and the daemon's `all_kilns` param exist so no
/// existing invocation can silently grow a blast radius.
pub(super) async fn cleanup(
    config: CliConfig,
    older_than: u32,
    dry_run: bool,
    all_kilns: bool,
) -> Result<()> {
    let sessions_path = sessions_dir(&config);

    if !sessions_path.exists() {
        println!("No sessions directory found.");
        return Ok(());
    }

    let client = daemon_client().await?;

    // The CLI's scope is the invoking kiln; `--all-kilns` replaces it rather
    // than adding to it, so the set is empty in that case and the daemon takes
    // the widening from the flag alone.
    // Scope is stated in NAMES now. A config whose kiln has no `[kilns]` entry
    // yields an empty scope, which this handler refuses outright rather than
    // sweeping — the fail-closed answer for a destructive verb.
    let session_kiln = config.session_kiln_name();
    let scope: &[crucible_core::config::KilnName] = if all_kilns {
        &[]
    } else {
        session_kiln.as_slice()
    };

    let result = client
        .session_cleanup(scope, older_than as u64, dry_run, all_kilns)
        .await?;

    let deleted = result["deleted"].as_array().cloned().unwrap_or_default();
    let total = result["total"].as_u64().unwrap_or(0);
    let is_dry_run = result["dry_run"].as_bool().unwrap_or(false);

    // Echo the scope the daemon actually applied rather than the one this
    // process assumed: the output used to read as kiln-scoped while the
    // handler swept every kiln on the box.
    let scope_note = if all_kilns {
        "every kiln on this machine".to_string()
    } else {
        format!("sessions sharing {}", config.kiln_path.display())
    };

    if total == 0 {
        println!(
            "No sessions older than {} days found in {}.",
            older_than, scope_note
        );
        return Ok(());
    }

    println!(
        "Found {} sessions older than {} days in {}:",
        total, older_than, scope_note
    );

    for id in &deleted {
        if let Some(s) = id.as_str() {
            println!("  {}", s);
        }
    }

    if is_dry_run {
        println!("\nDry run - no sessions deleted.");
    } else {
        println!("\nCleanup complete.");
    }

    Ok(())
}
