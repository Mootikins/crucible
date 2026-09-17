use crate::routes::helpers::{stream_version_frame, versioned};
use crate::routes::plugin_caller::PluginCaller;
use crate::services::daemon::AppState;
use crate::{error::WebResultExt, WebError};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{
    extract::{Path, Query, State},
    Json,
};
use crucible_core::protocol::SystemPayload;
use crucible_daemon::server::plugins::OptionAction;
use crucible_daemon::SessionEvent;
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::convert::Infallible;
use tokio_stream::StreamExt;
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::{router::OpenApiRouter, routes};

use super::session::{daemon_shape, OkResponse};

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
/// block can call itself `app`. `GET /api/plugins/events` **cannot** be gated
/// this way at all: browsers open it with `EventSource`, which sets no
/// headers. An identity for the push stream needs a different carrier.
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
        .routes(routes!(publication_event_stream))
        .routes(routes!(run_command))
}

/// One plugin command invocation.
#[derive(Debug, Deserialize, ToSchema)]
struct CommandRequest {
    name: String,
    #[serde(default)]
    args: serde_json::Value,
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

#[derive(Debug, Deserialize, ToSchema)]
struct InstallRequest {
    /// Plugin URL (e.g. "user/repo" or full git URL).
    url: String,
    branch: Option<String>,
    pin: Option<String>,
}

#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct RemoveQuery {
    /// Also delete the cloned directory. Without it the directory stays in a
    /// permanent search path and loads again on the next daemon start.
    #[serde(default)]
    purge: bool,
}

/// One discovered plugin, as `plugin.list`'s `plugin_info` rows describe it.
///
/// Every discovered plugin, not only the healthy ones: a plugin that failed to
/// load carries its reason in `last_error`, and dropping it would make "broken"
/// read as "not installed".
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginRow {
    pub(crate) name: String,
    /// `null` when the plugin has no fragment, or its fragment names no
    /// version. The daemon always writes the key, so `required` rather than
    /// optional.
    #[schema(required = true)]
    pub(crate) version: Option<String>,
    /// Where the plugin came from: `User`, `Runtime`, `EnvPath` or `Builtin`.
    /// A plain string, because the daemon writes `Display` output rather than
    /// a serde spelling.
    pub(crate) source: String,
    /// The lifecycle state: `Active`, `Error` or `Disabled`. A plain string
    /// for the same reason as `source`.
    pub(crate) state: String,
    /// Why the plugin is not `Active`, or `null` for a healthy one. Always
    /// written, so `required`.
    #[schema(required = true)]
    pub(crate) last_error: Option<String>,
    /// The absolute directory the plugin was discovered in.
    pub(crate) dir: String,
    pub(crate) tools: u64,
    pub(crate) commands: u64,
    pub(crate) handlers: u64,
    pub(crate) services: u64,
}

/// What `GET /api/plugins` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginListResponse {
    pub(crate) plugins: Vec<PluginRow>,
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
    let info = state.daemon.plugin_list_info().await.daemon_err()?;
    Ok(Json(PluginListResponse {
        plugins: daemon_shape(serde_json::Value::Array(info), "plugin.list")?,
    }))
}

/// Everything plugins published, keyed `key -> plugin -> value`.
///
/// The two levels are named and the values are not. That is the whole contract
/// of the channel: a plugin states what it offers and a client renders it, so a
/// contribution kind added tomorrow needs no change here. The daemon builds the
/// same two-level map (`crucible_lua::PublicationRegistry::all`), which is what
/// makes naming the envelope safe.
pub(crate) type PublicationsByKey = BTreeMap<String, BTreeMap<String, serde_json::Value>>;

/// What `GET /api/plugins/publications` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginPublicationsResponse {
    #[schema(schema_with = publications_schema)]
    pub(crate) publications: PublicationsByKey,
}

