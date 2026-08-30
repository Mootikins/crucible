//! The signatures the host declares for `cru.*`, and the Luau declaration
//! file generated from them.
//!
//! Every `cru.*` function used to be described to a plugin author as
//! `---@param ... any` / `---@return any`, which is not a description. Luau
//! can check a call against a real signature, so the host states one — in
//! Rust, next to the code that registers the function, rather than in a
//! hand-maintained `.luau` file that drifts.
//!
//! Two properties keep this honest:
//!
//! - **A signature must name a function the VM really has.** The generator
//!   walks the live `cru` table; a signature for a path that is not there is a
//!   test failure (`every_declared_signature_exists_on_the_vm`), because a
//!   declaration for a function nobody registered reads as authoritative and
//!   is worse than none.
//! - **An unsigned function is declared `(...any) -> any`, and counted.** The
//!   generated file says how many of its functions are unsigned, so the gap
//!   is visible rather than implied.
//!
//! `cru.log` carries no signature on purpose: it is a callable TABLE (it also
//! holds `levels`, `notify` and the rest), so the walker sees a namespace
//! rather than a function, and a function declaration would misstate its
//! shape.

use crate::signature::{LuaType, Param, Signature, VARIADIC};
use std::collections::BTreeMap;

/// One entry of the host's declared surface.
///
/// A TYPE, not a signature: `cru.on` has two accepted call shapes and `cru.log`
/// is a table you may also call, and Luau spells both as an intersection.
struct Declared {
    path: &'static str,
    ty: fn() -> LuaType,
    /// Bound to the loading plugin at load time, so a walk of an idle VM
    /// cannot see it. The declaration still belongs in the file — the API
    /// exists — and the VM-existence gate skips exactly these.
    bound_at_load: bool,
}

/// The common case: one call shape.
fn function(params: Vec<Param>, returns: Vec<LuaType>) -> LuaType {
    LuaType::Function(Box::new(Signature { params, returns }))
}

fn param(name: &str, ty: LuaType) -> Param {
    Param {
        name: name.to_string(),
        ty,
        description: None,
        optional: false,
    }
}

fn optional(name: &str, ty: LuaType) -> Param {
    Param {
        name: name.to_string(),
        ty,
        description: None,
        optional: true,
    }
}

fn string() -> LuaType {
    LuaType::String
}

fn any() -> LuaType {
    LuaType::Any
}

/// What a `cru.on` handler is called with: the context table, then the
/// event payload (`handlers/registry.rs`).
fn handler_type() -> LuaType {
    // `...any`, not `any`: most handlers return nothing and the interception
    // ones answer with a table. A declared `any` return — even `any?` — makes
    // every ordinary handler a type error ("not all codepaths return"), which
    // `luau-lsp analyze` reports against correct plugin code.
    LuaType::Function(Box::new(Signature {
        params: vec![param("ctx", any()), param("payload", any())],
        returns: vec![LuaType::Variadic(Box::new(any()))],
    }))
}

/// `cru.on`'s two accepted shapes, as one intersection.
fn on_declaration() -> LuaType {
    LuaType::Intersection(vec![
        function(
            vec![param("event", string()), param("handler", handler_type())],
            Vec::new(),
        ),
        function(
            vec![
                param("event", string()),
                param(
                    "opts",
                    LuaType::parse("{ pattern: string?, priority: number?, timeout_ms: number? }")
                        .expect("well formed"),
                ),
                param("handler", handler_type()),
            ],
            Vec::new(),
        ),
    ])
}

/// `cru.log` is a table you may also call: `cru.log("info", msg)` alongside
/// `cru.log.levels`, `cru.log.notify` and the rest. Luau spells that as an
/// intersection of the call type and the table type; the table half comes
/// from the VM walk, so only the call half is declared here.
fn log_declaration() -> LuaType {
    function(
        vec![param("level", string()), param("message", string())],
        Vec::new(),
    )
}

/// `cru.shell.exec` and `cru.shell.spawn` both answer with this.
fn shell_result() -> LuaType {
    LuaType::parse("{ success: boolean, exit_code: number, stdout: string, stderr: string }")
        .expect("the shell result type is well formed")
}

