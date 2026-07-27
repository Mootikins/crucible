//! [JSON Canvas 1.0](https://jsoncanvas.org) documents — the `.canvas` files
//! Obsidian writes for its infinite-canvas view.
//!
//! # Forward compatibility is load-bearing
//!
//! Obsidian itself, and more aggressively its plugin ecosystem, write keys that
//! are not in the spec (Advanced Canvas stores `styleAttributes`; per-node zoom
//! breakpoints show up too). A parser that models only the spec and
//! re-serializes from that model **silently destroys those keys on save** — the
//! user opens a canvas in Crucible, moves one card, and loses styling authored
//! elsewhere.
//!
//! Every type here therefore carries a `#[serde(flatten)] extra` map, and the
//! round-trip tests prove unknown keys survive at every level. Treat those as a
//! contract, not a nicety: they are the only thing standing between us and a
//! data-loss bug nobody notices until their canvas looks wrong.
//!
//! # What is *not* modelled here
//!
//! Containment — whether a `file` node is allowed to point where it points —
//! lives in [`containment`], because judging it needs a kiln root and this
//! module is deliberately pure.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub mod containment;

#[cfg(test)]
mod tests;

/// A parsed `.canvas` document.
///
/// Both arrays are optional in the spec; a canvas with neither is legal and
/// means "empty". Node order carries meaning — it *is* the z-order, first
/// lowest — so this is a `Vec`, never a set or map.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Canvas {
    // Always emitted, even when empty: Obsidian writes a fresh canvas as
    // `{"nodes":[],"edges":[]}`, and skipping empty arrays would silently
    // rewrite every empty canvas to `{}` on first save.
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Canvas {
    /// Parse a `.canvas` document.
    pub fn parse(source: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(source)
    }

    /// Serialize to the on-disk form Obsidian itself writes.
    ///
    /// Not `serde_json::to_string_pretty`. Obsidian uses tab indentation with
    /// one compact object per line and a specific key order, so a generic
    /// pretty-printer turns a 25-line canvas into 60 changed lines on the first
    /// save. In a vault under git or Obsidian Sync that is a whole-file diff for
    /// moving one card, and the two tools then reformat the file back and forth
    /// on every alternating edit.
    ///
    /// Matching the canonical layout instead means saving an untouched canvas
    /// produces no diff at all — verified byte-for-byte against the sample in
    /// the spec repository.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        // An EMPTY array is written compactly (`"edges":[]`) — a real vault
        // canvas with no edges proved it, and expanding it would change two
        // lines of every such file on first save.
        fn array<T>(
            out: &mut String,
            key: &str,
            items: &[T],
            render: impl Fn(&T) -> Result<String, serde_json::Error>,
        ) -> Result<(), serde_json::Error> {
            out.push('\t');
            out.push('"');
            out.push_str(key);
            out.push_str("\":[");
            if items.is_empty() {
                out.push(']');
                return Ok(());
            }
            out.push('\n');
            for (i, item) in items.iter().enumerate() {
                out.push_str("\t\t");
                out.push_str(&render(item)?);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str("\t]");
            Ok(())
        }

        let mut out = String::from("{\n");
        array(&mut out, "nodes", &self.nodes, |n| n.to_canonical_json())?;
        out.push_str(",\n");
        array(&mut out, "edges", &self.edges, serde_json::to_string)?;

        // Keys outside the spec keep the same one-per-line shape.
        for (key, value) in &self.extra {
            out.push_str(",\n\t");
            out.push_str(&serde_json::to_string(key)?);
            out.push(':');
            out.push_str(&serde_json::to_string(value)?);
        }
        out.push_str("\n}");
        Ok(out)
    }

    /// Every `file` node's path, in document order. A path may repeat — two
    /// cards can reference one note, which is not a mistake to dedupe here.
    pub fn file_paths(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().filter_map(|node| node.file_path())
    }

    /// Look up a node by id. Ids are unique per the spec, but nothing enforces
    /// that in a hand-edited file, so this returns the first match.
    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Edges whose endpoints are both `file` nodes, as
    /// `(from_path, to_path, edge)`.
    ///
    /// A deliberate, directional, optionally labelled relation between two
    /// notes — signal the wikilink graph cannot express.
    ///
    /// Currently consumed by the canvas renderer only. These relations are NOT
    /// stored in the link index: `note_links` is keyed by a single source path,
    /// so an A→B relation authored by a third document needs its own table, and
    /// `GraphLink` carries no label. See `docs/Meta/Analysis/Canvas.md`.
    pub fn note_relations(&self) -> impl Iterator<Item = (&str, &str, &Edge)> {
        self.edges.iter().filter_map(move |edge| {
            let from = self.node(&edge.from_node)?.file_path()?;
            let to = self.node(&edge.to_node)?.file_path()?;
            Some((from, to, edge))
        })
    }
}

