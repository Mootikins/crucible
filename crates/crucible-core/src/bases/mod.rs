//! Obsidian Bases documents and expressions. Unknown view/plugin options round-trip.
//!
//! Parsing a base compiles every expression once. Filters and summary
//! formulas must compile, or the base does not load. A formula that does not
//! compile loads with its error, and each cell that uses it shows that error,
//! as Obsidian does.
mod expression;
pub use expression::{
    js_number_text, BaseValue, BinaryOp, DurationValue, DurationWire, Expr, Function, Namespace,
    UnaryOp, MAX_DEPTH, MAX_NODES,
};

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use strum::{EnumIter, EnumString, IntoStaticStr};

pub type Extra = BTreeMap<String, serde_yaml::Value>;

/// Expression source with its parsed tree. It serializes as the source text.
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    source: String,
    expr: Expr,
}
impl Expression {
    pub fn parse(source: &str) -> anyhow::Result<Self> {
        Ok(Self {
            source: source.to_owned(),
            expr: Expr::parse(source)?,
        })
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn expr(&self) -> &Expr {
        &self.expr
    }
}
impl Serialize for Expression {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.source.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Expression {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let source = String::deserialize(deserializer)?;
        Self::parse(&source).map_err(|e| serde::de::Error::custom(format!("{source}: {e:#}")))
    }
}

/// A formula column. A formula that does not compile keeps its error.
#[derive(Debug, Clone, PartialEq)]
pub struct Formula {
    source: String,
    compiled: Result<Expr, String>,
}
impl Formula {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.to_owned(),
            compiled: Expr::parse(source).map_err(|e| format!("{e:#}")),
        }
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    /// The parsed tree, or the parse error message.
    pub fn expr(&self) -> Result<&Expr, &str> {
        self.compiled.as_ref().map_err(String::as_str)
    }
}
impl Serialize for Formula {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.source.serialize(serializer)
    }
}
impl<'de> Deserialize<'de> for Formula {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::new(&String::deserialize(deserializer)?))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Filter>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub formulas: BTreeMap<String, Formula>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, Property>,
    /// Custom summary formulas. Each one reads the column as `values`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub summaries: BTreeMap<String, Expression>,
    #[serde(default)]
    pub views: Vec<View>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_item_folder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_item_template: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}
impl BaseFile {
    pub fn parse(yaml: &str) -> anyhow::Result<Self> {
        let mut result: Self = serde_yaml::from_str(yaml)?;
        if result.views.is_empty() {
            result.views.push(serde_yaml::from_str(
                "type: table\nname: Table\nsort: [{property: file.name, direction: ASC}]",
            )?);
        }
        for view in &mut result.views {
            for summary in view.summaries.values_mut() {
                summary.resolve(&result.summaries)?;
            }
        }
        Ok(result)
    }
    pub fn to_yaml(&self) -> anyhow::Result<String> {
        Ok(serde_yaml::to_string(self)?)
    }
    pub fn view(&self, name: Option<&str>) -> anyhow::Result<&View> {
        match name {
            Some(name) => self
                .views
                .iter()
                .find(|v| v.name == name)
                .ok_or_else(|| anyhow::anyhow!("Unknown base view: {name}")),
            None => self
                .views
                .first()
                .ok_or_else(|| anyhow::anyhow!("Base has no views")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged, try_from = "RawFilter")]
pub enum Filter {
    Expression(Expression),
    Tree(FilterTree),
}
/// The YAML shape of a filter, before its expressions compile. Keeping the
/// text separate lets a parse error reach the user instead of serde's
/// "did not match any variant".
#[derive(Deserialize)]
#[serde(untagged)]
enum RawFilter {
    Expression(String),
    Tree(FilterTree),
}
impl TryFrom<RawFilter> for Filter {
    type Error = anyhow::Error;
    fn try_from(raw: RawFilter) -> anyhow::Result<Self> {
        Ok(match raw {
            RawFilter::Expression(source) => Self::Expression(
                Expression::parse(&source).map_err(|e| anyhow::anyhow!("{source}: {e:#}"))?,
            ),
            RawFilter::Tree(tree) => Self::Tree(tree),
        })
    }
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterTree {
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Vec<Filter>),
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Property {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(flatten)]
    pub extra: Extra,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    #[serde(rename = "type")]
    pub kind: ViewType,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Filter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub order: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sort: Vec<Sort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_by: Option<GroupBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group_order: Option<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    /// Column property → summary.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub summaries: BTreeMap<String, Summary>,
    #[serde(flatten)]
    pub extra: Extra,
}

/// The built-in summaries of Obsidian 1.14.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, EnumString, IntoStaticStr)]
pub enum SummaryKind {
    Average,
    Min,
    Max,
    Sum,
    Range,
    Median,
    Stddev,
    Earliest,
    Latest,
    Checked,
    Unchecked,
    Empty,
    Filled,
    Unique,
}
impl SummaryKind {
    pub fn name(self) -> &'static str {
        self.into()
    }
}
/// A view's summary: a built-in, or the name of a formula in
/// [`BaseFile::summaries`]. A base formula with a built-in's name wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum Summary {
    Builtin(SummaryKind),
    Custom(String),
}
impl Summary {
    fn resolve(&mut self, custom: &BTreeMap<String, Expression>) -> anyhow::Result<()> {
        let name = String::from(self.clone());
        *self = if custom.contains_key(&name) {
            Self::Custom(name)
        } else {
            Self::Builtin(
                name.parse()
                    .map_err(|_| anyhow::anyhow!("Unknown summary: {name}"))?,
            )
        };
        Ok(())
    }
}
impl From<String> for Summary {
    fn from(name: String) -> Self {
        name.parse().map_or(Self::Custom(name), Self::Builtin)
    }
}
impl From<Summary> for String {
    fn from(summary: Summary) -> Self {
        match summary {
            Summary::Builtin(kind) => kind.name().into(),
            Summary::Custom(name) => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "String", into = "String")]
pub enum ViewType {
    Table,
    Cards,
    List,
    Kanban,
    Other(String),
}
impl From<String> for ViewType {
    fn from(s: String) -> Self {
        match s.as_str() {
            "table" => Self::Table,
            "cards" => Self::Cards,
            "list" => Self::List,
            "kanban" => Self::Kanban,
            _ => Self::Other(s),
        }
    }
}
impl From<ViewType> for String {
    fn from(v: ViewType) -> Self {
        match v {
            ViewType::Table => "table".into(),
            ViewType::Cards => "cards".into(),
            ViewType::List => "list".into(),
            ViewType::Kanban => "kanban".into(),
            ViewType::Other(s) => s,
        }
    }
}
/// A property with a direction. Sorting and grouping have the same shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sort {
    pub property: String,
    pub direction: Direction,
    #[serde(flatten)]
    pub extra: Extra,
}
pub type GroupBy = Sort;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    ASC,
    DESC,
}

