//! The request and reply types of the daemon RPC methods.
//!
//! Both sides of the wire use them: a client serializes each type, and the
//! daemon handler deserializes the same type. They live in core so that no
//! client needs the daemon crate to name them.

mod agent;
mod common;
mod config;
mod lua;
mod notifications;
mod plugin;
mod proposals;
mod session;
mod storage;
mod subscription;
mod workflow;

#[cfg(test)]
mod golden_tests;

pub use agent::*;
pub use common::*;
pub use config::*;
pub use lua::*;
pub use notifications::*;
pub use plugin::*;
pub use proposals::*;
pub use session::*;
pub use storage::*;
pub use subscription::*;
pub use workflow::*;

/// The payloads below are the ones that the daemon handlers accepted when
/// they read each field by hand. The handlers now deserialize these types, so
/// the serde defaults on the types keep those payloads valid.
#[cfg(test)]
mod old_payloads {
    use super::*;
    use serde_json::json;

    fn parse<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
        serde_json::from_value(value).expect("an old payload must still parse")
    }

    #[test]
    fn absent_optional_fields_take_the_old_handler_defaults() {
        let search: SearchTextRequest = parse(json!({"kiln": "k", "query": "q"}));
        assert_eq!(search.limit, 20);

        let send: Scoped<MessageInput> = parse(json!({"session_id": "s", "content": "c"}));
        assert!(send.body.is_interactive);
        assert!(send.body.comments.is_empty());

        let send: Scoped<MessageInput> =
            parse(json!({"session_id": "s", "content": "c", "comments": null}));
        assert!(send.body.comments.is_empty());

        let cleanup: SessionCleanupRequest = parse(json!({"older_than_days": 3}));
        assert!(!cleanup.dry_run && !cleanup.all_kilns && cleanup.kilns.is_empty());

        let undo: Scoped<UndoCount> = parse(json!({"session_id": "s"}));
        assert_eq!(undo.body.count, None);

        let listed: SessionListRequest = parse(json!({}));
        assert!(listed.kilns.is_empty());
    }

    #[test]
    fn the_single_kiln_spelling_still_names_the_kiln_set() {
        let listed: SessionListRequest = parse(json!({"kiln": "docs"}));
        assert_eq!(listed.kilns, ["docs"]);
        let persisted: SessionListPersistedRequest = parse(json!({"kiln": "docs"}));
        assert_eq!(persisted.kilns, ["docs"]);
        let search: SessionSearchRequest = parse(json!({"query": "q", "kiln": "docs"}));
        assert_eq!(search.kilns, ["docs"]);
    }
}
