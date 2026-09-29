//! The wire shapes of the `plugin.*` RPCs.
//!
//! `crucible-daemon`'s `server/plugins.rs` and `server/plugin_install.rs`
//! build these types with a struct literal in place of a `json!` object;
//! `crucible-web`'s plugin routes forward them unchanged rather than keeping
//! a second, hand-written copy of each shape. One type, described once,
//! behind the `openapi` feature.
//!
//! Each reply below is its own type with its own required fields, even where
//! two replies share a few. A reply's field that is always present stays
//! required in its type; nothing here reaches for `Option` to paper over two
//! shapes at once.
//!
//! `plugin.list`'s own envelope (`plugins`, `plugin_info`, `errors`, `spec`)
//! stays a `crucible-daemon` type: its `spec` field is
//! `Vec<crucible_core::config::SpecEntry>`-shaped rows tagged with a rank
//! that only the daemon's plugin loader can resolve, and the web's `GET
//! /api/plugins` route sends `plugin_info` alone, under its own envelope. Its
//! item type, [`PluginInfo`], lives here because that item — not the
//! `plugin.list` envelope — is what a browser (and the TUI's plugin table)
//! actually reads.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::types::command_effect::CommandEffect;

/// One discovered plugin, as `plugin.list`'s `plugin_info` rows describe it.
///
/// Every discovered plugin, not only the healthy ones: a plugin that failed to
/// load carries its reason in `last_error`, and dropping it would make
/// "broken" read as "not installed".
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginInfo {
    pub name: String,
    /// `None` when the plugin has no fragment, or its fragment names no
    /// version. The daemon always writes the key, so `required` rather than
    /// optional.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub version: Option<String>,
    /// Where the plugin came from: `User`, `Runtime`, `EnvPath` or `Builtin`.
    /// A plain string, because this writes `Display` output rather than a
    /// serde spelling.
    pub source: String,
    /// The lifecycle state: `Active`, `Error` or `Disabled`. A plain string
    /// for the same reason as `source`.
    pub state: String,
    /// Why the plugin is not `Active`, or `None` for a healthy one. Always
    /// written, so `required`.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub last_error: Option<String>,
    /// The absolute directory the plugin was discovered in.
    pub dir: String,
    pub tools: u64,
    pub commands: u64,
    pub handlers: u64,
    pub services: u64,
}

/// A plugin directory that failed discovery before it became a plugin at
/// all — so it has no [`PluginInfo`] entry to carry its own error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDiscoveryError {
    pub path: String,
    pub error: String,
}

/// What `plugin.list` answers.
///
/// Not `ToSchema`: no web route sends this shape whole — `GET /api/plugins`
/// answers `plugin_info` alone, under its own envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginListReply {
    /// Every discovered plugin's name.
    pub plugins: Vec<String>,
    /// The same plugins, with version, source, state and capability counts.
    pub plugin_info: Vec<PluginInfo>,
    /// Directories that never became plugins at all.
    pub errors: Vec<PluginDiscoveryError>,
    /// The merged spec, in name order. `cru plugin list` reads the git rows
    /// from here: the spec lives on the plugin VM, so no client can evaluate
    /// it on its own.
    pub spec: Vec<crate::protocol::requests::PluginSpecRow>,
}

/// What `plugin.reload` answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginReloadReply {
    /// The plugin that was reloaded, echoed.
    pub name: String,
    pub reloaded: bool,
    pub tools: u64,
    pub commands: u64,
    pub handlers: u64,
    pub services: u64,
}

/// Everything plugins published, keyed `key -> plugin -> value`.
///
/// The two levels are named and the values are not. That is the whole
/// contract of the channel: a plugin states what it offers and a client
/// renders it, so a contribution kind added tomorrow needs no change here.
pub type PluginPublications = BTreeMap<String, BTreeMap<String, serde_json::Value>>;

/// What `plugin.publications` answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginPublicationsReply {
    #[cfg_attr(feature = "openapi", schema(schema_with = publications_schema))]
    pub publications: PluginPublications,
}

/// [`PluginPublications`] for the document: `key -> plugin -> opaque`.
#[cfg(feature = "openapi")]
fn publications_schema() -> utoipa::openapi::schema::Object {
    utoipa::openapi::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::Object)
        .additional_properties(Some(opaque_values()))
        .build()
}

/// A map whose values are whatever a plugin wrote.
///
/// Spelled by hand because `serde_json::Value` cannot be a derived map value:
/// `additionalProperties: true` is the same claim one level down.
#[cfg(feature = "openapi")]
fn opaque_values() -> utoipa::openapi::ObjectBuilder {
    utoipa::openapi::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::Object)
        .additional_properties(Some(
            utoipa::openapi::schema::AdditionalProperties::FreeForm(true),
        ))
}

/// [`opaque_values`] as a finished schema, for a field that is one level deep.
#[cfg(feature = "openapi")]
fn opaque_values_schema() -> utoipa::openapi::schema::Object {
    opaque_values().build()
}

/// What `plugin.options` answers: one settings tree per plugin.
///
/// **A deliberate narrowing to the envelope.** The tree itself stays opaque,
/// and that is not laziness about a shape nobody wrote down — the nodes are
/// fully described in `crucible-lua`'s `describe_node`. It is about numbers:
/// `order`, `min`, `max`, `step` and every entry under `values` reach here as
/// whatever `lua_to_json` made of the plugin's Lua, so reading one into an
/// `f64` and writing it back would send `100.0` where the daemon sent `100`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginOptionsReply {
    #[cfg_attr(feature = "openapi", schema(schema_with = opaque_values_schema))]
    pub options: BTreeMap<String, serde_json::Value>,
}

