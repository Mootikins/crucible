//! Tests for the typed session-event payloads.
//!
//! Split out of the payload modules: each `*Payload` group is declared there,
//! and the cases that pin its serde shape read apart from the declaration.

use super::*;
use crate::events::session_event::{FileChangeKind, ScriptingEvent};
use crate::events::SessionEvent;
use crate::interaction::{InteractionRequest, PermRequest};
use crate::protocol::SessionEventMessage;
use crate::types::mcp_status::McpServerInfo;
use crate::types::{PluginStatusEntry, ProviderInfo};
use std::collections::BTreeSet;
use std::path::PathBuf;

// ─────────────────────────────────────────────────────────────────────────
// The mechanism
// ─────────────────────────────────────────────────────────────────────────

/// The whole point of adjacent tagging: the typed constructor and the
/// hand-written one produce the same bytes. If this ever fails, every recorded
/// fixture and every persisted `session.jsonl` line is a wire break.
#[test]
fn setup_payloads_are_wire_identical_to_the_old_constructors() {
    let typed = SessionEventMessage::typed(
        "s1",
        SetupPayload::ContextLimitResolved(ContextLimitResolvedPayload {
            limit: 128_000,
            source: ContextLimitSource::Config,
        }),
    );
    let hand =
        SessionEventMessage::context_limit_resolved("s1", 128_000, ContextLimitSource::Config);
    assert_eq!(
        serde_json::to_value(&typed).unwrap(),
        serde_json::to_value(&hand).unwrap(),
    );
}

#[test]
fn to_wire_and_from_wire_round_trip() {
    let payload = SessionEventPayload::Turn(TurnPayload::TextDelta {
        content: "hello".into(),
    });
    let (event, data) = payload.to_wire();
    assert_eq!(event, "text_delta");
    assert_eq!(data, serde_json::json!({"content": "hello"}));

    let back = SessionEventPayload::from_wire(&event, &data).expect("round-trips");
    assert!(matches!(
        back,
        SessionEventPayload::Turn(TurnPayload::TextDelta { .. })
    ));
}

#[test]
fn context_clear_is_a_persisted_turn_marker() {
    let payload = SessionEventPayload::Turn(TurnPayload::ContextCleared {
        plugin: Some("alpha".into()),
    });
    let (name, data) = payload.to_wire();
    assert_eq!(name, "context_cleared");
    assert_eq!(data, serde_json::json!({"plugin": "alpha"}));
    assert!(payload.is_persisted());
    assert!(matches!(
        SessionEventPayload::from_wire(&name, &data),
        Ok(SessionEventPayload::Turn(
            TurnPayload::ContextCleared { .. }
        ))
    ));
}

/// An unknown name is an error that still carries the name, not a lossy
/// `#[serde(other)]` unit variant. Consumers pass `{event, data}` straight
/// through on this path.
#[test]
fn an_unknown_event_reports_its_name_rather_than_being_swallowed() {
    let err = SessionEventPayload::from_wire("invented_by_a_newer_daemon", &serde_json::json!({}))
        .expect_err("unknown names must not decode");
    match err {
        EventDecodeError::UnknownEvent { event } => assert_eq!(event, "invented_by_a_newer_daemon"),
        other => panic!("expected UnknownEvent, got {other:?}"),
    }
}

/// A known name with an undecodable payload is a *different* error, because the
/// consumer behaves differently: warn, rather than pass through.
#[test]
fn a_known_name_with_a_broken_payload_is_malformed_not_unknown() {
    // `request` is the one turn field with no default — an interaction request
    // is unusable without it.
    let err = SessionEventPayload::from_wire("interaction_requested", &serde_json::json!({}))
        .expect_err("a missing request must not decode");
    match err {
        EventDecodeError::MalformedPayload { event, .. } => {
            assert_eq!(event, "interaction_requested")
        }
        other => panic!("expected MalformedPayload, got {other:?}"),
    }
}

/// The trap adjacent tagging sets: a unit variant omits `data`, so `to_wire`
/// reports `null` where today's producers emit `{}`.
#[test]
fn payloadless_workflow_events_keep_an_empty_object_not_null() {
    for payload in [
        WorkflowPayload::WorkflowCompleted {},
        WorkflowPayload::WorkflowCancelled {},
    ] {
        let (event, data) = SessionEventPayload::from(payload).to_wire();
        assert_eq!(
            data,
            serde_json::json!({}),
            "{event}: unit variants would serialize `null` here"
        );
    }
}