/// The declared surface. Ordered by path so the generated file is stable.
const DECLARED: &[Declared] = &[
    Declared {
        path: "cru.json.decode",
        ty: || function(vec![param("text", string())], vec![any()]),
        bound_at_load: false,
    },
    Declared {
        // The options table is real: `cru.json.encode(value, { pretty = true })`
        // (`executor.rs`). Declaring one parameter made the documented pretty
        // form an arity error.
        path: "cru.json.encode",
        ty: || {
            function(
                vec![
                    param("value", any()),
                    optional(
                        "opts",
                        LuaType::parse("{ pretty: boolean? }").expect("well formed"),
                    ),
                ],
                vec![string()],
            )
        },
        bound_at_load: false,
    },
    Declared {
        // A property, not a call: plugins read `cru.kiln.active` and hand it
        // to `cru.kiln.path(name)` (`runtime/plugins/daily-notes/init.lua`).
        path: "cru.kiln.active",
        ty: || LuaType::Optional(Box::new(string())),
        bound_at_load: false,
    },
    Declared {
        // `(name, relative?)`, and the name is REQUIRED: `vault/mod.rs`
        // raises without one and joins the relative path when given.
        // Declaring it `(name: string?)` rejected `cru.kiln.path(kiln, ".crucible/proposals")`,
        // which is what `reflection` really calls.
        path: "cru.kiln.path",
        ty: || {
            function(
                vec![param("name", string()), optional("relative", string())],
                vec![string()],
            )
        },
        bound_at_load: false,
    },
    Declared {
        path: "cru.log",
        ty: log_declaration,
        bound_at_load: false,
    },
    Declared {
        // Two accepted shapes, and the options table is the SECOND argument
        // when there is one: `cru.on(event, handler)` or
        // `cru.on(event, opts, handler)` (`handlers/cru_on.rs`). The handler
        // is called with `(ctx, payload)` (`handlers/registry.rs`), so a
        // one-parameter handler type would reject correct plugin code.
        path: "cru.on",
        ty: on_declaration,
        bound_at_load: false,
    },
    Declared {
        // Never nil: `paths.rs` raises when no workspace is configured, so a
        // declared `string?` would make every caller nil-check what cannot be
        // nil.
        path: "cru.paths.workspace",
        ty: || function(Vec::new(), vec![string()]),
        bound_at_load: false,
    },
    Declared {
        // Bound to the loading plugin at load, so a walk of an idle VM never
        // sees it. The host declares it because it is the API regardless.
        path: "cru.plugin.publish",
        ty: || {
            function(
                vec![param("key", string()), param("value", any())],
                Vec::new(),
            )
        },
        bound_at_load: true,
    },
    Declared {
        path: "cru.plugin.options",
        ty: || {
            function(
                vec![param(
                    "tree",
                    LuaType::parse("{ args: table<string, any> }").expect("well formed"),
                )],
                Vec::new(),
            )
        },
        bound_at_load: true,
    },
    Declared {
        // One options TABLE, not a string: `session`, `key` and `text` are
        // required, `plugin`, `level` and `progress` are not
        // (`plugin_status.rs`). Declaring `(status: string)` rejected every
        // real call — `oci` makes six of them.
        path: "cru.plugin.set_status",
        ty: || {
            function(
                vec![param(
                    "status",
                    LuaType::parse(
                        "{ session: string, key: string, text: string, plugin: string?, \
                         level: string?, progress: any? }",
                    )
                    .expect("well formed"),
                )],
                Vec::new(),
            )
        },
        bound_at_load: false,
    },
    Declared {
        path: "cru.shell.exec",
        ty: || {
            function(
                vec![
                    param("command", string()),
                    // REQUIRED: the closure takes `Vec<String>`, and mlua
                    // refuses to build one from nil — `cru.shell.exec("git")`
                    // raises "error converting Lua nil to Vec<String>".
                    param("args", LuaType::Array(Box::new(string()))),
                    optional(
                        "options",
                        LuaType::parse(
                            "{ cwd: string?, env: table<string, string>?, stdin: string? }",
                        )
                        .expect("well formed"),
                    ),
                ],
                vec![shell_result()],
            )
        },
        bound_at_load: false,
    },
    Declared {
        path: "cru.shell.which",
        ty: || {
            function(
                vec![param("command", string())],
                vec![LuaType::Optional(Box::new(string()))],
            )
        },
        bound_at_load: false,
    },
    Declared {
        path: "cru.timer.clock",
        ty: || function(Vec::new(), vec![LuaType::Number]),
        bound_at_load: false,
    },
    Declared {
        // SECONDS, not milliseconds: `timer.rs` takes an `f64` into
        // `Duration::from_secs_f64`. The parameter name is the whole
        // declaration here — both spellings typecheck, so a wrong name sends
        // an author who wanted one second to sleep for a thousand.
        path: "cru.timer.sleep",
        ty: || function(vec![param("seconds", LuaType::Number)], Vec::new()),
        bound_at_load: false,
    },
    Declared {
        path: "cru.timer.spawn",
        ty: || {
            function(
                vec![param("task", LuaType::Function(Box::default()))],
                Vec::new(),
            )
        },
        bound_at_load: false,
    },
];