/// A map whose values are whatever a plugin wrote.
///
/// Spelled by hand because `serde_json::Value` cannot be a derived map value:
/// `ToSchema` covers it on its own, which is how a bare `Value` field reaches
/// the browser as `unknown`, but the derive needs more than that inside a
/// `BTreeMap`. `additionalProperties: true` is the same claim, one level down.
fn opaque_values() -> utoipa::openapi::ObjectBuilder {
    utoipa::openapi::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::Object)
        .additional_properties(Some(
            utoipa::openapi::schema::AdditionalProperties::FreeForm(true),
        ))
}

/// [`opaque_values`] as a finished schema, for a field that is one level deep.
fn opaque_values_schema() -> utoipa::openapi::schema::Object {
    opaque_values().build()
}

/// [`PublicationsByKey`] for the document: `key -> plugin -> opaque`.
fn publications_schema() -> utoipa::openapi::schema::Object {
    utoipa::openapi::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::Object)
        .additional_properties(Some(opaque_values()))
        .build()
}

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
        PublicationsQuery,
        ("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC),
    ),
    responses(
        (status = 200, body = PluginPublicationsResponse),
        (status = 403, description = "No caller identity was sent"),
        (status = 502, description = "The daemon could not read the publications, or answered a shape this route cannot read"),
    )
)]
async fn list_publications(
    State(state): State<AppState>,
    caller: PluginCaller,
    Query(q): Query<PublicationsQuery>,
) -> Result<Json<PluginPublicationsResponse>, WebError> {
    let publications = state.daemon.plugin_publications(q.key).await.daemon_err()?;
    let publications = daemon_shape(publications, "plugin.publications")?;
    Ok(Json(PluginPublicationsResponse {
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

/// Whether running a command changes state a user could lose, mirroring
/// [`crucible_lua::CommandEffect`].
///
/// **Declared by the plugin and verified by nothing.** A consumer must present
/// it as a claim, and a permission layer must treat it as a hint about what to
/// ask — never as permission to skip asking.
///
/// A closed set, not the open string a passthrough would give, because the
/// daemon writes `CommandEffect::as_str()` and nothing else: a command that
/// declares no effect arrives as `write`, never as an unknown word.
/// `the_mirrored_vocabularies_stay_closed` holds the two lists together.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CommandEffectRow {
    /// Changes nothing a user could lose. May compute, cache and publish.
    Read,
    /// Changes something a user could lose: a note, a file, a setting.
    Write,
}

/// One executable primitive a plugin declared, and the arguments it takes.
///
/// **A deliberate reshape, so it keeps its own struct.** The daemon builds a
/// row from `crucible_core::traits::tools::ToolDefinition`, but it sends three
/// of that type's seven fields — `name`, `description` and `parameters` — and
/// adds three the command registry owns: the declaring `plugin`, the
/// `hint`, and the declared `effect`. `category`, `returns`, `examples` and
/// `required_permissions` never reach a client. Re-exporting `ToolDefinition`
/// would therefore promise four keys the wire does not carry, which is why
/// this is not one of the types to derive `ToSchema` on in `crucible-core`.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginCommandRow {
    /// The plugin that declared it.
    pub(crate) plugin: String,
    /// Globally unique: the daemon refuses a second plugin claiming a name,
    /// which is why `POST /api/plugins/command` does not route by plugin.
    pub(crate) name: String,
    pub(crate) description: String,
    /// The one-line argument hint, or `null`. Always written, so `required`.
    #[schema(required = true)]
    pub(crate) hint: Option<String>,
    /// The declared parameters, as the JSON Schema `signature.rs` emits, or
    /// `null` for a command that takes none.
    ///
    /// Opaque on purpose, like a publication: the reader turns it into the
    /// controls a dialog draws, and a shape validated here would be one only
    /// today's plugins could send.
    pub(crate) parameters: serde_json::Value,
    pub(crate) effect: CommandEffectRow,
}

/// What `GET /api/plugins/commands` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginCommandsResponse {
    pub(crate) commands: Vec<PluginCommandRow>,
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
#[utoipa::path(
    get,
    path = "/api/plugins/commands",
    // Not the default `list_commands`: `GET /api/commands` takes that name, and
    // an operation id must be unique in the document. Two operations sharing
    // one id give the generated TypeScript ONE of the two shapes for both
    // routes, so this route's reply was typed as the slash-command list.
    operation_id = "list_plugin_commands",
    responses(
        (status = 200, body = PluginCommandsResponse),
        (status = 502, description = "The daemon could not list the commands, or answered a shape this route cannot read"),
    )
)]
async fn list_commands(
    State(state): State<AppState>,
) -> Result<Json<PluginCommandsResponse>, WebError> {
    let commands = state.daemon.plugin_commands().await.daemon_err()?;
    Ok(Json(PluginCommandsResponse {
        commands: daemon_shape(serde_json::Value::Array(commands), "plugin.commands")?,
    }))
}

