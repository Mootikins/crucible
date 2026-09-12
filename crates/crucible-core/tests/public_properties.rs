//! The gate between what a note's author wrote and what the daemon stamped.

use std::collections::HashMap;

use crucible_core::storage::note_store::{
    public_properties, DAEMON_PROPERTY_KEYS, SCOPE_PROPERTY_KEY,
};
use serde_json::json;

fn properties() -> HashMap<String, serde_json::Value> {
    HashMap::from([
        ("status".to_string(), json!("doing")),
        ("rating".to_string(), json!(4)),
        (
            SCOPE_PROPERTY_KEY.to_string(),
            json!({ "kind": "workspace", "path": "/home/someone/work" }),
        ),
    ])
}

#[test]
fn keeps_what_the_author_wrote() {
    let public = public_properties(&properties());
    assert_eq!(public.get("status"), Some(&json!("doing")));
    assert_eq!(public.get("rating"), Some(&json!(4)));
}

/// `scope` is not data with a sensitive field in it — it IS the same-workspace
/// visibility predicate, and it carries an absolute host path.
#[test]
fn never_hands_out_the_scope_stamp() {
    let public = public_properties(&properties());
    assert!(!public.contains_key(SCOPE_PROPERTY_KEY));
    let json = serde_json::to_string(&public).unwrap();
    assert!(!json.contains("/home/someone/work"), "a host path escaped: {json}");
}

/// The table is the gate. A stamped key that leaves it ships by default, so
/// this fails if `scope` is ever dropped from the list.
#[test]
fn every_stamped_key_is_in_the_table() {
    assert!(DAEMON_PROPERTY_KEYS.contains(&SCOPE_PROPERTY_KEY));
    for key in DAEMON_PROPERTY_KEYS {
        let all = HashMap::from([(key.to_string(), json!("stamped"))]);
        assert!(
            public_properties(&all).is_empty(),
            "{key} is in the table but still ships"
        );
    }
}

#[test]
fn an_empty_map_stays_empty() {
    assert!(public_properties(&HashMap::new()).is_empty());
}
