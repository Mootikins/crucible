//! Tests for the typed session-event payloads.
//!
//! Split out of the payload modules to keep each inside the 1000-line module
//! budget enforced by `no_new_oversized_modules`.

use super::*;
use crate::events::session_event::{FileChangeKind, InternalSessionEvent, ScriptingEvent};
use crate::events::SessionEvent;
use crate::interaction::{InteractionRequest, InteractionResponse, PermRequest};
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

/// The eight group enums, source-scanned. Order matters only for the error
/// message.
const GROUP_ENUMS: &[(&str, &str)] = &[
    ("TurnPayload", include_str!("turn.rs")),
    ("SetupPayload", include_str!("setup.rs")),
    ("SettingsPayload", include_str!("settings.rs")),
    ("JobPayload", include_str!("lifecycle.rs")),
    ("ReviewPayload", include_str!("lifecycle.rs")),
    ("NotificationPayload", include_str!("lifecycle.rs")),
    ("WorkflowPayload", include_str!("lifecycle.rs")),
    ("SystemPayload", include_str!("lifecycle.rs")),
];

/// Wire names of a group enum's variants: the `#[serde(rename)]` where one is
/// present, else the variant ident snake_cased.
fn variant_wire_names(src: &str, enum_name: &str) -> BTreeSet<String> {
    let needle = format!("pub enum {enum_name} {{\n");
    let start = src
        .find(&needle)
        .unwrap_or_else(|| panic!("`pub enum {enum_name} {{` not found — did the enum move?"))
        + needle.len();
    let body = &src[start..];
    let end = body
        .find("\n}\n")
        .unwrap_or_else(|| panic!("unterminated enum {enum_name}"));

    let mut names = BTreeSet::new();
    let mut depth = 0i32;
    let mut pending_rename: Option<String> = None;
    for line in body[..end].lines() {
        let trimmed = line.trim();
        if depth == 0 {
            if let Some(rest) = trimmed.strip_prefix("#[serde(rename = \"") {
                if let Some(name) = rest.split('"').next() {
                    pending_rename = Some(name.to_string());
                }
            } else if trimmed
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_uppercase())
            {
                let ident: String = trimmed
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                names.insert(pending_rename.take().unwrap_or_else(|| snake_case(&ident)));
            }
        }
        depth += line.matches(['{', '(']).count() as i32;
        depth -= line.matches(['}', ')']).count() as i32;
    }
    names
}

