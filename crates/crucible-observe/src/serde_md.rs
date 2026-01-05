//! Serde Serializer for Markdown output
//!
//! This module provides a custom serde Serializer that outputs Markdown
//! instead of JSON. Types that derive `Serialize` can be rendered to
//! Markdown using the same API as `serde_json`.
//!
//! # Example
//!
//! ```
//! use crucible_observe::{LogEvent, serde_md};
//!
//! let event = LogEvent::user("Hello!");
//! let md = serde_md::to_string(&event).unwrap();
//! assert!(md.contains("## User"));
//! ```
//!
//! # Design
//!
//! The serializer intercepts serde's data model and translates it to Markdown:
//! - Structs with known names (User, Assistant, etc.) get domain-specific formatting
//! - Internally tagged enums (via `#[serde(tag = "type")]`) appear as structs
//! - Unknown types fall back to JSON embedding
//!
//! This design leverages existing `#[derive(Serialize)]` without requiring
//! custom serialization logic per type.

use serde::ser::{self, Serialize};
use std::collections::BTreeMap;
use std::fmt::{self, Display, Write};

/// Errors that can occur during Markdown serialization
#[derive(Debug)]
pub enum Error {
    /// Custom error message
    Message(String),
    /// Formatting error
    Fmt(fmt::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Message(msg) => write!(f, "{msg}"),
            Error::Fmt(e) => write!(f, "format error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: Display>(msg: T) -> Self {
        Error::Message(msg.to_string())
    }
}

impl From<fmt::Error> for Error {
    fn from(e: fmt::Error) -> Self {
        Error::Fmt(e)
    }
}

/// Result type for Markdown serialization
pub type Result<T> = std::result::Result<T, Error>;

/// Serialize a value to Markdown string
pub fn to_string<T: Serialize>(value: &T) -> Result<String> {
    let mut serializer = Serializer::new();
    value.serialize(&mut serializer)?;
    Ok(serializer.output)
}

/// Serialize multiple values to Markdown, one per line
pub fn to_string_seq<T: Serialize>(values: &[T]) -> Result<String> {
    let mut output = String::new();
    for value in values {
        output.push_str(&to_string(value)?);
        output.push('\n');
    }
    Ok(output)
}

/// The Markdown serializer
pub struct Serializer {
    output: String,
}

impl Serializer {
    fn new() -> Self {
        Self {
            output: String::new(),
        }
    }
}

impl<'a> ser::Serializer for &'a mut Serializer {
    type Ok = ();
    type Error = Error;

    // Compound type serializers
    type SerializeSeq = SeqSerializer<'a>;
    type SerializeTuple = SeqSerializer<'a>;
    type SerializeTupleStruct = SeqSerializer<'a>;
    type SerializeTupleVariant = SeqSerializer<'a>;
    type SerializeMap = MapSerializer<'a>;
    type SerializeStruct = StructSerializer<'a>;
    type SerializeStructVariant = StructSerializer<'a>;

    // Primitives - just convert to string
    fn serialize_bool(self, v: bool) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<()> {
        write!(self.output, "{v}")?;
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        self.output.push_str(v);
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        // Render bytes as base64-ish representation
        write!(self.output, "`<{} bytes>`", v.len())?;
        Ok(())
    }

    fn serialize_none(self) -> Result<()> {
        // None produces no output
        Ok(())
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        write!(self.output, "{variant}")?;
        Ok(())
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<()> {
        write!(self.output, "**{variant}**: ")?;
        value.serialize(self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq> {
        Ok(SeqSerializer {
            ser: self,
            first: true,
        })
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Ok(SeqSerializer {
            ser: self,
            first: true,
        })
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Ok(SeqSerializer {
            ser: self,
            first: true,
        })
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        write!(self.output, "**{variant}**: ")?;
        Ok(SeqSerializer {
            ser: self,
            first: true,
        })
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        Ok(MapSerializer {
            ser: self,
            key: None,
        })
    }

    fn serialize_struct(self, name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Ok(StructSerializer {
            ser: self,
            name,
            fields: BTreeMap::new(),
        })
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        Ok(StructSerializer {
            ser: self,
            name: variant,
            fields: BTreeMap::new(),
        })
    }
}

/// Serializer for sequences (Vec, arrays, tuples)
pub struct SeqSerializer<'a> {
    ser: &'a mut Serializer,
    first: bool,
}

impl ser::SerializeSeq for SeqSerializer<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        if !self.first {
            self.ser.output.push('\n');
        }
        self.ser.output.push_str("- ");
        value.serialize(&mut *self.ser)?;
        self.first = false;
        Ok(())
    }

    fn end(self) -> Result<()> {
        if !self.first {
            self.ser.output.push('\n');
        }
        Ok(())
    }
}

impl ser::SerializeTuple for SeqSerializer<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<()> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for SeqSerializer<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<()> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleVariant for SeqSerializer<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<()> {
        ser::SerializeSeq::end(self)
    }
}

