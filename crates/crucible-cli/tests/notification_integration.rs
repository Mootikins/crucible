//! Integration tests for TUI notifications
//!
//! These tests require filesystem events and are marked as ignored
//! for regular test runs. Run with: cargo test -- --ignored

#[tokio::test]
#[ignore = "requires filesystem events - manual test only"]
async fn test_notifications_from_file_changes() {
    // TODO: Setup temp kiln
    // TODO: Start TUI in test mode
    // TODO: Create file
    // TODO: Verify notification appears
    // TODO: Wait for expiry
    // TODO: Verify notification gone
}

#[tokio::test]
#[ignore = "requires filesystem events - manual test only"]
async fn test_notification_truncation_in_real_tui() {
    // TODO: Setup temp kiln
    // TODO: Start TUI in test mode
    // TODO: Create file with very long name
    // TODO: Verify notification is truncated
}

#[tokio::test]
#[ignore = "requires filesystem events - manual test only"]
async fn test_error_notifications_take_priority() {
    // TODO: Setup temp kiln
    // TODO: Start TUI in test mode
    // TODO: Trigger file change
    // TODO: Trigger embedding failure
    // TODO: Verify error notification shown (not file change)
}
