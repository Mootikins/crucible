use crate::routes::plugin_caller::PluginCaller;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use crucible_core::protocol::requests::{
    PluginInstallRequest, PluginPublicationsRequest, PluginRunCommandRequest,
};
use crucible_core::protocol::SystemPayload;
use crucible_core::types::{
    PluginCommandsReply, PluginInfo, PluginInstallReply, PluginOptionCallReply, PluginOptionsReply,
    PluginPublicationsReply, PluginReloadReply, PluginRemoveReply, PluginRunCommandReply,
};
use crucible_daemon::server::plugins::OptionAction;
use crucible_daemon::SessionEvent;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

/// The header a caller declares itself in, as the document names it.
const CALLER_HEADER_DOC: &str =
    "Who is asking: `app`, or the plugin being drawn for. A request without it is refused.";

/// The ten plugin endpoints, six of which take a caller identity and four of
/// which do not.
///
/// Gated by [`PluginCaller`]: `POST /api/plugins` (install), `DELETE
/// /api/plugins/{name}`, `POST /api/plugins/{name}/reload` — app only; `POST
/// /api/plugins/{name}/option` and `POST /api/plugins/command` — the app, or
/// the plugin that owns the thing; `GET /api/plugins/publications` — narrowed
/// to the caller's own.
///
/// Ungated, and each for a reason worth knowing before you "finish the job":
/// `GET /api/plugins`, `/commands` and `/options` are enumerations the plugins
/// panel needs and no route rewrites, so gating them buys nothing while a
/// block can call itself `app`. The publication push stream,
/// `GET /api/events/system`, **cannot** be gated this way at all: browsers
/// open it with `EventSource`, which sets no headers. An identity for the push
/// stream needs a different carrier.
/// **No route here serves a plugin's own web assets, and adding one has a
/// precondition.**
///
/// A plugin's JavaScript would run on the app origin, where `script-src
/// 'self'` means what `web/src/pwa-options.ts` says it means, and where a
/// block can already call any endpoint as `app`. The decision on record is a
/// sandboxed iframe on an opaque origin with a `MessageChannel` bridge; the
/// evidence, the measurements and the envelope are in
/// `docs/Meta/Analysis/Plugin Web Delivery.md`.
///
/// The trigger is a third-party plugin that ships web assets. Build the
/// bridge first, then the route.
///
/// This note sits here rather than as a refusal in the install path, and the
/// reason is worth stating: a refusal keyed on a `web/` directory rejects a
/// plugin that keeps unrelated sources under that name, and `cru.rtp` lets a
/// plugin arrive through a runtime root without passing the install path at
/// all. A gate that is wrong in both directions is worse than a line the
/// person adding the route will read.
pub fn plugin_routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_plugins, install_plugin))
        .routes(routes!(remove_plugin))
        .routes(routes!(reload_plugin))
        .routes(routes!(list_publications))
        .routes(routes!(list_commands))
        .routes(routes!(list_options))
        .routes(routes!(option_call))
        .routes(routes!(run_command))
}

/// One read, write, or button press against a plugin's settings tree.
///
/// A single endpoint because the three differ only in which Lua callback they
/// reach — the same reason the daemon handler is one function. `value` is
/// present for a set and ignored otherwise.
#[derive(Debug, Deserialize, ToSchema)]
struct OptionRequest {
    /// Which callback to reach: `get`, `set` or `execute`.
    ///
    /// Described through [`option_action_schema`] rather than through a second
    /// Rust enum here. `crucible-daemon` declares [`OptionAction`] `pub` for
    /// exactly this route and says "one wire shape, one type"; a mirrored copy
    /// beside the handler is what that forbids.
    #[schema(schema_with = option_action_schema)]
    action: OptionAction,
    /// The path through the settings tree to the option.
    path: Vec<String>,
    /// The new value, for a `set`. Ignored otherwise.
    #[serde(default)]
    value: serde_json::Value,
}

/// The wire spelling of every action the option endpoint accepts.
///
/// An exhaustive match, so a variant added to the daemon's [`OptionAction`]
/// fails to compile here rather than reaching the document as an action no
/// client was told about.
fn option_action_spelling(action: OptionAction) -> &'static str {
    match action {
        OptionAction::Get => "get",
        OptionAction::Set => "set",
        OptionAction::Execute => "execute",
    }
}

