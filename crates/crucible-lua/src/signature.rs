//! One signature model, three renderings.
//!
//! A plugin declares a tool's parameters as metadata — `{ name = "query",
//! type = "string", desc = "…" }` — and three separate places used to decide
//! independently what those strings meant. JSON Schema generation understood
//! four primitive labels and silently rendered everything else as `"string"`,
//! so a `string[]` parameter reached an agent as a string. The stub generator
//! understood nothing and wrote `...any`. Luau, which has a real type system,
//! was told nothing at all.
//!
//! So the meaning lives in Rust, once. [`LuaType`] is parsed from a
//! declaration, and rendered as:
//!
//! 1. a **Luau type expression**, for the generated `cru.d.luau` declarations
//!    and for a plugin's own checked annotations;
//! 2. a **JSON Schema**, for the agent tool surface; and
//! 3. a **human label**, for `cru plugin list` and error messages.
//!
//! A declaration the model cannot parse is an error the author sees at load,
//! rather than a silent downgrade to `string`.
//!
//! ## The grammar
//!
//! ```text
//! type     := union
//! union    := postfix ("|" postfix)*
//! postfix  := primary "?"? "[]"*
//! primary  := "any" | "nil" | "boolean" | "number" | "string"
//!           | "table" | "array" "<" type ">"
//!           | "table" "<" type "," type ">"
//!           | "{" field ("," field)* "}"
//!           | NAME
//! field    := NAME ":" type | NAME "?" ":" type
//! ```

use serde_json::{json, Value as JsonValue};
use std::fmt;

/// A type a plugin may declare, and the host may check.
#[derive(Debug, Clone, PartialEq)]
pub enum LuaType {
    /// Unknown, and admitted to be unknown.
    Any,
    Nil,
    Boolean,
    Number,
    String,
    /// A table with numeric keys: `{T}` in Luau, an array in JSON Schema.
    Array(Box<LuaType>),
    /// A table with uniform keys and values.
    Map(Box<LuaType>, Box<LuaType>),
    /// A table with named fields.
    Record(Vec<Field>),
    /// One of several types.
    Union(Vec<LuaType>),
    /// `T?` — the type, or nothing.
    Optional(Box<LuaType>),
    /// A function.
    Function(Box<Signature>),
    /// Any number of values of one type: Luau's `...T`, in a parameter list
    /// or a return position. A callback declared `-> ...any` accepts both a
    /// handler that returns nothing and one that answers with a table, which
    /// a plain `any` return does not ("not all codepaths return").
    Variadic(Box<LuaType>),
    /// A function with more than one accepted shape, or a callable table.
    /// Luau spells both as an intersection: `((A) -> B) & ((C) -> D)`, and
    /// `((A) -> B) & { field: T }` for a table you may also call.
    Intersection(Vec<LuaType>),
    /// A type declared elsewhere, by name.
    Named(String),
    /// One exact string value: a Luau singleton type. A JSON Schema string
    /// `enum` becomes a [`LuaType::Union`] of these, so a tagged field such
    /// as a knob name reads as a closed set of literals, not a bare
    /// `string`.
    Literal(String),
}

/// One field of a record type.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub ty: LuaType,
    pub description: Option<String>,
}

/// One parameter of a function.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub ty: LuaType,
    pub description: Option<String>,
    /// Declared optional. Rendered as `T?` and left out of `required`.
    pub optional: bool,
}

/// The parameter name that means "the rest", rendered as Luau's `...`.
pub const VARIADIC: &str = "...";

/// A function's parameters and what it answers with.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Signature {
    pub params: Vec<Param>,
    /// Empty means `()`; one entry is the ordinary case.
    pub returns: Vec<LuaType>,
}

/// A declaration the model refused, with the text that was wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeError {
    pub declaration: String,
    pub reason: String,
}

impl fmt::Display for TypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "cannot read the type '{}': {}",
            self.declaration, self.reason
        )
    }
}

impl std::error::Error for TypeError {}

impl LuaType {
    /// Read a declaration. `"string"`, `"string?"`, `"string[]"`,
    /// `"array<number>"`, `"table<string, number>"`, `"string|number"`,
    /// `"{ name: string, count: number? }"`.
    pub fn parse(declaration: &str) -> Result<Self, TypeError> {
        let mut parser = Parser {
            input: declaration,
            rest: declaration.trim(),
        };
        let ty = parser.parse_union()?;
        parser.skip_space();
        if !parser.rest.is_empty() {
            return Err(parser.fail(format!("unexpected '{}'", parser.rest)));
        }
        Ok(ty)
    }

