use crucible_core::bases::{View, ViewType};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// How a cards or kanban cover image fills its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum ImageFit {
    #[default]
    Cover,
    Contain,
}
/// List view item markers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum Markers {
    #[default]
    Bullet,
    Number,
    None,
}
/// Table row height.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, strum::EnumString)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum RowHeight {
    #[default]
    Short,
    Medium,
    Tall,
    Extra,
}

/// Built-in presentation options, projected from preserved Obsidian view data.
/// Every field has its default, so clients do not repeat them.
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct ViewOptions {
    pub card_size: f64,
    pub column_width: f64,
    pub image: Option<String>,
    pub image_fit: ImageFit,
    pub image_aspect_ratio: f64,
    pub hide_empty_groups: bool,
    pub markers: Markers,
    pub indent_properties: bool,
    pub separator: String,
    pub row_height: RowHeight,
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
        fn choice<T: std::str::FromStr + Default>(value: Option<&str>) -> T {
            value.and_then(|v| v.parse().ok()).unwrap_or_default()
        }
        Self {
            card_size: positive("cardSize", 200.0),
            column_width: positive("columnWidth", 280.0),
            image: text("image").map(str::to_owned),
            image_fit: choice(text("imageFit")),
            image_aspect_ratio: positive(
                "imageAspectRatio",
                if view.kind == ViewType::Kanban {
                    0.5
                } else {
                    1.0
                },
            ),
            hide_empty_groups: boolean("hideEmptyGroups"),
            markers: choice(text("markers")),
            indent_properties: boolean("indentProperties"),
            separator: text("separator")
                .filter(|s| !s.is_empty())
                .unwrap_or(", ")
                .into(),
            row_height: choice(text("rowHeight")),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bases_view_options_read_closed_choices_and_default_the_rest() {
        let view: View = serde_yaml::from_str(
            "type: list\nname: L\nmarkers: number\nimageFit: contain\nrowHeight: tall\n",
        )
        .unwrap();
        let options = ViewOptions::from(&view);
        assert_eq!(options.markers, Markers::Number);
        assert_eq!(options.image_fit, ImageFit::Contain);
        assert_eq!(options.row_height, RowHeight::Tall);
        let view: View =
            serde_yaml::from_str("type: kanban\nname: K\nmarkers: stars\nrowHeight: 9\n").unwrap();
        let json = serde_json::to_value(ViewOptions::from(&view)).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "card_size": 200.0, "column_width": 280.0, "image": null,
                "image_fit": "cover", "image_aspect_ratio": 0.5,
                "hide_empty_groups": false, "markers": "bullet",
                "indent_properties": false, "separator": ", ",
                "row_height": "short", "column_size": {}
            })
        );
    }
}
