//! The wire shape of a discovered skill: what `skills.list`, `skills.search`
//! and `skills.get` answer.

use serde::{Deserialize, Serialize};

/// One skill in a `skills.list` or `skills.search` answer.
///
/// The two RPCs answer the same row, because a list and a search are the same
/// question asked of two different sets. A second row type here would let one
/// of them grow a field the other cannot report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkillSummary {
    /// The skill's name, which is also the key `skills.get` takes.
    pub name: String,
    /// The discovery scope the skill came from, as `SkillScope` spells it.
    pub scope: String,
    pub description: String,
    /// How many same-named skills this one shadows.
    pub shadowed_count: usize,
}

/// What `skills.list` and `skills.search` answer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkillsReply {
    pub skills: Vec<SkillSummary>,
}

/// What `skills.get` answers: one skill, with the body a summary omits.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct SkillDetail {
    pub name: String,
    /// The discovery scope the skill came from, as `SkillScope` spells it.
    pub scope: String,
    pub description: String,
    /// Where the skill file sits on disk.
    pub source_path: String,
    /// The agent the skill declares, when it declares one. Always written.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub agent: Option<String>,
    /// The licence the skill declares, when it declares one. Always written.
    #[cfg_attr(feature = "openapi", schema(required = true))]
    pub license: Option<String>,
    /// The skill's Markdown body, without its frontmatter.
    pub body: String,
}
