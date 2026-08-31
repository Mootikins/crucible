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
//! `cru.log` is a callable TABLE: the walker sees a namespace that also
//! answers to a call, so its declaration is an intersection of the two. The
//! CALL half is registered beside the closure in `executor.rs`; the table
//! half comes from the walk. Neither is stated here.

use crate::signature::{LuaType, Param, Signature, VARIADIC};
use std::collections::BTreeMap;

/// One entry of the host's declared surface.
///
/// A TYPE, not a signature: `cru.on` has two accepted call shapes, and Luau
/// spells that as an intersection.
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
/// Empty, and the gate keeps it that way.
///
/// Every `cru.*` function the plugin VM exposes now carries a declared type.
/// A new one fails `every_function_is_signed_or_listed` until its author
/// writes a signature beside its registration — or adds a line here, which is
/// then visible as the exception it is.
pub const UNSIGNED: &[&str] = &[];

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
    mock: (partial: any) -> any,
}

-- A deliberate stand-in for a host namespace. Identity at run time; it exists
-- so a partial mock can be assigned to a full namespace without the checker
-- reporting a defect where the author wrote exactly what they meant.
declare function mock(partial: any): any
"#;

/// An open file, as `io.open` answers with. Named so the declarations can
/// refer to it; the host implements it in `luau_compat.rs`.
/// Payload types: the tables the HOST builds and hands to a plugin callback.
///
/// A plugin cannot annotate a parameter without a type to name, and inventing
/// one per plugin would produce N copies of a contract the daemon owns, with
/// nothing checking any copy. These are that contract, written once.
///
/// Only fixed-shape payloads appear here. `cru.on`'s handler takes
/// `(ctx: any, payload: any)` because the payload shape varies per event name,
/// and a union of every event would type nothing usefully.
pub(crate) const PAYLOAD_TYPES: &str = r#"
export type PermissionRequest = {
    tool_name: string,
    args: any,
    file_path: string?,
    mode: string?,
    is_safe: boolean,
}

-- nil means "no opinion, show the normal prompt". A table with neither
-- `allow` nor `deny` true means the same thing.
export type PermissionDecision = {
    allow: boolean?,
    deny: boolean?,
}?

-- One piece of a statusline.
--
-- Rust userdata with two members: `:hl(group)` returns a styled copy, and
-- CALLING it with an options table returns a configured copy —
-- `sl.model{ max = 25 }`. `& (...)` is how Luau states a table that is also
-- callable.
--
-- It used to be `any`, which made `when(condition, item)` and
-- `any(...StatusItem)` constrain nothing: `cru.statusline.when("streaming", 42)`
-- typechecked and raised at run time.
export type StatusItem = ((opts: { [string]: any }?) -> StatusItem) & {
    hl: (self: StatusItem, group: string) -> StatusItem,
}

-- Anything a region will render. A bare STRING is a legitimate item —
-- `value_to_item` turns one into literal text — so every position that
-- accepts an item accepts a string too, and the shipped statusline puts a
-- `" "` separator directly in its list.
export type StatusSlot = StatusItem | string

-- A live WebSocket, as `cru.ws.connect` answers with.
--
-- Named rather than `any`: `connect` returned an untyped handle, so every
-- `ws:send`/`ws:receive`/`ws:close` in a plugin went unchecked and a
-- misspelled method read as legal. See `crucible-lua/src/ws.rs`.
--
-- `receive` answers nil on TIMEOUT and a frame otherwise, and RAISES on a
-- closed connection — the nil and the raise mean different things, and only
-- the nil is in the type.
-- One frame off the socket. A `close` frame carries no `data`, which is why
-- the field is optional; `text` and `binary` always carry one.
export type WebSocketFrame = { type: string, data: string? }

export type WebSocket = {
    send: (self: WebSocket, payload: string) -> (),
    send_binary: (self: WebSocket, payload: string) -> (),
    receive: (self: WebSocket, timeout_secs: number?) -> WebSocketFrame?,
    close: (self: WebSocket) -> (),
}
"#;

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

/// A UI node, as `cru.oil.*` answers with. Named so the declarations can
/// refer to it; the host implements it in `oil.rs` as `LuaNode` userdata.
///
/// Luau has no name for an mlua userdata, so without this every `cru.oil`
/// function would return `any` — and `text: (...any) -> any` is the unsigned
/// default written out longhand. The methods are `LuaNode`'s `UserData`
/// methods, read off that impl.
///
/// `OilStyle` is the table `parse::style_from_table` reads, and it is the
/// second argument of `cru.oil.text` and `cru.oil.badge`. `OilProps` is that
/// table plus the layout keys `create_box_node` reads, and it is the optional
/// first argument of `cru.oil.col` and `cru.oil.row`.
pub(crate) const OIL_TYPES: &str = r#"
export type OilStyle = {
    fg: string?,
    bg: string?,
    bold: boolean?,
    dim: boolean?,
    italic: boolean?,
    underline: boolean?,
}

export type OilProps = {
    gap: number?,
    padding: number?,
    margin: number?,
    border: (string | boolean)?,
    justify: string?,
    align: string?,
    fg: string?,
    bg: string?,
    bold: boolean?,
    dim: boolean?,
    italic: boolean?,
    underline: boolean?,
}