    /// The Luau type expression.
    pub fn to_luau(&self) -> String {
        match self {
            LuaType::Any => "any".to_string(),
            LuaType::Nil => "nil".to_string(),
            LuaType::Boolean => "boolean".to_string(),
            LuaType::Number => "number".to_string(),
            LuaType::String => "string".to_string(),
            LuaType::Array(item) => format!("{{ {} }}", item.to_luau()),
            LuaType::Map(key, value) => {
                format!("{{ [{}]: {} }}", key.to_luau(), value.to_luau())
            }
            LuaType::Record(fields) => {
                let rendered: Vec<String> = fields
                    .iter()
                    .map(|field| format!("{}: {}", field.name, field.ty.to_luau()))
                    .collect();
                format!("{{ {} }}", rendered.join(", "))
            }
            LuaType::Union(options) => options
                .iter()
                .map(LuaType::to_luau)
                .collect::<Vec<_>>()
                .join(" | "),
            // A function or an intersection needs parentheses before the
            // `?`, or Luau reads the `?` as part of the RETURN type:
            // `(stream: string, line: string) -> ()?` is an optional empty
            // return, not an optional callback. `cru.shell.spawn`'s
            // `options.on_line` is the first declaration to need it.
            LuaType::Optional(inner) => match **inner {
                LuaType::Function(_) | LuaType::Intersection(_) | LuaType::Union(_) => {
                    format!("({})?", inner.to_luau())
                }
                _ => format!("{}?", inner.to_luau()),
            },
            LuaType::Function(signature) => signature.to_luau(),
            LuaType::Variadic(inner) => format!("...{}", inner.to_luau()),
            LuaType::Intersection(parts) => parts
                .iter()
                .map(|part| format!("({})", part.to_luau()))
                .collect::<Vec<_>>()
                .join(" & "),
            LuaType::Named(name) => name.clone(),
            LuaType::Literal(value) => format!("{value:?}"),
        }
    }

    /// The JSON Schema an agent's tool surface advertises.
    pub fn to_json_schema(&self) -> JsonValue {
        match self {
            LuaType::Any => json!({}),
            LuaType::Nil => json!({ "type": "null" }),
            LuaType::Boolean => json!({ "type": "boolean" }),
            LuaType::Number => json!({ "type": "number" }),
            LuaType::String => json!({ "type": "string" }),
            LuaType::Array(item) => json!({ "type": "array", "items": item.to_json_schema() }),
            LuaType::Map(_, value) => {
                json!({ "type": "object", "additionalProperties": value.to_json_schema() })
            }
            LuaType::Record(fields) => {
                let mut properties = serde_json::Map::new();
                let mut required = Vec::new();
                for field in fields {
                    let mut schema = field.ty.to_json_schema();
                    if let (Some(description), Some(object)) =
                        (&field.description, schema.as_object_mut())
                    {
                        object.insert("description".to_string(), json!(description));
                    }
                    properties.insert(field.name.clone(), schema);
                    if !matches!(field.ty, LuaType::Optional(_)) {
                        required.push(field.name.clone());
                    }
                }
                json!({ "type": "object", "properties": properties, "required": required })
            }
            // JSON Schema spells a union as `anyOf`, and `T?` as the type or
            // null — the shape a model provider validates against.
            LuaType::Union(options) => {
                json!({ "anyOf": options.iter().map(LuaType::to_json_schema).collect::<Vec<_>>() })
            }
            LuaType::Optional(inner) => inner.to_json_schema(),
            // A function cannot cross the tool boundary. Saying so beats
            // advertising a shape the agent could try to send.
            LuaType::Function(_) | LuaType::Intersection(_) => json!({}),
            // A variadic cannot cross the tool boundary either.
            LuaType::Variadic(_) => json!({}),
            LuaType::Named(name) => json!({ "$comment": format!("plugin type {name}") }),
            LuaType::Literal(value) => json!({ "type": "string", "enum": [value] }),
        }
    }

