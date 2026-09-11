//! One rule that keeps a credential out of a config answer.
//!
//! A config value is served whole to callers who are not the operator: the
//! web API hands `GET /api/config` the daemon's entire effective config plus
//! one origin row per recorded leaf. `LlmProviderConfig::api_key` and
//! `WebConfig::api_key` redact on `Debug` and not on `Serialize`, so every
//! provider key and the web server's own long-lived key rode out in that one
//! answer.
//!
//! **The rule reads the NAME of a leaf, not its path.** A list of paths —
//! `web.api_key`, `llm.providers.*.api_key` — is a list somebody has to
//! remember to extend, and the day a new struct grows an `api_key` the list
//! is silently one entry short. A name rule applied at every depth covers
//! that field the moment it exists, which is the property this module is for:
//! a NEW secret-bearing field must not ship because nobody updated a list.
//!
//! The cost of a name rule is the opposite mistake — redacting an ordinary
//! leaf whose name reads like a credential. [`names_a_credential`] is
//! therefore anchored at the end of the name rather than matching anywhere in
//! it, so `max_tokens` and the `key` field of an origin row keep their values.

use serde_json::Value;

/// What a redacted value is replaced with.
///
/// A marker rather than a removal: the settings UI distinguishes "configured"
/// from "not configured", and a removed key reads as the second. A `null`
/// leaf holds no credential and is left as it is, so the two stay
/// distinguishable.
pub const REDACTED: &str = "[redacted]";

/// The words that mark a config leaf as a credential.
///
/// Each is matched as a whole trailing word — the leaf is exactly the word,
/// or ends with `_word`. `key` is deliberately NOT here as a standalone word:
/// an origin row is `{key, value, source}`, and its `key` field holds the
/// dotted path of the leaf, not a secret. The `_key` suffix is matched
/// separately for that reason.
const CREDENTIAL_WORDS: &[&str] = &["token", "secret", "password", "passphrase", "credential"];

/// Whether a config leaf's NAME says it holds a credential.
///
/// Matches `api_key`, `openai_api_key`, `secret_key`, `oauth_token`,
/// `client_secret`, `password`. Does not match `key` (the origin row's path
/// field), `value`, or a plural such as `max_tokens` — the anchor is the end
/// of the name, so a longer word ending in the credential word does not
/// accidentally qualify.
pub fn names_a_credential(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.ends_with("_key")
        || CREDENTIAL_WORDS
            .iter()
            .any(|word| name == *word || name.ends_with(&format!("_{word}")))
}

