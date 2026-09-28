//! Qualified decisions cross HTTP and the real daemon socket unchanged.
use axum::{body::Body, http::Request};
use crucible_core::{
    file_write::ExpectedBase,
    proposal::{ProposalAuthor, ProposalState},
    session::{PhysicalRoot, SessionId},
};
use crucible_daemon::{
    proposals::{proposals_root, ProposalStore},
    test_support::InProcessDaemonBuilder,
};
use serde_json::{json, Value};
use tower::ServiceExt;

#[tokio::test]
async fn web_decisions_select_the_second_kiln() {
    let dir = tempfile::tempdir().unwrap();
    let first = dir.path().join("first");
    let second = dir.path().join("second");
    std::fs::create_dir(&first).unwrap();
    std::fs::create_dir(&second).unwrap();
    let data = dir.path().join("data");
    let server = InProcessDaemonBuilder::at_data_home(data.clone())
        .with_kiln_at("first", &first)
        .with_kiln_at("second", &second)
        .start()
        .await
        .unwrap();
    let client = server.connect().await;
    let app =
        crucible_web::test_support::build_test_app(crucible_web::test_support::build_state(client));
    let store = ProposalStore::new(proposals_root(&data));
    for operation in ["accept", "reject", "resolve"] {
        let mut proposal = None;
        for root in [&first, &second] {
            let _ = std::fs::remove_file(root.join("a.md"));
            proposal = Some(
                store
                    .record_write(
                        ProposalAuthor::Plugin {
                            name: operation.into(),
                        },
                        &SessionId::parse(operation).unwrap(),
                        PhysicalRoot::from_top_level(root),
                        "a.md",
                        ExpectedBase::Absent,
                        "proposed\n".into(),
                    )
                    .unwrap(),
            );
        }
        let proposal = proposal.unwrap();
        let request = |action: &str, body: Value| {
            Request::builder()
                .method("POST")
                .uri(format!("/api/proposals/{}/{action}", proposal.id))
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap()
        };
        if operation == "resolve" {
            for root in [&first, &second] {
                std::fs::write(root.join("a.md"), "outside\n").unwrap();
            }
            let response = app
                .clone()
                .oneshot(request("accept", json!({})))
                .await
                .unwrap();
            assert!(response.status().is_success());
        }
        let body = if operation == "resolve" {
            json!({"root": second, "path": "a.md", "text": "resolved\n"})
        } else {
            json!({"files": [{"root": second, "path": "a.md"}]})
        };
        let response = app.clone().oneshot(request(operation, body)).await.unwrap();
        assert!(response.status().is_success());
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let answer: crucible_core::proposal::Proposal = serde_json::from_slice(&bytes).unwrap();
        match operation {
            "accept" => {
                assert!(!first.join("a.md").exists());
                assert_eq!(
                    std::fs::read_to_string(second.join("a.md")).unwrap(),
                    "proposed\n"
                );
                assert_eq!(answer.writes.len(), 1);
            }
            "reject" => {
                assert!(!first.join("a.md").exists());
                assert!(!second.join("a.md").exists());
                assert_eq!(answer.writes.len(), 1);
                assert_eq!(answer.writes[0].root.as_path(), second);
            }
            "resolve" => {
                let ProposalState::Conflicted { files } = answer.state else {
                    panic!("first conflict must remain")
                };
                assert_eq!(files.len(), 1);
                assert_eq!(files[0].root.as_path(), first);
                assert_eq!(answer.writes[1].new_text, "resolved\n");
                for root in [&first, &second] {
                    assert_eq!(
                        std::fs::read_to_string(root.join("a.md")).unwrap(),
                        "outside\n"
                    );
                }
            }
            _ => unreachable!(),
        }
    }
    server.shutdown().await;
}