/// The `cru.*` functions that carry no signature yet.
///
/// A ratchet, not a wishlist. `every_function_is_signed_or_listed` fails when
/// a function on the live VM is neither declared in `DECLARED` nor named
/// here, so a NEW function cannot ship undescribed — its author writes a
/// signature, or adds a line here, in the diff that adds the function. The
/// same test fails when an entry here no longer exists on the VM, so the list
/// cannot rot behind a rename.
///
/// It is also the work queue: every line is one `cru.*` function a plugin
/// author currently sees as `(...any) -> any`.
pub const UNSIGNED: &[&str] = &[
    "cru.check.boolean",
    "cru.check.func",
    "cru.check.number",
    "cru.check.one_of",
    "cru.check.string",
    "cru.check.table",
    "cru.config.get",
    "cru.config.set",
    "cru.context.register_validator",
    "cru.emitter.global",
    "cru.emitter.new",
    "cru.errors._capture",
    "cru.errors.recent",
    "cru.fs.copy",
    "cru.fs.list",
    "cru.fs.remove",
    "cru.get_session",
    "cru.health.error",
    "cru.health.get_results",
    "cru.health.info",
    "cru.health.ok",
    "cru.health.start",
    "cru.health.warn",
    "cru.http.delete",
    "cru.http.get",
    "cru.http.patch",
    "cru.http.post",
    "cru.http.put",
    "cru.http.request",
    "cru.inspect",
    "cru.isolation.require",
    "cru.json.array",
    "cru.kiln.backlinks",
    "cru.kiln.get",
    "cru.kiln.list",
    "cru.kiln.neighbors",
    "cru.kiln.outlinks",
    "cru.kiln.search",
    "cru.log.messages.clear",
    "cru.log.messages.hide",
    "cru.log.messages.show",
    "cru.log.messages.toggle",
    "cru.log.notify",
    "cru.log.notify_once",
    "cru.oil.badge",
    "cru.oil.bullet_list",
    "cru.oil.col",
    "cru.oil.component",
    "cru.oil.divider",
    "cru.oil.each",
    "cru.oil.either",
    "cru.oil.fragment",
    "cru.oil.input",
    "cru.oil.kv",
    "cru.oil.markup",
    "cru.oil.match_state",
    "cru.oil.numbered_list",
    "cru.oil.popup",
    "cru.oil.progress",
    "cru.oil.row",
    "cru.oil.scrollback",
    "cru.oil.spacer",
    "cru.oil.spinner",
    "cru.oil.text",
    "cru.oil.when",
    "cru.on_provider_auth",
    "cru.on_session_end",
    "cru.on_session_start",
    "cru.oq.convert",
    "cru.oq.detect",
    "cru.oq.format",
    "cru.oq.parse",
    "cru.oq.parse_as",
    "cru.oq.query",
    "cru.oq.toml",
    "cru.oq.toon",
    "cru.oq.yaml",
    "cru.paths.config",
    "cru.paths.session",
    "cru.paths.state",
    "cru.plugin.clear_status",
    "cru.plugin.config.get",
    "cru.ratelimit.new",
    "cru.retry",
    "cru.schedule",
    "cru.schedule.cancel",
    "cru.service.define",
    "cru.service.list",
    "cru.service.status",
    "cru.service.stop",
    "cru.session.cache_stats",
    "cru.session.can_undo",
    "cru.session.cancel",
    "cru.session.collect_subagents",
    "cru.session.complete",
    "cru.session.configure_agent",
    "cru.session.create",
    "cru.session.current",
    "cru.session.end_session",
    "cru.session.fork",
    "cru.session.get",
    "cru.session.inject",
    "cru.session.interaction_respond",
    "cru.session.list",
    "cru.session.messages",
    "cru.session.pause",
    "cru.session.resume",
    "cru.session.review_comment",
    "cru.session.review_list_hunks",
    "cru.session.review_resolve_comment",
    "cru.session.review_set_state",
    "cru.session.send_and_collect",
    "cru.session.send_message",
    "cru.session.set_output_validation",
    "cru.session.subscribe",
    "cru.session.undo",
    "cru.session.undo_depth",
    "cru.session.undo_history",
    "cru.session.unsubscribe",
    "cru.shell.spawn",
    "cru.storage.delete",
    "cru.storage.find",
    "cru.storage.get",
    "cru.storage.list",
    "cru.storage.set",
    "cru.tbl_deep_extend",
    "cru.tbl_get",
    "cru.timer.timeout",
    "cru.tools.batch",
    "cru.tools.call",
    "cru.tools.get_active",
    "cru.tools.list",
    "cru.tools.set_active",
    "cru.ui.ask",
    "cru.ui.ask_batch",
    "cru.ui.edit",
    "cru.ui.panel",
    "cru.ui.permission",
    "cru.ui.popup",
    "cru.ui.show",
    "cru.ws.connect",
];

