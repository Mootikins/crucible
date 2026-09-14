use super::super::reindex::reindex;
use super::{setup_test_session, test_config, test_sessions_dir};
use tempfile::TempDir;

/// The daemon retired `session.reindex` — dispatch answers METHOD_NOT_FOUND —
/// so the subcommand must never reach for it. The populated-sessions case is
/// the one that used to fall through to the RPC and could therefore only fail;
/// the two empty-directory cases never got that far, which is why the
/// regression went unnoticed.
#[tokio::test]
async fn reindex_reports_retirement_instead_of_calling_the_retired_rpc() {
    let tmp = TempDir::new().unwrap();
    let sessions_dir = test_sessions_dir(tmp.path());
    let _id = setup_test_session(&sessions_dir).await;

    let config = test_config(tmp.path());

    let result = reindex(config, false).await;
    assert!(result.is_ok(), "reindex should not error: {result:?}");
}

#[tokio::test]
async fn test_reindex_no_sessions_dir() {
    let tmp = TempDir::new().unwrap();
    let config = test_config(tmp.path());

    let result = reindex(config, false).await;
    assert!(result.is_ok());
}
