//! The prose the daemon serves to a settings UI must name commands that exist.
//!
//! `config.controls` carries a description for every app-config control and a
//! reason for every leaf that takes none. A frontend renders both as user
//! prose, so a command named there is an instruction. One named
//! `cru permissions`, which has never existed, next to a claim about where the
//! permission prompt writes — and nothing could fail, because the strings live
//! in `crucible-lua` and the command table lives here.
//!
//! The expectation is the running parser's own subcommand list, not a copy of
//! it: a command added, renamed or removed moves this gate with it.

use std::collections::BTreeSet;

use clap::CommandFactory;

use crate::cli::Cli;

/// Every `cru <word>` in the served control tree names a real subcommand.
#[test]
fn served_config_prose_names_only_real_cru_commands() {
    let command = Cli::command();
    let known: BTreeSet<String> = command
        .get_subcommands()
        .flat_map(|sub| {
            std::iter::once(sub.get_name().to_string())
                .chain(sub.get_all_aliases().map(str::to_string))
        })
        .collect();
    assert!(
        known.contains("config"),
        "the subcommand list must be the real one: {known:?}"
    );

    let served = serde_json::json!({
        "options": crucible_lua::options::app_config::app_config_options(),
        "read_only": crucible_lua::options::app_config::app_config_read_only(),
    });

    let mut named = BTreeSet::new();
    collect_cru_commands(&served, &mut named);
    assert!(
        !named.is_empty(),
        "the served tree names no command at all, so this gate would pass on anything"
    );

    let unknown: Vec<&String> = named.iter().filter(|name| !known.contains(*name)).collect();
    assert!(
        unknown.is_empty(),
        "the settings UI is told to run commands that do not exist: {unknown:?}"
    );
}

/// Walk every string in `value` and record the word after each `cru ` mention.
///
/// Only a space-separated mention counts. `cru.config.set` is Lua, not a shell
/// command, and a reader cannot type it at a prompt.
fn collect_cru_commands(value: &serde_json::Value, into: &mut BTreeSet<String>) {
    match value {
        serde_json::Value::String(text) => {
            for (index, _) in text.match_indices("cru ") {
                let rest = &text[index + "cru ".len()..];
                let word: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                    .collect();
                if !word.is_empty() {
                    into.insert(word);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_cru_commands(item, into);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                collect_cru_commands(item, into);
            }
        }
        _ => {}
    }
}
