//! The app config's own control tree — the second producer of one vocabulary.
//!
//! A plugin declares its settings as a live Lua table ([`super`]). The app
//! config cannot: `CliAppConfig` is a Rust type, and no Lua runs to describe
//! it. So the tree is declared here, in Rust, and rendered to the SAME JSON a
//! plugin's tree renders to — same `type` names, same `order` rule, same
//! `values` shape — so a frontend draws app settings and plugin settings with
//! one renderer.
//!
//! Reusing [`Control`] rather than inventing a second `type` vocabulary is
//! deliberate: two vocabularies would mean two renderers, and the second one
//! would drift. The module lives beside the plugin producer, next to the
//! vocabulary both share, because that is the state they have in common —
//! `Control::value_type` returns a `LuaType`, so the vocabulary cannot move
//! down into `crucible-core` without dragging `signature.rs` with it.
//!
//! **Defaults are read, not written.** A descriptor names a path and never
//! repeats the type's default value: [`app_config_defaults`] serialises
//! `CliAppConfig::default()` and the tree reads each leaf out of it, so a
//! control can never state a default the type does not have.
//!
//! **What is NOT here is stated, with the reason.** Some leaves take no
//! control: the seven that name a filesystem location, the unbounded maps and
//! lists, and the free-form `plugins` subtree. Each sits on
//! [`READ_ONLY_LEAVES`], [`LOCATION_REASON`] or [`FREE_FORM_SUBTREE`] with the
//! reason it takes none, and `every_config_leaf_has_a_control_or_a_listed_reason`
//! walks the leaves of `CliAppConfig::default()` — the running system, not a
//! hand-kept name list — and fails on any leaf that appears in neither place.

use crucible_core::config::{CliAppConfig, LoggingConfig, PermissionConfig, LOCATION_CONFIG_KEYS};
use serde_json::Value;

use super::Control;

/// One admissible value of a [`Control::Select`], with the prose that says
/// what choosing it does.
///
/// The per-variant description is what a plugin's `values` table cannot carry
/// (Ace3's is `value -> label`), and it is why an app-config select renders
/// with an explanation under each choice rather than a bare word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Choice {
    /// The value as it is serialised into the config.
    pub value: &'static str,
    /// The label a frontend shows.
    pub label: &'static str,
    /// What choosing this value does.
    pub desc: &'static str,
}

/// One node of the app-config control tree.
///
/// `path` is the dot-joined config path, which is also the key a save writes,
/// so a frontend needs no second mapping between a widget and a config key.
#[derive(Debug, Clone, Copy)]
pub struct AppControl {
    /// Dot-joined config path. The root sections have no dot.
    pub path: &'static str,
    /// What the leaf is, and therefore what it accepts.
    pub control: Control,
    /// The label a frontend shows.
    pub name: &'static str,
    /// What the setting does.
    pub desc: &'static str,
    /// Position among its siblings. Ace3's rule, which the plugin renderer
    /// already follows: lower first, `-1` last.
    pub order: i32,
    /// The admissible values of a select. Empty for every other kind.
    pub choices: &'static [Choice],
    /// Bounds of a range. `None` on every other kind.
    pub min: Option<f64>,
    /// Upper bound of a range.
    pub max: Option<f64>,
    /// Step of a range.
    pub step: Option<f64>,
}

impl AppControl {
    /// A container: holds no value, carries children.
    const fn group(path: &'static str, name: &'static str, desc: &'static str, order: i32) -> Self {
        Self::new(path, Control::Group, name, desc, order)
    }

    const fn new(
        path: &'static str,
        control: Control,
        name: &'static str,
        desc: &'static str,
        order: i32,
    ) -> Self {
        Self {
            path,
            control,
            name,
            desc,
            order,
            choices: &[],
            min: None,
            max: None,
            step: None,
        }
    }

    const fn with_choices(mut self, choices: &'static [Choice]) -> Self {
        self.choices = choices;
        self
    }

    const fn bounded(mut self, min: f64, max: f64, step: f64) -> Self {
        self.min = Some(min);
        self.max = Some(max);
        self.step = Some(step);
        self
    }

