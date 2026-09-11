//! Which session settings an agent can actually change.
//!
//! Crucible's knob set was modelled on the internal agent and then applied to
//! every agent. ACP carries almost none of it: the wire has a mode, a model
//! selector, and whatever else the agent advertises in `configOptions`. There
//! is no temperature and no token cap anywhere in the protocol.
//!
//! Applied anyway, some of the knobs became lies. `AcpAgentHandle` cached
//! temperature and max tokens in fields nothing read, so
//! `set_temperature(0.2)` was accepted, `get_temperature()` answered `0.2`,
//! and the agent process never heard about it. Nine sibling knobs on the same
//! handle already answered `NotSupported`, so this was an inconsistency inside
//! one impl rather than a decision.
//!
//! [`SessionKnob::on_acp`] is the classification, and it is a total function
//! of the knob identity — the `match` carries no wildcard arm, so a knob added
//! later does not compile until someone says what ACP does with it. The two
//! module-level denies below catch the other way out: silencing that error
//! with `_ => AcpKnob::Daemon` would hand every future knob a false "supported"
//! with nobody deciding. Both lints are needed. Clippy reports a wildcard
//! covering one remaining variant as `match_wildcard_for_single_variants` and
//! only a wildcard covering two or more as `wildcard_enum_match_arm`, and
//! knobs are added one at a time.
//!
//! There is deliberately no `Default` on [`AcpKnob`]. A default would make
//! "unclassified" mean something, and the only safe meaning is "someone
//! decides", which a type cannot express.

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use serde::{Deserialize, Serialize};

/// A per-session setting a client can read and write.
///
/// One variant per `session.set_*` / `session.get_*` RPC pair. The name is the
/// wire id, so `Temperature` is `temperature` in `session.list_knobs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(test, derive(strum::EnumIter))]
pub enum SessionKnob {
    /// Token budget for assembled context.
    ContextBudget,
    /// How context is assembled when it does not fit.
    ContextStrategy,
    /// Whether the kiln is searched before the first message.
    Precognition,
    /// Which model answers.
    Model,
    /// Which permission mode the session runs in.
    Mode,
}

/// What an ACP session can do with a knob.
///
/// No `Default`, deliberately: see the module docs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AcpKnob {
    /// The daemon implements it and the agent is not involved, so an ACP
    /// session supports it exactly as an internal one does.
    Daemon,
    /// An ACP method carries it.
    Wire,
    /// Supported only when the agent advertises a model selector in its
    /// `configOptions` — the `model` category, or `model_config` when it
    /// uses that spelling. An agent that advertises neither cannot switch
    /// model at all.
    AdvertisedModel,
    /// The protocol has no equivalent. Offering it would be a control that
    /// changes nothing.
    Absent,
}

/// One choice in an agent's select option.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentOptionChoice {
    /// The id to send back when this choice is picked.
    pub value: String,
    /// What to show for it.
    pub name: String,
}

/// The shape of an agent option's control.
///
/// ACP's `SessionConfigKind` is `#[non_exhaustive]`; a kind this does not
/// cover is dropped rather than guessed at, because a control rendered from a
/// shape nobody understood is worse than no control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentOptionKind {
    /// Pick one of several values.
    Select {
        /// The value the agent reports as current.
        current: String,
        /// Every value it accepts, in the order it listed them. Grouped
        /// options are flattened: the grouping is presentation, and this
        /// projection carries no group headers.
        choices: Vec<AgentOptionChoice>,
    },
    /// On or off.
    Toggle {
        /// The value the agent reports as current.
        current: bool,
    },
}

/// A setting an external agent advertised for itself.
///
/// Crucible has no knob for these: they belong to the agent, and a different
/// agent advertises different ones. A client renders them from this
/// description and sends the chosen value back; the daemon does not interpret
/// them beyond the model selector, which has its own control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentConfigOption {
    /// The id to name in `session.set_agent_option`.
    pub id: String,
    /// What to label the control.
    pub name: String,
    /// Optional help text the agent supplied.
    pub description: Option<String>,
    /// The agent's own category string, when it sent one. UX only: it exists
    /// so a client can place or icon a control, never for correctness.
    pub category: Option<String>,
    /// The control to draw.
    #[serde(flatten)]
    pub kind: AgentOptionKind,
}

/// One knob and whether this session can change it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnobDescriptor {
    /// The wire id, matching the `session.set_*` suffix.
    pub id: String,
    /// Whether this session can change it. `false` means the control should
    /// not be offered: the daemon refuses the call.
    pub supported: bool,
}

/// What `session.list_knobs` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionKnobSupport {
    /// Every knob Crucible has, answered for. A client that finds an id
    /// missing is talking to an older daemon, not to a session without it.
    pub knobs: Vec<KnobDescriptor>,
}

impl SessionKnobSupport {
    /// Whether the session can change `id`. An unknown id is not supported,
    /// which is the safe answer for a client newer than its daemon.
    pub fn supports(&self, id: &str) -> bool {
        self.knobs.iter().any(|k| k.id == id && k.supported)
    }
}