impl Serialize for FilterTree {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let (key, filters) = match self {
            Self::And(xs) => ("and", xs),
            Self::Or(xs) => ("or", xs),
            Self::Not(xs) => ("not", xs),
        };
        let mut map = serializer.serialize_map(Some(1))?;
        map.serialize_entry(key, filters)?;
        map.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;
    #[test]
    fn bases_roundtrip_obisidian_yaml_and_unknown_view_options() {
        let yaml = "filters:\n  and:\n    - 'file.hasTag(\"book\")'\n    - not:\n        - 'status == \"done\"'\nformulas:\n  bad: 'missing()'\nsummaries:\n  Spread: 'values.length'\nnewItemFolder: Books\npluginData: {x: [1, two]}\nviews:\n  - type: kanban\n    name: Board\n    groupBy: {property: note.status, direction: ASC}\n    groupOrder: [todo, done]\n    columnWidth: 280\n    summaries: {note.price: Sum, note.size: Spread}\n";
        let base = BaseFile::parse(yaml).unwrap();
        let output = base.to_yaml().unwrap();
        assert_eq!(
            serde_yaml::from_str::<serde_yaml::Value>(yaml).unwrap(),
            serde_yaml::from_str::<serde_yaml::Value>(&output).unwrap()
        );
        let view = base.view(None).unwrap();
        assert_eq!(view.kind, ViewType::Kanban);
        assert_eq!(
            view.summaries["note.price"],
            Summary::Builtin(SummaryKind::Sum)
        );
        assert_eq!(
            view.summaries["note.size"],
            Summary::Custom("Spread".into())
        );
        assert!(base.formulas["bad"].expr().is_err());
    }
    #[test]
    fn bases_reject_invalid_filters_and_preserve_custom_views() {
        for filter in [
            "{and: [], or: []}",
            "{unknown: []}",
            "42",
            "'x =='",
            "{and: true}",
            "'missing()'",
            "{and: ['nope(1)']}",
        ] {
            assert!(
                BaseFile::parse(&format!("filters: {filter}\nviews: []")).is_err(),
                "{filter}"
            );
        }
        let error = BaseFile::parse("filters: 'x =='").unwrap_err().to_string();
        assert!(error.contains("x =="), "{error}");
        let base = BaseFile::parse("views: [{type: map, name: Map, zoom: 12}]").unwrap();
        assert_eq!(base.views[0].kind, ViewType::Other("map".into()));
    }
    #[test]
    fn bases_reject_unknown_summaries_at_load() {
        for yaml in [
            "views: [{type: table, name: T, summaries: {note.x: Bogus}}]",
            "summaries: {Spread: 'nope('}\nviews: []",
        ] {
            assert!(BaseFile::parse(yaml).is_err(), "{yaml}");
        }
        let base = BaseFile::parse(
            "summaries: {Sum: 'values.length'}\nviews: [{type: table, name: T, summaries: {note.x: Sum}}]",
        )
        .unwrap();
        assert_eq!(
            base.views[0].summaries["note.x"],
            Summary::Custom("Sum".into())
        );
    }
    #[test]
    fn bases_summary_names_parse_back() {
        for kind in SummaryKind::iter() {
            assert_eq!(
                Summary::from(kind.name().to_owned()),
                Summary::Builtin(kind)
            );
        }
    }
    #[test]
    fn bases_expression_parser_preserves_precedence_and_postfix() {
        let expr = Expr::parse("1 + 2 * 3 == 7 && !false").unwrap();
        assert!(matches!(expr, Expr::Binary(BinaryOp::And, _, _)));
        for s in [
            "note[\"hello world\"]",
            "[1,2,3].reduce(acc + value, 0)",
            "{foo: 'bar'}.keys()",
            "/a[bc]+/i.matches('AB')",
            "(1.5).toFixed(2)",
        ] {
            Expr::parse(s).unwrap();
        }
        for s in ["a = b", "'unclosed", "[1,2", "foo() trailing"] {
            assert!(Expr::parse(s).is_err(), "{s}");
        }
    }
}