/// What `plugin.option_get` answers: one option's current value.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginOptionValue {
    /// Whatever the plugin's getter returned, and `null` is an answer.
    ///
    /// Opaque: an option's type belongs to the plugin, and the renderer reads
    /// the value against the node that declared it.
    pub value: serde_json::Value,
}

/// A bare acknowledgement: `plugin.option_set` and `plugin.option_execute`
/// answer nothing else.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginAck {
    pub ok: bool,
}

impl PluginAck {
    pub fn ok() -> Self {
        Self { ok: true }
    }
}

/// What one option call answers: a value for a `get`, an acknowledgement for
/// a `set` or an `execute`.
///
/// Untagged, because one web endpoint (`POST /api/plugins/{name}/option`)
/// serves three RPC methods with three different reply shapes.
///
/// **The variant order is load-bearing.** Serde takes the first variant that
/// fits, so a union whose first variant also fits the second's bodies reads
/// every one of them as the wrong thing. These two are disjoint: neither
/// field has a default, so `{"ok": true}` has no `value` and `{"value": …}`
/// has no `ok`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(untagged)]
pub enum PluginOptionCallReply {
    /// The answer to `action: "get"`.
    Value(PluginOptionValue),
    /// The answer to `action: "set"` and `action: "execute"`.
    Done(PluginAck),
}

/// One executable primitive a plugin declared, and the arguments it takes.
///
/// The daemon builds this from `crucible_core::traits::tools::ToolDefinition`,
/// but sends three of that type's seven fields — `name`, `description` and
/// `parameters` — and adds three the command registry owns: the declaring
/// `plugin`, the `hint`, and the declared `effect`. `category`, `returns`,
/// `examples` and `required_permissions` never reach a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginCommand {
    /// The plugin that declared it.
    pub plugin: String,
    /// Bare when unique; source-qualified when plugins share a name.
    pub name: String,
    pub description: String,
    /// The one-line argument hint, or `None`. Always written, so `required`.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub hint: Option<String>,
    /// The declared parameters, as the JSON Schema `signature.rs` emits, or
    /// `null` for a command that takes none.
    pub parameters: serde_json::Value,
    /// Declared by the plugin and verified by nothing. A consumer must
    /// present it as a claim, and a permission layer must treat it as a hint
    /// about what to ask — never as permission to skip asking.
    pub effect: CommandEffect,
}

/// What `plugin.commands` answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginCommandsReply {
    pub commands: Vec<PluginCommand>,
}

/// What `plugin.run_command` answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginRunCommandReply {
    /// The command that ran, echoed.
    pub name: String,
    /// Whatever the command's Lua `fn` returned.
    ///
    /// Opaque, like publications and options: what a command returns is the
    /// plugin's vocabulary, and a shape validated here would be one only
    /// today's plugins could send.
    pub result: serde_json::Value,
}

/// What one plugin install did, tagged by `kind`.
///
/// The wire projection of `crucible_daemon::daemon_plugins::bootstrap::
/// BootstrapOutcome` — that type stays a plain Rust enum with a `PathBuf`,
/// and this is the one place it is turned into JSON, via
/// `BootstrapOutcome::to_wire`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PluginInstallOutcome {
    /// Cloned, and checked out at the pin if one was given.
    Cloned {
        /// Where the clone landed.
        dest: String,
    },
    /// Already cloned at the expected destination; no work done.
    AlreadyPresent,
    /// Disabled in config; skipped.
    Disabled,
}

/// What `plugin.install` answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginInstallReply {
    /// The installed plugin's name, as the URL resolved it.
    pub name: String,
    /// The clone and the manifest record happened.
    pub installed: bool,
    /// The plugin also activated on the running daemon.
    ///
    /// `installed: true` with `loaded: false` is a plugin that reached the
    /// disk and broke on load; `error` says why, and the next boot tries
    /// again. A client must not read `installed` alone as success.
    pub loaded: bool,
    pub tools: u64,
    pub commands: u64,
    pub services: u64,
    /// Why the plugin did not load, or `None`. Always written, so
    /// `required`.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub error: Option<String>,
    /// A sentence about hot reload. The watcher's list is a boot-time
    /// snapshot, so a plugin installed at runtime works but is not
    /// rewatched.
    pub watch: String,
    pub outcome: PluginInstallOutcome,
    /// The installed manifest the entry was written to
    /// (`plugins.installed.json`).
    pub manifest: String,
}

/// What `plugin.remove` answers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct PluginRemoveReply {
    /// The plugin that was removed, as the manifest named it.
    pub name: String,
    /// The installed manifest the entry left.
    pub manifest: String,
    /// The directory that was deleted, or `None` without `?purge=true`.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub purged_dir: Option<String>,
    /// The manifest entry went but deleting the directory failed.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub purge_error: Option<String>,
    /// Removed without a purge, and the directory is still there. It sits in
    /// a permanent search path, so the next daemon start discovers and loads
    /// it again. `None` when nothing is left behind, which is the only case
    /// where "removed" means gone.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub kept_dir: Option<String>,
}