/// Every declared path, with its type.
pub fn declared_signatures() -> BTreeMap<&'static str, LuaType> {
    DECLARED
        .iter()
        .map(|entry| (entry.path, (entry.ty)()))
        .collect()
}

/// The Luau declaration for one function path, signed or not.
pub fn declaration_for(path: &str) -> LuaType {
    declared_signatures()
        .get(path)
        .cloned()
        .unwrap_or_else(unsigned)
}

/// What an unsigned function is declared as: it takes anything and answers
/// anything, which is exactly as much as the host currently knows.
pub fn unsigned() -> LuaType {
    function(
        vec![Param {
            name: VARIADIC.to_string(),
            ty: LuaType::Any,
            description: None,
            optional: false,
        }],
        vec![LuaType::Any],
    )
}

/// Paths whose function is bound to the loading plugin, so a walk of an idle
/// VM cannot see them. A gate that checks declarations against the VM skips
/// these — and nothing else.
pub fn bound_at_load() -> BTreeMap<&'static str, ()> {
    DECLARED
        .iter()
        .filter(|entry| entry.bound_at_load)
        .map(|entry| (entry.path, ()))
        .collect()
}

/// Whether a path carries a real signature.
pub fn is_signed(path: &str) -> bool {
    declared_signatures().contains_key(path)
}

/// The rest of the environment a plugin runs in, declared for the checker.
///
/// `declare cru: { … }` alone is not the plugin environment. A plugin reads
/// files with `io.open`, its entry module ends with `package.loaded[NAME]`,
/// its suite calls `describe`/`it`/`expect`, and every one of those is a host
/// global that Luau itself does not have. Declaring only `cru` made
/// `luau-lsp analyze` report "Unknown global 'io'" against correct code —
/// noise that trains an author to ignore the checker.
///
/// `os` is re-declared WHOLE, base functions included: a definitions file
/// replaces the type it names, so listing only the host's additions would
/// take `os.time` away.
///
/// The test-harness globals are here too. They exist only while a suite runs,
/// so a non-test file could call them without complaint — the alternative, a
/// second definitions file selected per file, buys strictness a plugin author
/// does not need and a false failure they would have to work around.
const HOST_ENVIRONMENT: &str = r#"
declare io: {
    open: (path: string, mode: string?) -> (LuaFile?, string?),
    lines: (path: string, format: (string | number)?) -> ((LuaFile) -> string?, LuaFile),
    close: (file: LuaFile) -> boolean,
    type: (value: any) -> string?,
}

