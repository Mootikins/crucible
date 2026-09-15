use super::*;
use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

/// The versioned corpus also works with `cru eval precognition`. Here the hash
/// provider proves indexing/retrieval plumbing, not semantic ranking quality.
#[tokio::test]
async fn agent_written_knowledge_is_indexed_and_reaches_a_new_sessions_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/fixtures/learning_loop_v1");
    let golden: toml::Value =
        toml::from_str(&std::fs::read_to_string(fixtures.join("golden.toml")).unwrap()).unwrap();
    let notes: Vec<serde_json::Value> = std::fs::read_dir(fixtures.join("corpus")).unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            serde_json::json!({ "path": entry.file_name().to_str().unwrap(), "content": std::fs::read_to_string(entry.path()).unwrap() })
        }).collect();
    let kiln = TempDir::new().unwrap();
    let sibling = TempDir::new().unwrap();
    let workspace = TempDir::new().unwrap();
    let sm =
        temp_session_manager_with_kilns(&[("knowledge", kiln.path()), ("sibling", sibling.path())]);
    let am = create_test_agent_manager_with_enrichment(
        sm.clone(),
        crucible_core::config::EmbeddingProviderConfig::mock(Some(384)),
    );
    let writer = sm
        .create_session(
            SessionType::Chat,
            vec![kiln_name("knowledge")],
            Some(workspace.path().into()),
            None,
        )
        .await
        .unwrap();
    let mut writer_agent = test_agent();
    writer_agent.tool_policy = Some(HashMap::from([(
        "create_note".into(),
        crucible_core::agent::ToolPolicy::Allow,
    )]));
    am.configure_agent(&writer.id, writer_agent).await.unwrap();
    am.install_agent_for_test(
        writer.id.to_string(),
        Arc::new(Mutex::new(Box::new(StreamingMockAgent {
            events: notes
                .iter()
                .enumerate()
                .map(|(i, note)| {
                    script::tool_call(&format!("note-{i}"), "create_note", note.clone())
                })
                .collect(),
        }))),
    );
    let (tx, mut rx) = broadcast::channel(128);
    am.send_message(
        &writer.id,
        "Remember these decisions".into(),
        &tx,
        false,
        None,
    )
    .await
    .unwrap();
    next_event_or_skip(&mut rx, "message_complete").await;
    for note in &notes {
        let path = kiln.path().join(note["path"].as_str().unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            note["content"].as_str().unwrap()
        );
        // The production index job, never a fabricated NoteRecord/vector.
        am.kiln_manager
            .process_file(kiln.path(), &path)
            .await
            .unwrap();
    }
    let path = sibling.path().join("Private.md");
    std::fs::write(
        &path,
        "SIBLING-ONLY-SECRET: the private launch code is copper-violet.",
    )
    .unwrap();
    am.kiln_manager
        .process_file(sibling.path(), &path)
        .await
        .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/x-ndjson")
                .set_body_string(concat!(
                    "{\"model\":\"llama3.2\",\"message\":{\"role\":\"assistant\",\"content\":\"Acknowledged.\"},\"done\":false}\n",
                    "{\"model\":\"llama3.2\",\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true}\n",
                )),
        )
        .mount(&server)
        .await;
    for case in golden["queries"].as_array().unwrap() {
        let reader = sm
            .create_session(
                SessionType::Chat,
                vec![kiln_name("knowledge")],
                Some(workspace.path().into()),
                None,
            )
            .await
            .unwrap();
        am.configure_agent(
            &reader.id,
            SessionAgent {
                endpoint: Some(server.uri()),
                precognition_enabled: true,
                ..test_agent()
            },
        )
        .await
        .unwrap();
        let query = case["question"].as_str().unwrap();
        crate::server::session::inject_context_impl(
            &sm,
            &am,
            &tx,
            &reader.id,
            "user",
            "INJECTED-PROVIDER-CONTEXT",
        )
        .await
        .unwrap();
        am.send_message(&reader.id, query.into(), &tx, false, None)
            .await
            .unwrap();
        let precognition = next_event_or_skip(&mut rx, "precognition_complete").await;
        assert!(
            precognition.data["notes_count"].as_u64().unwrap() > 0,
            "{precognition:?}"
        );
        // Require success, not merely a request followed by a provider error.
        let completed = first_event_of(&mut rx, &["message_complete", "ended"]).await;
        assert_eq!(completed.event, "message_complete", "{completed:?}");
        let requests = server.received_requests().await.unwrap();
        let request: serde_json::Value =
            serde_json::from_slice(&requests.last().unwrap().body).unwrap();
        let messages = request["messages"]
            .as_array()
            .expect("actual provider messages");
        let context = messages
            .iter()
            .filter(|m| m["role"] == "system")
            .map(ToString::to_string)
            .collect::<String>();
        assert!(
            context.contains(
                case["expect_note"]
                    .as_str()
                    .unwrap()
                    .trim_end_matches(".md")
            ),
            "{context}"
        );
        assert!(!context.contains("SIBLING-ONLY-SECRET"), "{context}");
        assert!(!context.contains("copper-violet"), "{context}");
        let expected_path = format!("{}.md", case["expect_note"].as_str().unwrap());
        let expected_body = notes
            .iter()
            .find(|note| note["path"] == expected_path)
            .unwrap()["content"]
            .as_str()
            .unwrap()
            .lines()
            .last()
            .unwrap();
        assert!(context.contains(expected_body), "{context}");
        assert_eq!(
            messages
                .iter()
                .filter(|m| m["content"] == "INJECTED-PROVIDER-CONTEXT" && m["role"] == "user")
                .count(),
            1
        );
        assert_eq!(messages.last().unwrap()["content"], query);
    }
}