/// `?key=` narrows to one contribution kind.
///
/// Without it every caller receives every plugin's published data. That is
/// more than a block drawing one key needs, and — once third-party block code
/// can run — more than it should be handed.
#[derive(Debug, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct PublicationsQuery {
    /// Narrow to one contribution kind.
    #[serde(default)]
    key: Option<String>,
}

/// What `GET /api/plugins/options` answers: one settings tree per plugin.
///
/// **A deliberate narrowing to the envelope.** The tree itself stays opaque,
/// and that is not laziness about a shape nobody wrote down — the nodes are
/// fully described in `crucible-lua`'s `describe_node`. It is about numbers:
/// `order`, `min`, `max`, `step` and every entry under `values` reach here as
/// whatever `lua_to_json` made of the plugin's Lua, so reading one into an
/// `f64` and writing it back would send `100.0` where the daemon sent `100`.
/// The browser renders a node generically anyway, so naming the map's keys is
/// the whole gain a mirrored node type would offer.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginOptionsResponse {
    #[schema(schema_with = opaque_values_schema)]
    pub(crate) options: BTreeMap<String, serde_json::Value>,
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
        (status = 200, body = PluginOptionsResponse),
        (status = 502, description = "The daemon could not read the settings trees, or answered a shape this route cannot read"),
    )
)]
async fn list_options(
    State(state): State<AppState>,
) -> Result<Json<PluginOptionsResponse>, WebError> {
    let options = state.daemon.plugin_options().await.daemon_err()?;
    Ok(Json(PluginOptionsResponse {
        options: daemon_shape(options, "plugin.options")?,
    }))
}

/// One option's current value.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginOptionValueResponse {
    /// Whatever the plugin's getter returned, and `null` is an answer.
    ///
    /// Opaque: an option's type belongs to the plugin, and the renderer reads
    /// the value against the node that declared it.
    pub(crate) value: serde_json::Value,
}

/// What `POST /api/plugins/{name}/option` answers.
///
/// Untagged, because one endpoint serves three actions: a `get` answers a
/// value where a `set` and an `execute` answer an acknowledgement.
///
/// **The variant order is load-bearing** — the hazard `ResumeSessionResponse`
/// was fixed for. Serde takes the first variant that fits, so a union whose
/// first variant also fits the second's bodies reads every one of them as the
/// wrong thing. These two are disjoint today because neither field has a
/// default: `{"ok": true}` has no `value` and `{"value": …}` has no `ok`.
/// `an_option_reply_reads_back_as_the_action_that_sent_it` asserts both
/// directions, so a `#[serde(default)]` added to either field fails there
/// rather than on a browser.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum PluginOptionCallResponse {
    /// The answer to `action: "get"`.
    Value(PluginOptionValueResponse),
    /// The answer to `action: "set"` and `action: "execute"`.
    Done(OkResponse),
}