/// Geometry and identity shared by every node type.
///
/// The spec models nodes as one object with a `type` discriminant. Reproducing
/// that by flattening both `kind` and `extra` does not work: serde's flattened
/// map also captures the keys the flattened enum reads, so `type` (and each
/// variant's own fields) would land in `extra` too and be written out twice.
/// The document still parses — JSON takes the last duplicate — but re-parsing
/// our own output fails with `duplicate field`, which is how the idempotency
/// test caught it.
///
/// So the wire form is handled explicitly by [`RawNode`], which splits the
/// type-specific keys out of the catch-all on the way in and merges them back
/// on the way out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "RawNode", into = "RawNode")]
pub struct Node {
    pub id: String,
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    pub color: Option<Color>,
    pub kind: NodeKind,
    /// Keys outside the spec, preserved verbatim.
    pub extra: Map<String, Value>,
}

/// The on-disk shape of a node: spec-common fields, the `type` tag, and one
/// undifferentiated bag for everything else.
///
/// Field order here IS the emitted key order, and it is chosen to match what
/// Obsidian writes so that saving an untouched canvas produces no diff. The
/// type-specific keys live in `rest`, which is why they land between `type` and
/// the geometry for `file`/`text`/`link` — and why a group's `label`, which
/// Obsidian writes *after* the geometry, is re-inserted there on the way out.
#[derive(Serialize, Deserialize)]
struct RawNode {
    id: String,
    #[serde(rename = "type")]
    node_type: String,
    #[serde(flatten)]
    rest: Map<String, Value>,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<Color>,
}

/// Pull a key out of the bag, erroring if it is absent — the spec's required
/// per-type fields.
fn required(rest: &mut Map<String, Value>, key: &str, node_type: &str) -> Result<Value, String> {
    rest.remove(key)
        .ok_or_else(|| format!("`{node_type}` node is missing required field `{key}`"))
}

fn as_string(value: Value, key: &str) -> Result<String, String> {
    match value {
        Value::String(s) => Ok(s),
        other => Err(format!("`{key}` must be a string, got {other}")),
    }
}

fn optional<T: serde::de::DeserializeOwned>(
    rest: &mut Map<String, Value>,
    key: &str,
) -> Result<Option<T>, String> {
    match rest.remove(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|e| format!("`{key}`: {e}")),
    }
}

impl TryFrom<RawNode> for Node {
    type Error = String;

    fn try_from(raw: RawNode) -> Result<Self, Self::Error> {
        let RawNode {
            id,
            x,
            y,
            width,
            height,
            color,
            node_type,
            mut rest,
        } = raw;

        let kind = match node_type.as_str() {
            "text" => NodeKind::Text {
                text: as_string(required(&mut rest, "text", "text")?, "text")?,
            },
            "file" => NodeKind::File {
                file: as_string(required(&mut rest, "file", "file")?, "file")?,
                subpath: optional(&mut rest, "subpath")?,
            },
            "link" => NodeKind::Link {
                url: as_string(required(&mut rest, "url", "link")?, "url")?,
            },
            "group" => NodeKind::Group {
                label: optional(&mut rest, "label")?,
                background: optional(&mut rest, "background")?,
                background_style: optional(&mut rest, "backgroundStyle")?,
            },
            other => return Err(format!("unknown canvas node type `{other}`")),
        };

        Ok(Node {
            id,
            x,
            y,
            width,
            height,
            color,
            kind,
            extra: rest,
        })
    }
}

impl From<Node> for RawNode {
    fn from(node: Node) -> Self {
        let Node {
            id,
            x,
            y,
            width,
            height,
            color,
            kind,
            mut extra,
        } = node;

        // Spec fields are written back into the bag so the emitted object has
        // exactly one occurrence of each key.
        let mut put = |key: &str, value: Value| {
            if !value.is_null() {
                extra.insert(key.to_string(), value);
            }
        };

        let node_type = match kind {
            NodeKind::Text { text } => {
                put("text", Value::String(text));
                "text"
            }
            NodeKind::File { file, subpath } => {
                put("file", Value::String(file));
                put("subpath", subpath.map(Value::String).unwrap_or(Value::Null));
                "file"
            }
            NodeKind::Link { url } => {
                put("url", Value::String(url));
                "link"
            }
            NodeKind::Group {
                label,
                background,
                background_style,
            } => {
                put("label", label.map(Value::String).unwrap_or(Value::Null));
                put(
                    "background",
                    background.map(Value::String).unwrap_or(Value::Null),
                );
                put(
                    "backgroundStyle",
                    background_style
                        .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
                        .unwrap_or(Value::Null),
                );
                "group"
            }
        };

        RawNode {
            id,
            x,
            y,
            width,
            height,
            color,
            node_type: node_type.to_string(),
            rest: extra,
        }
    }
}