    /// Read a JSON Schema (or an OpenAPI `Schema` serialized the same way)
    /// into the type it describes.
    ///
    /// Reads exactly the shapes `utoipa::ToSchema` writes into
    /// `openapi.json`, since that is the one schema source this reads: a
    /// `$ref` becomes a [`LuaType::Named`] to a type this module is asked to
    /// render separately; `oneOf`/`anyOf` becomes a [`LuaType::Union`]; an
    /// `object` with `properties` becomes a [`LuaType::Record`], with a
    /// property absent from `required` wrapped in [`LuaType::Optional`]; an
    /// `object` with `additionalProperties` and no `properties` becomes a
    /// [`LuaType::Map`]; an `array` becomes a [`LuaType::Array`]; a `string`
    /// `enum` becomes a [`LuaType::Literal`] union; a `type` that is a JSON
    /// array (utoipa's spelling of `Option<T>` on a primitive, `["string",
    /// "null"]`) becomes [`LuaType::Optional`]. A schema this cannot read —
    /// an empty object, a bare `{}` for `serde_json::Value` — becomes
    /// [`LuaType::Any`], which is the honest answer for "arbitrary JSON".
    pub fn from_json_schema(schema: &JsonValue) -> LuaType {
        if let Some(reference) = schema.get("$ref").and_then(JsonValue::as_str) {
            let name = reference.rsplit('/').next().unwrap_or(reference);
            return LuaType::Named(name.to_string());
        }

        let branches = schema
            .get("oneOf")
            .or_else(|| schema.get("anyOf"))
            .and_then(JsonValue::as_array);
        if let Some(branches) = branches {
            // `Option<Ref>` is `oneOf: [{"type": "null"}, {"$ref": ...}]`.
            // Two branches where one is exactly `null` is an optional value,
            // not a two-way choice.
            if branches.len() == 2 {
                let null_at = branches.iter().position(|branch| {
                    branch.get("type").and_then(JsonValue::as_str) == Some("null")
                });
                if let Some(null_at) = null_at {
                    let other = &branches[1 - null_at];
                    return LuaType::Optional(Box::new(LuaType::from_json_schema(other)));
                }
            }
            let options: Vec<LuaType> = branches.iter().map(LuaType::from_json_schema).collect();
            return if options.len() == 1 {
                options.into_iter().next().expect("checked len == 1")
            } else {
                LuaType::Union(options)
            };
        }

        // `allOf` composes schemas; this reads the one-item case (a `$ref`
        // plus a sibling description, which utoipa also writes for a
        // referenced enum with a doc comment) and otherwise gives up to `Any`
        // rather than guess at a merge.
        if let Some(parts) = schema.get("allOf").and_then(JsonValue::as_array) {
            if let [only] = parts.as_slice() {
                return LuaType::from_json_schema(only);
            }
            return LuaType::Any;
        }

        if let Some(values) = schema.get("enum").and_then(JsonValue::as_array) {
            let literals: Vec<LuaType> = values
                .iter()
                .filter_map(JsonValue::as_str)
                .map(|value| LuaType::Literal(value.to_string()))
                .collect();
            if !literals.is_empty() {
                return if literals.len() == 1 {
                    literals.into_iter().next().expect("checked len == 1")
                } else {
                    LuaType::Union(literals)
                };
            }
        }

        let ty = schema.get("type");
        // utoipa spells `Option<Primitive>` as a two-entry `type` array
        // rather than an `oneOf`: `["string", "null"]`.
        if let Some(types) = ty.and_then(JsonValue::as_array) {
            let names: Vec<&str> = types.iter().filter_map(JsonValue::as_str).collect();
            let nullable = names.contains(&"null");
            let rest: Vec<&&str> = names.iter().filter(|name| **name != "null").collect();
            if nullable && rest.len() == 1 {
                let mut without_null = schema.clone();
                without_null["type"] = json!(rest[0]);
                return LuaType::Optional(Box::new(LuaType::from_json_schema(&without_null)));
            }
        }

        match ty.and_then(JsonValue::as_str) {
            Some("null") => LuaType::Nil,
            Some("boolean") => LuaType::Boolean,
            Some("integer") | Some("number") => LuaType::Number,
            Some("string") => LuaType::String,
            Some("array") => {
                let item = schema
                    .get("items")
                    .map(LuaType::from_json_schema)
                    .unwrap_or(LuaType::Any);
                LuaType::Array(Box::new(item))
            }
            Some("object") | None => {
                if let Some(properties) = schema.get("properties").and_then(JsonValue::as_object) {
                    let required: Vec<&str> = schema
                        .get("required")
                        .and_then(JsonValue::as_array)
                        .map(|values| values.iter().filter_map(JsonValue::as_str).collect())
                        .unwrap_or_default();
                    let fields = properties
                        .iter()
                        .map(|(name, property)| {
                            let mut ty = LuaType::from_json_schema(property);
                            if !required.contains(&name.as_str())
                                && !matches!(ty, LuaType::Optional(_))
                            {
                                ty = LuaType::Optional(Box::new(ty));
                            }
                            Field {
                                name: name.clone(),
                                ty,
                                description: property
                                    .get("description")
                                    .and_then(JsonValue::as_str)
                                    .map(str::to_string),
                            }
                        })
                        .collect();
                    LuaType::Record(fields)
                } else if let Some(additional) = schema.get("additionalProperties") {
                    let value = if additional.is_object() {
                        LuaType::from_json_schema(additional)
                    } else {
                        LuaType::Any
                    };
                    LuaType::Map(Box::new(LuaType::String), Box::new(value))
                } else if schema.get("type").is_some() {
                    LuaType::Map(Box::new(LuaType::String), Box::new(LuaType::Any))
                } else {
                    // No `type`, no `properties`: `serde_json::Value`'s own
                    // schema, which utoipa writes as a bare `{}` (or `{}`
                    // plus a `description`). Arbitrary JSON is `any`.
                    LuaType::Any
                }
            }
            _ => LuaType::Any,
        }
    }