/// A consumer that predates the body still decodes the event; one that
/// knows the body reads it.
#[test]
fn notification_added_carries_the_body_when_present_and_tolerates_its_absence() {
    let old: NotificationPayload = serde_json::from_value(serde_json::json!({
        "event": "notification_added", "data": { "notification_id": "n1" }
    }))
    .unwrap();
    assert!(matches!(
        old,
        NotificationPayload::NotificationAdded {
            notification: None,
            ..
        }
    ));

    let new: NotificationPayload = serde_json::from_value(serde_json::json!({
        "event": "notification_added",
        "data": {
            "notification_id": "n1",
            "notification": { "id": "n1", "kind": "toast", "message": "hi" }
        }
    }))
    .unwrap();
    let NotificationPayload::NotificationAdded {
        notification: Some(n),
        ..
    } = new
    else {
        panic!("the body was present on the wire");
    };
    assert_eq!(n.message, "hi");
}

/// Metadata supplies routing and every wire name belongs to exactly one group.
#[test]
fn declared_events_have_unique_routes() {
    use strum::IntoEnumIterator;
    let mut names = BTreeSet::new();
    for group in Group::iter() {
        assert!(!group.wire_names().is_empty());
        for name in group.wire_names() {
            assert!(names.insert(name), "duplicate wire name: {name}");
            assert_eq!(Group::of(name), Some(group));
            assert!(!matches!(
                SessionEventPayload::from_wire(name, &serde_json::json!({})),
                Err(EventDecodeError::UnknownEvent { .. }),
            ));
        }
    }
}

event_payload! {
    #[derive(Debug, serde::Serialize, serde::Deserialize)]
    #[serde(tag = "event", content = "data")]
    enum MetadataProbe {
        /// A name whose spelling cannot be inferred from its Rust identifier.
        "probe:empty" => Empty {},
        "probe.value" => Value { value: u64 },
        "probe_tuple" => Tuple(String),
    }
}

#[test]
fn declared_metadata_matches_serde_for_struct_empty_and_tuple_variants() {
    let cases = [
        (MetadataProbe::Empty {}, serde_json::json!({})),
        (
            MetadataProbe::Value { value: 7 },
            serde_json::json!({"value":7}),
        ),
        (MetadataProbe::Tuple("x".into()), serde_json::json!("x")),
    ];
    let fixtures = ["probe:empty", "probe.value", "probe_tuple"];
    assert_eq!(MetadataProbe::WIRE_NAMES, fixtures);
    for ((value, data), event) in cases.into_iter().zip(fixtures) {
        let fixture = serde_json::json!({"event":event,"data":data});
        assert_eq!(serde_json::to_value(&value).unwrap(), fixture);
        let decoded: MetadataProbe = serde_json::from_value(fixture.clone()).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), fixture);
    }
}

