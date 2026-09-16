//! The precedence rule between the config layer and the state layer.
//!
//! Crucible learns a registration from two places. The **config layer** is what
//! the user authored — a `[kilns]` entry today, an `init.lua` declaration after
//! the config migration. The **state layer** is what the daemon was told at
//! runtime and wrote down: `<data_home>/kilns.json`.
//!
//! The rule is one sentence: **the config layer wins over the state layer on a
//! name conflict.** A state entry the config layer shadows is not deleted and
//! not silently dropped — it comes back in [`Overlay::shadowed`] so a caller can
//! log it, and so `cru kiln list` can show the user which side owns the name.
//!
//! This function takes normalized [`Registration`] values rather than a config
//! document, so it is format-agnostic by construction: it does the same work
//! over a TOML-derived layer and over a Lua-derived one.

use std::collections::BTreeMap;
use std::path::PathBuf;

/// Which layer declared a registration.
///
/// The three strings the `kiln.registry_list` RPC reports, so the wire
/// vocabulary and the in-process vocabulary cannot drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RegistrationOrigin {
    /// Declared in the config the user authored.
    Config,
    /// Written into the state store by a registration command.
    Registered,
    /// Neither: a directory a session opened by path, or project discovery
    /// found. It has no entry in either layer.
    Discovered,
}

impl RegistrationOrigin {
    /// The wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Registered => "registered",
            Self::Discovered => "discovered",
        }
    }
}

impl std::fmt::Display for RegistrationOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One registration, from whichever layer declared it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registration {
    /// The name the entry is addressed by.
    pub name: String,
    /// Where it points. Absolute, and canonical as far as the layer could make
    /// it — comparison here is textual, so a caller that skips normalization
    /// gets two entries where the user has one.
    pub path: PathBuf,
    /// Which layer this came from.
    pub origin: RegistrationOrigin,
    /// Do not open or index this entry until it is asked for by name.
    pub lazy: bool,
    /// Crucible derived this entry; the user did not name it.
    pub auto: bool,
}

impl Registration {
    /// A config-layer entry.
    #[must_use]
    pub fn config(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            origin: RegistrationOrigin::Config,
            lazy: false,
            auto: false,
        }
    }

    /// A state-layer entry.
    #[must_use]
    pub fn registered(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            origin: RegistrationOrigin::Registered,
            lazy: false,
            auto: false,
        }
    }

    /// Builder: mark the entry lazy.
    #[must_use]
    pub fn with_lazy(mut self, lazy: bool) -> Self {
        self.lazy = lazy;
        self
    }

    /// Builder: mark the entry as one Crucible derived.
    #[must_use]
    pub fn with_auto(mut self, auto: bool) -> Self {
        self.auto = auto;
        self
    }
}

/// A state entry the config layer out-ranks.
///
/// Kept rather than dropped because the user cannot act on what they cannot
/// see: the state entry survives in `kilns.json` and only `cru kiln forget`
/// removes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShadowedRegistration {
    /// The contested name.
    pub name: String,
    /// Where the config layer points it.
    pub config_path: PathBuf,
    /// Where the state layer points it.
    pub state_path: PathBuf,
}

/// The merged view, plus everything the merge out-ranked.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Overlay {
    /// The effective registrations, in name order.
    pub effective: Vec<Registration>,
    /// State entries a config entry of the same name out-ranked, in name order.
    pub shadowed: Vec<ShadowedRegistration>,
}

/// Merge the state layer under the config layer.
///
/// The config layer wins on a name conflict. A state entry that names the same
/// path as its config entry is not a conflict — it is one registration the user
/// wrote down twice — so it is absorbed silently and reported nowhere.
///
/// Order in, order out: the result is sorted by name so every diagnostic built
/// from it is stable.
#[must_use]
pub fn overlay_registrations(
    config: impl IntoIterator<Item = Registration>,
    state: impl IntoIterator<Item = Registration>,
) -> Overlay {
    let merged = overlay_layers(
        config,
        state,
        // The FOLDED name, because a kiln name resolves case-insensitively.
        // Keyed by the raw string, `Docs` in the config and `docs` in the
        // state store are two entries that both reach the registry, where the
        // second silently re-points the first — a conflict the user is never
        // shown because the overlay never saw one.
        |entry| crate::config::KilnName::fold_str(&entry.name),
        |declared, entry| declared.path == entry.path,
    );
    Overlay {
        effective: merged.effective,
        shadowed: merged
            .shadowed
            .into_iter()
            .map(|s| ShadowedRegistration {
                name: s.name,
                config_path: s.config.path,
                state_path: s.state.path,
            })
            .collect(),
    }
}

/// A state entry the config layer out-ranks, over any layered type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shadowed<T> {
    /// The contested name.
    pub name: String,
    /// What the config layer says under that name.
    pub config: T,
    /// What the state layer says.
    pub state: T,
}

/// The merged view over any layered type, plus what the merge out-ranked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayeredOverlay<T> {
    /// The effective entries, in name order.
    pub effective: Vec<T>,
    /// State entries a config entry of the same name out-ranked, in name order.
    pub shadowed: Vec<Shadowed<T>>,
}

