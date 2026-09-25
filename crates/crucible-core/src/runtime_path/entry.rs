//! The roots on the path, and where each came from.
//!
//! [`super::asset`] says what lives under a root. This says which roots there
//! are, and the priority of each: one table, [`default_priority`], for every
//! kind — replacing three different rules that ran in two different
//! directions.

use std::path::PathBuf;

/// Where a root came from. [`Origin::level`] and [`default_priority`] give its
/// priority; the declaration order means nothing.
///
/// The ranking is not arbitrary. Anything the user named outranks anything
/// Crucible supplies, and a plugin's own contribution sits below every
/// user-named root so a plugin can never shadow what the user wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Origin {
    /// `$CRUCIBLE_PLUGIN_PATH`: plugin directories. Highest, for dev and CI.
    Env,
    /// `$CRUCIBLE_RUNTIME`: a whole runtime root, at the level `env`.
    EnvRuntime,
    /// `<workspace>/<workspace_root>` — a project directory.
    Workspace,
    /// `<kiln>/.crucible`.
    Kiln,
    /// A `runtimepath` entry, by its index in that list.
    Config(usize),
    /// A `[harnesses]` row: another agent tool's directory the user named.
    Harness,
    /// `~/.config/crucible`.
    ///
    /// Distinct from [`Self::UserRuntime`]. Plugins, cards, skills and themes
    /// read here; `cru setup` writes to the *other* one. Collapsing the two is
    /// what made shipped themes invisible.
    UserConfig,
    /// `~/.config/crucible/runtime` — where `cru setup` copies the tree.
    UserRuntime,
    /// A loaded plugin's own directory.
    ///
    /// In Vim a plugin directory is a `runtimepath` entry that may carry
    /// `colors/` and `plugin/` of its own. This is that, and it is what lets a
    /// plugin ship skills, cards and themes. Ranked below every user-named
    /// root deliberately: a plugin contributes, it does not override.
    Plugin,
    /// Shipped: exe-relative, then the tree extracted from the binary.
    Bundled,
}

/// A named band of priority. The user moves a level with
/// `sources.priority = { <level> = N }`, and a kiln takes a level with
/// `priority = "<level>"`.
///
/// A closed set: [`Self::name`] and [`Self::default_priority`] are
/// exhaustive, and [`Self::parse`] walks every variant.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    strum::EnumIter,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(rename_all = "lowercase")]
pub enum PriorityLevel {
    Env,
    Personal,
    Workspace,
    Kiln,
    Runtimepath,
    Harness,
    Runtime,
    Plugin,
    Builtin,
}

impl PriorityLevel {
    /// The name a config file uses.
    pub fn name(self) -> &'static str {
        match self {
            PriorityLevel::Env => "env",
            PriorityLevel::Personal => "personal",
            PriorityLevel::Workspace => "workspace",
            PriorityLevel::Kiln => "kiln",
            PriorityLevel::Runtimepath => "runtimepath",
            PriorityLevel::Harness => "harness",
            PriorityLevel::Runtime => "runtime",
            PriorityLevel::Plugin => "plugin",
            PriorityLevel::Builtin => "builtin",
        }
    }

    /// The level a config file names, or `None` for an unknown name.
    pub fn parse(name: &str) -> Option<Self> {
        use strum::IntoEnumIterator;
        Self::iter().find(|level| level.name() == name)
    }

    /// The priority of the level when the config does not move it. The user
    /// names personal text, so it is above everything except the environment,
    /// which is for development and CI.
    pub fn default_priority(self) -> i32 {
        match self {
            PriorityLevel::Env => 1000,
            PriorityLevel::Personal => 900,
            PriorityLevel::Workspace => 800,
            PriorityLevel::Kiln => 700,
            PriorityLevel::Runtimepath => 600,
            PriorityLevel::Harness => 500,
            PriorityLevel::Runtime => 300,
            PriorityLevel::Plugin => 200,
            PriorityLevel::Builtin => 100,
        }
    }
}

/// The level overrides a config gives in `sources.priority`. A level the map
/// does not name keeps its default priority.
pub type LevelPriorities = std::collections::BTreeMap<PriorityLevel, i32>;

/// The priority of `level` under `overrides`.
pub fn level_priority(overrides: &LevelPriorities, level: PriorityLevel) -> i32 {
    overrides
        .get(&level)
        .copied()
        .unwrap_or_else(|| level.default_priority())
}

/// The priority a config gives one source: a level name or a number.
///
/// `kilns.<name>.priority = "personal"` or `= 750`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Priority {
    Level(PriorityLevel),
    Number(i32),
}

impl Priority {
    /// The number this priority stands for under `overrides`.
    pub fn resolve(self, overrides: &LevelPriorities) -> i32 {
        match self {
            Priority::Level(level) => level_priority(overrides, level),
            Priority::Number(number) => number,
        }
    }
}