/// [`OptionAction`] as a closed string schema, built from its own variants.
fn option_action_schema() -> utoipa::openapi::schema::Object {
    utoipa::openapi::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::String)
        .enum_values(Some(
            [OptionAction::Get, OptionAction::Set, OptionAction::Execute]
                .map(option_action_spelling),
        ))
        .build()
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct RemoveQuery {
    /// Also delete the cloned directory. Without it the directory stays in a
    /// permanent search path and loads again on the next daemon start.
    #[serde(default)]
    purge: bool,
}

/// What `GET /api/plugins` answers.
///
/// The daemon's own `plugin.list` reply carries this same data under
/// `plugin_info` (see `crucible_core::types::PluginListReply`); this route
/// renames it to `plugins`, which is the key the plugins panel has always
/// read. The row type is not renamed: [`PluginInfo`] is the core type,
/// forwarded unchanged.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginListResponse {
    pub(crate) plugins: Vec<PluginInfo>,
}

/// `GET /api/plugins` — list discovered plugins with rich metadata
/// (name, version, source, state, dir, capability counts).
#[utoipa::path(
    get,
    path = "/api/plugins",
    responses(
        (status = 200, body = PluginListResponse),
        (status = 502, description = "The daemon could not list the plugins, or answered a shape this route cannot read"),
    )
)]
async fn list_plugins(State(state): State<AppState>) -> Result<Json<PluginListResponse>, WebError> {
    let plugins = state.daemon.plugin_list_info().await.daemon_err()?;
    Ok(Json(PluginListResponse { plugins }))
}

/// Everything plugins published, keyed `key -> plugin -> value`.
///
/// The two levels are named and the values are not. That is the whole contract
/// of the channel: a plugin states what it offers and a client renders it, so a
/// contribution kind added tomorrow needs no change here. The daemon builds the
/// same two-level map (`crucible_lua::PublicationRegistry::all`), which is what
/// makes naming the envelope safe.
/// The daemon's own `key -> plugin -> value` map (see
/// `crucible_core::types::PluginPublications`), kept as an alias here so the
/// rest of this file reads the same as it did before the type moved.
pub(crate) type PublicationsByKey = crucible_core::types::PluginPublications;

/// `GET /api/plugins/publications` — what plugins published about themselves.
///
/// The two levels are read; the values are passed through verbatim. Nothing
/// here interprets one, which is the point: the frontend used to learn what
/// isolation a box offered by having the server match on the shape of the `oci`
/// plugin's config, so one plugin's schema lived in the rendering layer and a
/// second isolating plugin would not have appeared at all.
#[utoipa::path(
    get,
    path = "/api/plugins/publications",
    params(
        PluginPublicationsRequest,
        ("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC),
    ),
    responses(
        (status = 200, body = PluginPublicationsReply),
        (status = 403, description = "No caller identity was sent"),
        (status = 502, description = "The daemon could not read the publications, or answered a shape this route cannot read"),
    )
)]
async fn list_publications(
    State(state): State<AppState>,
    caller: PluginCaller,
    Query(q): Query<PluginPublicationsRequest>,
) -> Result<Json<PluginPublicationsReply>, WebError> {
    let publications = state.daemon.plugin_publications(q.key).await.daemon_err()?;
    Ok(Json(PluginPublicationsReply {
        publications: narrow_to_caller(publications, &caller),
    }))
}

/// Keep only what the caller may see: everything for the app, a plugin's own
/// rows for a plugin.
///
/// Narrowing rather than refusing, because `?key=` is a courtesy and this is
/// the boundary the courtesy stands in for. The answer is keyed `key -> plugin
/// -> value`, so a plugin's own rows are the inner entries under its name; a
/// key left with nothing under it is dropped rather than answered as empty.
///
/// `PluginBlockPanel` reads every plugin's publications with no key and is the
/// app, so it passes through here untouched. Narrow the app too and that panel
/// goes blank.
fn narrow_to_caller(publications: PublicationsByKey, caller: &PluginCaller) -> PublicationsByKey {
    let PluginCaller::Plugin(plugin) = caller else {
        return publications;
    };
    publications
        .into_iter()
        .filter_map(|(key, mut by_plugin)| {
            let value = by_plugin.remove(plugin.as_str())?;
            Some((key, BTreeMap::from([(plugin.clone(), value)])))
        })
        .collect()
}