/// `POST /api/plugins/:name/option` — read, write, or press one option.
#[utoipa::path(
    post,
    path = "/api/plugins/{name}/option",
    params(
        ("name" = String, Path, description = "The plugin whose settings tree is read or written"),
        ("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC),
    ),
    request_body = OptionRequest,
    responses(
        (status = 200, body = PluginOptionCallResponse),
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
) -> Result<Json<PluginOptionCallResponse>, WebError> {
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
            Ok(Json(PluginOptionCallResponse::Value(
                PluginOptionValueResponse { value },
            )))
        }
        OptionAction::Set => {
            state
                .daemon
                .plugin_option_set(&name, req.path, req.value)
                .await
                .daemon_err()?;
            Ok(Json(PluginOptionCallResponse::Done(OkResponse::ok())))
        }
        OptionAction::Execute => {
            state
                .daemon
                .plugin_option_execute(&name, req.path)
                .await
                .daemon_err()?;
            Ok(Json(PluginOptionCallResponse::Done(OkResponse::ok())))
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

/// `GET /api/plugins/events` — a push when a plugin's published data changes.
///
/// The counterpart to `GET /api/plugins/publications`: that answers "what is
/// true now", this says "read it again". A panel drawing a plugin's own state
/// would otherwise poll on a timer and still show a stale value between ticks.
///
/// Same shape as the file-tree stream in `routes/fs.rs`, including the
/// load-bearing ordering: subscribe the LOCAL broker channel before telling the
/// daemon to forward, because `EventBroker::dispatch` drops events for a
/// session id with no local subscriber and the window between the two calls
/// would lose the first event.
#[utoipa::path(
    get,
    path = "/api/plugins/events",
    responses((
        status = 200,
        content_type = "text/event-stream",
        body = PublicationChangedEvent,
        headers((
            "X-Crucible-Stream-Version" = u64,
            description = "The stream protocol this build speaks (also the first \
                           `stream_version` frame, for clients whose transport \
                           cannot read headers)"
        ))
    ))
)]
async fn publication_event_stream(
    State(state): State<AppState>,
) -> Result<
    ([(axum::http::HeaderName, String); 1], Sse<impl Stream<Item = Result<Event, Infallible>>>),
    WebError,
> {
    let rx = state.events.subscribe("system").await;
    state.daemon.subscribe_sticky("system").await.daemon_err()?;

    let stream = futures::stream::iter([Ok(stream_version_frame())]).chain(
        tokio_stream::wrappers::BroadcastStream::new(rx)
            .filter_map(|result| result.ok())
            .filter_map(|event| {
                // The system channel carries the file watcher and the
                // classification prompt too; this stream is only about plugin data.
                PublicationChangedEvent::from_daemon_event(&event).map(|pe| {
                    let data = serde_json::to_string(&pe).unwrap_or_default();
                    Ok(Event::default()
                        .event(PublicationChangedEvent::EVENT_NAME)
                        .data(data))
                })
            }),
    );

    Ok(versioned(Sse::new(stream).keep_alive(KeepAlive::default())))
}

/// What `POST /api/plugins/command` answers.
///
/// The browser's hand-written caller types this as a bare `unknown`, so a
/// reader had to know to look under `result` without anything saying so. The
/// envelope is the daemon's (`server/plugins.rs`'s `handle_plugin_run_command`),
/// and naming it costs the plugin nothing: `result` stays opaque.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginRunCommandResponse {
    /// The command that ran, echoed.
    pub(crate) name: String,
    /// Whatever the command's Lua `fn` returned.
    ///
    /// Opaque, like publications and options: what a command returns is the
    /// plugin's vocabulary, and a shape validated here would be one only
    /// today's plugins could send.
    pub(crate) result: serde_json::Value,
}

