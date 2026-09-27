use crucible_core::bases::{View, ViewType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Built-in presentation options, projected from preserved Obsidian view data.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ViewOptions {
    pub card_size: f64,
    pub column_width: f64,
    pub image: Option<String>,
    pub image_fit: String,
    pub image_aspect_ratio: f64,
    pub hide_empty_groups: bool,
    pub markers: String,
    pub indent_properties: bool,
    pub separator: String,
    pub row_height: String,
    pub column_size: BTreeMap<String, f64>,
}
impl From<&View> for ViewOptions {
    fn from(view: &View) -> Self {
        let text = |key: &str| view.extra.get(key).and_then(|v| v.as_str());
        let positive = |key: &str, default: f64| {
            view.extra
                .get(key)
                .and_then(|v| v.as_f64())
                .filter(|n| n.is_finite() && *n > 0.0)
                .unwrap_or(default)
        };
        let boolean = |key: &str| {
            view.extra
                .get(key)
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        };
        Self {
            card_size: positive("cardSize", 200.0),
            column_width: positive("columnWidth", 280.0),
            image: text("image").map(str::to_owned),
            image_fit: if text("imageFit") == Some("contain") {
                "contain"
            } else {
                "cover"
            }
            .into(),
            image_aspect_ratio: positive(
                "imageAspectRatio",
                if view.kind == ViewType::Kanban {
                    0.5
                } else {
                    1.0
                },
            ),
            hide_empty_groups: boolean("hideEmptyGroups"),
            markers: match text("markers") {
                Some("number") => "number",
                Some("none") => "none",
                _ => "bullet",
            }
            .into(),
            indent_properties: boolean("indentProperties"),
            separator: text("separator")
                .filter(|s| !s.is_empty())
                .unwrap_or(", ")
                .into(),
            row_height: text("rowHeight").unwrap_or("").into(),
            column_size: view
                .extra
                .get("columnSize")
                .and_then(|v| serde_yaml::from_value::<BTreeMap<String, f64>>(v.clone()).ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|(_, v)| v.is_finite() && *v > 0.0)
                .collect(),
        }
    }
}