/// `GET /api/plugins/commands` — the executable primitives plugins declared.
///
/// Each entry carries `plugin`, `name`, `description`, `hint`, `parameters`
/// and `effect`. The names are read, so the browser is told what a row holds;
/// `parameters` is passed through verbatim, so a caller offering a command as
/// a button — with a dialog built from the declared parameters — needs no
/// change on this side when a plugin ships a new one.
///
/// `parameters` crosses as opaque JSON today. It comes from the same
/// `ToolDefinition` a tool uses, so shaping it like `signature.rs`'s JSON
/// Schema output is what would let a dialog be generated rather than
/// hand-read — see `docs/Meta/Analysis/The Plugin Contract.md`.
///
/// The row type is `crucible_core::types::PluginCommand`, forwarded
/// unchanged: its `effect` field is `crucible_core::types::CommandEffect`,
/// the same closed vocabulary `crucible-lua` re-exports rather than mirrors.
#[utoipa::path(
    get,
    path = "/api/plugins/commands",
    // Not the default `list_commands`: `GET /api/commands` takes that name, and
    // an operation id must be unique in the document. Two operations sharing
    // one id give the generated TypeScript ONE of the two shapes for both
    // routes, so this route's reply was typed as the slash-command list.
    operation_id = "list_plugin_commands",
    responses(
        (status = 200, body = PluginCommandsReply),
        (status = 502, description = "The daemon could not list the commands, or answered a shape this route cannot read"),
    )
)]
async fn list_commands(
    State(state): State<AppState>,
) -> Result<Json<PluginCommandsReply>, WebError> {
    let commands = state.daemon.plugin_commands().await.daemon_err()?;
    Ok(Json(PluginCommandsReply { commands }))
}

/// `GET /api/plugins/options` — the settings trees plugins declared.
///
/// Keyed by plugin, and each tree is passed through verbatim. Nothing here knows what any
/// option means: a node is `{type, name, desc, order, …}` and the renderer
/// draws it from that, so a plugin shipped tomorrow gets a settings pane with
/// no change on this side. Re-read rather than cached — a tree's
/// function-valued fields describe the box as it is now.
#[utoipa::path(
    get,
    path = "/api/plugins/options",
    responses(
        (status = 200, body = PluginOptionsReply),
        (status = 502, description = "The daemon could not read the settings trees, or answered a shape this route cannot read"),
    )
)]
async fn list_options(State(state): State<AppState>) -> Result<Json<PluginOptionsReply>, WebError> {
    let options = state.daemon.plugin_options().await.daemon_err()?;
    Ok(Json(PluginOptionsReply { options }))
}

/// `POST /api/plugins/:name/option` — read, write, or press one option.
///
/// The reply is `crucible_core::types::PluginOptionCallReply`: a value for a
/// `get`, an acknowledgement for a `set` or an `execute`. See that type's
/// docs for why its variant order is load-bearing.
#[utoipa::path(
    post,
    path = "/api/plugins/{name}/option",
    params(
        ("name" = String, Path, description = "The plugin whose settings tree is read or written"),
        ("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC),
    ),
    request_body = OptionRequest,
    responses(
        (status = 200, body = PluginOptionCallReply),
        (status = 403, description = "No caller identity was sent, or the caller draws for another plugin"),
        (status = 422, description = "`path` names no option, or the plugin refused the value"),
        (status = 502, description = "The daemon could not reach the plugin"),
    )
)]
async fn option_call(
    State(state): State<AppState>,
    Path(name): Path<String>,
    caller: PluginCaller,
    Json(req): Json<OptionRequest>,
) -> Result<Json<PluginOptionCallReply>, WebError> {
    // `{name}` is caller-supplied, so without this a block reads and writes
    // any plugin's settings tree by naming it.
    caller.require_speaks_for(&name, "read or write the settings")?;
    if req.path.is_empty() {
        return Err(WebError::Validation(
            "`path` must name an option".to_string(),
        ));
    }
    match req.action {
        OptionAction::Get => {
            let value = state
                .daemon
                .plugin_option_get(&name, req.path)
                .await
                .daemon_err()?;
            Ok(Json(PluginOptionCallReply::Value(
                crucible_core::types::PluginOptionValue { value },
            )))
        }
        OptionAction::Set => {
            state
                .daemon
                .plugin_option_set(&name, req.path, req.value)
                .await
                .daemon_err()?;
            Ok(Json(PluginOptionCallReply::Done(
                crucible_core::types::PluginAck::ok(),
            )))
        }
        OptionAction::Execute => {
            state
                .daemon
                .plugin_option_execute(&name, req.path)
                .await
                .daemon_err()?;
            Ok(Json(PluginOptionCallReply::Done(
                crucible_core::types::PluginAck::ok(),
            )))
        }
    }
}