/// Serializer for maps (HashMap, BTreeMap)
pub struct MapSerializer<'a> {
    ser: &'a mut Serializer,
    key: Option<String>,
}

impl ser::SerializeMap for MapSerializer<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        // Capture key as string
        let mut key_ser = Serializer::new();
        key.serialize(&mut key_ser)?;
        self.key = Some(key_ser.output);
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        let key = self.key.take().unwrap_or_default();
        write!(self.ser.output, "**{key}**: ")?;
        value.serialize(&mut *self.ser)?;
        self.ser.output.push('\n');
        Ok(())
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

/// Serializer for structs - this is where the magic happens for LogEvent
pub struct StructSerializer<'a> {
    ser: &'a mut Serializer,
    name: &'static str,
    fields: BTreeMap<&'static str, String>,
}

impl ser::SerializeStruct for StructSerializer<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        // For certain fields, preserve JSON format instead of markdown
        // This is needed for serde_json::Value fields like "args"
        let serialized = if key == "args" {
            // Use serde_json to keep JSON structure
            serde_json::to_string(value).unwrap_or_else(|_| {
                let mut s = Serializer::new();
                let _ = value.serialize(&mut s);
                s.output
            })
        } else {
            // Use our markdown serializer
            let mut value_ser = Serializer::new();
            value.serialize(&mut value_ser)?;
            value_ser.output
        };
        self.fields.insert(key, serialized);
        Ok(())
    }

    fn end(self) -> Result<()> {
        // For internally tagged enums, get the variant from "type" field
        // Otherwise use the struct name
        let variant = self
            .fields
            .get("type")
            .map(|s| s.as_str())
            .unwrap_or(self.name);

        // Domain-specific markdown rendering based on variant/struct name
        match variant {
            "user" | "User" => render_user(&mut self.ser.output, &self.fields)?,
            "assistant" | "Assistant" => render_assistant(&mut self.ser.output, &self.fields)?,
            "system" | "System" => render_system(&mut self.ser.output, &self.fields)?,
            "tool_call" | "ToolCall" => render_tool_call(&mut self.ser.output, &self.fields)?,
            "tool_result" | "ToolResult" => render_tool_result(&mut self.ser.output, &self.fields)?,
            "error" | "Error" => render_error(&mut self.ser.output, &self.fields)?,
            // For TokenUsage and other nested types, inline render
            "TokenUsage" => {
                let empty = String::new();
                let input = self
                    .fields
                    .get("in")
                    .or(self.fields.get("input"))
                    .unwrap_or(&empty);
                let output = self
                    .fields
                    .get("out")
                    .or(self.fields.get("output"))
                    .unwrap_or(&empty);
                write!(self.ser.output, "{input} in / {output} out")?;
            }
            // Unknown structs: render as key-value list
            _ => {
                writeln!(self.ser.output, "### {}", self.name)?;
                for (key, value) in &self.fields {
                    if *key != "type" {
                        // Skip the serde tag field
                        writeln!(self.ser.output, "**{key}**: {value}")?;
                    }
                }
            }
        }
        Ok(())
    }
}

impl ser::SerializeStructVariant for StructSerializer<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        ser::SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<()> {
        ser::SerializeStruct::end(self)
    }
}

// Domain-specific renderers for LogEvent variants

fn render_user(output: &mut String, fields: &BTreeMap<&str, String>) -> Result<()> {
    writeln!(output, "## User\n")?;
    if let Some(content) = fields.get("content") {
        writeln!(output, "{content}")?;
    }
    Ok(())
}

fn render_assistant(output: &mut String, fields: &BTreeMap<&str, String>) -> Result<()> {
    let model = fields.get("model").filter(|s| !s.is_empty());

    if let Some(m) = model {
        writeln!(output, "## Assistant ({m})\n")?;
    } else {
        writeln!(output, "## Assistant\n")?;
    }

    if let Some(content) = fields.get("content") {
        writeln!(output, "{content}")?;
    }

    // Token info as italic footnote
    if let Some(tokens) = fields.get("tokens") {
        if !tokens.is_empty() {
            writeln!(output, "\n*Tokens: {tokens}*")?;
        }
    }

    Ok(())
}

fn render_system(output: &mut String, fields: &BTreeMap<&str, String>) -> Result<()> {
    writeln!(output, "<details><summary>System Prompt</summary>\n")?;
    if let Some(content) = fields.get("content") {
        writeln!(output, "{content}")?;
    }
    writeln!(output, "\n</details>")?;
    Ok(())
}

fn render_tool_call(output: &mut String, fields: &BTreeMap<&str, String>) -> Result<()> {
    let name = fields.get("name").map(|s| s.as_str()).unwrap_or("unknown");
    let id = fields.get("id").map(|s| s.as_str()).unwrap_or("?");

    writeln!(output, "### Tool: `{name}` (id: {id})\n")?;

    if let Some(args) = fields.get("args") {
        // Args is already serialized, try to pretty-print JSON
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(args) {
            writeln!(output, "```json")?;
            writeln!(
                output,
                "{}",
                serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| args.clone())
            )?;
            writeln!(output, "```")?;
        } else {
            writeln!(output, "```\n{args}\n```")?;
        }
    }

    Ok(())
}

