use std::sync::Arc;

use async_trait::async_trait;
use crucible_cli::chat::{SlashCommand, SlashCommandRegistryBuilder};
use crucible_core::traits::chat::{ChatContext, ChatResult, CommandHandler};
use crucible_core::traits::{Registry, RegistryBuilder};

struct NoopHandler;

#[async_trait]
impl CommandHandler for NoopHandler {
    async fn execute(&self, _args: &str, _ctx: &mut dyn ChatContext) -> ChatResult<()> {
        Ok(())
    }
}

#[test]
fn contract_empty_registry_reports_empty_state() {
    let registry = SlashCommandRegistryBuilder::default().build();

    assert!(registry.is_empty(), "new registry should report empty");
    assert_eq!(registry.len(), 0, "empty registry length should be zero");
}

#[test]
fn contract_get_and_contains_are_consistent() {
    let handler: Arc<dyn CommandHandler> = Arc::new(NoopHandler);
    let registry = SlashCommandRegistryBuilder::default()
        .command("help", Arc::clone(&handler), "Show help")
        .command("search", Arc::clone(&handler), "Search notes")
        .build();

    assert!(registry.contains("help"));
    assert!(registry.get("help").is_some());
    assert!(!registry.contains("missing"));
    assert!(registry.get("missing").is_none());
}

#[test]
fn contract_iter_and_len_match_registered_entries() {
    let handler: Arc<dyn CommandHandler> = Arc::new(NoopHandler);
    let registry = SlashCommandRegistryBuilder::default()
        .command("a", Arc::clone(&handler), "Command A")
        .command("b", Arc::clone(&handler), "Command B")
        .command("c", Arc::clone(&handler), "Command C")
        .build();

    let iter_count = registry.iter().count();

    assert_eq!(iter_count, registry.len(), "iter() must enumerate len() entries");
}

#[test]
fn contract_builder_register_build_roundtrip_preserves_values() {
    let handler: Arc<dyn CommandHandler> = Arc::new(NoopHandler);
    let direct = SlashCommand::new(Arc::clone(&handler), "direct", "Direct registration", None);

    let registry = SlashCommandRegistryBuilder::default()
        .register("direct".to_string(), direct)
        .build();

    let value = registry
        .get("direct")
        .expect("registered key should be retrievable after build");
    assert_eq!(value.descriptor.name, "direct");
    assert_eq!(value.descriptor.description, "Direct registration");
}