    /// The path of the group that holds this node, or `None` at the root.
    pub fn parent(&self) -> Option<&'static str> {
        self.path.rsplit_once('.').map(|(parent, _)| parent)
    }

    /// The last path segment — what a frontend uses as the node key.
    pub fn key(&self) -> &'static str {
        self.path.rsplit_once('.').map_or(self.path, |(_, key)| key)
    }
}

/// One config leaf no control writes, and why.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadOnlyLeaf {
    /// The dot-joined config path of the leaf, or of the subtree that holds it.
    pub path: &'static str,
    /// Why this leaf takes no control. Never empty: an entry with no reason is
    /// how a parity gate turns into a denylist that grows quietly.
    pub reason: &'static str,
}

/// Why the seven [`LOCATION_CONFIG_KEYS`] take no control.
///
/// Stated once and applied to the constant rather than retyped as seven
/// entries, so a location key added there arrives here already covered — and
/// with the same reason, because there is only one.
pub const LOCATION_REASON: &str = "A location key names WHERE the daemon acts. \
    The RPC socket has no authentication, so a control that wrote one would let \
    any local process re-point the daemon without a path passing the \
    registration floor.";

/// The one subtree leaf parity is not definable over.
///
/// `plugins` is a free-form map of per-plugin sections: its leaves are whatever
/// the installed plugins read, so no fixed control list can be complete and no
/// gate can prove one is. Each plugin describes its own settings through
/// `cru.plugin.options{}` instead, which is the same vocabulary reached from
/// the side that knows the answer.
pub const FREE_FORM_SUBTREE: ReadOnlyLeaf = ReadOnlyLeaf {
    path: "plugins",
    reason: "Free-form and unbounded: every installed plugin owns its own \
        section, and each one declares its settings through cru.plugin.options.",
};

/// The leaves that take no control, each with the reason it takes none.
pub const READ_ONLY_LEAVES: &[ReadOnlyLeaf] = &[
    ReadOnlyLeaf {
        path: "acp.agents",
        reason: "An unbounded map of named agent profiles. A profile is a \
            record, not a leaf, so it is authored in init.lua.",
    },
    ReadOnlyLeaf {
        path: "context.rules_files",
        reason: "An ORDERED free-text list, and no control kind holds one: \
            multiselect needs a declared value set, and text would store a \
            string where the config holds an array.",
    },
    ReadOnlyLeaf {
        path: "enrichment",
        reason: "The embedding provider is a tagged union — its leaf set \
            depends on the chosen `type` — so a flat control table cannot \
            describe it without describing four shapes at once.",
    },
    ReadOnlyLeaf {
        path: "llm.models",
        reason: "An unbounded map of model aliases to provider records.",
    },
    ReadOnlyLeaf {
        path: "llm.providers",
        reason: "An unbounded map of provider records, each holding an \
            api_key. Credentials belong to `cru auth`, which stores them \
            outside the config.",
    },
    ReadOnlyLeaf {
        path: "mcp.servers",
        reason: "An unbounded list of upstream server records, each naming a \
            command to spawn.",
    },
    ReadOnlyLeaf {
        path: "permissions.allow",
        reason: "An unbounded list of match patterns, written by hand in \
            init.lua; no control kind holds an ordered free-text list. The \
            permission prompt's \"always allow\" does not write here — that \
            grant goes to the daemon's pattern store, under the whitelists \
            directory.",
    },
    ReadOnlyLeaf {
        path: "permissions.ask",
        reason: "An unbounded list of match patterns, edited the same way \
            `permissions.allow` is.",
    },
    ReadOnlyLeaf {
        path: "permissions.deny",
        reason: "An unbounded list of match patterns, edited the same way \
            `permissions.allow` is.",
    },
    ReadOnlyLeaf {
        path: "schedules",
        reason: "An unbounded list of records, each carrying Lua to run on a \
            timer. Authoring one is authoring code.",
    },
    ReadOnlyLeaf {
        path: "web",
        reason: "Every leaf here is the web server's own authentication and \
            reach — api_key, remote_shell, allowed_hosts, registration_roots. \
            The RPC socket is unauthenticated, so a control would let a local \
            process disarm the gate that guards the browser surface.",
    },
    ReadOnlyLeaf {
        path: "workspace",
        reason: "Names filesystem locations and grants the web API read and \
            write reach over them, which is the LOCATION_CONFIG_KEYS rule \
            applied one level down.",
    },
];

