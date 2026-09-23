/// Names shared by status producers and both renderers. Unknown names render as info.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StatusColorGroup {
    Ok,
    Warn,
    Danger,
    Info,
    Hue0,
    Hue1,
    Hue2,
    Hue3,
    Hue4,
    Hue5,
    Hue6,
    Hue7,
}

impl StatusColorGroup {
    pub const ALL: [Self; 12] = [
        Self::Ok,
        Self::Warn,
        Self::Danger,
        Self::Info,
        Self::Hue0,
        Self::Hue1,
        Self::Hue2,
        Self::Hue3,
        Self::Hue4,
        Self::Hue5,
        Self::Hue6,
        Self::Hue7,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Danger => "danger",
            Self::Info => "info",
            Self::Hue0 => "hue-0",
            Self::Hue1 => "hue-1",
            Self::Hue2 => "hue-2",
            Self::Hue3 => "hue-3",
            Self::Hue4 => "hue-4",
            Self::Hue5 => "hue-5",
            Self::Hue6 => "hue-6",
            Self::Hue7 => "hue-7",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name {
            "ok" => Self::Ok,
            "warn" => Self::Warn,
            "danger" => Self::Danger,
            "info" => Self::Info,
            "hue-0" => Self::Hue0,
            "hue-1" => Self::Hue1,
            "hue-2" => Self::Hue2,
            "hue-3" => Self::Hue3,
            "hue-4" => Self::Hue4,
            "hue-5" => Self::Hue5,
            "hue-6" => Self::Hue6,
            "hue-7" => Self::Hue7,
            _ => Self::Info,
        }
    }
}

/// FNV-1a fixes the hue independently of process and hash-map seeds.
pub fn plugin_hue(plugin_name: &str) -> StatusColorGroup {
    let hash = plugin_name.bytes().fold(0x811c9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x01000193)
    });
    match hash % 8 {
        0 => StatusColorGroup::Hue0,
        1 => StatusColorGroup::Hue1,
        2 => StatusColorGroup::Hue2,
        3 => StatusColorGroup::Hue3,
        4 => StatusColorGroup::Hue4,
        5 => StatusColorGroup::Hue5,
        6 => StatusColorGroup::Hue6,
        _ => StatusColorGroup::Hue7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_groups_are_complete_and_unknown_names_fall_back_to_info() {
        for group in StatusColorGroup::ALL {
            assert_eq!(StatusColorGroup::from_name(group.name()), group);
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
    }
}