/// A plugin's published data changed, delivered to the browser.
///
/// Carries who published and under which key, never the value — the same
/// contract the daemon event has, and for the same reason: a publication is
/// opaque JSON of the plugin's own choosing. The browser refetches through
/// `GET /api/plugins/publications`.
///
/// Projected through a named type rather than passed through as raw `data`,
/// which is the treatment `SurfaceChangedEvent` already gets: a field the
/// daemon renames then breaks the browser with nothing on this side to notice.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct PublicationChangedEvent {
    pub plugin: String,
    pub key: String,
}

impl PublicationChangedEvent {
    /// The SSE `event:` name the browser listens for.
    ///
    /// Read from the daemon's own payload, never written again here: this file
    /// once held a literal of its own, and the name it had to match lived two
    /// crates away with nothing comparing them.
    pub const EVENT_NAME: &'static str = SystemPayload::PUBLICATION_CHANGED;

    /// Project a daemon event into this shape, or `None` for anything else.
    ///
    /// `key` is required: an event that cannot say which key changed would make
    /// every block refetch every publication, which is worse than missing it.
    pub fn from_daemon_event(ev: &SessionEvent) -> Option<Self> {
        if ev.event != Self::EVENT_NAME {
            return None;
        }
        Some(Self {
            plugin: ev.data["plugin"].as_str().unwrap_or_default().to_string(),
            key: ev.data["key"].as_str()?.to_string(),
        })
    }
}

/// `POST /api/plugins/command` — invoke a plugin command by name.
///
/// Not under `/{name}` because the name sent here resolves through the daemon's
/// command registry. The result is passed
/// through verbatim, like publications and options: what a command returns is
/// the plugin's vocabulary, and a shape this layer validated would be a shape
/// only today's plugins could send.
///
/// The reply is `crucible_core::types::PluginRunCommandReply`, forwarded
/// unchanged.
#[utoipa::path(
    post,
    path = "/api/plugins/command",
    params(("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC)),
    request_body = PluginRunCommandRequest,
    responses(
        (status = 200, body = PluginRunCommandReply),
        (status = 403, description = "No caller identity was sent, or the command belongs to another plugin"),
        (status = 422, description = "`name` names no command"),
        (status = 502, description = "The daemon could not run the command, or answered a shape this route cannot read"),
    )
)]
async fn run_command(
    State(state): State<AppState>,
    caller: PluginCaller,
    Json(req): Json<PluginRunCommandRequest>,
) -> Result<Json<PluginRunCommandReply>, WebError> {
    if req.name.trim().is_empty() {
        return Err(WebError::Validation(
            "`name` must name a command".to_string(),
        ));
    }
    refuse_another_plugins_command(&state, &caller, &req.name).await?;
    let result = state
        .daemon
        .plugin_run_command(&req.name, req.args, req.session_id.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(result))
}

/// A caller drawing for plugin X may invoke only X's commands.
///
/// The owning plugin is already on every entry `plugin.commands` returns, so
/// this is a comparison rather than a second registry — but it costs one extra
/// daemon round trip per invocation, which is why the app skips it.
///
/// A command nobody owns is refused too. An unknown name from a plugin caller
/// cannot be attributed, and "cannot attribute" is the same answer as "not
/// yours" — the alternative leaks which command names exist by their error.
async fn refuse_another_plugins_command(
    state: &AppState,
    caller: &PluginCaller,
    command: &str,
) -> Result<(), WebError> {
    let PluginCaller::Plugin(plugin) = caller else {
        return Ok(());
    };
    let commands = state.daemon.plugin_commands().await.daemon_err()?;
    let owner = commands.iter().find_map(|c| {
        let listed = &c.name;
        let owner = &c.plugin;
        if listed == command || (!listed.contains(':') && format!("{owner}:{listed}") == command) {
            Some(owner)
        } else {
            None
        }
    });

    match owner {
        Some(owner) if owner == plugin => Ok(()),
        _ => Err(WebError::Forbidden(format!(
            "`{command}` is not a command of plugin `{plugin}`"
        ))),
    }
}

/// `POST /api/plugins/:name/reload` — reload a plugin by name.
///
/// The reply is `crucible_core::types::PluginReloadReply` (counts of tools,
/// commands, etc.), forwarded unchanged.
#[utoipa::path(
    post,
    path = "/api/plugins/{name}/reload",
    params(
        ("name" = String, Path, description = "The plugin to reload"),
        ("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC),
    ),
    responses(
        (status = 200, body = PluginReloadReply),
        (status = 403, description = "The caller is not the app"),
        (status = 502, description = "The reload failed, or the daemon answered a shape this route cannot read"),
    )
)]
async fn reload_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
    caller: PluginCaller,
) -> Result<Json<PluginReloadReply>, WebError> {
    caller.require_app("reload a plugin")?;
    let result = state.daemon.plugin_reload(&name).await.daemon_err()?;
    Ok(Json(result))
}