/// Redact every credential in a config answer, in place.
///
/// Two shapes carry one, and both are handled here rather than at each call
/// site, because a caller that handles one and forgets the other is exactly
/// how the leak shipped:
///
/// 1. **A nested config tree.** Any leaf whose own name names a credential,
///    at any depth, on any branch.
/// 2. **A flat origin row**, `{key, value, source, …}`. The credential sits
///    under `value`, whose name says nothing; the row names its leaf in the
///    sibling `key` field, so the rule reads the last segment of that path.
pub fn redact_credentials(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // Shape 2 first: the row's own `value` field is redacted by the
            // path it carries, before the walk below descends into it.
            let row_holds_a_credential = map
                .get("key")
                .and_then(Value::as_str)
                .is_some_and(path_names_a_credential);
            if row_holds_a_credential {
                if let Some(held) = map.get_mut("value") {
                    redact_in_place(held);
                }
            }
            for (name, child) in map.iter_mut() {
                if names_a_credential(name) {
                    redact_in_place(child);
                } else {
                    redact_credentials(child);
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(redact_credentials),
        _ => {}
    }
}

/// Whether a dot-joined config path ends in a leaf that names a credential.
fn path_names_a_credential(path: &str) -> bool {
    path.rsplit('.').next().is_some_and(names_a_credential)
}

/// Replace a value that a credential may sit in.
///
/// `null` means the leaf is unset, and there is nothing to hide; marking it
/// would tell a reader a key is configured when none is. Everything else
/// becomes the marker, whatever its type — a credential held in a nested
/// record must not survive because it was not a bare string.
fn redact_in_place(value: &mut Value) {
    if !value.is_null() {
        *value = Value::String(REDACTED.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_rule_matches_a_credential_name_at_the_end_and_nowhere_else() {
        for name in [
            "api_key",
            "API_KEY",
            "openai_api_key",
            "secret_key",
            "token",
            "oauth_token",
            "secret",
            "client_secret",
            "password",
            "passphrase",
            "credential",
        ] {
            assert!(names_a_credential(name), "{name} names a credential");
        }
        for name in [
            // The origin row's own path field, which holds `chat.model`.
            "key",
            // …and the field the path points at.
            "value",
            // A count of tokens is not a token.
            "max_tokens",
            "tokens",
            "keyring_path",
            "secrets_file",
            "model",
        ] {
            assert!(!names_a_credential(name), "{name} names no credential");
        }
    }

    #[test]
    fn a_credential_is_redacted_at_every_depth_and_under_any_parent() {
        let mut value = json!({
            "web": { "api_key": "web-secret", "port": 4321 },
            "llm": { "providers": {
                "openai": { "type": "openai", "api_key": "sk-one" },
                "zai": { "type": "zai", "api_key": "sk-two" },
            }},
            "mcp": { "servers": [ { "name": "one", "auth_token": "tok" } ] },
            // A field no list knows about, on a struct that does not exist
            // yet. Nothing registers it, and it is redacted anyway — that is
            // the property this module exists for.
            "future": { "some": { "new": { "service_api_key": "sk-future" } } },
        });
        redact_credentials(&mut value);

        assert_eq!(value["web"]["api_key"], json!(REDACTED));
        assert_eq!(value["web"]["port"], json!(4321), "an ordinary leaf stays");
        assert_eq!(
            value["llm"]["providers"]["openai"]["api_key"],
            json!(REDACTED)
        );
        assert_eq!(value["llm"]["providers"]["zai"]["api_key"], json!(REDACTED));
        assert_eq!(
            value["llm"]["providers"]["openai"]["type"],
            json!("openai"),
            "the record must survive, redacted rather than dropped"
        );
        assert_eq!(
            value["mcp"]["servers"][0]["auth_token"],
            json!(REDACTED),
            "an array is walked too"
        );
        assert_eq!(
            value["future"]["some"]["new"]["service_api_key"],
            json!(REDACTED),
            "a credential no list knows about is covered by the rule alone"
        );
    }

    #[test]
    fn an_origin_row_is_redacted_by_the_path_it_carries() {
        let mut value = json!({ "origins": [
            { "key": "llm.providers.openai.api_key", "value": "sk-one", "source": "lua" },
            { "key": "web.api_key", "value": "web-secret", "source": "lua" },
            { "key": "chat.model", "value": "gpt-5", "source": "settings" },
        ]});
        redact_credentials(&mut value);

        let rows = value["origins"].as_array().expect("rows");
        assert_eq!(rows[0]["value"], json!(REDACTED));
        assert_eq!(
            rows[0]["key"],
            json!("llm.providers.openai.api_key"),
            "the path a row names is not itself a credential"
        );
        assert_eq!(rows[1]["value"], json!(REDACTED));
        assert_eq!(
            rows[2]["value"],
            json!("gpt-5"),
            "an ordinary leaf's row keeps its value"
        );
    }

    #[test]
    fn an_unset_credential_stays_null_so_configured_reads_differently() {
        let mut value = json!({ "web": { "api_key": null } });
        redact_credentials(&mut value);
        assert_eq!(value["web"]["api_key"], json!(null));
    }

    /// The false-positive gate, read off the running type rather than a list
    /// of leaf names.
    ///
    /// A default config holds no credential — every `api_key` defaults to
    /// `None` — so redacting it must change NOTHING. The day the rule starts
    /// matching an ordinary leaf that has a default value, or the day a
    /// credential grows a default, this fails and names the difference.
    #[cfg(feature = "toml")]
    #[test]
    fn redacting_the_default_config_changes_nothing() {
        let defaults = serde_json::to_value(crate::config::CliAppConfig::default())
            .expect("the default config serialises");
        let mut redacted = defaults.clone();
        redact_credentials(&mut redacted);
        assert_eq!(
            redacted, defaults,
            "the rule matched a leaf of the default config that holds no credential"
        );
    }

    #[test]
    fn a_credential_held_in_a_record_does_not_survive_by_not_being_a_string() {
        let mut value = json!({ "api_key": { "env": "OPENAI_API_KEY", "resolved": "sk-one" } });
        redact_credentials(&mut value);
        assert_eq!(value["api_key"], json!(REDACTED));
    }
}
