//! Which session settings an agent can actually change.
//!
//! Crucible's knob set was modelled on the internal agent and then applied to
//! every agent. ACP carries almost none of it: the wire has a mode, a model
//! selector, and whatever else the agent advertises in `configOptions`. There
//! is no temperature and no token cap anywhere in the protocol.
//!
//! Applied anyway, three of the knobs became lies. `AcpAgentHandle` cached
//! temperature, thinking budget and max tokens in fields nothing read, so
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
    /// Sampling temperature.
    Temperature,
    /// Cap on the tokens one reply may use.
    MaxTokens,
    /// Reasoning-token budget.
    ThinkingBudget,
    /// The instruction block that opens the conversation.
    SystemPrompt,
    /// Cap on tool-call rounds in one turn.
    MaxIterations,
    /// Wall-clock cap on one turn.
    ExecutionTimeout,
    /// Token budget for assembled context.
    ContextBudget,
    /// How context is assembled when it does not fit.
    ContextStrategy,
    /// The model's context window, when it must be stated rather than known.
    ContextWindow,
    /// What the turn's output is checked against.
    OutputValidation,
    /// How many times a failed validation is retried.
    ValidationRetries,
    /// The fraction of the window that triggers a compaction.
    AutocompactThreshold,
    /// Whether the kiln is searched before the first message.
    Precognition,
    /// How many notes that search injects.
    PrecognitionResults,
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
        Self::Temperature,
        Self::MaxTokens,
        Self::ThinkingBudget,
        Self::SystemPrompt,
        Self::MaxIterations,
        Self::ExecutionTimeout,
        Self::ContextBudget,
        Self::ContextStrategy,
        Self::ContextWindow,
        Self::OutputValidation,
        Self::ValidationRetries,
        Self::AutocompactThreshold,
        Self::Precognition,
        Self::PrecognitionResults,
    ];

    /// The wire id, which is also the `session.set_*` suffix.
    pub fn id(self) -> &'static str {
        match self {
            Self::Temperature => "temperature",
            Self::MaxTokens => "max_tokens",
            Self::ThinkingBudget => "thinking_budget",
            Self::SystemPrompt => "system_prompt",
            Self::MaxIterations => "max_iterations",
            Self::ExecutionTimeout => "execution_timeout",
            Self::ContextBudget => "context_budget",
            Self::ContextStrategy => "context_strategy",
            Self::ContextWindow => "context_window",
            Self::OutputValidation => "output_validation",
            Self::ValidationRetries => "validation_retries",
            Self::AutocompactThreshold => "autocompact_threshold",
            Self::Precognition => "precognition",
            Self::PrecognitionResults => "precognition_results",
            Self::Model => "model",
            Self::Mode => "mode",
        }
    }

    /// What an ACP session can do with this knob.
    ///
    /// Exhaustive by construction. The reasoning behind the `Absent` arms is
    /// one of two facts about ACP: the protocol has no field for the value
    /// (temperature, token caps, the system prompt), or the external agent
    /// runs its own turn loop and owns its own history, which makes a
    /// daemon-side cap or context policy describe work the daemon does not do.
    pub fn on_acp(self) -> AcpKnob {
        match self {
            // No field anywhere in the protocol.
            Self::Temperature | Self::MaxTokens | Self::SystemPrompt => AcpKnob::Absent,

            // The agent runs its own turn loop, so a daemon-side cap on
            // rounds or wall-clock governs nothing it does.
            Self::MaxIterations | Self::ExecutionTimeout => AcpKnob::Absent,

            // The agent owns its history, so the daemon assembles no context
            // to budget, trim or compact.
            Self::ContextBudget
            | Self::ContextStrategy
            | Self::ContextWindow
            | Self::AutocompactThreshold => AcpKnob::Absent,

            // Validation runs over a turn the daemon drives.
            Self::OutputValidation | Self::ValidationRetries => AcpKnob::Absent,

            // Retrieval is the daemon's, and it reaches an external agent as
            // injected prompt text. An ACP session uses it exactly as an
            // internal one does.
            Self::Precognition | Self::PrecognitionResults => AcpKnob::Daemon,

            // ACP has a `thought_level` config option, but it is a select of
            // names and this knob is a token count. Mapping one onto the other
            // means inventing which count a level stands for, so the agent's
            // selector is offered as itself — an agent config option a client
            // renders — rather than projected onto a number it does not mean.
            Self::ThinkingBudget => AcpKnob::Absent,

            // `session/set_config_option`, when the agent lists a selector.
            Self::Model => AcpKnob::AdvertisedModel,

            // `session/set_mode`.
            Self::Mode => AcpKnob::Wire,
        }
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

    /// The three knobs this table exists for.
    ///
    /// An `AcpAgentHandle` cached these and reported them back, so a user set
    /// a temperature the agent never heard. Naming them here means a later
    /// edit that quietly reclassifies one has to say so.
    #[test]
    fn the_knobs_acp_cannot_carry_are_absent() {
        assert_eq!(SessionKnob::Temperature.on_acp(), AcpKnob::Absent);
        assert_eq!(SessionKnob::MaxTokens.on_acp(), AcpKnob::Absent);
        assert_eq!(SessionKnob::SystemPrompt.on_acp(), AcpKnob::Absent);
    }

    /// Retrieval is the daemon's work, not the agent's, and reaches an
    /// external agent as injected prompt text. Classifying it `Absent` would
    /// hide a setting that demonstrably works.
    #[test]
    fn precognition_stays_available_to_an_acp_session() {
        assert_eq!(SessionKnob::Precognition.on_acp(), AcpKnob::Daemon);
        assert_eq!(SessionKnob::PrecognitionResults.on_acp(), AcpKnob::Daemon);
    }

    /// The model is the one knob whose support depends on what the agent
    /// listed at the handshake, so it may never be reported as
    /// unconditionally supported.
    #[test]
    fn the_agent_decides_whether_the_model_can_change() {
        assert_eq!(SessionKnob::Model.on_acp(), AcpKnob::AdvertisedModel);
    }

    /// A token count and a named reasoning level are different things, and
    /// the number that would connect them does not exist. The agent's
    /// `thought_level` selector reaches a client as an agent config option
    /// instead.
    #[test]
    fn the_thinking_budget_is_not_projected_onto_a_thought_level() {
        assert_eq!(SessionKnob::ThinkingBudget.on_acp(), AcpKnob::Absent);
    }
}