/// The app-config control tree, flat. [`app_config_options`] nests it.
///
/// A group carries only its label and its order; every leaf names a path that
/// [`app_config_defaults`] holds, and its default comes from there.
pub const APP_CONTROLS: &[AppControl] = &[
    // ---- chat ----
    AppControl::group(
        "chat",
        "Chat",
        "The model a new session talks to, and what the transcript shows.",
        10,
    ),
    AppControl::new(
        "chat.model",
        Control::Input,
        "Model",
        "Default model for a new session. Empty uses the built-in default, and \
         an agent card may override it.",
        1,
    ),
    AppControl::new(
        "chat.agent_preference",
        Control::Select,
        "Agent preference",
        "Which kind of agent a session starts with when nothing names one.",
        2,
    )
    .with_choices(&[
        Choice {
            value: "acp",
            label: "External (ACP)",
            desc: "Start an external ACP agent, such as claude or opencode.",
        },
        Choice {
            value: "crucible",
            label: "Crucible",
            desc: "Start Crucible's own agent, which runs the daemon's tools \
                   and hooks.",
        },
    ]),
    AppControl::new(
        "chat.endpoint",
        Control::Input,
        "Endpoint",
        "Base URL for an OpenAI-compatible or Ollama provider. Empty uses the \
         provider's own default.",
        3,
    ),
    AppControl::new(
        "chat.show_thinking",
        Control::Toggle,
        "Show thinking",
        "Stream a model's reasoning tokens into the transcript instead of a \
         spinner.",
        4,
    ),
    AppControl::new(
        "chat.show_diffs",
        Control::Toggle,
        "Show diffs",
        "Render the diff body under an edit or write tool call, rather than \
         the header alone.",
        5,
    ),
    AppControl::new(
        "chat.context_budget",
        Control::Range,
        "Context budget",
        "Token budget for the assembled context. Empty derives it from the \
         model's own window.",
        6,
    )
    .bounded(0.0, 2_000_000.0, 1024.0),
    AppControl::new(
        "chat.precognition_results",
        Control::Range,
        "Precognition results",
        "How many notes the pre-turn kiln search injects. It runs on a \
         session's first message only.",
        7,
    )
    .bounded(0.0, 25.0, 1.0),
    AppControl::new(
        "chat.autocompact_threshold",
        Control::Range,
        "Autocompact threshold",
        "Compact the transcript once it passes this fraction of the context \
         budget. 0 turns it off.",
        8,
    )
    .bounded(0.0, 1.0, 0.05),
    AppControl::new(
        "chat.response_tail_chars",
        Control::Range,
        "Reply tail for handlers",
        "How much of a reply a turn:complete handler reads, counted from the \
         end of it. 0 sends the whole reply.",
        9,
    )
    .bounded(0.0, 20_000.0, 100.0),
    AppControl::new(
        "chat.system_prompt",
        Control::Text,
        "System prompt",
        "What a new session tells the model about itself. An agent card's own \
         prompt wins, and an on_session_start hook can extend this one.",
        10,
    ),
    // ---- cli ----
    AppControl::group(
        "cli",
        "Terminal",
        "How the terminal client draws a session.",
        20,
    ),
    AppControl::group(
        "cli.highlighting",
        "Syntax highlighting",
        "Colouring of code blocks and diffs.",
        1,
    ),
    AppControl::new(
        "cli.highlighting.enabled",
        Control::Toggle,
        "Highlight code",
        "Colour code blocks and diff bodies.",
        1,
    ),
    AppControl::new(
        "cli.highlighting.theme",
        Control::Input,
        "Theme",
        "Name of the syntect theme to highlight with. The installed set \
         depends on this box, so the name is typed rather than picked.",
        2,
    ),
    // ---- acp ----
    AppControl::group(
        "acp",
        "External agents",
        "How Crucible talks to an agent over the Agent Client Protocol.",
        30,
    ),
    AppControl::new(
        "acp.default_agent",
        Control::Input,
        "Default agent",
        "Name of the agent profile `cru chat --acp` uses when the command \
         names none.",
        1,
    ),
    AppControl::new(
        "acp.streaming_timeout_minutes",
        Control::Range,
        "Streaming timeout",
        "Minutes to wait for one complete response before the turn fails.",
        2,
    )
    .bounded(1.0, 120.0, 1.0),
    // ---- llm ----
    AppControl::group(
        "llm",
        "Providers",
        "Which configured provider a session uses.",
        40,
    ),
    AppControl::new(
        "llm.default",
        Control::Input,
        "Default provider",
        "Name of the provider instance to use when nothing names one.",
        1,
    ),
    // ---- logging ----
    AppControl::group("logging", "Logging", "What the daemon records.", 50),
    AppControl::new(
        "logging.level",
        Control::Select,
        "Log level",
        "The lowest severity the daemon writes.",
        1,
    )
    .with_choices(&[
        Choice {
            value: "trace",
            label: "Trace",
            desc: "Every step, including per-message detail. Very loud.",
        },
        Choice {
            value: "debug",
            label: "Debug",
            desc: "Enough to follow one turn through the daemon.",
        },
        Choice {
            value: "info",
            label: "Info",
            desc: "Lifecycle events only: sessions, plugins, boot.",
        },
        Choice {
            value: "warn",
            label: "Warn",
            desc: "Only what went wrong but did not stop the daemon.",
        },
        Choice {
            value: "error",
            label: "Error",
            desc: "Only failures.",
        },
    ]),
    // ---- permissions ----
    AppControl::group(
        "permissions",
        "Permissions",
        "What a turn may do without asking.",
        60,
    ),
    AppControl::new(
        "permissions.default",
        Control::Select,
        "Default decision",
        "What happens to a tool call that no rule matches.",
        1,
    )
    .with_choices(&[
        Choice {
            value: "allow",
            label: "Allow",
            desc: "Run it. Every unmatched call proceeds unprompted.",
        },
        Choice {
            value: "deny",
            label: "Deny",
            desc: "Refuse it. Only a call an allow rule matches runs.",
        },
        Choice {
            value: "ask",
            label: "Ask",
            desc: "Prompt, and let the answer decide this one call.",
        },
    ]),
    // ---- server ----
    AppControl::group("server", "Daemon", "How the daemon keeps sessions.", 70),
    AppControl::new(
        "server.auto_archive_hours",
        Control::Range,
        "Auto-archive after",
        "Hours a session may sit idle before the daemon archives it. Zero \
         archives on the next sweep.",
        1,
    )
    .bounded(0.0, 8760.0, 1.0),
    AppControl::new(
        "server.idle_shutdown_minutes",
        Control::Range,
        "Exit when idle for",
        "Minutes with no connected client and no running job before a daemon \
         that owns its own process exits. Zero keeps it running for ever. \
         Sessions are persisted, so the next command starts a fresh one.",
        2,
    )
    .bounded(0.0, 1440.0, 5.0),
    // ---- top-level ----
    AppControl::new(
        "default_kiln",
        Control::Input,
        "Default kiln",
        "Name of the kiln a session stores in and scopes tools to. It names a \
         registered kiln, not a path.",
        80,
    ),
];