    /// The type a Rust type's own `utoipa` schema describes.
    ///
    /// The one call [`crate::json_binding::Json`] makes to declare itself:
    /// `T::schema()` gives the same JSON Schema `openapi.json` writes for
    /// `T`, and [`Self::from_json_schema`] reads it. A binding that carries
    /// `Json<T>` needs no hand Luau string; its declaration is this.
    pub fn of_schema<T: utoipa::PartialSchema>() -> LuaType {
        let schema = serde_json::to_value(T::schema()).unwrap_or_else(|_| json!({}));
        LuaType::from_json_schema(&schema)
    }
}

impl Signature {
    /// The Luau function type, as a declaration's right-hand side.
    pub fn to_luau(&self) -> String {
        let params: Vec<String> = self
            .params
            .iter()
            .map(|param| {
                let ty = if param.optional && !matches!(param.ty, LuaType::Optional(_)) {
                    LuaType::Optional(Box::new(param.ty.clone())).to_luau()
                } else {
                    param.ty.to_luau()
                };
                // A variadic carries no name in a type position: Luau writes
                // `(...any) -> any`, and `(...: any)` is a parse error — one
                // that would take the whole declarations file down with it,
                // since every unsigned function renders this way.
                if param.name == VARIADIC {
                    format!("...{ty}")
                } else {
                    format!("{}: {}", param.name, ty)
                }
            })
            .collect();
        let returns = match self.returns.len() {
            0 => "()".to_string(),
            1 => self.returns[0].to_luau(),
            _ => format!(
                "({})",
                self.returns
                    .iter()
                    .map(LuaType::to_luau)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        };
        format!("({}) -> {}", params.join(", "), returns)
    }

    /// The JSON Schema for the tool's arguments: one object, one property per
    /// parameter, the non-optional ones required.
    pub fn to_input_schema(&self) -> JsonValue {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();
        for param in &self.params {
            let mut schema = param.ty.to_json_schema();
            if let Some(object) = schema.as_object_mut() {
                object.insert(
                    "description".to_string(),
                    json!(param.description.clone().unwrap_or_default()),
                );
            }
            properties.insert(param.name.clone(), schema);
            if !param.optional && !matches!(param.ty, LuaType::Optional(_)) {
                required.push(param.name.clone());
            }
        }
        json!({ "type": "object", "properties": properties, "required": required })
    }
}

struct Parser<'a> {
    input: &'a str,
    rest: &'a str,
}

impl<'a> Parser<'a> {
    fn fail(&self, reason: impl Into<String>) -> TypeError {
        TypeError {
            declaration: self.input.to_string(),
            reason: reason.into(),
        }
    }

    fn skip_space(&mut self) {
        self.rest = self.rest.trim_start();
    }

    fn eat(&mut self, token: &str) -> bool {
        self.skip_space();
        match self.rest.strip_prefix(token) {
            Some(remainder) => {
                self.rest = remainder;
                true
            }
            None => false,
        }
    }

    fn parse_union(&mut self) -> Result<LuaType, TypeError> {
        let mut options = vec![self.parse_intersection()?];
        while self.eat("|") {
            options.push(self.parse_intersection()?);
        }
        if options.len() == 1 {
            return Ok(options.remove(0));
        }
        Ok(LuaType::Union(options))
    }

    fn parse_intersection(&mut self) -> Result<LuaType, TypeError> {
        let mut parts = vec![self.parse_postfix()?];
        while self.eat("&") {
            parts.push(self.parse_postfix()?);
        }
        if parts.len() == 1 {
            return Ok(parts.remove(0));
        }
        Ok(LuaType::Intersection(parts))
    }

    fn parse_postfix(&mut self) -> Result<LuaType, TypeError> {
        let mut ty = self.parse_primary()?;
        loop {
            if self.eat("[]") {
                ty = LuaType::Array(Box::new(ty));
            } else if self.eat("?") {
                ty = LuaType::Optional(Box::new(ty));
            } else {
                return Ok(ty);
            }
        }
    }