impl Node {
    /// Serialize one node with Obsidian's key order.
    ///
    /// Built as an ordered list rather than a struct because the order is not
    /// uniform: `file`, `text` and `link` put their content key immediately
    /// after `type`, while a `group` puts `label` *after* the geometry. Values
    /// go through `serde_json` so escaping stays correct.
    fn to_canonical_json(&self) -> Result<String, serde_json::Error> {
        let mut fields: Vec<(String, Value)> = vec![
            ("id".into(), Value::String(self.id.clone())),
            ("type".into(), Value::String(self.type_name().into())),
        ];

        let mut geometry_first = Vec::new();
        let mut geometry_last = Vec::new();
        match &self.kind {
            NodeKind::Text { text } => {
                geometry_first.push(("text".into(), Value::String(text.clone())));
            }
            NodeKind::File { file, subpath } => {
                geometry_first.push(("file".into(), Value::String(file.clone())));
                if let Some(sub) = subpath {
                    geometry_first.push(("subpath".into(), Value::String(sub.clone())));
                }
            }
            NodeKind::Link { url } => {
                geometry_first.push(("url".into(), Value::String(url.clone())));
            }
            NodeKind::Group {
                label,
                background,
                background_style,
            } => {
                if let Some(label) = label {
                    geometry_last.push(("label".into(), Value::String(label.clone())));
                }
                if let Some(background) = background {
                    geometry_last.push(("background".into(), Value::String(background.clone())));
                }
                if let Some(style) = background_style {
                    geometry_last.push(("backgroundStyle".into(), serde_json::to_value(style)?));
                }
            }
        }

        fields.append(&mut geometry_first);
        fields.push(("x".into(), Value::from(self.x)));
        fields.push(("y".into(), Value::from(self.y)));
        fields.push(("width".into(), Value::from(self.width)));
        fields.push(("height".into(), Value::from(self.height)));
        fields.append(&mut geometry_last);
        if let Some(color) = &self.color {
            fields.push(("color".into(), serde_json::to_value(color)?));
        }
        for (key, value) in &self.extra {
            fields.push((key.clone(), value.clone()));
        }

        let mut out = String::from("{");
        for (i, (key, value)) in fields.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&serde_json::to_string(key)?);
            out.push(':');
            out.push_str(&serde_json::to_string(value)?);
        }
        out.push('}');
        Ok(out)
    }

    fn type_name(&self) -> &'static str {
        match self.kind {
            NodeKind::Text { .. } => "text",
            NodeKind::File { .. } => "file",
            NodeKind::Link { .. } => "link",
            NodeKind::Group { .. } => "group",
        }
    }

    /// The referenced path, for `file` nodes only.
    pub fn file_path(&self) -> Option<&str> {
        match &self.kind {
            NodeKind::File { file, .. } => Some(file.as_str()),
            _ => None,
        }
    }
}

/// The four spec node types.
///
/// Deliberately not `Serialize`/`Deserialize`: the wire mapping lives in
/// [`RawNode`], and a second derived mapping here would be a parallel
/// implementation that could silently drift from it.
#[derive(Debug, Clone, PartialEq)]
pub enum NodeKind {
    /// Plain text with Markdown syntax, stored inline in the canvas.
    Text { text: String },
    /// A reference to a file inside the kiln.
    File {
        file: String,
        /// A heading or block anchor within the file. Always starts with `#`.
        subpath: Option<String>,
    },
    /// An external URL.
    Link { url: String },
    /// A visual container. Membership is *geometric* — the spec has no child
    /// list — so anything asking "what is in this group" computes it from
    /// bounds.
    Group {
        label: Option<String>,
        background: Option<String>,
        background_style: Option<BackgroundStyle>,
    },
}

/// How a group's background image is rendered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackgroundStyle {
    Cover,
    Ratio,
    Repeat,
}

/// A connection between two nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    #[serde(rename = "fromNode")]
    pub from_node: String,
    #[serde(rename = "fromSide", default, skip_serializing_if = "Option::is_none")]
    pub from_side: Option<Side>,
    #[serde(rename = "fromEnd", default, skip_serializing_if = "Option::is_none")]
    pub from_end: Option<End>,
    #[serde(rename = "toNode")]
    pub to_node: String,
    #[serde(rename = "toSide", default, skip_serializing_if = "Option::is_none")]
    pub to_side: Option<Side>,
    #[serde(rename = "toEnd", default, skip_serializing_if = "Option::is_none")]
    pub to_end: Option<End>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Edge {
    /// The rendered arrowhead at the source. Spec default is [`End::None`].
    pub fn from_end_or_default(&self) -> End {
        self.from_end.unwrap_or(End::None)
    }

    /// The rendered arrowhead at the target. Spec default is [`End::Arrow`] —
    /// note this differs from the source default, so an edge with neither key
    /// set is a one-way arrow, not a plain line.
    pub fn to_end_or_default(&self) -> End {
        self.to_end.unwrap_or(End::Arrow)
    }
}

/// Which edge of a node a connection attaches to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Top,
    Right,
    Bottom,
    Left,
}

/// Whether a connection terminates in an arrowhead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum End {
    None,
    Arrow,
}

/// A canvas colour.
///
/// On the wire this is always a JSON string: either `#RRGGBB` or one of the six
/// presets `"1"`–`"6"`. Presets are numeric *strings*, not numbers, and writing
/// them back as numbers makes Obsidian stop recognising them — so this stays a
/// transparent newtype over the raw string rather than an enum that might
/// normalise the spelling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Color(pub String);