/// `CliAppConfig::default()` as JSON, with each unset optional subtree filled
/// in with what the daemon behaves as if it held.
///
/// An `Option` subtree serialises to `null` when unset, but the daemon does not
/// read `null` — it falls back to the type's own default, so that fallback is
/// the value a control must show. Filling it here keeps every default derived
/// from a type rather than transcribed into a descriptor, and
/// `every_control_names_a_leaf_the_config_actually_has` fails when a control
/// names a path this value does not hold.
pub fn app_config_defaults() -> Value {
    let mut defaults =
        serde_json::to_value(CliAppConfig::default()).unwrap_or(Value::Object(Default::default()));
    fill_absent(&mut defaults, "logging", &LoggingConfig::default());
    fill_absent(&mut defaults, "permissions", &PermissionConfig::default());
    defaults
}

/// Replace a `null` top-level subtree with the serialised default of its type.
fn fill_absent<T: serde::Serialize>(defaults: &mut Value, key: &str, filled: &T) {
    let Some(slot) = defaults.get_mut(key) else {
        return;
    };
    if slot.is_null() {
        if let Ok(value) = serde_json::to_value(filled) {
            *slot = value;
        }
    }
}

/// The default value at a config path, or `None` when the path is not one the
/// config holds.
pub fn default_at(defaults: &Value, path: &str) -> Option<Value> {
    let mut node = defaults;
    for segment in path.split('.') {
        node = node.get(segment)?;
    }
    Some(node.clone())
}