    fn parse_primary(&mut self) -> Result<LuaType, TypeError> {
        self.skip_space();
        if self.eat("{") {
            return self.parse_record();
        }
        if self.rest.starts_with("...") {
            self.rest = &self.rest[3..];
            return Ok(LuaType::Variadic(Box::new(self.parse_postfix()?)));
        }
        if self.eat("(") {
            return self.parse_parenthesised();
        }

        let name = self.parse_name()?;
        match name.as_str() {
            "any" => Ok(LuaType::Any),
            "nil" | "null" | "void" => Ok(LuaType::Nil),
            "boolean" | "bool" => Ok(LuaType::Boolean),
            "number" | "integer" | "int" | "float" => Ok(LuaType::Number),
            "string" | "str" => Ok(LuaType::String),
            "array" | "list" => {
                if !self.eat("<") {
                    // A bare `array` is a table of anything.
                    return Ok(LuaType::Array(Box::new(LuaType::Any)));
                }
                let item = self.parse_union()?;
                if !self.eat(">") {
                    return Err(self.fail("expected '>' to close the element type"));
                }
                Ok(LuaType::Array(Box::new(item)))
            }
            "table" | "object" | "map" => {
                if !self.eat("<") {
                    return Ok(LuaType::Map(
                        Box::new(LuaType::String),
                        Box::new(LuaType::Any),
                    ));
                }
                let key = self.parse_union()?;
                if !self.eat(",") {
                    return Err(self.fail("expected ',' between the key and value types"));
                }
                let value = self.parse_union()?;
                if !self.eat(">") {
                    return Err(self.fail("expected '>' to close the table type"));
                }
                Ok(LuaType::Map(Box::new(key), Box::new(value)))
            }
            other => Ok(LuaType::Named(other.to_string())),
        }
    }

    /// `(` has two meanings: a function's parameter list, and grouping. Only
    /// the `->` after the closing paren tells them apart, so both are parsed
    /// here.
    fn parse_parenthesised(&mut self) -> Result<LuaType, TypeError> {
        let mut params = Vec::new();
        let mut grouped: Option<LuaType> = None;

        if !self.eat(")") {
            loop {
                self.skip_space();
                // `name: type`, or a bare type when this turns out to be a
                // grouping rather than a parameter list.
                let checkpoint = self.rest;
                let name = self.parse_param_name().ok();
                let named = name.is_some() && {
                    let optional = self.eat("?");
                    if self.eat(":") {
                        true
                    } else {
                        self.rest = checkpoint;
                        let _ = optional;
                        false
                    }
                };

                if named {
                    let name = name.expect("a named parameter has a name");
                    let ty = self.parse_union()?;
                    params.push(Param {
                        name,
                        ty,
                        description: None,
                        optional: false,
                    });
                } else {
                    self.rest = checkpoint;
                    let ty = self.parse_union()?;
                    if params.is_empty() {
                        grouped = Some(ty.clone());
                    }
                    // `...T` carries no name in a type position, and
                    // `Signature::to_luau` renders the name VARIADIC as the
                    // bare `...T` Luau wants. Give a parsed variadic any
                    // other name and it renders `arg1: ...any`, which is a
                    // parse error that takes the whole declarations file
                    // down. `host_api::unsigned()` never hit this because it
                    // builds the parameter by hand rather than parsing one.
                    // `to_luau` writes the `...` itself for a parameter named
                    // VARIADIC, so the parameter holds the ELEMENT type. Left
                    // wrapped, `...any` renders `......any`.
                    let (name, ty) = match ty {
                        LuaType::Variadic(inner) => (VARIADIC.to_string(), *inner),
                        ty => (format!("arg{}", params.len() + 1), ty),
                    };
                    params.push(Param {
                        name,
                        ty,
                        description: None,
                        optional: false,
                    });
                }

                if self.eat(",") {
                    continue;
                }
                if self.eat(")") {
                    break;
                }
                return Err(self.fail("expected ',' or ')' in a parameter list"));
            }
        }

        if !self.eat("->") {
            // A grouping: `(T)`. Anything else with no arrow is a mistake.
            return match grouped {
                Some(ty) if params.len() == 1 => Ok(ty),
                _ => Err(self.fail("expected '->' after a parameter list")),
            };
        }

        let returns = self.parse_returns()?;
        // A trailing `?` on a parameter belongs to its type, and
        // `LuaType::Optional` already carries it.
        for param in &mut params {
            param.optional = matches!(param.ty, LuaType::Optional(_));
        }
        Ok(LuaType::Function(Box::new(Signature { params, returns })))
    }

    /// What follows `->`: `()`, one type, or `(A, B)`.
    fn parse_returns(&mut self) -> Result<Vec<LuaType>, TypeError> {
        self.skip_space();
        if self.rest.starts_with("()") {
            self.rest = &self.rest[2..];
            return Ok(Vec::new());
        }
        if self.eat("(") {
            let mut returns = vec![self.parse_union()?];
            while self.eat(",") {
                returns.push(self.parse_union()?);
            }
            if !self.eat(")") {
                return Err(self.fail("expected ')' to close a return list"));
            }
            return Ok(returns);
        }
        Ok(vec![self.parse_union()?])
    }

