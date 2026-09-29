//! The golden request fixtures: the JSON that each request type writes.
//!
//! Each fixture in `assets/fixtures/golden/requests/` holds the params of one
//! RPC request shape, as the client sends them. A test builds the value with
//! the request type, writes it, and compares the JSON with the fixture. The
//! test then reads the fixture back and writes it again, so the daemon side
//! reads the same fields.
//!
//! A difference is a wire change. To write the fixtures again after an
//! intended change, run the tests with `CRUCIBLE_WRITE_GOLDEN=1`.

use super::*;
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fixtures/golden/requests")
        .join(format!("{name}.json"))
}

/// Compare the JSON of each case with the fixture `name`.
///
/// `cases` holds one value with every optional field set and, where the type
/// has optional fields, one value with none of them set.
fn golden<T: Serialize + DeserializeOwned>(name: &str, cases: &[T]) {
    let actual = Value::Array(
        cases
            .iter()
            .map(|case| serde_json::to_value(case).expect("a request writes JSON"))
            .collect(),
    );
    let path = fixture_path(name);
    if std::env::var_os("CRUCIBLE_WRITE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().expect("a fixture has a directory"))
            .expect("the fixture directory is writable");
        let text = serde_json::to_string_pretty(&actual).expect("JSON writes") + "\n";
        std::fs::write(&path, text).expect("the fixture is writable");
    }
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let expected: Value = serde_json::from_str(&text).expect("the fixture is JSON");
    assert_eq!(
        actual,
        expected,
        "the request JSON differs from {}. The wire changed",
        path.display()
    );
    for case in expected.as_array().expect("a fixture is an array") {
        let read: T = serde_json::from_value(case.clone())
            .unwrap_or_else(|e| panic!("{name}: the fixture does not read back: {e}"));
        assert_eq!(
            &serde_json::to_value(read).expect("a request writes JSON"),
            case,
            "{name}: the fixture does not survive a read and a write"
        );
    }
}

fn scope() -> crate::storage::Scope {
    crate::storage::Scope::Workspace {
        path: PathBuf::from("/work/space"),
    }
}

fn branch() -> crate::diff::DiffsetSource {
    serde_json::from_value(json!({
        "kind": "branch",
        "root": "/repo",
        "base": "main",
        "head": "topic",
    }))
    .expect("a branch source reads")
}

fn kiln_name() -> crate::config::KilnName {
    serde_json::from_value(json!("docs")).expect("a kiln name reads")
}

#[test]
fn note_ref_methods() {
    // `get_note_by_name` and `get_backlinks`.
    golden(
        "note_ref",
        &[
            NoteRef {
                kiln: "/kiln".into(),
                name: "Note".into(),
                scope: Some(scope()),
            },
            NoteRef {
                kiln: "/kiln".into(),
                name: "Note".into(),
                scope: None,
            },
        ],
    );
}

#[test]
fn kiln_ref_methods() {
    // `kiln.graph` and `note.list`.
    golden(
        "kiln_ref",
        &[
            KilnRef {
                kiln: "/kiln".into(),
                scope: Some(scope()),
            },
            KilnRef {
                kiln: "/kiln".into(),
                scope: None,
            },
        ],
    );
}

#[test]
fn diffset_ref_methods() {
    // `diff.get` and `diff.comments`.
    golden("diffset_ref", &[DiffsetRef { source: branch() }]);
}

#[test]
fn diff_comment_key_methods() {
    // `diff.resolve_comment` and `diff.delete_comment`.
    golden(
        "diff_comment_key",
        &[DiffCommentKey {
            source: branch(),
            comment_id: "c1".into(),
        }],
    );
}