/// `POST /api/plugins` — clone a plugin from a git URL and record it in
/// the installed manifest (`plugins.installed.json`), the same record
/// `cru plugin add` writes. The operator's own spec entries live in
/// `init.lua`, which nothing here edits. Synchronous; can take 10+ seconds.
///
/// The reply is `crucible_core::types::PluginInstallReply`, forwarded
/// unchanged. Its `outcome` field is `crucible_core::types::
/// PluginInstallOutcome`, the wire projection of
/// `crucible_daemon::BootstrapOutcome` (`BootstrapOutcome::to_wire`) — the
/// type `server/plugin_install.rs` matches on to decide it.
#[utoipa::path(
    post,
    path = "/api/plugins",
    params(("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC)),
    request_body = PluginInstallRequest,
    responses(
        (status = 200, body = PluginInstallReply),
        (status = 403, description = "The caller is not the app"),
        (status = 422, description = "The URL is empty, or the plugin is already declared in init.lua"),
        (status = 502, description = "The clone failed, or the daemon answered a shape this route cannot read"),
    )
)]
async fn install_plugin(
    State(state): State<AppState>,
    caller: PluginCaller,
    Json(req): Json<PluginInstallRequest>,
) -> Result<Json<PluginInstallReply>, WebError> {
    // The largest of the plugin routes: it clones code from a URL the caller
    // chose and loads it. Nothing a block draws needs this.
    caller.require_app("install a plugin")?;
    if req.url.trim().is_empty() {
        return Err(WebError::Validation("plugin URL must not be empty".into()));
    }
    let result = state
        .daemon
        .plugin_install(&req.url, req.branch.as_deref(), req.pin.as_deref())
        .await
        .daemon_err()?;
    Ok(Json(result))
}

