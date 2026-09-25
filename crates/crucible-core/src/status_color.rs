use serde::{Deserialize, Serialize};

/// Names shared by status producers and both renderers. Unknown names render
/// as info.
///
/// One table: serde, strum and the OpenAPI schema all read the names below,
/// and `EnumIter` gives every consumer the complete set.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    strum::EnumIter,
    strum::EnumString,
    strum::IntoStaticStr,
)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub enum StatusColorGroup {
    #[serde(rename = "ok")]
    #[strum(serialize = "ok")]
    Ok,
    #[serde(rename = "warn")]
    #[strum(serialize = "warn")]
    Warn,
    #[serde(rename = "danger")]
    #[strum(serialize = "danger")]
    Danger,
    #[serde(rename = "info")]
    #[strum(serialize = "info")]
    Info,
    #[serde(rename = "hue-0")]
    #[strum(serialize = "hue-0")]
    Hue0,
    #[serde(rename = "hue-1")]
    #[strum(serialize = "hue-1")]
    Hue1,
    #[serde(rename = "hue-2")]
    #[strum(serialize = "hue-2")]
    Hue2,
    #[serde(rename = "hue-3")]
    #[strum(serialize = "hue-3")]
    Hue3,
    #[serde(rename = "hue-4")]
    #[strum(serialize = "hue-4")]
    Hue4,
    #[serde(rename = "hue-5")]
    #[strum(serialize = "hue-5")]
    Hue5,
    #[serde(rename = "hue-6")]
    #[strum(serialize = "hue-6")]
    Hue6,
    #[serde(rename = "hue-7")]
    #[strum(serialize = "hue-7")]
    Hue7,
}

impl StatusColorGroup {
    /// Every group, in declaration order.
    pub fn all() -> impl Iterator<Item = Self> {
        <Self as strum::IntoEnumIterator>::iter()
    }

    /// The name that Lua, the wire and the web CSS use.
    pub fn name(self) -> &'static str {
        self.into()
    }

    /// The group a plugin named. A name that is no group renders as info.
    pub fn from_name(name: &str) -> Self {
        name.parse().unwrap_or(Self::Info)
    }

    /// Whether the group is one of the fixed palette hues rather than a
    /// semantic group.
    pub const fn is_hue(self) -> bool {
        match self {
            Self::Ok | Self::Warn | Self::Danger | Self::Info => false,
            Self::Hue0
            | Self::Hue1
            | Self::Hue2
            | Self::Hue3
            | Self::Hue4
            | Self::Hue5
            | Self::Hue6
            | Self::Hue7 => true,
        }
    }
}

/// FNV-1a fixes the hue independently of process and hash-map seeds.
pub fn plugin_hue(plugin_name: &str) -> StatusColorGroup {
    let hash = plugin_name.bytes().fold(0x811c9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    });
    let hues: Vec<_> = StatusColorGroup::all()
        .filter(|group| group.is_hue())
        .collect();
    hues[hash as usize % hues.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_groups_are_complete_and_unknown_names_fall_back_to_info() {
        for group in StatusColorGroup::all() {
            assert_eq!(StatusColorGroup::from_name(group.name()), group);
            assert_eq!(
                serde_json::to_value(group).unwrap(),
                serde_json::json!(group.name()),
                "the wire name is the Lua name"
            );
        }
        assert_eq!(
            StatusColorGroup::from_name("unrecognised"),
            StatusColorGroup::Info
        );
    }

    #[test]
    fn plugin_hues_are_stable_and_cover_the_palette() {
        assert_eq!(plugin_hue("goal"), plugin_hue("goal"));
        let hues: std::collections::HashSet<_> = (0..256)
            .map(|n| plugin_hue(&format!("plugin-{n}")))
            .collect();
        assert_eq!(hues.len(), 8);
        assert!(hues.iter().all(|hue| hue.is_hue()));
    }
}