/// `POST /api/plugins/command` — invoke a plugin command by name.
///
/// Not under `/{name}` because a command's name is already globally unique —
/// the daemon refuses a second plugin claiming one — so routing by plugin would
/// ask the caller for something it does not need to know. The result is passed
/// through verbatim, like publications and options: what a command returns is
/// the plugin's vocabulary, and a shape this layer validated would be a shape
/// only today's plugins could send.
#[utoipa::path(
    post,
    path = "/api/plugins/command",
    params(("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC)),
    request_body = CommandRequest,
    responses(
        (status = 200, body = PluginRunCommandResponse),
        (status = 403, description = "No caller identity was sent, or the command belongs to another plugin"),
        (status = 422, description = "`name` names no command"),
        (status = 502, description = "The daemon could not run the command, or answered a shape this route cannot read"),
    )
)]
async fn run_command(
    State(state): State<AppState>,
    caller: PluginCaller,
    Json(req): Json<CommandRequest>,
) -> Result<Json<PluginRunCommandResponse>, WebError> {
    if req.name.trim().is_empty() {
        return Err(WebError::Validation(
            "`name` must name a command".to_string(),
        ));
    }
    refuse_another_plugins_command(&state, &caller, &req.name).await?;
    let result = state
        .daemon
        .plugin_run_command(&req.name, req.args)
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(result, "plugin.run_command")?))
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
    let owner = commands
        .iter()
        .find(|c| c.get("name").and_then(serde_json::Value::as_str) == Some(command))
        .and_then(|c| c.get("plugin").and_then(serde_json::Value::as_str));

    match owner {
        Some(owner) if owner == plugin => Ok(()),
        _ => Err(WebError::Forbidden(format!(
            "`{command}` is not a command of plugin `{plugin}`"
        ))),
    }
}

/// What `POST /api/plugins/{name}/reload` answers: the capabilities the
/// reloaded spec declares.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginReloadResponse {
    /// The plugin that was reloaded, echoed.
    pub(crate) name: String,
    pub(crate) reloaded: bool,
    pub(crate) tools: u64,
    pub(crate) commands: u64,
    pub(crate) handlers: u64,
    pub(crate) services: u64,
}

/// `POST /api/plugins/:name/reload` — reload a plugin by name.
/// Returns the daemon's reload response (counts of tools, commands, etc.).
#[utoipa::path(
    post,
    path = "/api/plugins/{name}/reload",
    params(
        ("name" = String, Path, description = "The plugin to reload"),
        ("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC),
    ),
    responses(
        (status = 200, body = PluginReloadResponse),
        (status = 403, description = "The caller is not the app"),
        (status = 502, description = "The reload failed, or the daemon answered a shape this route cannot read"),
    )
)]
async fn reload_plugin(
    State(state): State<AppState>,
    Path(name): Path<String>,
    caller: PluginCaller,
) -> Result<Json<PluginReloadResponse>, WebError> {
    caller.require_app("reload a plugin")?;
    let result = state.daemon.plugin_reload(&name).await.daemon_err()?;
    Ok(Json(daemon_shape(result, "plugin.reload")?))
}

/// `POST /api/plugins` — clone a plugin from a git URL and record it in
/// the installed manifest (`plugins.installed.json`), the same record
/// `cru plugin add` writes. The operator's own spec entries live in
/// `init.lua`, which nothing here edits. Synchronous; can take 10+ seconds.
/// What the clone did, tagged by `kind`.
///
/// Mirrors `crucible_daemon::BootstrapOutcome`, which is what
/// `server/plugin_install.rs` matches on to write these three objects.
/// `the_mirrored_vocabularies_stay_closed` fails to compile if a fourth
/// outcome appears there.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PluginInstallOutcomeRow {
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

/// What `POST /api/plugins` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginInstallResponse {
    /// The installed plugin's name, as the URL resolved it.
    pub(crate) name: String,
    /// The clone and the manifest record happened.
    pub(crate) installed: bool,
    /// The plugin also activated on the running daemon.
    ///
    /// `installed: true` with `loaded: false` is a plugin that reached the disk
    /// and broke on load; `error` says why, and the next boot tries again. A
    /// client must not read `installed` alone as success.
    pub(crate) loaded: bool,
    pub(crate) tools: u64,
    pub(crate) commands: u64,
    pub(crate) services: u64,
    /// Why the plugin did not load, or `null`. Always written, so `required`.
    #[schema(required = true)]
    pub(crate) error: Option<String>,
    /// A sentence about hot reload. The watcher's list is a boot-time
    /// snapshot, so a plugin installed at runtime works but is not rewatched.
    pub(crate) watch: String,
    pub(crate) outcome: PluginInstallOutcomeRow,
    /// The installed manifest the entry was written to
    /// (`plugins.installed.json`).
    ///
    /// The browser's hand-written type calls this key `plugins_toml`, which the
    /// daemon has not sent since the record moved out of TOML — so today that
    /// field reads `undefined` at run time and nothing said so. A12 renames it.
    pub(crate) manifest: String,
}