impl Origin {
    /// The level of a root of this origin.
    pub fn level(self) -> PriorityLevel {
        match self {
            Origin::Env | Origin::EnvRuntime => PriorityLevel::Env,
            Origin::Workspace => PriorityLevel::Workspace,
            Origin::Kiln => PriorityLevel::Kiln,
            Origin::Config(_) => PriorityLevel::Runtimepath,
            Origin::Harness => PriorityLevel::Harness,
            Origin::UserConfig => PriorityLevel::Personal,
            Origin::UserRuntime => PriorityLevel::Runtime,
            Origin::Plugin => PriorityLevel::Plugin,
            Origin::Bundled => PriorityLevel::Builtin,
        }
    }

    /// The source name of a root of this origin, before `build_path` makes
    /// it specific. A `runtimepath` entry is `config-N`, from 1.
    pub fn default_source_name(self) -> String {
        match self {
            Origin::Config(index) => format!("config-{}", index + 1),
            Origin::EnvRuntime => "env-runtime".to_string(),
            Origin::Env
            | Origin::Workspace
            | Origin::Kiln
            | Origin::Harness
            | Origin::UserConfig
            | Origin::UserRuntime
            | Origin::Plugin
            | Origin::Bundled => self.level().name().to_string(),
        }
    }
}

/// The default priority of a root of `origin`. A later `runtimepath` entry
/// is one lower than the entry before it.
pub fn default_priority(origin: Origin) -> i32 {
    let base = origin.level().default_priority();
    match origin {
        Origin::Config(index) => base.saturating_sub(i32::try_from(index).unwrap_or(i32::MAX)),
        Origin::Env
        | Origin::EnvRuntime
        | Origin::Workspace
        | Origin::Kiln
        | Origin::Harness
        | Origin::UserConfig
        | Origin::UserRuntime
        | Origin::Plugin
        | Origin::Bundled => base,
    }
}

/// The position of the config home inside level 900: after a personal kiln
/// (0) and `agent_directories` (1). A tie at 900 is never ambiguous.
pub const CONFIG_HOME_WITHIN: u8 = 2;
/// The position of `agent_directories` inside level 900.
pub const AGENT_DIRECTORIES_WITHIN: u8 = 1;

/// One root on the path.
///
/// Two existing knobs name a *leaf* directory rather than a root, so this is
/// an enum and not a bare `PathBuf`:
///
/// - `CRUCIBLE_PLUGIN_PATH=/x` searches `/x`, not `/x/plugins`
/// - `agent_directories` entries are card directories
///
/// A leaf serves exactly one asset kind and is skipped by every other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryKind {
    /// `<path>/<subdir>/` for every kind that reaches this origin.
    Root(PathBuf),
    /// `<path>/` itself, for one kind only.
    Leaf(PathBuf, super::asset::RuntimeAsset),
}

/// A root, its provenance, and the harness it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEntry {
    pub kind: EntryKind,
    pub origin: Origin,
    /// The harness a `[harnesses]` root belongs to, for provenance in
    /// `cru skills list` and the web view. `None` for a root the user named
    /// directly.
    ///
    /// An open string, not an enum: adding a vendor must be a config row, not
    /// a code change. Render layers treat it as an opaque label.
    pub harness: Option<String>,
    /// The prefix of a full name, such as `personal` in `personal:helper`.
    pub name: String,
    /// A higher priority wins. See [`crate::sources`].
    pub priority: i32,
    /// The position inside one priority. See [`crate::sources::Source`].
    pub within: u8,
}

impl RuntimeEntry {
    /// A root serving every kind that reaches `origin`.
    pub fn root(path: impl Into<PathBuf>, origin: Origin) -> Self {
        Self::new(EntryKind::Root(path.into()), origin)
    }

    /// A directory that *is* one kind's directory, not a root above it.
    pub fn leaf(
        path: impl Into<PathBuf>,
        origin: Origin,
        asset: super::asset::RuntimeAsset,
    ) -> Self {
        Self::new(EntryKind::Leaf(path.into(), asset), origin)
    }

    fn new(kind: EntryKind, origin: Origin) -> Self {
        Self {
            kind,
            origin,
            harness: None,
            name: origin.default_source_name(),
            priority: default_priority(origin),
            within: match origin {
                Origin::UserConfig => CONFIG_HOME_WITHIN,
                Origin::Env
                | Origin::EnvRuntime
                | Origin::Workspace
                | Origin::Kiln
                | Origin::Config(_)
                | Origin::Harness
                | Origin::UserRuntime
                | Origin::Plugin
                | Origin::Bundled => 0,
            },
        }
    }

    /// Tag this entry with the harness it came from.
    pub fn with_harness(mut self, harness: impl Into<String>) -> Self {
        self.harness = Some(harness.into());
        self
    }

