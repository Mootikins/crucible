//! Session identifiers, re-exported from their canonical home.
//!
//! This module used to define a second `SessionId` that validated a *format* —
//! `{type}-{YYYYMMDD}-{HHMM}-{4 hex}` — rather than a path component. It was
//! worse than redundant. `Session::new` has always minted
//! `chat-2026-01-04T1530-a1b2c3`, which that parser rejects, so every consumer
//! filtering through it (`list_sessions`, the three observe handlers before
//! they stopped using it) silently matched none of a running daemon's sessions
//! — a validator with no true positives and no security value, sitting next to
//! the one place a session id genuinely needs checking.
//!
//! There is now one [`SessionId`], in `crucible-core`, and it checks the
//! property the joins actually depend on: that the id is a single ordinary
//! path component.

pub use crucible_core::session::InvalidSessionId as SessionIdError;
pub use crucible_core::session::{SessionId, SessionType};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_minted_id_is_accepted_by_the_validator_that_guards_the_sessions_root() {
        // The regression this module existed to cause: the ids the daemon
        // writes were rejected by the parser meant to recognize them.
        let id = SessionId::generate(SessionType::Chat);
        assert_eq!(SessionId::parse(id.as_str()).unwrap(), id);
    }

    #[test]
    fn test_session_type_display() {
        assert_eq!(SessionType::Chat.to_string(), "chat");
        assert_eq!(SessionType::Workflow.to_string(), "workflow");
        assert_eq!(SessionType::Agent.to_string(), "agent");
    }

    #[test]
    fn test_session_type_parse() {
        assert_eq!("chat".parse::<SessionType>().unwrap(), SessionType::Chat);
        assert_eq!(
            "workflow".parse::<SessionType>().unwrap(),
            SessionType::Workflow
        );
        assert_eq!("agent".parse::<SessionType>().unwrap(), SessionType::Agent);
        assert!("unknown".parse::<SessionType>().is_err());
    }
}
