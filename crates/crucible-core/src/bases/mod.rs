//! Obsidian Bases documents and expressions. Unknown view/plugin options round-trip.
mod expression;
pub use expression::{BaseValue, Expr};
pub use BaseValue as Value;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type Formula = String;
pub type Summary = String;
pub type Extra = BTreeMap<String, serde_yaml::Value>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filters: Option<Filter>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub formulas: BTreeMap<String, Formula>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, Property>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub summaries: BTreeMap<String, Summary>,
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
        let result: Self = serde_yaml::from_str(yaml)?;
        if let Some(f) = &result.filters {
            f.validate()?;
        }
        for v in &result.views {
            if let Some(f) = &v.filters {
                f.validate()?;
            }
        }
        for formula in result.formulas.values().chain(result.summaries.values()) {
            Expr::parse(formula)?;
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
#[serde(untagged)]
pub enum Filter {
    Expression(String),
    Tree(FilterTree),
}
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FilterTree {
    And(Vec<Filter>),
    Or(Vec<Filter>),
    Not(Vec<Filter>),
}
impl Filter {
    pub fn validate(&self) -> anyhow::Result<()> {
        match self {
            Self::Expression(s) => {
                Expr::parse(s)?;
            }
            Self::Tree(FilterTree::And(fs) | FilterTree::Or(fs) | FilterTree::Not(fs)) => {
                for f in fs {
                    f.validate()?;
                }
            }
        }
        Ok(())
    }
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub summaries: BTreeMap<String, String>,
    #[serde(flatten)]
    pub extra: Extra,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupBy {
    pub property: String,
    pub direction: Direction,
    #[serde(flatten)]
    pub extra: Extra,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sort {
    pub property: String,
    pub direction: Direction,
    #[serde(flatten)]
    pub extra: Extra,
}
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
    #[test]
    fn bases_roundtrip_obisidian_yaml_and_unknown_view_options() {
        let yaml = "filters:\n  and:\n    - 'file.hasTag(\"book\")'\n    - not:\n        - 'status == \"done\"'\nnewItemFolder: Books\npluginData: {x: [1, two]}\nviews:\n  - type: kanban\n    name: Board\n    groupBy: {property: note.status, direction: ASC}\n    groupOrder: [todo, done]\n    columnWidth: 280\n";
        let base = BaseFile::parse(yaml).unwrap();
        let output = base.to_yaml().unwrap();
        assert_eq!(
            serde_yaml::from_str::<serde_yaml::Value>(yaml).unwrap(),
            serde_yaml::from_str::<serde_yaml::Value>(&output).unwrap()
        );
        assert_eq!(base.view(None).unwrap().kind, ViewType::Kanban);
    }
    #[test]
    fn bases_reject_invalid_filters_and_preserve_custom_views() {
        for filter in [
            "{and: [], or: []}",
            "{unknown: []}",
            "42",
            "'x =='",
            "{and: true}",
        ] {
            assert!(
                BaseFile::parse(&format!("filters: {filter}\nviews: []")).is_err(),
                "{filter}"
            );
        }
        let base = BaseFile::parse("views: [{type: map, name: Map, zoom: 12}]").unwrap();
        assert_eq!(base.views[0].kind, ViewType::Other("map".into()));
    }
    #[test]
    fn bases_expression_parser_preserves_precedence_and_postfix() {
        let expr = Expr::parse("1 + 2 * 3 == 7 && !false").unwrap();
        assert!(matches!(expr,Expr::Binary(ref op,_,_) if op=="&&"));
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