/// The control declared for a config path, if there is one.
pub fn control_for(path: &str) -> Option<&'static AppControl> {
    APP_CONTROLS.iter().find(|control| control.path == path)
}

/// Why the leaf at `path` takes no control, if it takes none.
pub fn read_only_reason(path: &str) -> Option<&'static str> {
    let top = path.split('.').next().unwrap_or(path);
    if LOCATION_CONFIG_KEYS.contains(&top) {
        return Some(LOCATION_REASON);
    }
    if top == FREE_FORM_SUBTREE.path {
        return Some(FREE_FORM_SUBTREE.reason);
    }
    READ_ONLY_LEAVES
        .iter()
        .find(|entry| entry.path == path || path.starts_with(&format!("{}.", entry.path)))
        .map(|entry| entry.reason)
}

/// The control tree as the JSON a frontend renders, in the shape a plugin's
/// tree renders to.
///
/// One extra field per leaf, `default`, which a plugin node has no use for:
/// a plugin owns its storage and answers `get`, while app config is stored by
/// the daemon and read through `config.effective`, so the renderer needs the
/// default separately to show "unset" honestly.
pub fn app_config_options() -> Value {
    let defaults = app_config_defaults();
    serde_json::json!({
        "type": Control::Group.as_str(),
        "name": "Crucible",
        "args": children_of(None, &defaults),
    })
}

/// Every leaf that takes no control, each with the reason it takes none.
///
/// The same three sources [`read_only_reason`] answers from, as a list a
/// frontend can render. A settings pane that showed only the controls would
/// leave a user hunting for `data_home` and finding nothing, so the read-only
/// half travels with the writable half and states why it is read-only. The
/// reason is the constant's own prose, never a second wording here.
pub fn app_config_read_only() -> Value {
    let rows = LOCATION_CONFIG_KEYS
        .iter()
        .map(|key| (*key, LOCATION_REASON))
        .chain(
            READ_ONLY_LEAVES
                .iter()
                .map(|entry| (entry.path, entry.reason)),
        )
        .chain(std::iter::once((
            FREE_FORM_SUBTREE.path,
            FREE_FORM_SUBTREE.reason,
        )))
        .map(|(path, reason)| serde_json::json!({ "path": path, "reason": reason }))
        .collect::<Vec<_>>();
    Value::Array(rows)
}

/// The described children of one group, ordered.
fn children_of(parent: Option<&str>, defaults: &Value) -> Vec<Value> {
    let mut children: Vec<&AppControl> = APP_CONTROLS
        .iter()
        .filter(|control| control.parent() == parent)
        .collect();
    children.sort_by_key(|control| rank(control.order));
    children
        .into_iter()
        .map(|control| describe(control, defaults))
        .collect()
}

/// Ace3's order rule, which the plugin renderer already follows: a negative
/// order sorts last, everything else by value.
fn rank(order: i32) -> i64 {
    if order < 0 {
        i64::MAX
    } else {
        i64::from(order)
    }
}

