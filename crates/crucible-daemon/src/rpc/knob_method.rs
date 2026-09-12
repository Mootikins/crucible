//! Which JSON-RPC method writes each session knob.
//!
//! A knob's write method is NOT its id with a `session.set_` prefix.
//! [`SessionKnob::Model`] is `session.switch_model`, which carries no `set_`
//! prefix at all. A gate that looked for the prefix therefore never saw the
//! model knob, and neither front end was ever checked for it.
//!
//! So the mapping is declared here, once, as a total function of the knob
//! identity. The `match` carries no wildcard arm, and the two module-level
//! denies below close the other way out: an arm added as `_ =>
//! RpcMethod::SessionSetMode` would give a new knob a wrong method with
//! nobody deciding. Both lints are necessary. Clippy reports a wildcard that
//! covers one remaining variant as `match_wildcard_for_single_variants` and
//! only a wildcard that covers two or more as `wildcard_enum_match_arm`, and
//! knobs arrive one at a time.
//!
//! The return type is [`RpcMethod`], not a string. A string can name a method
//! the daemon does not answer; an [`RpcMethod`] cannot, because every variant
//! comes from the one `rpc_methods!` table that also builds `METHODS`.
//! [`RpcMethod`] has no `Default`, for the reason `AcpKnob` has none: a
//! default would make "unclassified" mean something, and the only safe
//! meaning is "someone decides".

#![deny(clippy::wildcard_enum_match_arm)]
#![deny(clippy::match_wildcard_for_single_variants)]

use crucible_core::types::SessionKnob;

use super::RpcMethod;

/// The method a client calls to write `knob`.
///
/// Total over [`SessionKnob`]: a knob added later does not compile until
/// someone names its method.
#[must_use]
pub fn rpc_set_method(knob: SessionKnob) -> RpcMethod {
    match knob {
        // No `set_` prefix. The method predates the knob vocabulary and the
        // name is on the wire, so the gate learns the exception instead.
        SessionKnob::Model => RpcMethod::SessionSwitchModel,
        SessionKnob::Mode => RpcMethod::SessionSetMode,
        SessionKnob::ContextStrategy => RpcMethod::SessionSetContextStrategy,
        SessionKnob::Precognition => RpcMethod::SessionSetPrecognition,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rpc::METHODS;
    use std::collections::BTreeSet;

    /// Every knob names a method the daemon advertises.
    ///
    /// Walks `SessionKnob::ALL`. `crucible-core` derives `EnumIter` only under
    /// `cfg(test)`, so the iterator is not visible from this crate; core's own
    /// `the_all_array_lists_every_variant` proves the array holds every
    /// variant, and this test then proves each one reaches a real method. The
    /// join against `METHODS` is what `daemon.capabilities` reports, so a knob
    /// whose method is not advertised cannot be reached by any client.
    #[test]
    fn every_knob_names_an_advertised_method() {
        for knob in SessionKnob::ALL.iter().copied() {
            let method = rpc_set_method(knob).as_str();
            assert!(
                METHODS.contains(&method),
                "knob `{}` maps to `{method}`, which METHODS does not advertise",
                knob.id()
            );
        }
    }

    /// Two knobs that share a write method would make one of them
    /// unreachable, and the gates downstream would still pass.
    #[test]
    fn no_two_knobs_share_a_method() {
        let methods: Vec<RpcMethod> = SessionKnob::ALL
            .iter()
            .copied()
            .map(rpc_set_method)
            .collect();
        let unique: BTreeSet<RpcMethod> = methods.iter().copied().collect();
        assert_eq!(
            methods.len(),
            unique.len(),
            "two knobs share a write method: {methods:?}"
        );
    }

    /// The model knob is the reason this module exists. Its method carries no
    /// `set_` prefix, so a prefix scan misses it.
    #[test]
    fn the_model_knob_keeps_its_prefixless_method() {
        assert_eq!(
            rpc_set_method(SessionKnob::Model),
            RpcMethod::SessionSwitchModel
        );
        assert!(!RpcMethod::SessionSwitchModel.as_str().contains(".set_"));
    }
}
