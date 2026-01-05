//! LogEvent-specific Markdown serialization
//!
//! This module extends `crucible_core::serde_md` with domain-specific
//! rendering for LogEvent variants (User, Assistant, ToolCall, etc.).
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

use crate::events::LogEvent;
use crucible_core::serde_md::{Error, Result};
use serde::ser::{self, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;

/// Serialize a LogEvent to Markdown string
pub fn to_string(event: &LogEvent) -> Result<String> {
    let mut serializer = LogEventSerializer::new();
    event.serialize(&mut serializer)?;
    Ok(serializer.output)
}

/// Serialize multiple LogEvents to Markdown
pub fn to_string_seq(events: &[LogEvent]) -> Result<String> {
    let mut output = String::new();
    for event in events {
        output.push_str(&to_string(event)?);
        output.push('\n');
    }
    Ok(output)
}

/// LogEvent-specific Markdown serializer
///
/// Extends core's Serializer with domain-specific rendering for
/// chat event types (User, Assistant, ToolCall, etc.)
pub struct LogEventSerializer {
    output: String,
}

impl LogEventSerializer {
    fn new() -> Self {
        Self {
            output: String::new(),
        }
    }
}

impl<'a> ser::Serializer for &'a mut LogEventSerializer {
    type Ok = ();
    type Error = Error;

    type SerializeSeq = SeqSerializer<'a>;
    type SerializeTuple = SeqSerializer<'a>;
    type SerializeTupleStruct = SeqSerializer<'a>;
    type SerializeTupleVariant = SeqSerializer<'a>;
    type SerializeMap = MapSerializer<'a>;
    type SerializeStruct = StructSerializer<'a>;
    type SerializeStructVariant = StructSerializer<'a>;

    // Primitives delegate to simple string conversion
    fn serialize_bool(self, v: bool) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_i8(self, v: i8) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_i16(self, v: i16) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_i32(self, v: i32) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_i64(self, v: i64) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_u8(self, v: u8) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_u16(self, v: u16) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_u32(self, v: u32) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_u64(self, v: u64) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_f32(self, v: f32) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_f64(self, v: f64) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_char(self, v: char) -> Result<()> {
        write!(self.output, "{v}").map_err(Error::from)
    }

    fn serialize_str(self, v: &str) -> Result<()> {
        self.output.push_str(v);
        Ok(())
    }

    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        write!(self.output, "`<{} bytes>`", v.len()).map_err(Error::from)
    }

    fn serialize_none(self) -> Result<()> {
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
        write!(self.output, "{variant}").map_err(Error::from)
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
        write!(self.output, "**{variant}**: ").map_err(Error::from)?;
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
        write!(self.output, "**{variant}**: ").map_err(Error::from)?;
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

/// Sequence serializer
pub struct SeqSerializer<'a> {
    ser: &'a mut LogEventSerializer,
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

/// Map serializer
pub struct MapSerializer<'a> {
    ser: &'a mut LogEventSerializer,
    key: Option<String>,
}

impl ser::SerializeMap for MapSerializer<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<()> {
        let mut key_ser = LogEventSerializer::new();
        key.serialize(&mut key_ser)?;
        self.key = Some(key_ser.output);
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<()> {
        let key = self.key.take().unwrap_or_default();
        write!(self.ser.output, "**{key}**: ").map_err(Error::from)?;
        value.serialize(&mut *self.ser)?;
        self.ser.output.push('\n');
        Ok(())
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

/// Struct serializer with LogEvent-specific rendering
pub struct StructSerializer<'a> {
    ser: &'a mut LogEventSerializer,
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
        // For JSON fields like "args", preserve JSON format
        let serialized = if key == "args" {
            serde_json::to_string(value).unwrap_or_else(|_| {
                let mut s = LogEventSerializer::new();
                let _ = value.serialize(&mut s);
                s.output
            })
        } else {
            let mut value_ser = LogEventSerializer::new();
            value.serialize(&mut value_ser)?;
            value_ser.output
        };
        self.fields.insert(key, serialized);
        Ok(())
    }

    fn end(self) -> Result<()> {
        let variant = self
            .fields
            .get("type")
            .map(|s| s.as_str())
            .unwrap_or(self.name);

        // Domain-specific rendering for LogEvent variants
        match variant {
            "user" | "User" => render_user(&mut self.ser.output, &self.fields)?,
            "assistant" | "Assistant" => render_assistant(&mut self.ser.output, &self.fields)?,
            "system" | "System" => render_system(&mut self.ser.output, &self.fields)?,
            "tool_call" | "ToolCall" => render_tool_call(&mut self.ser.output, &self.fields)?,
            "tool_result" | "ToolResult" => render_tool_result(&mut self.ser.output, &self.fields)?,
            "error" | "Error" => render_error(&mut self.ser.output, &self.fields)?,
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
                write!(self.ser.output, "{input} in / {output} out").map_err(Error::from)?;
            }
            // Unknown: fall back to generic key-value
            _ => {
                writeln!(self.ser.output, "### {}", self.name).map_err(Error::from)?;
                for (key, value) in &self.fields {
                    if *key != "type" {
                        writeln!(self.ser.output, "**{key}**: {value}").map_err(Error::from)?;
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
    writeln!(output, "## User\n").map_err(Error::from)?;
    if let Some(content) = fields.get("content") {
        writeln!(output, "{content}").map_err(Error::from)?;
    }
    Ok(())
}

fn render_assistant(output: &mut String, fields: &BTreeMap<&str, String>) -> Result<()> {
    let model = fields.get("model").filter(|s| !s.is_empty());

    if let Some(m) = model {
        writeln!(output, "## Assistant ({m})\n").map_err(Error::from)?;
    } else {
        writeln!(output, "## Assistant\n").map_err(Error::from)?;
    }

    if let Some(content) = fields.get("content") {
        writeln!(output, "{content}").map_err(Error::from)?;
    }

    if let Some(tokens) = fields.get("tokens") {
        if !tokens.is_empty() {
            writeln!(output, "\n*Tokens: {tokens}*").map_err(Error::from)?;
        }
    }

    Ok(())
}

fn render_system(output: &mut String, fields: &BTreeMap<&str, String>) -> Result<()> {
    writeln!(output, "<details><summary>System Prompt</summary>\n").map_err(Error::from)?;
    if let Some(content) = fields.get("content") {
        writeln!(output, "{content}").map_err(Error::from)?;
    }
    writeln!(output, "\n</details>").map_err(Error::from)?;
    Ok(())
}

fn render_tool_call(output: &mut String, fields: &BTreeMap<&str, String>) -> Result<()> {
    let name = fields.get("name").map(|s| s.as_str()).unwrap_or("unknown");
    let id = fields.get("id").map(|s| s.as_str()).unwrap_or("?");

    writeln!(output, "### Tool: `{name}` (id: {id})\n").map_err(Error::from)?;

    if let Some(args) = fields.get("args") {
        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(args) {
            writeln!(output, "```json").map_err(Error::from)?;
            writeln!(
                output,
                "{}",
                serde_json::to_string_pretty(&parsed).unwrap_or_else(|_| args.clone())
            )
            .map_err(Error::from)?;
            writeln!(output, "```").map_err(Error::from)?;
        } else {
            writeln!(output, "```\n{args}\n```").map_err(Error::from)?;
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
        writeln!(output, "#### Result (id: {id}) - ERROR\n").map_err(Error::from)?;
        writeln!(output, "```\n{err}\n```").map_err(Error::from)?;
    } else {
        let marker = if truncated { " (truncated)" } else { "" };
        writeln!(output, "#### Result (id: {id}){marker}\n").map_err(Error::from)?;
        if let Some(result) = fields.get("result") {
            writeln!(output, "```\n{result}\n```").map_err(Error::from)?;
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

    writeln!(output, "> **{severity}:** {message}").map_err(Error::from)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::TokenUsage;

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
        let user_pos = md.find("## User").unwrap();
        let asst_pos = md.find("## Assistant").unwrap();
        assert!(user_pos < asst_pos);
    }
}
