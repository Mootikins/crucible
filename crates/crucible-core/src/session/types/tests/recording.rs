use super::super::enums::{RecordingMode, SessionType};
use super::super::session::Session;
use std::path::PathBuf;

#[test]
fn test_recording_mode_serialization() {
    // RecordingMode::Granular should serialize to "granular"
    let granular = RecordingMode::Granular;
    let json = serde_json::to_string(&granular).unwrap();
    assert_eq!(json, "\"granular\"");

    // RecordingMode::Coarse should serialize to "coarse"
    let coarse = RecordingMode::Coarse;
    let json = serde_json::to_string(&coarse).unwrap();
    assert_eq!(json, "\"coarse\"");
}

#[test]
fn test_session_recording_mode_roundtrip() {
    // Create Session with recording_mode, serialize, deserialize, verify
    let kiln = PathBuf::from("/home/user/notes");
    let session =
        Session::new(SessionType::Chat, kiln).with_recording_mode(RecordingMode::Granular);

    let json = serde_json::to_string(&session).unwrap();
    assert!(json.contains("\"recording_mode\":\"granular\""));

    let parsed: Session = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.recording_mode, Some(RecordingMode::Granular));
}

#[test]
fn test_session_is_granular() {
    let kiln = PathBuf::from("/home/user/notes");

    // Granular mode returns true
    let granular_session =
        Session::new(SessionType::Chat, kiln.clone()).with_recording_mode(RecordingMode::Granular);
    assert!(granular_session.is_granular());

    // Coarse mode returns false
    let coarse_session =
        Session::new(SessionType::Chat, kiln.clone()).with_recording_mode(RecordingMode::Coarse);
    assert!(!coarse_session.is_granular());

    // None returns false
    let no_mode_session = Session::new(SessionType::Chat, kiln);
    assert!(!no_mode_session.is_granular());
}

#[test]
fn test_session_recording_jsonl_path() {
    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln);

    assert_eq!(session.recording_jsonl_path(), "recording.jsonl");
}

#[test]
fn test_session_recording_mode_omitted_when_none() {
    // When recording_mode is None, it should be omitted from JSON
    let kiln = PathBuf::from("/home/user/notes");
    let session = Session::new(SessionType::Chat, kiln);

    let json = serde_json::to_string(&session).unwrap();
    assert!(!json.contains("recording_mode"));
}

#[test]
fn test_session_recording_mode_backward_compat_old_json_without_field() {
    // Old JSON without recording_mode should deserialize to None
    let old_json = r#"{
        "id": "chat-2025-01-08T1530-abc123",
        "session_type": "chat",
        "kiln": "/home/user/notes",
        "workspace": "/home/user/notes",
        "state": "active",
        "started_at": "2025-01-08T15:30:00Z"
    }"#;

    let session: Session = serde_json::from_str(old_json).unwrap();
    assert_eq!(session.recording_mode, None);
    assert!(!session.is_granular());
}