    /// The path this entry names, whatever its kind.
    pub fn path(&self) -> &std::path::Path {
        match &self.kind {
            EntryKind::Root(p) => p,
            EntryKind::Leaf(p, _) => p,
        }
    }
}

/// One directory a kind may be resolved from, with the entry it came from.
/// Its priority is on the [`crate::sources::Source`] that holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPath {
    pub path: PathBuf,
    pub origin: Origin,
    pub harness: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// Each level name parses back to its level, and no two levels share a
    /// name or a default priority.
    #[test]
    fn every_level_name_parses_back() {
        let mut names = std::collections::BTreeSet::new();
        let mut priorities = std::collections::BTreeSet::new();
        for level in PriorityLevel::iter() {
            assert_eq!(PriorityLevel::parse(level.name()), Some(level));
            assert!(names.insert(level.name()), "{level:?} reuses a name");
            assert!(
                priorities.insert(level.default_priority()),
                "{level:?} reuses a priority"
            );
        }
        assert_eq!(PriorityLevel::parse("nope"), None);
    }

    /// The default priorities follow the documented order, highest first.
    #[test]
    fn levels_are_ordered_personal_first_after_env() {
        let order: Vec<i32> = [
            PriorityLevel::Env,
            PriorityLevel::Personal,
            PriorityLevel::Workspace,
            PriorityLevel::Kiln,
            PriorityLevel::Runtimepath,
            PriorityLevel::Harness,
            PriorityLevel::Runtime,
            PriorityLevel::Plugin,
            PriorityLevel::Builtin,
        ]
        .iter()
        .map(|l| l.default_priority())
        .collect();
        assert!(order.windows(2).all(|w| w[0] > w[1]), "{order:?}");
        assert_eq!(order.len(), PriorityLevel::iter().count());
    }

    /// The name a config file writes for a level is [`PriorityLevel::name`],
    /// so the serde names and the table cannot drift.
    #[test]
    fn a_level_serializes_under_its_name() {
        for level in PriorityLevel::iter() {
            assert_eq!(
                serde_json::to_value(level).unwrap(),
                serde_json::json!(level.name())
            );
        }
        assert_eq!(
            serde_json::from_value::<Priority>(serde_json::json!("personal")).unwrap(),
            Priority::Level(PriorityLevel::Personal)
        );
        assert_eq!(
            serde_json::from_value::<Priority>(serde_json::json!(750)).unwrap(),
            Priority::Number(750)
        );
        assert!(serde_json::from_value::<Priority>(serde_json::json!("nope")).is_err());
    }

    /// An override moves one level and leaves the others.
    #[test]
    fn an_override_moves_only_the_level_it_names() {
        let overrides = LevelPriorities::from([(PriorityLevel::Workspace, 950)]);
        assert_eq!(level_priority(&overrides, PriorityLevel::Workspace), 950);
        assert_eq!(level_priority(&overrides, PriorityLevel::Kiln), 700);
        assert_eq!(
            Priority::Level(PriorityLevel::Workspace).resolve(&overrides),
            950
        );
    }

    /// A later `runtimepath` entry ranks below the one before it.
    #[test]
    fn a_later_runtimepath_entry_ranks_lower() {
        assert_eq!(default_priority(Origin::Config(0)), 600);
        assert_eq!(default_priority(Origin::Config(2)), 598);
        assert_eq!(Origin::Config(2).default_source_name(), "config-3");
    }

    /// The priorities rank the user roots above a plugin and the shipped tree.
    #[test]
    fn priority_ranks_user_roots_above_plugin_and_bundled() {
        let p = default_priority;
        assert!(p(Origin::Env) > p(Origin::Workspace));
        assert!(p(Origin::Config(0)) > p(Origin::Harness));
        assert!(p(Origin::UserConfig) > p(Origin::UserRuntime));
        assert!(p(Origin::UserRuntime) > p(Origin::Plugin));
        assert!(p(Origin::Plugin) > p(Origin::Bundled));
    }

    /// A plugin never outranks a root the user named.
    #[test]
    fn a_plugin_root_never_outranks_a_user_named_root() {
        for user in [
            Origin::Env,
            Origin::Workspace,
            Origin::Kiln,
            Origin::Config(0),
            Origin::Harness,
            Origin::UserConfig,
            Origin::UserRuntime,
        ] {
            assert!(
                default_priority(user) > default_priority(Origin::Plugin),
                "{user:?} must have a higher priority than a plugin"
            );
        }
    }

    #[test]
    fn path_reads_through_either_kind() {
        let root = RuntimeEntry::root("/a", Origin::UserConfig);
        let leaf = RuntimeEntry::leaf("/b", Origin::Env, super::super::asset::RuntimeAsset::Cards);
        assert_eq!(root.path(), std::path::Path::new("/a"));
        assert_eq!(leaf.path(), std::path::Path::new("/b"));
    }
}