/// Each `SystemPayload` wire-name const must equal what serde writes.
///
/// `#[serde(rename = ...)]` takes a literal, so the name exists twice in
/// `lifecycle.rs`: once in the attribute, once as the const another crate
/// reads. This is the only tie available, and it is a behavioural one — it
/// serializes the variant and reads the `event` field back, so no text scan
/// can satisfy it and a changed rename fails here.
#[test]
fn a_system_events_const_matches_its_serde_name() {
    let cases: [(&str, SystemPayload); 3] = [
        (
            SystemPayload::SURFACE_CHANGED,
            SystemPayload::SurfaceChanged {
                plugin: "kanban".to_string(),
                name: "board".to_string(),
                version: 1,
                session: None,
                withdrawn: false,
            },
        ),
        (
            SystemPayload::PUBLICATION_CHANGED,
            SystemPayload::PublicationChanged {
                plugin: "kanban".to_string(),
                key: "kanban:board".to_string(),
            },
        ),
        (
            SystemPayload::PROPOSAL_CHANGED,
            SystemPayload::ProposalChanged {
                id: crate::proposal::ProposalId::generate(),
            },
        ),
    ];

    for (declared, payload) in cases {
        let wire = serde_json::to_value(&payload).expect("a payload serializes");
        let serde_name = wire["event"]
            .as_str()
            .unwrap_or_else(|| panic!("no `event` field in {wire} — adjacent tagging changed"));
        assert_eq!(
            declared, serde_name,
            "the const and the serde rename disagree; a crate reading the const \
             would filter on a name the daemon never sends"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The fixture sweep — the plan's progress meter, now unconditional
// ─────────────────────────────────────────────────────────────────────────

/// Every recorded fixture is real daemon output. Each distinct `event` in each
/// one must decode into a typed payload; there is no allowlist left.
///
/// This catches *name* drift, not shape drift — 15 fixtures do not contain all
/// 70 names. The per-variant goldens in `rpc.rs` are the shape coverage.
#[test]
fn every_recorded_event_decodes_into_a_typed_payload() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fixtures");
    let mut unaccounted: BTreeSet<String> = BTreeSet::new();
    let mut seen = 0usize;
    let mut names: BTreeSet<String> = BTreeSet::new();

    for entry in std::fs::read_dir(&root).expect("fixtures dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        for line in std::fs::read_to_string(&path).unwrap().lines() {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let Some(name) = v.get("event").and_then(|e| e.as_str()) else {
                continue;
            };
            seen += 1;
            names.insert(name.to_string());
            let data = v.get("data").cloned().unwrap_or(serde_json::Value::Null);
            if let Err(e) = SessionEventPayload::from_wire(name, &data) {
                unaccounted.insert(format!(
                    "{}: {name}: {e}",
                    path.file_name().unwrap().to_string_lossy()
                ));
            }
        }
    }

    assert!(seen > 0, "fixture sweep found no events — wrong path?");
    assert!(
        unaccounted.is_empty(),
        "recorded events that do not decode ({} distinct names seen): {unaccounted:#?}",
        names.len(),
    );
}

// ─────────────────────────────────────────────────────────────────────────
// Persistence and the two vocabularies
// ─────────────────────────────────────────────────────────────────────────

/// Pins the persist set against the list `should_persist` matched by hand
/// before it became a typed match. The original names, less the removed
/// `ended`, plus the ACP late-merge event `tool_call_update` whose absence
/// erased claude-agent-acp's arguments from every replay, and its old name
/// `tool_call_args_update`, which old transcripts hold.
#[test]
fn the_persist_set_is_unchanged_from_the_hand_written_name_list() {
    let persisted = [
        "user_message",
        "thinking",
        "segment_complete",
        "message_complete",
        "tool_call",
        "tool_result",
        "model_switched",
        "precognition_complete",
        "tool_call_update",
        "tool_call_args_update",
    ];
    for name in persisted {
        let payload = SessionEventPayload::from_wire(name, &serde_json::json!({}))
            .unwrap_or_else(|e| panic!("{name} must decode from an empty payload: {e}"));
        assert!(payload.is_persisted(), "{name} must be persisted");
    }

    for name in ["text_delta", "post_llm_call", "context_injected"] {
        let payload = SessionEventPayload::from_wire(name, &serde_json::json!({})).unwrap();
        assert!(!payload.is_persisted(), "{name} persist decision changed");
    }
}

/// `turn_finished` has one wire shape: snake_case status, and no key for an
/// absent stop reason or error. It is persisted, so a replay knows where each
/// turn ended.
#[test]
fn turn_finished_has_a_pinned_wire_shape_and_is_persisted() {
    use crate::turn::{StopReason, TurnStatus};

    let msg = SessionEventMessage::turn_finished(
        "s1",
        TurnStatus::Completed,
        Some(StopReason::MaxTokens),
        None,
    );
    assert_eq!(msg.event, "turn_finished");
    assert_eq!(
        msg.data,
        serde_json::json!({"status": "completed", "stop_reason": "max_tokens"})
    );
    assert!(msg.payload().unwrap().is_persisted());

    let cases = [
        (TurnStatus::Completed, "completed"),
        (TurnStatus::Cancelled, "cancelled"),
        (TurnStatus::HandlerCancelled, "handler_cancelled"),
        (TurnStatus::TimedOut, "timed_out"),
        (TurnStatus::Failed, "failed"),
    ];
    for (status, wire) in cases {
        let msg = SessionEventMessage::turn_finished("s1", status, None, Some("why".into()));
        assert_eq!(
            msg.data,
            serde_json::json!({"status": wire, "error": "why"})
        );
    }
}