declare os: {
    time: (when: any?) -> number,
    date: (format: string?, when: number?) -> any,
    clock: () -> number,
    difftime: (later: number, earlier: number) -> number,
    getenv: (name: string) -> string?,
    tmpname: () -> string,
    remove: (path: string) -> (boolean?, string?),
    rename: (from: string, to: string) -> (boolean?, string?),
}

declare package: {
    loaded: { [string]: any },
    preload: { [string]: any },
    searchpath: (name: string, path: string?) -> (string?, string?),
}

declare function require(name: string): any

declare function describe(name: string, body: () -> ()): ()
declare function it(name: string, body: () -> ()): ()
declare function pending(name: string, body: (() -> ())?): ()
declare function before_each(body: () -> ()): ()
declare function after_each(body: () -> ()): ()
declare function run_tests(): { passed: number, failed: number }

declare expect: {
    equal: (expected: any, actual: any, message: string?) -> (),
    equals: (expected: any, actual: any, message: string?) -> (),
    deep_equal: (expected: any, actual: any, message: string?) -> (),
    truthy: (value: any, message: string?) -> (),
    falsy: (value: any, message: string?) -> (),
    is_nil: (value: any, message: string?) -> (),
    is_not_nil: (value: any, message: string?) -> (),
    is_string: (value: any, message: string?) -> (),
    is_number: (value: any, message: string?) -> (),
    is_table: (value: any, message: string?) -> (),
    is_function: (value: any, message: string?) -> (),
    has_error: (body: () -> (), message: string?) -> (),
}

declare test_mocks: {
    setup: (fixture: { [string]: any }?) -> (),
    reset: () -> (),
    get_calls: (namespace: string, name: string) -> { any },
}
"#;

/// An open file, as `io.open` answers with. Named so the declarations can
/// refer to it; the host implements it in `luau_compat.rs`.
const FILE_TYPE: &str = r#"
export type LuaFile = {
    read: (self: LuaFile, format: (string | number)?) -> string?,
    lines: (self: LuaFile, format: (string | number)?) -> (LuaFile) -> string?,
    write: (self: LuaFile, ...any) -> LuaFile,
    seek: (self: LuaFile, whence: string?, offset: number?) -> number?,
    flush: (self: LuaFile) -> LuaFile,
    close: (self: LuaFile) -> boolean,
}
"#;

/// Render a Luau declaration file for the function paths given.
///
/// The shape is a nested `declare` of the `cru` table, so `luau-analyze` reads
/// `cru.shell.exec("git", { "status" })` as a call with a known result type.
pub fn render_declarations(paths: &[String], values: &[crate::stubs::ValueMember]) -> String {
    render_declarations_with(paths, values, &crate::host_registry::HostSignatures::default())
}