impl SessionKnob {
    /// Every knob, in the order a settings panel reads best.
    ///
    /// Hand-written because `strum::EnumIter` is derived only under `cfg(test)`
    /// and this list is walked in production. The test below proves the two
    /// agree, so a variant added without an entry here fails the suite rather
    /// than quietly disappearing from `session.list_knobs`.
    pub const ALL: &'static [SessionKnob] = &[
        Self::Model,
        Self::Mode,
        Self::ContextBudget,
        Self::ContextStrategy,
        Self::Precognition,
    ];

    /// The wire id, which is also the `session.set_*` suffix.
    pub fn id(self) -> &'static str {
        match self {
            Self::ContextBudget => "context_budget",
            Self::ContextStrategy => "context_strategy",
            Self::Precognition => "precognition",
            Self::Model => "model",
            Self::Mode => "mode",
        }
    }

    /// What an ACP session can do with this knob.
    ///
    /// Exhaustive by construction. The reasoning behind the `Absent` arms is
    /// one fact about ACP: the external agent runs its own turn loop and owns
    /// its own history, which makes a daemon-side context policy describe work
    /// the daemon does not do.
    pub fn on_acp(self) -> AcpKnob {
        match self {
            // The agent owns its history, so the daemon assembles no context
            // to budget, trim or compact.
            Self::ContextBudget | Self::ContextStrategy => AcpKnob::Absent,

            // Retrieval is the daemon's, and it reaches an external agent as
            // injected prompt text. An ACP session uses it exactly as an
            // internal one does.
            Self::Precognition => AcpKnob::Daemon,

            // `session/set_config_option`, when the agent lists a selector.
            Self::Model => AcpKnob::AdvertisedModel,

            // `session/set_mode`.
            Self::Mode => AcpKnob::Wire,
        }
    }
}

impl AgentConfigOption {
    /// Project one ACP config option, or `None` for a shape this does not
    /// cover.
    ///
    /// `None` also covers the model selector: Crucible has its own model
    /// control fed by `session.list_models`, and rendering the agent's
    /// selector beside it would put two controls on one setting.
    pub fn from_acp(option: &crate::types::acp::schema::SessionConfigOption) -> Option<Self> {
        use crate::types::acp::schema::{
            SessionConfigKind, SessionConfigOptionCategory, SessionConfigSelectOptions,
        };

        if matches!(
            option.category,
            Some(SessionConfigOptionCategory::Model)
                | Some(SessionConfigOptionCategory::ModelConfig)
        ) {
            return None;
        }

        let kind = match &option.kind {
            SessionConfigKind::Select(select) => {
                let choices = match &select.options {
                    SessionConfigSelectOptions::Ungrouped(list) => list
                        .iter()
                        .map(|c| AgentOptionChoice {
                            value: c.value.to_string(),
                            name: c.name.clone(),
                        })
                        .collect(),
                    SessionConfigSelectOptions::Grouped(groups) => groups
                        .iter()
                        .flat_map(|g| {
                            g.options.iter().map(|c| AgentOptionChoice {
                                value: c.value.to_string(),
                                name: c.name.clone(),
                            })
                        })
                        .collect(),
                    // The enum is `#[non_exhaustive]`; an unknown shape lists
                    // nothing, which the emptiness check below then drops.
                    _ => Vec::new(),
                };
                if choices.is_empty() {
                    return None;
                }
                AgentOptionKind::Select {
                    current: select.current_value.to_string(),
                    choices,
                }
            }
            SessionConfigKind::Boolean(toggle) => AgentOptionKind::Toggle {
                current: toggle.current_value,
            },
            // A kind added to the protocol later. Dropping it is the honest
            // answer: a control drawn from a shape nobody read is worse than
            // no control.
            _ => return None,
        };

        Some(Self {
            id: option.id.to_string(),
            name: option.name.clone(),
            description: option.description.clone(),
            category: option.category.as_ref().and_then(|c| {
                serde_json::to_value(c)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
            }),
            kind,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    /// `ALL` really is all of them.
    ///
    /// The array is walked in production and the iterator is what the
    /// compiler knows, so this is the only thing that keeps a new variant
    /// from being silently absent from every settings panel.
    #[test]
    fn the_all_array_lists_every_variant() {
        let from_iter: std::collections::BTreeSet<&str> =
            SessionKnob::iter().map(SessionKnob::id).collect();
        let from_array: std::collections::BTreeSet<&str> = SessionKnob::ALL
            .iter()
            .copied()
            .map(SessionKnob::id)
            .collect();
        assert_eq!(
            from_iter, from_array,
            "SessionKnob::ALL and the enum disagree; a knob is missing from one of them"
        );
    }

    /// Every knob is classified, and the ids are distinct.
    ///
    /// The classification itself cannot fail to be total — rustc's
    /// exhaustiveness check sees to that — so what is worth proving is that
    /// the id table stayed in step with the enum, which no compiler checks.
    #[test]
    fn every_knob_has_a_distinct_id() {
        let ids: Vec<&str> = SessionKnob::iter().map(SessionKnob::id).collect();
        let unique: std::collections::BTreeSet<&str> = ids.iter().copied().collect();
        assert_eq!(
            ids.len(),
            unique.len(),
            "two knobs share an id, so one of them is unreachable over RPC: {ids:?}"
        );
        assert!(ids.iter().all(|id| !id.is_empty()));
    }

    /// Retrieval is the daemon's work, not the agent's, and reaches an
    /// external agent as injected prompt text. Classifying it `Absent` would
    /// hide a setting that demonstrably works.
    #[test]
    fn precognition_stays_available_to_an_acp_session() {
        assert_eq!(SessionKnob::Precognition.on_acp(), AcpKnob::Daemon);
    }

    /// The model is the one knob whose support depends on what the agent
    /// listed at the handshake, so it may never be reported as
    /// unconditionally supported.
    #[test]
    fn the_agent_decides_whether_the_model_can_change() {
        assert_eq!(SessionKnob::Model.on_acp(), AcpKnob::AdvertisedModel);
    }
}