#[test]
fn diff_comment_method() {
    golden(
        "diff.comment",
        &[
            DiffCommentRequest {
                source: branch(),
                root: Some(crate::session::PhysicalRoot::from_top_level("/repo")),
                path: "a.md".into(),
                from: Some("old.md".into()),
                side: crate::session::CommentSide::Current,
                line_start: 3,
                line_end: Some(5),
                body: "Fix this.".into(),
                author: Some(crate::session::CommentAuthor::Agent),
            },
            DiffCommentRequest {
                source: branch(),
                root: None,
                path: "a.md".into(),
                from: None,
                side: crate::session::CommentSide::Base,
                line_start: 1,
                line_end: None,
                body: "Why?".into(),
                author: None,
            },
        ],
    );
}

#[test]
fn session_id_methods() {
    // `session.get`, `session.pause`, `session.end`, `lua.shutdown_session`,
    // `workflow.cancel` and the other methods that name only a session.
    golden("session_id", &[Scoped::session("s1")]);
}

#[test]
fn session_page_methods() {
    // `session.history` and `session.resume_from_storage`.
    golden(
        "session_page",
        &[
            Scoped::new(
                "s1",
                Page {
                    limit: Some(10),
                    offset: Some(20),
                },
            ),
            Scoped::new("s1", Page::default()),
        ],
    );
}

#[test]
fn session_scoped_methods() {
    golden(
        "session.events_after",
        &[SessionEventsAfterRequest {
            session_id: "s1".into(),
            after: 42,
        }],
    );
    golden(
        "session.send_message",
        &[
            SessionSendMessageRequest {
                session_id: "s1".into(),
                content: "hello".into(),
                is_interactive: false,
                permission_mode: Some("plan".into()),
                comments: vec![crate::diff::CommentRef {
                    id: "c1".into(),
                    source: branch(),
                }],
            },
            SessionSendMessageRequest {
                session_id: "s1".into(),
                content: "hello".into(),
                is_interactive: true,
                permission_mode: None,
                comments: Vec::new(),
            },
        ],
    );
    golden(
        "session.interaction_respond",
        &[SessionInteractionRespondRequest {
            session_id: "s1".into(),
            request_id: "r1".into(),
            response: json!({"kind": "ask", "selected": ["yes"]}),
        }],
    );
    golden(
        "session.inject_context",
        &[SessionInjectContextRequest {
            session_id: "s1".into(),
            role: "system".into(),
            content: "context".into(),
        }],
    );
    golden(
        "session.test_interaction",
        &[
            SessionTestInteractionRequest {
                session_id: "s1".into(),
                interaction_type: Some("permission".into()),
                question: Some("Why?".into()),
                action: Some("rm -rf".into()),
            },
            SessionTestInteractionRequest {
                session_id: "s1".into(),
                interaction_type: None,
                question: None,
                action: None,
            },
        ],
    );
    golden(
        "session.fork",
        &[
            SessionForkRequest {
                session_id: "s1".into(),
                up_to: Some(4),
            },
            SessionForkRequest {
                session_id: "s1".into(),
                up_to: None,
            },
        ],
    );
    golden(
        "session.dismiss_notification",
        &[SessionDismissNotificationRequest {
            session_id: "s1".into(),
            notification_id: "n1".into(),
        }],
    );
    golden(
        "session.set_title",
        &[SessionSetTitleRequest {
            session_id: "s1".into(),
            title: "A title".into(),
        }],
    );
    golden(
        "session.render_markdown",
        &[
            SessionRenderMarkdownRequest {
                session_id: "s1".into(),
                include_timestamps: Some(true),
                include_tokens: Some(false),
                include_tools: Some(true),
                max_content_length: Some(80),
            },
            SessionRenderMarkdownRequest {
                session_id: "s1".into(),
                include_timestamps: None,
                include_tokens: None,
                include_tools: None,
                max_content_length: None,
            },
        ],
    );
    golden(
        "session.export_to_file",
        &[
            SessionExportToFileRequest {
                session_id: "s1".into(),
                output_path: Some("/tmp/out.md".into()),
                include_timestamps: Some(true),
            },
            SessionExportToFileRequest {
                session_id: "s1".into(),
                output_path: None,
                include_timestamps: None,
            },
        ],
    );
    golden(
        "session.configure_agent",
        &[SessionConfigureAgentRequest {
            session_id: "s1".into(),
            agent: json!({"agent_type": "internal", "model": "m"}),
        }],
    );
    golden(
        "session.set_plugin_approval",
        &[SessionPluginApprovalRequest {
            session_id: "s1".into(),
            plugin: "p".into(),
            approval: "allow".into(),
        }],
    );
    golden(
        "session.get_plugin_approval",
        &[SessionPluginRequest {
            session_id: "s1".into(),
            plugin: "p".into(),
        }],
    );
    golden(
        "session.undo",
        &[
            SessionUndoRequest {
                session_id: "s1".into(),
                count: Some(2),
            },
            SessionUndoRequest {
                session_id: "s1".into(),
                count: None,
            },
        ],
    );
    golden(
        "session.connect_kiln",
        &[SessionKilnRequest {
            session_id: "s1".into(),
            kiln: kiln_name(),
        }],
    );
    golden(
        "session.set_workspace",
        &[
            SessionSetWorkspaceRequest {
                session_id: "s1".into(),
                workspace: Some("/work".into()),
            },
            SessionSetWorkspaceRequest {
                session_id: "s1".into(),
                workspace: None,
            },
        ],
    );
    golden(
        "session.add_notification",
        &[SessionAddNotificationRequest {
            session_id: "s1".into(),
            notification: serde_json::from_value(json!({
                "id": "n1",
                "kind": "toast",
                "message": "hi",
                "scope": {},
                "created_at": null,
            }))
            .expect("a notification reads"),
        }],
    );
    golden(
        "lua.init_session",
        &[
            LuaInitSessionRequest {
                session_id: "s1".into(),
                kiln_path: Some("/kiln".into()),
            },
            LuaInitSessionRequest {
                session_id: "s1".into(),
                kiln_path: None,
            },
        ],
    );
    golden(
        "lua.register_commands",
        &[LuaRegisterCommandsRequest {
            session_id: "s1".into(),
            commands: vec![json!({"name": "hello"})],
        }],
    );
}