/// [`render_declarations`], reading the signatures a VM's own registrations
/// recorded.
///
/// This is the direction of travel: a declaration written beside its closure
/// (`crate::host_registry::Ns`) is checked against the Rust types, so it
/// cannot drift. `DECLARED` below holds what has not moved yet, plus the
/// handful of members no closure can carry.
pub fn render_declarations_with(
    paths: &[String],
    values: &[crate::stubs::ValueMember],
    registered: &crate::host_registry::HostSignatures,
) -> String {
    let mut tree = Node::default();
    for path in paths {
        tree.insert_path(path, Member::Function);
    }
    // Non-function members, so an exact table type does not reject a correct
    // read of `cru.kiln.active`.
    for value in values {
        tree.insert_path(&value.path, Member::Field(format!("{}?", value.luau_type)));
    }
    // Declared members the walk cannot see: a field absent from the VM the
    // stubs were rendered from (no kiln is active), and a function bound to
    // the loading plugin at load time (`cru.plugin.publish`). Both are the
    // API; the walk simply cannot observe them from an idle VM.
    for (path, ty) in declared_signatures() {
        let member = if is_function_type(&ty) {
            Member::Function
        } else {
            Member::Field(ty.to_luau())
        };
        tree.insert_path(path, member);
    }

    // A registration's own declaration wins over the static table.
    for path in registered.paths() {
        tree.insert_path(&path, Member::Function);
    }
    let signed = paths
        .iter()
        .filter(|path| is_signed(path) || registered.get(path).is_some())
        .count();
    let mut out = String::new();
    out.push_str("--!strict\n");
    out.push_str("-- Generated by `cru plugin stubs`. Do not edit.\n");
    out.push_str(&format!(
        "-- {} of {} functions carry a declared signature; the rest are\n\
         -- `(...any) -> any` until one is written in `crucible-lua/src/host_api.rs`.\n\n",
        signed,
        paths.len()
    ));
    out.push_str(FILE_TYPE.trim_start());
    out.push_str(HOST_ENVIRONMENT);
    out.push('\n');
    out.push_str("declare cru: ");
    tree.render(&mut out, 0, registered);
    out.push('\n');
    out
}

/// What lives at a path.
enum Member {
    /// A function, whose type comes from `DECLARED` or is `(...any) -> any`.
    Function,
    /// A value, with its rendered Luau type.
    Field(String),
}

/// Whether a declared type is something you call. Public so a gate can hold
/// functions and fields to different standards of existence.
pub fn is_callable_declaration(ty: &LuaType) -> bool {
    is_function_type(ty)
}

/// Whether a declared type is something you call.
fn is_function_type(ty: &LuaType) -> bool {
    match ty {
        LuaType::Function(_) => true,
        LuaType::Intersection(parts) => parts.iter().any(is_function_type),
        _ => false,
    }
}

/// One level of the `cru` table while it is being rendered.
#[derive(Default)]
struct Node {
    children: BTreeMap<String, Node>,
    /// The full path this node sits at, once one is known.
    path: Option<String>,
    /// Whether a function lives at this path. A node can be BOTH a function
    /// and a table — `cru.log` is a table you may also call.
    callable: bool,
    /// The rendered type, for a non-function member.
    field: Option<String>,
}

impl Node {
    fn insert_path(&mut self, path: &str, member: Member) {
        let segments: Vec<&str> = path.split('.').collect();
        if segments.first() != Some(&"cru") {
            return;
        }
        self.insert(&segments[1..], path, "", member);
    }

    fn insert(&mut self, segments: &[&str], path: &str, prefix: &str, member: Member) {
        if self.path.is_none() && !prefix.is_empty() {
            self.path = Some(prefix.to_string());
        }
        match segments {
            [] => {
                self.path = Some(path.to_string());
                match member {
                    Member::Function => self.callable = true,
                    Member::Field(ty) => self.field = Some(ty),
                }
            }
            [head, tail @ ..] => {
                let child_prefix = if prefix.is_empty() {
                    format!("cru.{head}")
                } else {
                    format!("{prefix}.{head}")
                };
                self.children
                    .entry((*head).to_string())
                    .or_default()
                    .insert(tail, path, &child_prefix, member)
            }
        }
    }

