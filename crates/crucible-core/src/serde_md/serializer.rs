//! Core Serializer implementation for Markdown output

use super::{Error, Result};
use serde::ser::{self, Serialize};
use std::collections::BTreeMap;
use std::fmt::Write;

/// Serialize a value to Markdown string
pub fn to_string<T: Serialize>(value: &T) -> Result<String> {
    let mut serializer = Serializer::new();
    value.serialize(&mut serializer)?;
    Ok(serializer.output)
}

/// Serialize a value to pretty Markdown string (same as to_string for now)
pub fn to_string_pretty<T: Serialize>(value: &T) -> Result<String> {
    to_string(value)
}

/// The Markdown serializer
pub struct Serializer {
    pub(crate) output: String,
}

impl Serializer {
    pub fn new() -> Self {
        Self {
            output: String::new(),
        }
    }

    /// Get the output string
    pub fn into_output(self) -> String {
        self.output
    }
}

impl Default for Serializer {
    fn default() -> Self {
        Self::new()
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
        write!(self.output, "`<{} bytes>`", v.len())?;
        Ok(())
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

/// Serializer for structs - collects fields, then renders in end()
pub struct StructSerializer<'a> {
    ser: &'a mut Serializer,
    name: &'static str,
    fields: BTreeMap<&'static str, String>,
}

impl StructSerializer<'_> {
    /// Get the struct/variant name
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Get collected fields
    pub fn fields(&self) -> &BTreeMap<&'static str, String> {
        &self.fields
    }

    /// Get the effective variant name (from "type" field or struct name)
    pub fn variant_name(&self) -> &str {
        self.fields
            .get("type")
            .map(|s| s.as_str())
            .unwrap_or(self.name)
    }
}

impl ser::SerializeStruct for StructSerializer<'_> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<()> {
        // Serialize value to a temporary string
        let mut value_ser = Serializer::new();
        value.serialize(&mut value_ser)?;
        self.fields.insert(key, value_ser.output);
        Ok(())
    }

    fn end(self) -> Result<()> {
        // Default rendering: key-value pairs
        // Subclasses/wrappers can override by checking variant_name()
        let variant = self
            .fields
            .get("type")
            .map(|s| s.as_str())
            .unwrap_or(self.name);

        writeln!(self.ser.output, "### {variant}")?;
        for (key, value) in &self.fields {
            if *key != "type" {
                writeln!(self.ser.output, "**{key}**: {value}")?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_primitives() {
        assert_eq!(to_string(&42i32).unwrap(), "42");
        assert_eq!(to_string(&true).unwrap(), "true");
        assert_eq!(to_string(&"hello").unwrap(), "hello");
    }

    #[test]
    fn test_simple_struct() {
        #[derive(serde::Serialize)]
        struct Point {
            x: i32,
            y: i32,
        }

        let p = Point { x: 10, y: 20 };
        let md = to_string(&p).unwrap();

        assert!(md.contains("### Point"));
        assert!(md.contains("**x**: 10"));
        assert!(md.contains("**y**: 20"));
    }

    #[test]
    fn test_tagged_enum() {
        #[derive(serde::Serialize)]
        #[serde(tag = "type", rename_all = "snake_case")]
        enum Event {
            Message { content: String },
            Error { code: i32 },
        }

        let event = Event::Message {
            content: "Hello".into(),
        };
        let md = to_string(&event).unwrap();

        // The variant comes through as "type" field
        assert!(md.contains("### message") || md.contains("### Event"));
        assert!(md.contains("**content**: Hello"));
    }

    #[test]
    fn test_sequence() {
        let items = vec!["apple", "banana", "cherry"];
        let md = to_string(&items).unwrap();

        assert!(md.contains("- apple"));
        assert!(md.contains("- banana"));
        assert!(md.contains("- cherry"));
    }
}