/// The workflow requests have no `Deserialize` yet, so this test compares
/// only what the client writes.
#[test]
fn workflow_methods() {
    fn written<T: Serialize>(name: &str, cases: &[T]) {
        let actual = serde_json::to_value(cases).expect("a request writes JSON");
        let path = fixture_path(name);
        if std::env::var_os("CRUCIBLE_WRITE_GOLDEN").is_some() {
            std::fs::create_dir_all(path.parent().expect("a fixture has a directory"))
                .expect("the fixture directory is writable");
            let text = serde_json::to_string_pretty(&actual).expect("JSON writes") + "\n";
            std::fs::write(&path, text).expect("the fixture is writable");
        }
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let expected: Value = serde_json::from_str(&text).expect("the fixture is JSON");
        assert_eq!(actual, expected, "{name}: the wire changed");
    }
    written(
        "workflow.start",
        &[
            WorkflowStartRequest {
                session_id: "s1".into(),
                source: "# Flow".into(),
                path: Some("flow.md".into()),
            },
            WorkflowStartRequest {
                session_id: "s1".into(),
                source: "# Flow".into(),
                path: None,
            },
        ],
    );
    written(
        "workflow.approve_gate",
        &[WorkflowApproveGateRequest {
            session_id: "s1".into(),
            gate_id: "g1".into(),
        }],
    );
}

#[test]
fn fs_methods() {
    golden(
        "fs.list_dir",
        &[FsListDirRequest {
            root: "/repo".into(),
            rel_path: "src".into(),
            show_ignored: true,
            show_hidden: false,
        }],
    );
    golden(
        "fs.move",
        &[FsMoveRequest {
            root: "/repo".into(),
            kind: "project".into(),
            from_rel: "a.md".into(),
            to_rel: "b.md".into(),
        }],
    );
    golden(
        "fs.path",
        &[FsPathRequest {
            root: "/repo".into(),
            kind: "kiln".into(),
            rel_path: "dir".into(),
        }],
    );
}