fn describe(control: &AppControl, defaults: &Value) -> Value {
    let mut node = serde_json::Map::new();
    node.insert("key".into(), serde_json::json!(control.key()));
    node.insert("path".into(), serde_json::json!(control.path));
    node.insert("type".into(), serde_json::json!(control.control.as_str()));
    node.insert("name".into(), serde_json::json!(control.name));
    node.insert("desc".into(), serde_json::json!(control.desc));
    node.insert("order".into(), serde_json::json!(control.order));
    for (key, bound) in [
        ("min", control.min),
        ("max", control.max),
        ("step", control.step),
    ] {
        if let Some(bound) = bound {
            node.insert(key.into(), serde_json::json!(bound));
        }
    }
    if !control.choices.is_empty() {
        node.insert(
            "values".into(),
            serde_json::json!(control
                .choices
                .iter()
                .map(|choice| serde_json::json!({
                    "value": choice.value,
                    "label": choice.label,
                    "desc": choice.desc,
                }))
                .collect::<Vec<_>>()),
        );
    }
    if control.control.is_leaf() {
        node.insert(
            "default".into(),
            default_at(defaults, control.path).unwrap_or(Value::Null),
        );
        // Every app-config leaf is writable through `config.save`; whether one
        // SAVE is refused is a per-leaf pin, which `config.origin` answers at
        // render time and no static descriptor can.
        node.insert("writable".into(), serde_json::json!(true));
    } else {
        node.insert(
            "args".into(),
            serde_json::json!(children_of(Some(control.path), defaults)),
        );
    }
    Value::Object(node)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every serialised leaf of the default config, dot-joined.
    ///
    /// An empty object or array is a leaf: it is a value the config holds, and
    /// a value with no control is exactly what this walk exists to find.
    fn leaves(value: &Value, path: &mut String, out: &mut Vec<String>) {
        match value {
            Value::Object(map) if !map.is_empty() => {
                for (key, child) in map {
                    let saved = path.len();
                    if !path.is_empty() {
                        path.push('.');
                    }
                    path.push_str(key);
                    leaves(child, path, out);
                    path.truncate(saved);
                }
            }
            _ => out.push(path.clone()),
        }
    }

    fn default_leaves() -> Vec<String> {
        let mut out = Vec::new();
        leaves(&app_config_defaults(), &mut String::new(), &mut out);
        out
    }

    /// A leaf is covered by a control on it, or by controls INSIDE it — an
    /// unset optional subtree serialises as one `null` leaf while the settings
    /// it stands for are its children.
    fn controlled(leaf: &str) -> bool {
        APP_CONTROLS.iter().any(|control| {
            control.control.is_leaf()
                && (control.path == leaf || control.path.starts_with(&format!("{leaf}.")))
        })
    }

    /// The gate. Derived from the running system: `CliAppConfig::default()`
    /// serialises itself, so a field added to the type arrives here without
    /// anyone remembering to list it.
    #[test]
    fn every_config_leaf_has_a_control_or_a_listed_reason() {
        let unexplained: Vec<String> = default_leaves()
            .into_iter()
            .filter(|leaf| !controlled(leaf) && read_only_reason(leaf).is_none())
            .collect();

        assert!(
            unexplained.is_empty(),
            "these config leaves reach no settings control and sit on no \
             read-only list: {unexplained:#?}. Give each one an entry in \
             APP_CONTROLS, or an entry in READ_ONLY_LEAVES with the reason it \
             takes none.",
        );
    }

    /// The read-only half the settings UI draws is the same list
    /// `read_only_reason` answers from. A row with a reason the function does
    /// not give is a second wording of the same rule, and the two would drift.
    #[test]
    fn every_read_only_row_carries_the_reason_the_rule_gives() {
        let rows = app_config_read_only();
        let rows = rows.as_array().expect("a row per read-only leaf");
        for row in rows {
            let path = row["path"].as_str().expect("a row names its path");
            assert_eq!(
                row["reason"].as_str(),
                read_only_reason(path),
                "{path}: the row's reason must be the rule's own",
            );
            assert!(
                !row["reason"].as_str().unwrap_or_default().is_empty(),
                "{path}: a read-only leaf with no reason is a dead end",
            );
        }
        for key in LOCATION_CONFIG_KEYS {
            assert!(
                rows.iter().any(|row| row["path"] == serde_json::json!(key)),
                "{key} names where the daemon acts, so it must render read-only                  with its reason rather than not render at all",
            );
        }
    }

    /// A control that names a path the config does not hold renders a widget
    /// whose save lands nowhere.
    #[test]
    fn every_control_names_a_leaf_the_config_actually_has() {
        let defaults = app_config_defaults();
        for control in APP_CONTROLS {
            let value = default_at(&defaults, control.path).unwrap_or_else(|| {
                panic!("{} names a path CliAppConfig does not hold", control.path)
            });
            assert_eq!(
                control.control.is_leaf(),
                !value.is_object() || value.as_object().is_some_and(|map| map.is_empty()),
                "{}: a group must hold an object and a leaf must not",
                control.path,
            );
        }
    }

    /// Both lists answering for one leaf means one of the two answers is
    /// unread, and nobody knows which.
    #[test]
    fn no_leaf_is_both_controlled_and_read_only() {
        for control in APP_CONTROLS {
            assert!(
                read_only_reason(control.path).is_none(),
                "{} has a control AND a read-only reason",
                control.path,
            );
        }
    }

    /// A read-only entry with no leaf behind it is a name that quiets the gate
    /// without describing anything.
    #[test]
    fn every_read_only_entry_names_a_real_leaf_and_gives_a_reason() {
        let leaves: BTreeSet<String> = default_leaves().into_iter().collect();
        for entry in READ_ONLY_LEAVES.iter().chain([&FREE_FORM_SUBTREE]) {
            assert!(
                leaves.contains(entry.path),
                "{} is on the read-only list but is not a leaf of the default \
                 config: {leaves:#?}",
                entry.path,
            );
            assert!(
                entry.reason.len() > 20,
                "{} needs a reason, not a placeholder",
                entry.path,
            );
        }
    }

    /// `Control` already says which kinds need a declared value set. A select
    /// with no choices renders an empty dropdown; choices on a toggle are
    /// decoration nothing reads.
    #[test]
    fn only_a_kind_that_requires_values_declares_choices() {
        for control in APP_CONTROLS {
            assert_eq!(
                control.control.requires_values(),
                !control.choices.is_empty(),
                "{}: {:?} and its choices disagree",
                control.path,
                control.control,
            );
        }
    }

    /// The bounds belong to the kind that has them.
    #[test]
    fn only_a_range_declares_bounds() {
        for control in APP_CONTROLS {
            let bounded = control.min.is_some() || control.max.is_some() || control.step.is_some();
            assert_eq!(
                bounded,
                control.control == Control::Range,
                "{}: only a range carries min/max/step",
                control.path,
            );
            if let (Some(min), Some(max)) = (control.min, control.max) {
                assert!(min < max, "{}: min is not below max", control.path);
            }
        }
    }

    /// The serde rename of an enum variant is the value the config stores. If
    /// a variant is renamed and the choices are not, the default stops being
    /// selectable and the dropdown opens on nothing.
    #[test]
    fn a_select_offers_the_value_the_type_defaults_to() {
        let defaults = app_config_defaults();
        for control in APP_CONTROLS.iter().filter(|c| !c.choices.is_empty()) {
            let Some(Value::String(default)) = default_at(&defaults, control.path) else {
                continue;
            };
            assert!(
                control.choices.iter().any(|choice| choice.value == default),
                "{}: the type defaults to {default:?}, which no choice offers",
                control.path,
            );
        }
    }

    /// Two siblings with one order sort by whatever the sort happens to do.
    #[test]
    fn siblings_order_uniquely() {
        for control in APP_CONTROLS {
            let clashes = APP_CONTROLS
                .iter()
                .filter(|other| {
                    other.parent() == control.parent()
                        && other.order == control.order
                        && other.path != control.path
                })
                .count();
            assert_eq!(
                clashes, 0,
                "{} shares its order with a sibling",
                control.path
            );
        }
    }

    /// The projection is the whole point: a control nobody can render is not
    /// declared, it is filed.
    #[test]
    fn the_rendered_tree_carries_every_control_once() {
        let tree = app_config_options();
        let mut seen = Vec::new();
        collect_paths(&tree, &mut seen);
        let declared: BTreeSet<&str> = APP_CONTROLS.iter().map(|c| c.path).collect();
        let rendered: BTreeSet<&str> = seen.iter().map(String::as_str).collect();
        assert_eq!(declared, rendered, "the tree and the table disagree");
        assert_eq!(seen.len(), declared.len(), "a control rendered twice");
    }

    fn collect_paths(node: &Value, out: &mut Vec<String>) {
        if let Some(path) = node.get("path").and_then(Value::as_str) {
            out.push(path.to_string());
        }
        for child in node
            .get("args")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            collect_paths(child, out);
        }
    }
}