/// An event that does not say how the turn ended is malformed, not a
/// completed turn: a default status would report a failure as a success.
#[test]
fn a_turn_finished_without_a_status_is_malformed() {
    let err = SessionEventPayload::from_wire("turn_finished", &serde_json::json!({}))
        .expect_err("a missing status must not decode");
    assert!(matches!(err, EventDecodeError::MalformedPayload { .. }));
}

/// A person's own turn keeps the wire shape it always had, and only a turn a
/// `turn:complete` handler asked for names its origin. The line is persisted,
/// so a replay says who asked for each turn.
#[test]
fn only_a_plugin_turn_names_its_origin_on_the_wire() {
    let user = SessionEventMessage::user_message("s1", "m-1", "hello");
    assert_eq!(
        user.data,
        serde_json::json!({"message_id": "m-1", "content": "hello"})
    );

    let plugin = SessionEventMessage::plugin_message("s1", "m-2", "keep going", "alpha");
    assert_eq!(plugin.event, "user_message");
    assert_eq!(
        plugin.data,
        serde_json::json!({
            "message_id": "m-2",
            "content": "keep going",
            "origin": { "kind": "plugin", "name": "alpha" },
        })
    );
    assert!(plugin.payload().unwrap().is_persisted());
}

/// The origin of a `user_message`, decoded from `data`.
fn origin_of(data: serde_json::Value) -> Result<Option<crate::turn::TurnOrigin>, EventDecodeError> {
    match SessionEventPayload::from_wire("user_message", &data)? {
        SessionEventPayload::Turn(TurnPayload::UserMessage { origin, .. }) => Ok(origin),
        other => panic!("not a user_message: {other:?}"),
    }
}

/// A log from before the nested origin still names the plugin. An origin
/// that names no plugin does not decode, so no client reads it as a person.
#[test]
fn an_old_flat_origin_decodes_as_the_nested_one() {
    use crate::turn::TurnOrigin;
    let flat = serde_json::json!({"message_id": "m", "content": "c", "origin": "plugin", "plugin": "goal"});
    assert_eq!(
        origin_of(flat).unwrap(),
        Some(TurnOrigin::Plugin("goal".into()))
    );
    let user = serde_json::json!({"message_id": "m", "content": "c", "origin": "user"});
    assert_eq!(origin_of(user).unwrap(), Some(TurnOrigin::User));
    let nested = serde_json::json!({"message_id": "m", "content": "c", "origin": {"kind": "user"}});
    assert_eq!(origin_of(nested).unwrap(), Some(TurnOrigin::User));
    let nameless = serde_json::json!({"message_id": "m", "content": "c", "origin": "plugin"});
    assert!(matches!(
        origin_of(nameless),
        Err(EventDecodeError::MalformedPayload { .. })
    ));
}

/// A `session_initialized` whose model is empty must NOT be persisted: the setup
/// task runs before `session.configure_agent`, so it almost always carries `""`,
/// and an empty model on resume looks like an answer.
#[test]
fn a_session_initialized_is_persisted_only_once_the_model_is_known() {
    let with_model = SessionEventPayload::from_wire(
        "session_initialized",
        &serde_json::json!({
            "model": "glm-5", "mode": "ask", "agent_name": null,
            "kilns": ["notes"], "workspace_path": "/w",
        }),
    )
    .unwrap();
    assert!(with_model.is_persisted());

    let without = SessionEventPayload::from_wire(
        "session_initialized",
        &serde_json::json!({
            "model": "", "mode": "ask", "agent_name": null,
            "kilns": ["notes"], "workspace_path": "/w",
        }),
    )
    .unwrap();
    assert!(!without.is_persisted());
}

/// `ALL` is hand-written; the compiler does not check it. `EnumIter` walks what
/// the compiler *does* know.
#[test]
fn every_scripting_event_variant_is_listed() {
    use strum::IntoEnumIterator;
    let listed: Vec<ScriptingEvent> = ScriptingEvent::ALL.to_vec();
    let known: Vec<ScriptingEvent> = ScriptingEvent::iter().collect();
    assert_eq!(listed, known, "ScriptingEvent::ALL is missing a variant");
}