    fn render(
        &self,
        out: &mut String,
        depth: usize,
        registered: &crate::host_registry::HostSignatures,
    ) {
        let call = self.path.as_deref().filter(|_| self.callable);
        let declared = |path: &str| {
            registered
                .get(path)
                .unwrap_or_else(|| declaration_for(path))
                .to_luau()
        };

        if self.children.is_empty() {
            match (call, &self.field) {
                (Some(path), _) => out.push_str(&declared(path)),
                (None, Some(field)) => out.push_str(field),
                // A namespace the VM has with nothing in it.
                (None, None) => out.push_str("{}"),
            }
            return;
        }

        // A table you may also call — `cru.log` — is an intersection of the
        // call type and the table type. Luau has no other way to say it, and
        // omitting the call half makes `cru.log("info", msg)` a type error
        // against a table.
        if let Some(path) = call {
            out.push_str(&format!("({}) & ", declared(path)));
        }

        let indent = "    ".repeat(depth + 1);
        let closing = "    ".repeat(depth);
        out.push_str("{\n");
        for (name, child) in &self.children {
            out.push_str(&indent);
            out.push_str(name);
            out.push_str(": ");
            child.render(out, depth + 1, registered);
            out.push_str(",\n");
        }
        out.push_str(&closing);
        out.push('}');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signed_function_declares_its_real_types() {
        let rendered = render_declarations(
            &["cru.shell.exec".to_string(), "cru.shell.which".to_string()],
            &[],
        );
        assert!(
            rendered.contains("exec: (command: string, args: { string },"),
            "the signature must reach the declaration: {rendered}"
        );
        assert!(
            rendered.contains("stdout: string"),
            "the result shape must reach the declaration: {rendered}"
        );
    }

    #[test]
    fn an_unsigned_function_is_declared_as_taking_anything() {
        let rendered = render_declarations(&["cru.nothing.here".to_string()], &[]);
        assert!(
            rendered.contains("here: (...any) -> any"),
            "an unsigned function must be visible as unsigned: {rendered}"
        );
    }

    /// The header states the gap. A reader must be able to see how much of
    /// the surface is actually described.
    #[test]
    fn the_header_counts_the_signed_functions() {
        let rendered = render_declarations(
            &["cru.shell.exec".to_string(), "cru.nothing.here".to_string()],
            &[],
        );
        assert!(
            rendered.contains("1 of 2 functions carry a declared signature"),
            "the header must count: {rendered}"
        );
    }

    #[test]
    fn the_declaration_nests_the_namespaces() {
        let rendered = render_declarations(
            &["cru.fs.exists".to_string(), "cru.json.encode".to_string()],
            &[],
        );
        assert!(rendered.starts_with("--!strict\n"));
        assert!(rendered.contains("declare cru: {"));
        assert!(rendered.contains("fs: {"));
        assert!(rendered.contains("json: {"));
        assert!(rendered.contains("exists: (path: string) -> boolean"));
    }

    /// Every declared type is well formed. A `LuaType::parse` in the table
    /// that raises would take the generator down with it, and a function type
    /// that does not render as a call would be a parse error in the file.
    #[test]
    fn every_declared_type_renders() {
        for (path, ty) in declared_signatures() {
            let rendered = ty.to_luau();
            assert!(!rendered.is_empty(), "{path} rendered nothing");
            if is_function_type(&ty) {
                assert!(
                    rendered.starts_with('(') && rendered.contains("->"),
                    "{path} is declared callable but rendered {rendered}"
                );
            }
        }
    }

    /// A member that is not a function is declared as itself.
    /// `cru.kiln.active` is a string a plugin READS; declaring it callable
    /// made every correct use of it a type error.
    #[test]
    fn a_field_is_declared_as_a_value() {
        assert_eq!(
            declaration_for("cru.kiln.active").to_luau(),
            "string?",
            "a property must not be declared as a call"
        );
        let rendered = render_declarations(
            &[],
            &[crate::stubs::ValueMember {
                path: "cru.session.id".to_string(),
                luau_type: "string".to_string(),
            }],
        );
        assert!(rendered.contains("id: string?"), "{rendered}");
    }

    /// A callable table renders both halves, so `cru.log("info", msg)` and
    /// `cru.log.levels` both typecheck.
    #[test]
    fn a_callable_table_renders_as_an_intersection() {
        let rendered =
            render_declarations(&["cru.log".to_string(), "cru.log.notify".to_string()], &[]);
        assert!(
            rendered.contains("log: ((level: string, message: string) -> ()) & {"),
            "{rendered}"
        );
    }
}
