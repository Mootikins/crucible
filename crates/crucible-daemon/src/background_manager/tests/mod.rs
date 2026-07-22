use super::*;
use tokio::sync::broadcast;

mod bash;

pub(super) fn create_manager() -> BackgroundJobManager {
    let (tx, _) = broadcast::channel(16);
    BackgroundJobManager::new(tx)
}