/// Every shared name with a scripting event is the one that event reports.
///
/// The correspondence this file used to test — `as_scripting_event` against
/// `event_type` — is now a type identity: both read the name off
/// [`ScriptingEvent`], so they cannot disagree and nothing needs to check it.
///
/// What is left to check is the arms themselves: an `event_type` arm written
/// back as a bare literal silently reopens the drift. Red-proofed by doing
/// exactly that — `Self::MessageReceived { .. } => "user_message"` fails here.
#[test]
fn every_scripting_name_is_one_an_event_reports() {
    for scripting in ScriptingEvent::ALL {
        let Some(event) = event_reporting(*scripting) else {
            continue;
        };
        assert_eq!(
            event.event_type(),
            scripting.as_str(),
            "`{scripting}` is not the name its own event reports"
        );
    }
}

/// The `SessionEvent` that reports each [`ScriptingEvent`], for the round
/// trip above.
///
/// Exhaustive on purpose: a variant added to the shared set must be placed
/// here, or this does not compile. `None` is a name the wire reports through
/// `as_scripting_event` with no scripting-side event behind it; plan T3-B7
/// removed those variants because nothing constructed them.
fn event_reporting(scripting: ScriptingEvent) -> Option<SessionEvent> {
    match scripting {
        ScriptingEvent::MessageReceived => Some(SessionEvent::MessageReceived {
            content: String::new(),
            participant_id: String::new(),
        }),
        ScriptingEvent::InteractionRequested => Some(SessionEvent::InteractionRequested {
            request_id: String::new(),
            request: InteractionRequest::Permission(PermRequest::bash(["true"])),
        }),
        ScriptingEvent::PrecognitionComplete
        | ScriptingEvent::TextDelta
        | ScriptingEvent::AgentThinking
        | ScriptingEvent::AgentResponded
        | ScriptingEvent::ToolCalled
        | ScriptingEvent::ToolCompleted
        | ScriptingEvent::InteractionCompleted => None,
    }
}

/// The transport-only events say so rather than inventing a name.
#[test]
fn a_transport_only_event_has_no_scripting_name() {
    assert!(TurnPayload::PostLlmCall {
        response_summary: String::new(),
        model: String::new(),
        duration_ms: 0,
    }
    .as_scripting_event()
    .is_none());
}

// ─────────────────────────────────────────────────────────────────────────
// Field-level shapes
// ─────────────────────────────────────────────────────────────────────────

/// The producer wrote `kind` with `format!("{kind}")` and the consumer matched
/// two string arms. Typing it is only wire-safe if `Serialize` agrees with
/// `Display`.
#[test]
fn file_change_kind_serializes_exactly_as_it_displays() {
    for kind in [FileChangeKind::Created, FileChangeKind::Modified] {
        assert_eq!(
            serde_json::to_value(kind).unwrap(),
            serde_json::Value::String(kind.to_string()),
        );
    }
}

/// `file_moved` carries `from`/`to` and no `path`. The old consumer read
/// `data["path"]` before matching the name, so this event could never reach a
/// Lua handler.
#[test]
fn file_moved_decodes_from_its_own_from_to_payload() {
    let payload = SessionEventPayload::from_wire(
        "file_moved",
        &serde_json::json!({"from": "/w/a.md", "to": "/w/b.md"}),
    )
    .expect("file_moved must decode");
    match payload {
        SessionEventPayload::System(SystemPayload::FileMoved { from, to }) => {
            assert_eq!(from, PathBuf::from("/w/a.md"));
            assert_eq!(to, PathBuf::from("/w/b.md"));
        }
        other => panic!("expected FileMoved, got {other:?}"),
    }
}