export type OilNode = {
    with_style: (self: OilNode, style: OilStyle) -> OilNode,
    with_padding: (self: OilNode, padding: number) -> OilNode,
    with_border: (self: OilNode, border: string?) -> OilNode,
    with_margin: (self: OilNode, margin: number) -> OilNode,
    gap: (self: OilNode, gap: number) -> OilNode,
    justify: (self: OilNode, justify: string) -> OilNode,
    align: (self: OilNode, align: string) -> OilNode,
}
"#;

/// Render a Luau declaration file for the function paths given.
///
/// The shape is a nested `declare` of the `cru` table, so `luau-analyze` reads
/// `cru.shell.exec("git", { "status" })` as a call with a known result type.
pub fn render_declarations(paths: &[String], values: &[crate::stubs::ValueMember]) -> String {
    render_declarations_with(
        paths,
        values,
        &crate::host_registry::HostSignatures::default(),
    )
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
    //
    // Only where the VM HAS the namespace. These were merged into every
    // profile unconditionally, so `cru-statusline.d.luau` advertised
    // `cru.json`, `cru.kiln` and `cru.on` on a VM carrying `cru.statusline`
    // and nothing else — the same "describes a stand-in" defect the profiles
    // exist to prevent, surviving inside the renderer that builds them.
    //
    // The namespace is the segment after `cru`: `cru.kiln.active` rides in on
    // `cru.kiln` being there, which is the case it was written for.
    let present: std::collections::BTreeSet<&str> = paths
        .iter()
        .chain(values.iter().map(|value| &value.path))
        .filter_map(|path| path.strip_prefix("cru."))
        .map(|rest| rest.split('.').next().unwrap_or(rest))
        .collect();
    for (path, ty) in declared_signatures() {
        let namespace = path
            .strip_prefix("cru.")
            .map(|rest| rest.split('.').next().unwrap_or(rest));
        if !namespace.is_some_and(|ns| present.contains(ns)) {
            continue;
        }
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
    let unsigned = paths.len().saturating_sub(signed);
    out.push_str(&format!(
        "-- {signed} of {} functions carry a declared signature.\n",
        paths.len()
    ));
    out.push_str(
        "-- What these do NOT catch: a misspelled key in an all-optional\n\
         -- options table. Luau reads `{ bld = true }` against\n\
         -- `{ bold: boolean? }` as a table that simply omits every field,\n\
         -- which is legal. A typo in a REQUIRED field is caught, because the\n\
         -- field then reads as missing. Nearly every `cru.*` options table is\n\
         -- all-optional, so this is a ceiling, not an omission.\n",
    );
    if unsigned > 0 {
        out.push_str(&format!(
            "-- The other {unsigned} are `(...any) -> any`: no argument checked, no\n\
             -- result checked. Declare one beside its registration with `Ns::func`.\n"
        ));
    }
    out.push('\n');
    out.push_str(FILE_TYPE.trim_start());
    out.push_str(PAYLOAD_TYPES);
    out.push_str(OIL_TYPES);
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

    /// A VM with the modules whose signatures live beside their closures, and
    /// the signatures those registrations recorded.
    ///
    /// The tests below read a REGISTERED signature rather than a table entry,
    /// because that is now where a migrated function's type comes from.
    fn registered_vm() -> crate::host_registry::HostSignatures {
        let lua = mlua::Lua::new();
        let cru = crate::lua_util::get_or_create_namespace(&lua, "cru").expect("cru");
        crate::executor::register_log_function(&lua, &cru).expect("cru.log");
        crate::fs::register_fs_module(&lua).expect("cru.fs");
        crate::shell::register_shell_module(&lua, crate::shell::PluginShellPolicy::default())
            .expect("cru.shell");
        crate::host_registry::HostSignatures::of(&lua)
    }

    #[test]
    fn a_signed_function_declares_its_real_types() {
        let rendered = render_declarations_with(
            &["cru.shell.exec".to_string(), "cru.shell.which".to_string()],
            &[],
            &registered_vm(),
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
        let rendered = render_declarations_with(
            &["cru.shell.exec".to_string(), "cru.nothing.here".to_string()],
            &[],
            &registered_vm(),
        );
        assert!(
            rendered.contains("1 of 2 functions carry a declared signature"),
            "the header must count: {rendered}"
        );
    }

    #[test]
    fn the_declaration_nests_the_namespaces() {
        let rendered = render_declarations_with(
            &["cru.fs.exists".to_string(), "cru.json.encode".to_string()],
            &[],
            &registered_vm(),
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
    /// `cru.log.levels` both typecheck. The call half is registered beside
    /// its closure, so it is read from the VM rather than from `DECLARED`.
    #[test]
    fn a_callable_table_renders_as_an_intersection() {
        let rendered = render_declarations_with(
            &["cru.log".to_string(), "cru.log.notify".to_string()],
            &[],
            &registered_vm(),
        );
        assert!(
            rendered.contains("log: ((level: string, message: string) -> ()) & {"),
            "{rendered}"
        );
    }
}
