use super::*;

mod bash;

pub(super) fn create_manager() -> BackgroundJobManager {
    let (tx, _) = crate::EventBus::channel(16);
    BackgroundJobManager::new(tx)
}