fn render_tool_result(output: &mut String, fields: &BTreeMap<&str, String>) -> Result<()> {
    let id = fields.get("id").map(|s| s.as_str()).unwrap_or("?");
    let truncated = fields
        .get("truncated")
        .map(|s| s == "true")
        .unwrap_or(false);
    let error = fields.get("error").filter(|s| !s.is_empty());

    if let Some(err) = error {
        writeln!(output, "#### Result (id: {id}) - ERROR\n")?;
        writeln!(output, "```\n{err}\n```")?;
    } else {
        let marker = if truncated { " (truncated)" } else { "" };
        writeln!(output, "#### Result (id: {id}){marker}\n")?;
        if let Some(result) = fields.get("result") {
            writeln!(output, "```\n{result}\n```")?;
        }
    }

    Ok(())
}

fn render_error(output: &mut String, fields: &BTreeMap<&str, String>) -> Result<()> {
    let recoverable = fields
        .get("recoverable")
        .map(|s| s == "true")
        .unwrap_or(false);
    let severity = if recoverable { "Warning" } else { "Error" };
    let message = fields.get("message").map(|s| s.as_str()).unwrap_or("");

    writeln!(output, "> **{severity}:** {message}")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{LogEvent, TokenUsage};

    #[test]
    fn test_user_event() {
        let event = LogEvent::user("Hello, world!");
        let md = to_string(&event).unwrap();

        assert!(md.contains("## User"));
        assert!(md.contains("Hello, world!"));
    }

    #[test]
    fn test_assistant_event() {
        let event = LogEvent::assistant("Hi there!");
        let md = to_string(&event).unwrap();

        assert!(md.contains("## Assistant"));
        assert!(md.contains("Hi there!"));
    }

    #[test]
    fn test_assistant_with_model() {
        let event = LogEvent::assistant_with_model(
            "Response text",
            "claude-3-haiku",
            Some(TokenUsage {
                input: 100,
                output: 50,
            }),
        );
        let md = to_string(&event).unwrap();

        assert!(md.contains("## Assistant (claude-3-haiku)"));
        assert!(md.contains("Response text"));
        assert!(md.contains("Tokens:"));
        assert!(md.contains("100"));
        assert!(md.contains("50"));
    }

    #[test]
    fn test_system_event() {
        let event = LogEvent::system("You are a helpful assistant.");
        let md = to_string(&event).unwrap();

        assert!(md.contains("<details>"));
        assert!(md.contains("System Prompt"));
        assert!(md.contains("You are a helpful assistant."));
    }

    #[test]
    fn test_tool_call_event() {
        let event = LogEvent::tool_call(
            "tc_001",
            "read_file",
            serde_json::json!({"path": "test.rs"}),
        );
        let md = to_string(&event).unwrap();

        assert!(md.contains("### Tool: `read_file`"));
        assert!(md.contains("tc_001"));
        assert!(md.contains("```json"));
        assert!(md.contains("test.rs"));
    }

    #[test]
    fn test_tool_result_event() {
        let event = LogEvent::tool_result("tc_001", "fn main() {}");
        let md = to_string(&event).unwrap();

        assert!(md.contains("#### Result"));
        assert!(md.contains("tc_001"));
        assert!(md.contains("fn main()"));
    }

    #[test]
    fn test_tool_error_event() {
        let event = LogEvent::tool_error("tc_001", "File not found");
        let md = to_string(&event).unwrap();

        assert!(md.contains("ERROR"));
        assert!(md.contains("File not found"));
    }

    #[test]
    fn test_error_recoverable() {
        let event = LogEvent::error("Rate limited", true);
        let md = to_string(&event).unwrap();

        assert!(md.contains("**Warning:**"));
        assert!(md.contains("Rate limited"));
    }

    #[test]
    fn test_error_fatal() {
        let event = LogEvent::error("Connection lost", false);
        let md = to_string(&event).unwrap();

        assert!(md.contains("**Error:**"));
        assert!(md.contains("Connection lost"));
    }

    #[test]
    fn test_sequence_of_events() {
        let events = vec![LogEvent::user("Question"), LogEvent::assistant("Answer")];
        let md = to_string_seq(&events).unwrap();

        assert!(md.contains("## User"));
        assert!(md.contains("## Assistant"));
        // Verify ordering
        let user_pos = md.find("## User").unwrap();
        let asst_pos = md.find("## Assistant").unwrap();
        assert!(user_pos < asst_pos);
    }

    #[test]
    fn test_primitives() {
        assert_eq!(to_string(&42i32).unwrap(), "42");
        assert_eq!(to_string(&true).unwrap(), "true");
        assert_eq!(to_string(&"hello").unwrap(), "hello");
    }
}