/// The four `data.result` shapes `tool_call.rs` produces, pinned. Untagged
/// decoding depends on `result` and `error` staying disjoint.
#[test]
fn tool_result_body_covers_every_shape_the_daemon_produces() {
    let bare = serde_json::json!({"result": "ok"});
    assert!(matches!(
        ToolResultBody::of(&bare),
        Some(ToolResultBody::Ok {
            spill_path: None,
            render: None,
            ..
        })
    ));

    let spilled = serde_json::json!({"result": "[900 lines, 12KB — full output in …]", "spill_path": "/s/t/1"});
    match ToolResultBody::of(&spilled) {
        Some(ToolResultBody::Ok { spill_path, .. }) => {
            assert_eq!(spill_path.as_deref(), Some("/s/t/1"))
        }
        other => panic!("expected Ok with spill_path, got {other:?}"),
    }

    let rendered = serde_json::json!({"result": "ok", "render": {"summary": "read 3 files"}});
    let summary = ToolResultBody::of(&rendered).and_then(|b| b.render()?.summary.clone());
    assert_eq!(summary.as_deref(), Some("read 3 files"));

    let failed = serde_json::json!({"error": "User denied permission"});
    assert_eq!(
        ToolResultBody::of(&failed).as_ref().and_then(|b| b.error()),
        Some("User denied permission"),
    );

    // A bare string body is neither variant, and the caller keeps its fallback.
    assert!(ToolResultBody::of(&serde_json::json!("plain text")).is_none());
}

// ─────────────────────────────────────────────────────────────────────────
// Setup payload shapes (pre-existing coverage, unchanged)
// ─────────────────────────────────────────────────────────────────────────

#[test]
fn session_initialized_shape() {
    let p = SessionInitializedPayload {
        model: "glm-5".into(),
        mode: "ask".into(),
        agent_name: None,
        kilns: vec![crate::config::KilnName::parse("notes").unwrap()],
        workspace_path: PathBuf::from("/w"),
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["model"], "glm-5");
    assert_eq!(v["mode"], "ask");
    assert!(v["agent_name"].is_null());
    assert_eq!(v["workspace_path"], "/w");
    // Names, and only names. This payload is broadcast to every subscriber and
    // persisted into `session.jsonl` once the model is known; it used to carry
    // `kiln_path`, the resolved directory of whichever kiln sorted first.
    assert_eq!(v["kilns"], serde_json::json!(["notes"]));
    assert!(
        v.get("kiln_path").is_none(),
        "the kiln directory is gone from the announcement: {v}"
    );
}

