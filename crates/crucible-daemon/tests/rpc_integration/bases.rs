use super::server::TestServer;
use crucible_daemon::DaemonClient;
use serde_json::json;

#[tokio::test]
async fn bases_cross_the_socket_and_write_the_same_note() {
    let server = TestServer::start().await.unwrap();
    let client = DaemonClient::connect_to(&server.socket_path).await.unwrap();
    let root = server.socket_path.parent().unwrap().join("kiln");
    let yaml="filters: 'status == \"todo\"'\nviews: [{type: table, name: Tasks, order: [file.name, note.status]}]";
    tokio::fs::create_dir(root.join("boards")).await.unwrap();
    tokio::fs::write(root.join("Host.md"), "![[Tasks.base#Tasks]]")
        .await
        .unwrap();
    tokio::fs::write(root.join("boards/Tasks.base"), yaml)
        .await
        .unwrap();
    let params =
        json!({"kiln":"kiln","source":{"path":"Tasks.base"},"view":"Tasks","this":"Host.md"});
    assert_eq!(
        client
            .call("base.list", json!({"kiln":"kiln"}))
            .await
            .unwrap(),
        json!(["boards/Tasks.base"])
    );
    assert_eq!(
        client.call("base.views", params.clone()).await.unwrap(),
        json!([{"name":"Tasks","type":"table"}])
    );
    let mut create = params.clone();
    create["name"] = json!("First");
    assert_eq!(
        client.call("base.create_entry", create).await.unwrap()["path"],
        "First.md"
    );
    let query = client.call("base.query", params.clone()).await.unwrap();
    assert_eq!(query["rows"].as_array().unwrap().len(), 1);
    assert_eq!(query["source_path"], "boards/Tasks.base");
    let hash = query["rows"][0]["ancestor_hash"].clone();
    let edit =
        json!({"kiln":"kiln","path":"First.md","key":"status","value":"done","ancestor_hash":hash});
    assert_eq!(
        client
            .call("base.set_property", edit.clone())
            .await
            .unwrap()["status"],
        "applied"
    );
    assert_eq!(
        client.call("base.set_property", edit).await.unwrap()["status"],
        "stale"
    );
    assert!(client.call("base.query", params).await.unwrap()["rows"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(client
        .call(
            "base.query",
            json!({"kiln":"unknown","source":{"path":"Tasks.base"}})
        )
        .await
        .is_err());
    tokio::fs::create_dir(root.join("archive")).await.unwrap();
    let content = tokio::fs::read_to_string(root.join("First.md"))
        .await
        .unwrap();
    let moved=client.call("base.set_property",json!({"kiln":"kiln","path":"First.md","key":"file.folder","value":"archive","ancestor_hash":crucible_core::note_edit::disk_hash(&content)})).await.unwrap();
    assert_eq!(moved["status"], "applied", "{moved}");
    assert!(root.join("archive/First.md").exists());
    assert!(!root.join("First.md").exists());
    server.shutdown().await;
}
