use crate::config::CliConfig;
use anyhow::Result;

/// `cru session reindex` is retired.
///
/// The RPC it drove indexed `{kiln}/.crucible/sessions` into that kiln's
/// NoteStore; sessions no longer live in a kiln, so there is no per-kiln
/// session corpus to rebuild and `rpc/dispatch.rs` answers the method with
/// METHOD_NOT_FOUND. Short-circuiting here rather than deleting the subcommand
/// means an existing invocation gets the explanation instead of either an
/// unknown-subcommand error or an RPC failure it cannot act on.
///
/// It exits successfully because the end state it was asked for — sessions not
/// indexed as kiln notes — already holds. The one leftover it cannot fix is
/// noted for the user.
pub(super) async fn reindex(_config: CliConfig, _force: bool) -> Result<()> {
    println!("`cru session reindex` is retired.");
    println!();
    println!(
        "Sessions are stored in the daemon's own root now, not inside a kiln, and are no \
         longer indexed as kiln notes. There is nothing left for this command to rebuild."
    );
    println!();
    println!(
        "If an earlier reindex wrote `sessions/*` note rows into a kiln's index, they are \
         stale and should be purged."
    );

    Ok(())
}