/// A session that reaches no kiln announces an EMPTY set, not a path.
///
/// The producer used to spell this `.next().unwrap_or_default()`, so a
/// kiln-less session announced `""` — which every path helper downstream reads
/// as the daemon's own data directory.
#[test]
fn a_kiln_less_session_announces_no_kilns_rather_than_the_empty_path() {
    let p = SessionInitializedPayload {
        model: "glm-5".into(),
        mode: "ask".into(),
        agent_name: None,
        kilns: Vec::new(),
        workspace_path: PathBuf::from("/w"),
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["kilns"], serde_json::json!([]));
    let rendered = serde_json::to_string(&v).unwrap();
    assert!(
        !rendered.contains(r#""""#),
        "no empty string stands in for a kiln: {rendered}"
    );
}

#[test]
fn session_initialized_shape_with_agent() {
    let p = SessionInitializedPayload {
        model: "sonnet-4".into(),
        mode: "plan".into(),
        agent_name: Some("claude".into()),
        kilns: vec![crate::config::KilnName::parse("kiln").unwrap()],
        workspace_path: PathBuf::from("/ws"),
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["agent_name"], "claude");
}

#[test]
fn providers_listed_shape() {
    let p = ProvidersListedPayload {
        providers: vec![ProviderInfo {
            name: "OpenAI".into(),
            provider_type: "openai".into(),
            available: true,
            default_model: Some("gpt-4o".into()),
            models: vec!["gpt-4o".into()],
            endpoint: Some("https://api.openai.com/v1".into()),
            reason: Some("config".into()),
            is_local: false,
        }],
    };
    let v = serde_json::to_value(&p).unwrap();
    assert!(v["providers"].is_array());
    assert_eq!(v["providers"][0]["name"], "OpenAI");
    assert_eq!(v["providers"][0]["provider_type"], "openai");
    assert_eq!(v["providers"][0]["available"], true);
    assert_eq!(v["providers"][0]["is_local"], false);
}

#[test]
fn context_limit_resolved_shape() {
    let p = ContextLimitResolvedPayload {
        limit: 128_000,
        source: ContextLimitSource::ProviderApi,
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["limit"], 128_000);
    assert_eq!(v["source"], "provider_api");
}

#[test]
fn context_limit_source_snake_case() {
    assert_eq!(
        serde_json::to_value(ContextLimitSource::ProviderApi).unwrap(),
        serde_json::Value::String("provider_api".into()),
    );
    assert_eq!(
        serde_json::to_value(ContextLimitSource::Config).unwrap(),
        serde_json::Value::String("config".into()),
    );
    assert_eq!(
        serde_json::to_value(ContextLimitSource::Default).unwrap(),
        serde_json::Value::String("default".into()),
    );

    // round-trip deserialization
    let back: ContextLimitSource =
        serde_json::from_value(serde_json::Value::String("provider_api".into())).unwrap();
    assert_eq!(back, ContextLimitSource::ProviderApi);
}

/// An older `cru` against a newer daemon must still render the limit.
///
/// Without the `#[serde(other)]` arm an unrecognised source failed the *whole*
/// payload decode, so `chat_runner/commands.rs` warned and dropped the event —
/// the client lost a number it could render perfectly well because it did not
/// recognise the label saying where the number came from. The source is not
/// rendered anywhere; the limit is.
#[test]
fn an_unknown_source_from_a_newer_daemon_still_yields_the_limit() {
    let payload: ContextLimitResolvedPayload = serde_json::from_value(serde_json::json!({
        "limit": 200_000,
        "source": "some_source_invented_after_this_build",
    }))
    .expect("an unknown source must not fail the whole payload");

    assert_eq!(payload.limit, 200_000);
    assert_eq!(payload.source, ContextLimitSource::Unknown);
}

#[test]
fn workspace_indexed_shape() {
    let p = WorkspaceIndexedPayload {
        files: vec!["src/lib.rs".into(), "README.md".into()],
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["files"], serde_json::json!(["src/lib.rs", "README.md"]));
}

#[test]
fn kiln_notes_indexed_shape() {
    let p = KilnNotesIndexedPayload {
        notes: vec!["Daily/2026-04-17.md".into()],
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["notes"], serde_json::json!(["Daily/2026-04-17.md"]));
}

#[test]
fn plugins_discovered_shape() {
    let p = PluginsDiscoveredPayload {
        plugins: vec![
            PluginStatusEntry {
                name: "kiln-expert".into(),
                version: Some("0.1.0".into()),
                state: "loaded".into(),
                error: None,
            },
            // Discovered, not loaded: nothing has read its spec table, so the
            // host knows no version. The wire says so with `null` — a front
            // end can render "unknown", which it cannot do for "0.0.0".
            PluginStatusEntry {
                name: "not-loaded-yet".into(),
                version: None,
                state: "discovered".into(),
                error: None,
            },
        ],
    };
    let v = serde_json::to_value(&p).unwrap();
    assert!(v["plugins"].is_array());
    assert_eq!(v["plugins"][0]["name"], "kiln-expert");
    assert_eq!(v["plugins"][0]["version"], "0.1.0");
    assert_eq!(v["plugins"][0]["state"], "loaded");
    assert!(v["plugins"][0]["error"].is_null());
    assert_eq!(v["plugins"][1]["name"], "not-loaded-yet");
    // The key has to be PRESENT and null. `v["..."]["version"]` reads a
    // missing key as null too, so a `skip_serializing_if` that dropped the
    // field would satisfy a null check alone while a front end saw
    // `undefined` and drew whatever that renders as.
    let unloaded = v["plugins"][1].as_object().expect("an entry object");
    assert!(
        unloaded.contains_key("version"),
        "the unloaded entry must carry the version key: {v}"
    );
    assert!(
        unloaded["version"].is_null(),
        "an unloaded plugin must report no version: {v}"
    );
}

#[test]
fn mcp_servers_ready_shape() {
    let p = McpServersReadyPayload {
        servers: vec![McpServerInfo {
            name: "context7".into(),
            prefix: "c7".into(),
            tools: vec!["query-docs".into(), "resolve-library-id".into()],
            connected: true,
        }],
    };
    let v = serde_json::to_value(&p).unwrap();
    assert!(v["servers"].is_array());
    assert_eq!(v["servers"][0]["name"], "context7");
    assert_eq!(v["servers"][0]["prefix"], "c7");
    assert_eq!(v["servers"][0]["connected"], true);
    assert_eq!(
        v["servers"][0]["tools"],
        serde_json::json!(["query-docs", "resolve-library-id"]),
    );
}
