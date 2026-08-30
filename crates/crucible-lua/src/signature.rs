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
        }
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
                let name = self.parse_name().ok();
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
                    params.push(Param {
                        name: format!("arg{}", params.len() + 1),
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
}