#[utoipa::path(
    post,
    path = "/api/plugins",
    params(("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC)),
    request_body = InstallRequest,
    responses(
        (status = 200, body = PluginInstallResponse),
        (status = 403, description = "The caller is not the app"),
        (status = 422, description = "The URL is empty, or the plugin is already declared in init.lua"),
        (status = 502, description = "The clone failed, or the daemon answered a shape this route cannot read"),
    )
)]
async fn install_plugin(
    State(state): State<AppState>,
    caller: PluginCaller,
    Json(req): Json<InstallRequest>,
) -> Result<Json<PluginInstallResponse>, WebError> {
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
    Ok(Json(daemon_shape(result, "plugin.install")?))
}

/// What `DELETE /api/plugins/{name}` answers.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub(crate) struct PluginRemoveResponse {
    /// The plugin that was removed, as the manifest named it.
    pub(crate) name: String,
    /// The installed manifest the entry left. See
    /// [`PluginInstallResponse::manifest`] for the name the browser still uses.
    pub(crate) manifest: String,
    /// The directory that was deleted, or `null` without `?purge=true`.
    #[schema(required = true)]
    pub(crate) purged_dir: Option<String>,
    /// The manifest entry went but deleting the directory failed.
    #[schema(required = true)]
    pub(crate) purge_error: Option<String>,
    /// Removed without a purge, and the directory is still there.
    ///
    /// It sits in a permanent search path, so the next daemon start discovers
    /// and loads it again. `null` when nothing is left behind, which is the
    /// only case where "removed" means gone.
    #[schema(required = true)]
    pub(crate) kept_dir: Option<String>,
}