/// `DELETE /api/plugins/:name?purge=true` — remove a plugin declaration.
///
/// The reply is `crucible_core::types::PluginRemoveReply`, forwarded
/// unchanged.
#[utoipa::path(
    delete,
    path = "/api/plugins/{name}",
    params(
        ("name" = String, Path, description = "The plugin to remove"),
        RemoveQuery,
        ("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC),
    ),
    responses(
        (status = 200, body = PluginRemoveReply),
        (status = 403, description = "The caller is not the app"),
        (status = 422, description = "The plugin is declared in init.lua, or is not in the installed manifest"),
        (status = 502, description = "The removal failed, or the daemon answered a shape this route cannot read"),
    )
)]
async fn remove_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
    caller: PluginCaller,
    Query(query): Query<RemoveQuery>,
) -> Result<Json<PluginRemoveReply>, WebError> {
    caller.require_app("remove a plugin")?;
    let result = state
        .daemon
        .plugin_remove(&name, query.purge)
        .await
        .daemon_err()?;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::plugin_caller::{APP_CALLER, PLUGIN_CALLER_HEADER};
    use crate::test_support::{shape_as, survives};
    use crucible_core::types::{CommandEffect, PluginInstallOutcome};
    use crucible_daemon::BootstrapOutcome;
    use crucible_lua::PublicationRegistry;

    #[test]
    fn test_plugin_routes_builds() {
        let _router = plugin_routes();
    }

    // =====================================================================
    // Every handler answers the shape it declares
    // =====================================================================

    /// The app's own identity, which six of these routes refuse to act without.
    fn as_app() -> Vec<(&'static str, String)> {
        vec![(PLUGIN_CALLER_HEADER, APP_CALLER.to_string())]
    }

    #[tokio::test]
    async fn list_plugins_answers_the_declared_shape() {
        let listing: PluginListResponse = shape_as("GET", "/api/plugins", None, Vec::new()).await;

        let plugin = &listing.plugins[0];
        assert_eq!(plugin.name, "mock-plugin");
        assert_eq!(plugin.version.as_deref(), Some("0.1.0"));
        // Written for every row, so a broken plugin stays distinguishable from
        // one that is not installed.
        assert_eq!(plugin.last_error, None);
        assert_eq!(plugin.tools, 3);
    }

    #[tokio::test]
    async fn list_publications_answers_the_declared_shape() {
        let published: PluginPublicationsReply =
            shape_as("GET", "/api/plugins/publications", None, as_app()).await;

        // The app sees every plugin's rows: narrowing it too is what would
        // blank the plugins panel.
        assert_eq!(published.publications.len(), 2);
        assert_eq!(
            published.publications["everything"]["mock-plugin"],
            serde_json::json!({ "narrowed": false })
        );
    }

    #[tokio::test]
    async fn list_commands_answers_the_declared_shape() {
        let commands: PluginCommandsReply =
            shape_as("GET", "/api/plugins/commands", None, Vec::new()).await;

        let command = &commands.commands[0];
        assert_eq!(command.name, "mock_command");
        assert_eq!(command.hint.as_deref(), Some("<arg>"));
        // A claim the plugin makes, never a permission to skip asking.
        assert_eq!(command.effect, CommandEffect::Write);
        assert!(command.parameters.is_array(), "{}", command.parameters);
    }

    #[tokio::test]
    async fn list_options_answers_the_declared_shape() {
        let options: PluginOptionsReply =
            shape_as("GET", "/api/plugins/options", None, Vec::new()).await;

        // The tree is opaque by design, so the assertion is about the envelope
        // and about the tree arriving untouched.
        let tree = &options.options["mock-plugin"];
        assert_eq!(tree["type"], "group");
        assert_eq!(tree["args"][0]["key"], "image");
    }

    #[tokio::test]
    async fn option_call_answers_the_declared_shape() {
        let read: PluginOptionCallReply = shape_as(
            "POST",
            "/api/plugins/mock-plugin/option",
            Some(serde_json::json!({ "action": "get", "path": ["image"] })),
            as_app(),
        )
        .await;
        let PluginOptionCallReply::Value(read) = read else {
            panic!("a get answers a value");
        };
        assert_eq!(read.value, serde_json::json!("alpine"));

        for action in ["set", "execute"] {
            let done: PluginOptionCallReply = shape_as(
                "POST",
                "/api/plugins/mock-plugin/option",
                Some(serde_json::json!({ "action": action, "path": ["image"], "value": "debian" })),
                as_app(),
            )
            .await;
            assert!(
                matches!(done, PluginOptionCallReply::Done(ref ack) if ack.ok),
                "a {action} answers an acknowledgement, not {done:?}"
            );
        }
    }

    #[tokio::test]
    async fn run_command_answers_the_declared_shape() {
        let ran: PluginRunCommandReply = shape_as(
            "POST",
            "/api/plugins/command",
            Some(serde_json::json!({ "name": "mock_command", "args": {} })),
            as_app(),
        )
        .await;

        assert_eq!(ran.name, "mock_command");
        // The browser's hand-written caller types this reply as `unknown`, so
        // nothing told a reader the answer sits under `result`.
        assert_eq!(ran.result["branches"][0], "main");
    }

    #[tokio::test]
    async fn plugin_caller_can_use_full_name_of_unique_command() {
        let ran: PluginRunCommandReply = shape_as(
            "POST",
            "/api/plugins/command",
            Some(serde_json::json!({ "name": "mock-plugin:mock_command", "args": {} })),
            vec![(PLUGIN_CALLER_HEADER, "mock-plugin".to_string())],
        )
        .await;
        assert_eq!(ran.name, "mock-plugin:mock_command");
    }

    #[tokio::test]
    async fn reload_plugin_answers_the_declared_shape() {
        let reloaded: PluginReloadReply =
            shape_as("POST", "/api/plugins/mock-plugin/reload", None, as_app()).await;

        assert_eq!(reloaded.name, "mock-plugin");
        assert!(reloaded.reloaded);
        assert_eq!(reloaded.handlers, 2);
    }

    #[tokio::test]
    async fn install_plugin_answers_the_declared_shape() {
        let installed: PluginInstallReply = shape_as(
            "POST",
            "/api/plugins",
            Some(serde_json::json!({ "url": "user/repo" })),
            as_app(),
        )
        .await;

        assert_eq!(installed.name, "installed-plugin");
        assert!(installed.installed && installed.loaded);
        assert_eq!(installed.error, None);
        assert!(matches!(
            installed.outcome,
            PluginInstallOutcome::Cloned { .. }
        ));
        // The daemon writes the record it used. The browser still reads a
        // `plugins_toml` key that stopped existing when the record left TOML.
        assert_eq!(installed.manifest, "/tmp/plugins.installed.json");
    }

    #[tokio::test]
    async fn remove_plugin_answers_the_declared_shape() {
        let removed: PluginRemoveReply = shape_as(
            "DELETE",
            "/api/plugins/removed-plugin?purge=true",
            None,
            as_app(),
        )
        .await;

        assert_eq!(removed.name, "removed-plugin");
        assert_eq!(removed.purged_dir, None);
        // Nothing left behind, which is the only case where "removed" means
        // gone for good.
        assert_eq!(removed.kept_dir, None);
    }

    // =====================================================================
    // The replies write back the objects the daemon sent
    // =====================================================================

    /// Everything a plugin published survives [`PluginPublicationsReply`].
    ///
    /// Built from `crucible_lua::PublicationRegistry`, which is the type
    /// `handle_plugin_publications` serialises, so the two-level map this route
    /// now names is the one the daemon actually writes. The values stay opaque,
    /// and the test proves exactly that: a string, a number, a nested object
    /// and a null all come back as they went in.
    #[test]
    fn a_publication_writes_back_the_object_plugin_publications_sent() {
        let registry = PublicationRegistry::new();
        registry.set("oci", "targets", serde_json::json!({ "axis": "runtime" }));
        registry.set("oci", "version", serde_json::json!(3));
        registry.set("kanban", "targets", serde_json::json!("opaque"));
        registry.set("kanban", "board", serde_json::Value::Null);

        survives::<PublicationsByKey>(&registry.all());
    }

    /// Narrowing keeps a plugin's own rows byte-for-byte.
    ///
    /// A key left with nothing under it is dropped rather than answered as
    /// empty, so a block cannot tell "no answer" from "somebody else's answer".
    #[test]
    fn narrowing_keeps_only_the_callers_own_rows() {
        let registry = PublicationRegistry::new();
        registry.set("oci", "targets", serde_json::json!({ "axis": "runtime" }));
        registry.set("kanban", "targets", serde_json::json!("opaque"));
        registry.set("kanban", "board", serde_json::json!([1, 2]));

        let all: PublicationsByKey =
            serde_json::from_value(serde_json::to_value(registry.all()).unwrap()).unwrap();

        let mine = narrow_to_caller(all.clone(), &PluginCaller::Plugin("oci".to_string()));
        assert_eq!(
            serde_json::to_value(&mine).unwrap(),
            serde_json::json!({ "targets": { "oci": { "axis": "runtime" } } }),
            "`board` has nothing of oci's under it, so the key goes too"
        );

        // The app is not narrowed at all.
        assert_eq!(narrow_to_caller(all.clone(), &PluginCaller::App), all);
    }

    /// The daemon's `BootstrapOutcome` is what `server/plugin_install.rs`
    /// matches on to write the `outcome` object, so every variant it can take
    /// is read back here. `outcome_object` is exhaustive: a fourth variant
    /// fails to compile rather than reaching the browser as a `kind` nothing
    /// draws.
    #[test]
    fn every_install_outcome_writes_back_what_plugin_install_sent() {
        for outcome in [
            BootstrapOutcome::Cloned {
                dest: std::path::PathBuf::from("/tmp/installed-plugin"),
            },
            BootstrapOutcome::AlreadyPresent,
            BootstrapOutcome::Disabled,
        ] {
            let sent = serde_json::json!({
                "name": "installed-plugin",
                "installed": true,
                "loaded": false,
                "tools": 0,
                "commands": 0,
                "services": 0,
                "error": "the plugin's setup() failed",
                "watch": "not hot-watched until restart",
                "outcome": outcome_object(&outcome),
                "manifest": "/tmp/plugins.installed.json",
            });

            survives::<PluginInstallReply>(&sent);
        }
    }

    /// The `outcome` object for each bootstrap result, spelled exactly as
    /// `handle_plugin_install` spells it. No wildcard arm, ever.
    fn outcome_object(outcome: &BootstrapOutcome) -> serde_json::Value {
        match outcome {
            BootstrapOutcome::Cloned { dest } => serde_json::json!({
                "kind": "cloned",
                "dest": dest.to_string_lossy(),
            }),
            BootstrapOutcome::AlreadyPresent => serde_json::json!({ "kind": "already_present" }),
            BootstrapOutcome::Disabled => serde_json::json!({ "kind": "disabled" }),
        }
    }

    /// **The untagged union's variant order is load-bearing.**
    ///
    /// `ResumeSessionResponse` listed a variant whose required fields were a
    /// subset of another's, so serde took the first that fit and every reply of
    /// the second kind read back as the wrong thing, with no error. These two
    /// are disjoint because neither field has a default. This test asserts both
    /// directions, so adding a `#[serde(default)]` fails here rather than on a
    /// settings pane.
    #[test]
    fn an_option_reply_reads_back_as_the_action_that_sent_it() {
        for value in [
            serde_json::json!("alpine"),
            serde_json::json!(null),
            serde_json::json!({ "nested": true }),
        ] {
            let wire = serde_json::json!({ "value": value });
            let read: PluginOptionCallReply = serde_json::from_value(wire.clone())
                .unwrap_or_else(|e| panic!("a get's reply must read back as a value: {e}"));
            assert!(
                matches!(read, PluginOptionCallReply::Value(_)),
                "`{wire}` read back as an acknowledgement"
            );
            assert_eq!(serde_json::to_value(read).unwrap(), wire);
        }

        let wire = serde_json::json!({ "ok": true });
        let read: PluginOptionCallReply = serde_json::from_value(wire.clone())
            .unwrap_or_else(|e| panic!("a set's reply must read back as an acknowledgement: {e}"));
        assert!(
            matches!(read, PluginOptionCallReply::Done(_)),
            "`{wire}` read back as a value"
        );
        assert_eq!(serde_json::to_value(read).unwrap(), wire);
    }

    // =====================================================================
    // The declared vocabularies stay closed
    // =====================================================================

    /// The wire spelling of every effect a command can declare.
    ///
    /// An exhaustive match, so a variant added to `CommandEffect` fails to
    /// compile here rather than reaching a client as a word this test never
    /// checked.
    fn effect_spelling(effect: CommandEffect) -> &'static str {
        match effect {
            CommandEffect::Read => "read",
            CommandEffect::Write => "write",
        }
    }

    #[test]
    fn the_declared_vocabularies_stay_closed() {
        for effect in [CommandEffect::Read, CommandEffect::Write] {
            let spelling = effect_spelling(effect);
            assert_eq!(effect.as_str(), spelling, "the daemon's spelling moved");
            serde_json::from_value::<CommandEffect>(serde_json::json!(spelling))
                .unwrap_or_else(|e| panic!("`CommandEffect` cannot read `{spelling}`: {e}"));
        }

        // The request side of the same rule: the document's three actions are
        // built from the daemon's own enum, so the browser is offered exactly
        // what `OptionRequest` can read.
        for action in [OptionAction::Get, OptionAction::Set, OptionAction::Execute] {
            let spelling = option_action_spelling(action);
            let request: OptionRequest = serde_json::from_value(serde_json::json!({
                "action": spelling,
                "path": ["image"],
            }))
            .unwrap_or_else(|e| panic!("`OptionRequest` cannot read `{spelling}`: {e}"));
            assert_eq!(request.action, action);
        }
    }

    /// The browser listens for `publication_changed` and reads `plugin` and
    /// `key` off the frame (`web/src/components/blocks/usePublication.ts`).
    /// This is the whole contract, and it crosses a language boundary, so it
    /// gets a test on this side of it.
    #[test]
    fn a_publication_frame_keeps_the_name_and_the_fields() {
        let ev = SessionEvent::new(
            "system",
            SystemPayload::PUBLICATION_CHANGED,
            serde_json::json!({ "plugin": "kanban", "key": "kanban:board" }),
        );
        let projected = PublicationChangedEvent::from_daemon_event(&ev).expect("projects");
        assert_eq!(projected.plugin, "kanban");
        assert_eq!(projected.key, "kanban:board");
        assert_eq!(
            serde_json::to_value(&projected).unwrap(),
            serde_json::json!({ "plugin": "kanban", "key": "kanban:board" }),
            "the browser reads these two names and no others"
        );
    }

    /// The system channel also carries the file watcher and the classification
    /// prompt. A stream that forwarded those would wake every plugin block.
    #[test]
    fn another_system_event_is_not_a_publication() {
        let ev = SessionEvent::new(
            "system",
            "file_changed",
            serde_json::json!({ "path": "/w/a.md" }),
        );
        assert!(PublicationChangedEvent::from_daemon_event(&ev).is_none());
    }

    /// A frame that cannot say WHICH key changed would make every block refetch
    /// every publication, which is worse than dropping it.
    #[test]
    fn a_publication_frame_without_a_key_is_dropped() {
        let ev = SessionEvent::new(
            "system",
            SystemPayload::PUBLICATION_CHANGED,
            serde_json::json!({ "plugin": "kanban" }),
        );
        assert!(PublicationChangedEvent::from_daemon_event(&ev).is_none());
    }
}