    fn parse_record(&mut self) -> Result<LuaType, TypeError> {
        // `{ [K]: V }` — an index signature, which is how Luau writes a map.
        self.skip_space();
        if self.eat("[") {
            let key = self.parse_union()?;
            if !self.eat("]") {
                return Err(self.fail("expected ']' to close an index signature"));
            }
            if !self.eat(":") {
                return Err(self.fail("expected ':' after an index signature"));
            }
            let value = self.parse_union()?;
            if !self.eat("}") {
                return Err(self.fail("expected '}' to close a table type"));
            }
            return Ok(LuaType::Map(Box::new(key), Box::new(value)));
        }

        // `{ T }` — an array. Told apart from a record by what follows the
        // first token: a record's is `:`.
        let checkpoint = self.rest;
        if !self.rest.starts_with('}') {
            let element = self.parse_union();
            let is_array = element.is_ok() && {
                self.skip_space();
                self.rest.starts_with('}')
            };
            if is_array {
                self.rest = &self.rest[1..];
                return Ok(LuaType::Array(Box::new(element.expect("checked"))));
            }
            self.rest = checkpoint;
        }

        let mut fields = Vec::new();
        loop {
            self.skip_space();
            if self.eat("}") {
                return Ok(LuaType::Record(fields));
            }
            let name = self.parse_name()?;
            let optional = self.eat("?");
            if !self.eat(":") {
                return Err(self.fail(format!("expected ':' after the field '{name}'")));
            }
            let ty = self.parse_union()?;
            fields.push(Field {
                name,
                ty: if optional {
                    LuaType::Optional(Box::new(ty))
                } else {
                    ty
                },
                description: None,
            });
            if self.eat(",") || self.eat(";") {
                continue;
            }
            if self.eat("}") {
                return Ok(LuaType::Record(fields));
            }
            return Err(self.fail("expected ',' or '}' after a field"));
        }
    }

    /// A PARAMETER name: an identifier, and never a dotted one.
    ///
    /// `parse_name` allows dots, because a type name may be dotted. Reusing
    /// it here read `...children: any` as a parameter literally named
    /// `...children`, which rendered back verbatim — and a named variadic is
    /// a Luau syntax error that takes the whole generated file down. Luau has
    /// no syntax for naming a variadic; only its element type survives.
    fn parse_param_name(&mut self) -> Result<String, TypeError> {
        self.skip_space();
        let end = self
            .rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
            .unwrap_or(self.rest.len());
        if end == 0 {
            return Err(self.fail("expected a parameter name"));
        }
        let (name, remainder) = self.rest.split_at(end);
        self.rest = remainder;
        Ok(name.to_string())
    }

