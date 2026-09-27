//! Real CLI processes query and write through an isolated daemon process.
mod cli_e2e_helpers;
use cli_e2e_helpers::TestDaemon;
use serde_json::Value;

#[test]
fn bases_cli_queries_and_creates_through_the_daemon() {
    let daemon = TestDaemon::start();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Tasks.base"), "filters: 'status == \"todo\"'\nviews: [{type: table, name: Tasks, order: [file.name, note.status]}]").unwrap();
    let output = |args: &[&str]| daemon.command().args(args).output().unwrap();
    let run = |args: &[&str]| {
        let output = output(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    run(&["kiln", "register", "Work", root.path().to_str().unwrap()]);
    let created = run(&[
        "base",
        "create",
        "Tasks.base",
        "--kiln",
        "Work",
        "--name",
        "One",
    ]);
    let created: Value = serde_json::from_str(&created).unwrap();
    assert_eq!(created["status"], "applied");
    assert_eq!(created["path"], "One.md");
    let query = |format: &str| {
        run(&[
            "base",
            "query",
            "Tasks.base",
            "--kiln",
            "Work",
            "--format",
            format,
        ])
    };
    let rows: Value = serde_json::from_str(&query("json")).unwrap();
    assert_eq!(rows[0]["path"], "One.md");
    assert_eq!(query("paths"), "One.md\n");
    assert!(query("csv").contains("One,todo"));
    let data: Value = serde_json::from_str(&query("data")).unwrap();
    assert_eq!(data["rows"][0]["path"], "One.md");
    assert_eq!(
        serde_json::from_str::<Value>(&run(&["base", "list", "--kiln", "Work"])).unwrap(),
        serde_json::json!(["Tasks.base"])
    );
    assert_eq!(
        serde_json::from_str::<Value>(&run(&["base", "views", "Tasks.base", "--kiln", "Work"]))
            .unwrap(),
        serde_json::json!([{"name": "Tasks", "type": "table"}])
    );

    let hash = data["rows"][0]["ancestor_hash"].as_str().unwrap();
    let set = [
        "base",
        "set",
        "One.md",
        "status",
        "done",
        "--kiln",
        "Work",
        "--ancestor-hash",
        hash,
    ];
    let applied: Value = serde_json::from_str(&run(&set)).unwrap();
    assert_eq!(applied["status"], "applied", "{applied}");
    let note = std::fs::read_to_string(root.path().join("One.md")).unwrap();
    assert!(note.contains("status: done"), "{note}");

    // The hash is now stale: the write must fail and leave the note alone.
    let stale = output(&set);
    assert!(!stale.status.success(), "a stale write exited 0");
    let stderr = String::from_utf8_lossy(&stale.stderr);
    assert!(stderr.contains("\"status\": \"stale\""), "{stderr}");
    assert_eq!(
        std::fs::read_to_string(root.path().join("One.md")).unwrap(),
        note
    );

    // Neither a value nor --delete: clap refuses before the daemon sees it.
    let missing = output(&[
        "base",
        "set",
        "One.md",
        "status",
        "--kiln",
        "Work",
        "--ancestor-hash",
        hash,
    ]);
    assert_eq!(missing.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("required"),
        "{}",
        String::from_utf8_lossy(&missing.stderr)
    );
}