fn snake_case(ident: &str) -> String {
    let mut out = String::new();
    for (i, c) in ident.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// `Group::of` decides which enum a name decodes into, so a declared variant
/// it does not know is an event nothing can decode — `payload()` returns
/// `UnknownEvent` and the consumer takes its passthrough arm, so the feature
/// silently disappears.
///
/// Derived from the enums rather than from a hand-maintained name list. The
/// list this used to read (`EVENT_NAMES`) had no non-test consumer: it existed
/// so this test could diff it against the variants that were already the source
/// of truth.
#[test]
fn group_of_knows_every_declared_event() {
    let mut declared = BTreeSet::new();
    for (name, src) in GROUP_ENUMS {
        let group = variant_wire_names(src, name);
        assert!(
            !group.is_empty(),
            "{name}: extracted no variants — the scan markers moved, fix this test"
        );
        declared.extend(group);
    }
    let unknown: Vec<_> = declared
        .iter()
        .filter(|n| Group::of(n).is_none())
        .cloned()
        .collect();
    assert!(
        unknown.is_empty(),
        "declared by a payload enum but absent from Group::of: {unknown:?}"
    );
}

/// And the reverse: each name must land in the group whose enum declares it.
#[test]
fn group_of_routes_each_name_to_the_enum_that_declares_it() {
    let expected = [
        (Group::Turn, "TurnPayload"),
        (Group::Setup, "SetupPayload"),
        (Group::Settings, "SettingsPayload"),
        (Group::Job, "JobPayload"),
        (Group::Review, "ReviewPayload"),
        (Group::Notification, "NotificationPayload"),
        (Group::Workflow, "WorkflowPayload"),
        (Group::System, "SystemPayload"),
    ];
    for (group, enum_name) in expected {
        let src = GROUP_ENUMS
            .iter()
            .find(|(n, _)| *n == enum_name)
            .expect("group enum listed")
            .1;
        for name in variant_wire_names(src, enum_name) {
            assert_eq!(
                Group::of(&name),
                Some(group),
                "`{name}` is declared by {enum_name} but Group::of sends it elsewhere"
            );
        }
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
/// before it became a typed match. Same nine names, same answers.
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
        "ended",
        "precognition_complete",
    ];
    for name in persisted {
        let payload = SessionEventPayload::from_wire(name, &serde_json::json!({}))
            .unwrap_or_else(|e| panic!("{name} must decode from an empty payload: {e}"));
        assert!(payload.is_persisted(), "{name} must be persisted");
    }

    for name in ["text_delta", "post_llm_call", "context_injected", "ended"] {
        let payload = SessionEventPayload::from_wire(name, &serde_json::json!({})).unwrap();
        assert_eq!(
            payload.is_persisted(),
            name == "ended",
            "{name} persist decision changed"
        );
    }
}

/// A `session_initialized` whose model is empty must NOT be persisted: the setup
/// task runs before `session.configure_agent`, so it almost always carries `""`,
/// and an empty model on resume looks like an answer.
#[test]
fn a_session_initialized_is_persisted_only_once_the_model_is_known() {
    let with_model = SessionEventPayload::from_wire(
        "session_initialized",
        &serde_json::json!({
            "model": "glm-5", "mode": "normal", "agent_name": null,
            "kilns": ["notes"], "workspace_path": "/w",
        }),
    )
    .unwrap();
    assert!(with_model.is_persisted());

    let without = SessionEventPayload::from_wire(
        "session_initialized",
        &serde_json::json!({
            "model": "", "mode": "normal", "agent_name": null,
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

/// Every shared name is the one its own event reports.
///
/// The correspondence this file used to test — `as_scripting_event` against
/// `event_type` — is now a type identity: both read the name off
/// [`ScriptingEvent`], so they cannot disagree and nothing needs to check it.
/// That test `include_str!`d two files, sliced each between literal markers,
/// and collected every quoted lowercase-ish literal it found, with a
/// `len() > 20` canary admitting the extraction was fragile.
///
/// What is left to check is the arms themselves: an `event_type` arm written
/// back as a bare literal silently reopens the drift. Red-proofed by doing
/// exactly that — `Self::ToolCalled { .. } => "tool_call"` fails here.
#[test]
fn every_scripting_name_is_one_an_event_reports() {
    for scripting in ScriptingEvent::ALL {
        let event = event_reporting(*scripting);
        assert_eq!(
            event.event_type(),
            scripting.as_str(),
            "`{scripting}` is not the name its own event reports"
        );
    }
}

/// One `SessionEvent` per [`ScriptingEvent`], for the round trip above.
///
/// Exhaustive on purpose: a variant added to the shared set must be given an
/// event that reports it, or this does not compile.
fn event_reporting(scripting: ScriptingEvent) -> SessionEvent {
    match scripting {
        ScriptingEvent::MessageReceived => SessionEvent::MessageReceived {
            content: String::new(),
            participant_id: String::new(),
        },
        ScriptingEvent::TextDelta => SessionEvent::TextDelta {
            delta: String::new(),
            seq: 0,
        },
        ScriptingEvent::AgentThinking => SessionEvent::AgentThinking {
            thought: String::new(),
        },
        ScriptingEvent::AgentResponded => SessionEvent::AgentResponded {
            content: String::new(),
            tool_calls: Vec::new(),
        },
        ScriptingEvent::ToolCalled => SessionEvent::ToolCalled {
            name: String::new(),
            args: serde_json::Value::Null,
            description: None,
            source: None,
        },
        ScriptingEvent::ToolCompleted => SessionEvent::ToolCompleted {
            name: String::new(),
            result: String::new(),
            error: None,
            terminate: false,
        },
        ScriptingEvent::SessionEnded => SessionEvent::SessionEnded {
            reason: String::new(),
        },
        ScriptingEvent::InteractionRequested => SessionEvent::InteractionRequested {
            request_id: String::new(),
            request: InteractionRequest::Permission(PermRequest::bash(["true"])),
        },
        ScriptingEvent::InteractionCompleted => SessionEvent::InteractionCompleted {
            request_id: String::new(),
            response: InteractionResponse::Cancelled,
        },
        ScriptingEvent::PrecognitionComplete => {
            SessionEvent::internal(InternalSessionEvent::PrecognitionComplete {
                notes_count: 0,
                query_summary: String::new(),
                kilns_searched: 0,
                kilns_filtered: 0,
                kilns_failed: 0,
            })
        }
    }
}

/// The transport-only events say so rather than inventing a name.
#[test]
fn a_transport_only_event_has_no_scripting_name() {
    assert!(TurnPayload::PostLlmCall {
        response_summary: String::new(),
        model: String::new(),
        duration_ms: 0,
        token_count: None,
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
            summary: None,
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

    let summarized = serde_json::json!({"result": "ok", "summary": "read 3 files"});
    match ToolResultBody::of(&summarized) {
        Some(ToolResultBody::Ok { summary, .. }) => {
            assert_eq!(summary.as_deref(), Some("read 3 files"))
        }
        other => panic!("expected Ok with summary, got {other:?}"),
    }

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
        mode: "normal".into(),
        agent_name: None,
        kilns: vec![crate::config::KilnName::parse("notes").unwrap()],
        workspace_path: PathBuf::from("/w"),
    };
    let v = serde_json::to_value(&p).unwrap();
    assert_eq!(v["model"], "glm-5");
    assert_eq!(v["mode"], "normal");
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
        mode: "normal".into(),
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
        plugins: vec![PluginStatusEntry {
            name: "kiln-expert".into(),
            version: "0.1.0".into(),
            state: "loaded".into(),
            error: None,
        }],
    };
    let v = serde_json::to_value(&p).unwrap();
    assert!(v["plugins"].is_array());
    assert_eq!(v["plugins"][0]["name"], "kiln-expert");
    assert_eq!(v["plugins"][0]["version"], "0.1.0");
    assert_eq!(v["plugins"][0]["state"], "loaded");
    assert!(v["plugins"][0]["error"].is_null());
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