    fn parse_name(&mut self) -> Result<String, TypeError> {
        self.skip_space();
        let end = self
            .rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
            .unwrap_or(self.rest.len());
        if end == 0 {
            return Err(self.fail(if self.rest.is_empty() {
                "the declaration is empty".to_string()
            } else {
                format!("expected a type name at '{}'", self.rest)
            }));
        }
        let (name, remainder) = self.rest.split_at(end);
        self.rest = remainder;
        Ok(name.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(declaration: &str) -> LuaType {
        LuaType::parse(declaration).unwrap_or_else(|e| panic!("{e}"))
    }

    #[test]
    fn the_primitives_parse_and_render() {
        for (declaration, luau) in [
            ("string", "string"),
            ("number", "number"),
            ("boolean", "boolean"),
            ("any", "any"),
        ] {
            assert_eq!(parse(declaration).to_luau(), luau);
        }
    }

    /// The shapes the old label matching silently rendered as `string`.
    #[test]
    fn the_compound_forms_survive_the_round_trip() {
        assert_eq!(parse("string[]").to_luau(), "{ string }");
        assert_eq!(parse("array<number>").to_luau(), "{ number }");
        assert_eq!(parse("string?").to_luau(), "string?");
        assert_eq!(
            parse("table<string, number>").to_luau(),
            "{ [string]: number }"
        );
        assert_eq!(parse("string|number").to_luau(), "string | number");
        assert_eq!(
            parse("{ name: string, count: number? }").to_luau(),
            "{ name: string, count: number? }"
        );
        assert_eq!(parse("string[][]").to_luau(), "{ { string } }");
    }

    #[test]
    fn an_unknown_name_is_a_named_type_not_a_refusal() {
        assert_eq!(parse("KilnEntry"), LuaType::Named("KilnEntry".to_string()));
    }

    /// A variadic cannot be NAMED. Luau has no syntax for it, so a
    /// declaration that tries takes the whole generated file down — the
    /// analyzer answers `Expected ')' … got ':'` and every `cru.*` call in
    /// every plugin becomes an unknown global at once.
    ///
    /// The parser used to accept it, because a parameter name was read with
    /// the TYPE-name rule, which allows dots: `...children` read as one
    /// parameter named `...children` and rendered back verbatim.
    #[test]
    fn a_named_variadic_is_refused() {
        let err = LuaType::parse("(props: any?, ...children: any) -> any")
            .expect_err("a named variadic must be refused");
        assert_eq!(err.declaration, "(props: any?, ...children: any) -> any");
    }

    /// A parsed variadic parameter renders as `...T`, never `arg1: ...T`.
    ///
    /// `(arg1: ...any)` is a Luau parse error, and one bad parameter takes
    /// the whole generated declarations file down. `cru.oil.col` is the first
    /// function to declare a variadic by parsing one, rather than by building
    /// the parameter in Rust the way `host_api::unsigned()` does.
    #[test]
    fn a_parsed_variadic_keeps_no_parameter_name() {
        assert_eq!(parse("(...any) -> any").to_luau(), "(...any) -> any");
        assert_eq!(
            parse("(condition: boolean, ...any) -> Node").to_luau(),
            "(condition: boolean, ...any) -> Node"
        );
    }

    /// A declaration the model cannot read is an error the author sees, not a
    /// silent downgrade to `string`.
    #[test]
    fn a_malformed_declaration_is_refused_with_its_text() {
        for declaration in ["", "array<string", "{ name string }", "table<string>"] {
            let err = LuaType::parse(declaration).expect_err("must refuse");
            assert_eq!(err.declaration, declaration);
            assert!(!err.reason.is_empty(), "the refusal says why");
        }
    }

    #[test]
    fn an_array_schema_carries_its_element_type() {
        let schema = parse("string[]").to_json_schema();
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "string");
    }

    #[test]
    fn a_record_schema_lists_the_required_fields() {
        let schema = parse("{ query: string, limit: number? }").to_json_schema();
        assert_eq!(schema["properties"]["query"]["type"], "string");
        assert_eq!(schema["properties"]["limit"]["type"], "number");
        assert_eq!(schema["required"], json!(["query"]));
    }

    #[test]
    fn a_union_schema_is_any_of() {
        let schema = parse("string|number").to_json_schema();
        assert_eq!(schema["anyOf"][0]["type"], "string");
        assert_eq!(schema["anyOf"][1]["type"], "number");
    }

    #[test]
    fn a_signature_renders_both_ways() {
        let signature = Signature {
            params: vec![
                Param {
                    name: "query".to_string(),
                    ty: LuaType::String,
                    description: Some("what to search for".to_string()),
                    optional: false,
                },
                Param {
                    name: "limit".to_string(),
                    ty: LuaType::Number,
                    description: None,
                    optional: true,
                },
            ],
            returns: vec![LuaType::Array(Box::new(LuaType::Named(
                "SearchHit".to_string(),
            )))],
        };

        assert_eq!(
            signature.to_luau(),
            "(query: string, limit: number?) -> { SearchHit }"
        );
        let schema = signature.to_input_schema();
        assert_eq!(schema["properties"]["query"]["type"], "string");
        assert_eq!(
            schema["properties"]["query"]["description"],
            "what to search for"
        );
        assert_eq!(schema["required"], json!(["query"]));
    }

    /// A variadic is `...any`, never `...: any`. The named form is a parse
    /// error, and every unsigned host function renders through this path — so
    /// getting it wrong makes the whole declarations file unreadable.
    #[test]
    fn a_variadic_parameter_renders_without_a_name() {
        let signature = Signature {
            params: vec![Param {
                name: VARIADIC.to_string(),
                ty: LuaType::Any,
                description: None,
                optional: false,
            }],
            returns: vec![LuaType::Any],
        };
        assert_eq!(signature.to_luau(), "(...any) -> any");
    }

    /// Two accepted call shapes, or a table that is also callable: Luau
    /// spells both as an intersection.
    #[test]
    fn an_intersection_renders_every_part() {
        let first = Signature {
            params: vec![Param {
                name: "event".to_string(),
                ty: LuaType::String,
                description: None,
                optional: false,
            }],
            returns: Vec::new(),
        };
        let second = Signature {
            params: vec![Param {
                name: "count".to_string(),
                ty: LuaType::Number,
                description: None,
                optional: false,
            }],
            returns: Vec::new(),
        };
        let ty = LuaType::Intersection(vec![
            LuaType::Function(Box::new(first)),
            LuaType::Function(Box::new(second)),
        ]);
        assert_eq!(
            ty.to_luau(),
            "((event: string) -> ()) & ((count: number) -> ())"
        );
    }

    /// A function with nothing to say renders `()`, not `nil`: a caller that
    /// assigns the result of a void call is a type error worth having.
    #[test]
    fn a_void_signature_renders_as_unit() {
        let signature = Signature::default();
        assert_eq!(signature.to_luau(), "() -> ()");
    }

    mod from_json_schema {
        use super::*;

        #[test]
        fn a_ref_becomes_a_named_type() {
            let ty =
                LuaType::from_json_schema(&json!({ "$ref": "#/components/schemas/ToolRender" }));
            assert_eq!(ty, LuaType::Named("ToolRender".to_string()));
        }

        /// `utoipa`'s spelling of `Option<Ref>`: two branches, one of them
        /// exactly `{"type": "null"}`, is optional — not a two-way choice.
        #[test]
        fn one_of_null_and_a_ref_is_optional_not_a_union() {
            let ty = LuaType::from_json_schema(&json!({
                "oneOf": [
                    { "type": "null" },
                    { "$ref": "#/components/schemas/ToolRender" }
                ]
            }));
            assert_eq!(
                ty,
                LuaType::Optional(Box::new(LuaType::Named("ToolRender".to_string())))
            );
        }

        /// A real two-way choice (no `null` branch) stays a union.
        #[test]
        fn one_of_two_refs_is_a_union() {
            let ty = LuaType::from_json_schema(&json!({
                "oneOf": [
                    { "$ref": "#/components/schemas/A" },
                    { "$ref": "#/components/schemas/B" }
                ]
            }));
            assert_eq!(
                ty,
                LuaType::Union(vec![
                    LuaType::Named("A".to_string()),
                    LuaType::Named("B".to_string())
                ])
            );
        }

        /// `utoipa`'s spelling of `Option<Primitive>`: a two-entry `type`
        /// array, not a `oneOf`.
        #[test]
        fn a_nullable_primitive_type_array_is_optional() {
            let ty = LuaType::from_json_schema(&json!({ "type": ["string", "null"] }));
            assert_eq!(ty, LuaType::Optional(Box::new(LuaType::String)));
        }

        #[test]
        fn a_string_enum_becomes_a_literal_union() {
            let ty = LuaType::from_json_schema(&json!({
                "type": "string",
                "enum": ["model", "mode", "context_strategy"]
            }));
            assert_eq!(
                ty,
                LuaType::Union(vec![
                    LuaType::Literal("model".to_string()),
                    LuaType::Literal("mode".to_string()),
                    LuaType::Literal("context_strategy".to_string()),
                ])
            );
            assert_eq!(ty.to_luau(), "\"model\" | \"mode\" | \"context_strategy\"");
        }

        #[test]
        fn a_required_and_an_optional_field_are_told_apart() {
            let ty = LuaType::from_json_schema(&json!({
                "type": "object",
                "required": ["kind"],
                "properties": {
                    "kind": { "type": "string" },
                    "tool": { "type": "string" }
                }
            }));
            let LuaType::Record(fields) = ty else {
                panic!("expected a record");
            };
            let kind = fields.iter().find(|f| f.name == "kind").unwrap();
            let tool = fields.iter().find(|f| f.name == "tool").unwrap();
            assert_eq!(kind.ty, LuaType::String);
            assert_eq!(tool.ty, LuaType::Optional(Box::new(LuaType::String)));
        }

        #[test]
        fn additional_properties_becomes_a_map() {
            let ty = LuaType::from_json_schema(&json!({
                "type": "object",
                "additionalProperties": { "type": "number" }
            }));
            assert_eq!(
                ty,
                LuaType::Map(Box::new(LuaType::String), Box::new(LuaType::Number))
            );
        }

        #[test]
        fn an_array_carries_its_element_type() {
            let ty = LuaType::from_json_schema(&json!({
                "type": "array",
                "items": { "type": "boolean" }
            }));
            assert_eq!(ty, LuaType::Array(Box::new(LuaType::Boolean)));
        }

        /// `serde_json::Value`'s own schema — no `type`, no `properties` —
        /// is arbitrary JSON, honestly `any`.
        #[test]
        fn an_untyped_schema_is_any() {
            assert_eq!(LuaType::from_json_schema(&json!({})), LuaType::Any);
            assert_eq!(
                LuaType::from_json_schema(&json!({ "description": "arbitrary" })),
                LuaType::Any
            );
        }

        /// `LuaType::of_schema` reads a real `utoipa` schema end to end, not
        /// just the hand-built fixtures above.
        #[test]
        fn of_schema_reads_a_real_derived_type() {
            let ty = LuaType::of_schema::<crucible_core::types::CanonicalToolCall>();
            let LuaType::Record(fields) = ty else {
                panic!("expected a record");
            };
            let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
            assert!(names.contains(&"kind"));
            assert!(names.contains(&"tool"));
            let kind = fields.iter().find(|f| f.name == "kind").unwrap();
            // `kind` is in `CanonicalToolCall`'s `required`.
            assert_eq!(kind.ty, LuaType::String);
        }
    }
}
