//! Real CLI processes query and write through an isolated daemon process.
mod cli_e2e_helpers;
use cli_e2e_helpers::TestDaemon;
use serde_json::Value;

#[test]
fn bases_cli_queries_and_creates_through_the_daemon() {
    let daemon = TestDaemon::start();
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("Tasks.base"), "filters: 'status == \"todo\"'\nviews: [{type: table, name: Tasks, order: [file.name, note.status]}]").unwrap();
    let run = |args: &[&str]| {
        let output = daemon.command().args(args).output().unwrap();
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
    assert_eq!(
        serde_json::from_str::<Value>(&created).unwrap()["path"],
        "One.md"
    );
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
    assert_eq!(rows["rows"][0]["path"], "One.md");
    assert_eq!(query("paths"), "One.md\n");
    assert!(query("csv").contains("\"One.md\",\"todo\""));
}