/// The precedence rule itself, over any two named layers.
///
/// [`overlay_registrations`] is this function over [`Registration`], and the
/// generic form exists because Crucible overlays three registries whose entries
/// are not the same shape: kilns and projects are path-shaped, the LLM provider
/// table is not, and projects additionally carry the kilns they use. Writing
/// the rule once and passing in what "same name" and "same value" mean is the
/// only way all three cannot drift apart.
///
/// `name_of` gives the key. `agree` decides whether a state entry that shares a
/// name with a config entry is a CONFLICT or just the same thing written down
/// twice — the second case is absorbed silently and reported nowhere, because
/// there is nothing for the user to resolve.
#[must_use]
pub fn overlay_layers<T>(
    config: impl IntoIterator<Item = T>,
    state: impl IntoIterator<Item = T>,
    name_of: impl Fn(&T) -> String,
    agree: impl Fn(&T, &T) -> bool,
) -> LayeredOverlay<T>
where
    T: Clone,
{
    let config: BTreeMap<String, T> = config
        .into_iter()
        .map(|entry| (name_of(&entry), entry))
        .collect();

    let mut shadowed = Vec::new();
    let mut effective = config.clone();

    for entry in state {
        let name = name_of(&entry);
        match config.get(&name) {
            Some(declared) if agree(declared, &entry) => {}
            Some(declared) => shadowed.push(Shadowed {
                name,
                config: declared.clone(),
                state: entry,
            }),
            None => {
                effective.insert(name, entry);
            }
        }
    }

    shadowed.sort_by(|a, b| a.name.cmp(&b.name));
    LayeredOverlay {
        effective: effective.into_values().collect(),
        shadowed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(overlay: &Overlay) -> Vec<&str> {
        overlay
            .effective
            .iter()
            .map(|entry| entry.name.as_str())
            .collect()
    }

    /// A state entry the config layer never mentions is a registration in its
    /// own right. This is the whole reason the state layer exists.
    #[test]
    fn a_state_only_name_survives_the_overlay() {
        let overlay = overlay_registrations(
            [Registration::config("docs", "/a/docs")],
            [Registration::registered("notes", "/a/notes")],
        );

        assert_eq!(names(&overlay), ["docs", "notes"]);
        assert!(overlay.shadowed.is_empty());
        assert_eq!(
            overlay.effective[1].origin,
            RegistrationOrigin::Registered,
            "the origin must survive, or `cru kiln list` cannot say which side owns the name"
        );
    }

    /// Two layers spelling one name differently is a CONFLICT, not two kilns.
    /// Names resolve case-insensitively, so keying the overlay by the raw
    /// string let both entries through and the state one landed last.
    #[test]
    fn two_layers_that_differ_only_in_case_are_one_contested_name() {
        let overlay = overlay_registrations(
            [Registration::config("Crucible Help", "/a/docs")],
            [Registration::registered("crucible help", "/b/docs")],
        );

        assert_eq!(
            names(&overlay),
            ["Crucible Help"],
            "the config layer wins, and it wins with its own spelling"
        );
        assert_eq!(
            overlay.shadowed.len(),
            1,
            "the user must be able to see the entry that does nothing: {:?}",
            overlay.shadowed
        );
    }

    /// The precedence rule itself. The config layer wins, and the loser is
    /// reported rather than dropped — a registration the user cannot see is a
    /// registration they cannot remove.
    #[test]
    fn the_config_layer_wins_a_name_conflict_and_the_loser_is_reported() {
        let overlay = overlay_registrations(
            [Registration::config("notes", "/a/notes")],
            [Registration::registered("notes", "/b/notes")],
        );

        assert_eq!(overlay.effective.len(), 1);
        assert_eq!(overlay.effective[0].path, PathBuf::from("/a/notes"));
        assert_eq!(overlay.effective[0].origin, RegistrationOrigin::Config);
        assert_eq!(
            overlay.shadowed,
            vec![ShadowedRegistration {
                name: "notes".to_string(),
                config_path: PathBuf::from("/a/notes"),
                state_path: PathBuf::from("/b/notes"),
            }]
        );
    }

    /// Both layers naming the same directory is what migration produces, and
    /// what a user who registers a kiln and then declares it gets. It is one
    /// registration, and reporting it as a conflict would train the user to
    /// ignore the report.
    #[test]
    fn the_same_name_and_path_in_both_layers_is_not_a_conflict() {
        let overlay = overlay_registrations(
            [Registration::config("notes", "/a/notes")],
            [Registration::registered("notes", "/a/notes")],
        );

        assert_eq!(names(&overlay), ["notes"]);
        assert_eq!(overlay.effective[0].origin, RegistrationOrigin::Config);
        assert!(overlay.shadowed.is_empty());
    }

    #[test]
    fn the_result_is_sorted_by_name() {
        let overlay = overlay_registrations(
            [
                Registration::config("zeta", "/z"),
                Registration::config("alpha", "/a"),
            ],
            [Registration::registered("mid", "/m")],
        );

        assert_eq!(names(&overlay), ["alpha", "mid", "zeta"]);
    }
}