/// `DELETE /api/plugins/:name?purge=true` — remove a plugin declaration.
#[utoipa::path(
    delete,
    path = "/api/plugins/{name}",
    params(
        ("name" = String, Path, description = "The plugin to remove"),
        RemoveQuery,
        ("x-crucible-plugin" = String, Header, description = CALLER_HEADER_DOC),
    ),
    responses(
        (status = 200, body = PluginRemoveResponse),
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
) -> Result<Json<PluginRemoveResponse>, WebError> {
    caller.require_app("remove a plugin")?;
    let result = state
        .daemon
        .plugin_remove(&name, query.purge)
        .await
        .daemon_err()?;
    Ok(Json(daemon_shape(result, "plugin.remove")?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routes::plugin_caller::{APP_CALLER, PLUGIN_CALLER_HEADER};
    use crate::test_support::{shape_as, survives};
    use crucible_daemon::BootstrapOutcome;
    use crucible_lua::{CommandEffect, PublicationRegistry};

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
        let published: PluginPublicationsResponse =
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
        let commands: PluginCommandsResponse =
            shape_as("GET", "/api/plugins/commands", None, Vec::new()).await;

        let command = &commands.commands[0];
        assert_eq!(command.name, "mock_command");
        assert_eq!(command.hint.as_deref(), Some("<arg>"));
        // A claim the plugin makes, never a permission to skip asking.
        assert_eq!(command.effect, CommandEffectRow::Write);
        assert!(command.parameters.is_array(), "{}", command.parameters);
    }

    #[tokio::test]
    async fn list_options_answers_the_declared_shape() {
        let options: PluginOptionsResponse =
            shape_as("GET", "/api/plugins/options", None, Vec::new()).await;

        // The tree is opaque by design, so the assertion is about the envelope
        // and about the tree arriving untouched.
        let tree = &options.options["mock-plugin"];
        assert_eq!(tree["type"], "group");
        assert_eq!(tree["args"][0]["key"], "image");
    }

    #[tokio::test]
    async fn option_call_answers_the_declared_shape() {
        let read: PluginOptionCallResponse = shape_as(
            "POST",
            "/api/plugins/mock-plugin/option",
            Some(serde_json::json!({ "action": "get", "path": ["image"] })),
            as_app(),
        )
        .await;
        let PluginOptionCallResponse::Value(read) = read else {
            panic!("a get answers a value");
        };
        assert_eq!(read.value, serde_json::json!("alpine"));

        for action in ["set", "execute"] {
            let done: PluginOptionCallResponse = shape_as(
                "POST",
                "/api/plugins/mock-plugin/option",
                Some(serde_json::json!({ "action": action, "path": ["image"], "value": "debian" })),
                as_app(),
            )
            .await;
            assert!(
                matches!(done, PluginOptionCallResponse::Done(ref ack) if ack.ok),
                "a {action} answers an acknowledgement, not {done:?}"
            );
        }
    }

    #[tokio::test]
    async fn run_command_answers_the_declared_shape() {
        let ran: PluginRunCommandResponse = shape_as(
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
    async fn reload_plugin_answers_the_declared_shape() {
        let reloaded: PluginReloadResponse =
            shape_as("POST", "/api/plugins/mock-plugin/reload", None, as_app()).await;

        assert_eq!(reloaded.name, "mock-plugin");
        assert!(reloaded.reloaded);
        assert_eq!(reloaded.handlers, 2);
    }

    #[tokio::test]
    async fn install_plugin_answers_the_declared_shape() {
        let installed: PluginInstallResponse = shape_as(
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
            PluginInstallOutcomeRow::Cloned { .. }
        ));
        // The daemon writes the record it used. The browser still reads a
        // `plugins_toml` key that stopped existing when the record left TOML.
        assert_eq!(installed.manifest, "/tmp/plugins.installed.json");
    }

    #[tokio::test]
    async fn remove_plugin_answers_the_declared_shape() {
        let removed: PluginRemoveResponse = shape_as(
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

    /// Everything a plugin published survives [`PluginPublicationsResponse`].
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

            survives::<PluginInstallResponse>(&sent);
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
            let read: PluginOptionCallResponse = serde_json::from_value(wire.clone())
                .unwrap_or_else(|e| panic!("a get's reply must read back as a value: {e}"));
            assert!(
                matches!(read, PluginOptionCallResponse::Value(_)),
                "`{wire}` read back as an acknowledgement"
            );
            assert_eq!(serde_json::to_value(read).unwrap(), wire);
        }

        let wire = serde_json::json!({ "ok": true });
        let read: PluginOptionCallResponse = serde_json::from_value(wire.clone())
            .unwrap_or_else(|e| panic!("a set's reply must read back as an acknowledgement: {e}"));
        assert!(
            matches!(read, PluginOptionCallResponse::Done(_)),
            "`{wire}` read back as a value"
        );
        assert_eq!(serde_json::to_value(read).unwrap(), wire);
    }

    // =====================================================================
    // The mirrored vocabularies stay closed
    // =====================================================================

    /// The wire spelling of every effect a command can declare.
    ///
    /// An exhaustive match, so a variant added to `crucible_lua`'s enum fails
    /// to compile here rather than reaching [`CommandEffectRow`] as a word it
    /// cannot read.
    fn effect_spelling(effect: CommandEffect) -> &'static str {
        match effect {
            CommandEffect::Read => "read",
            CommandEffect::Write => "write",
        }
    }

    #[test]
    fn the_mirrored_vocabularies_stay_closed() {
        for effect in [CommandEffect::Read, CommandEffect::Write] {
            let spelling = effect_spelling(effect);
            assert_eq!(effect.as_str(), spelling, "the daemon's spelling moved");
            serde_json::from_value::<CommandEffectRow>(serde_json::json!(spelling))
                .unwrap_or_else(|e| panic!("`CommandEffectRow` cannot read `{spelling}`: {e}"));
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
