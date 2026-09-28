//! The readers of a stored session, pinned against the fixtures in
//! `assets/fixtures/`. A change of a reader or of the fold shows here as a
//! diff of a golden file. `CRUCIBLE_WRITE_GOLDEN=1` writes the files.

use crate::test_support::{assert_golden, fixture_path, stored_log, READER_FIXTURES};

fn golden(dir: &str, file: String) -> std::path::PathBuf {
    fixture_path("golden").join(dir).join(file)
}

#[test]
fn the_markdown_export_of_each_fixture_matches_its_golden_file() {
    for name in READER_FIXTURES {
        let events = crate::observe::parse_session_log(&stored_log(name));
        let stem = name.trim_end_matches(".jsonl");
        let plain = crate::observe::render_to_markdown(&events, &Default::default());
        assert_golden(&golden("export", format!("{stem}.md")), &plain);
        let timed = crate::observe::render_to_markdown(
            &events,
            &crate::observe::RenderOptions {
                include_timestamps: true,
                ..Default::default()
            },
        );
        assert_golden(&golden("export", format!("{stem}.timestamps.md")), &timed);
    }
}

#[test]
fn the_lua_history_rows_of_each_fixture_match_their_golden_file() {
    for name in READER_FIXTURES {
        let events = crate::observe::parse_session_log(&stored_log(name));
        let rows = crate::session_bridge::message_rows(&events, None, true);
        let stem = name.trim_end_matches(".jsonl");
        assert_golden(
            &golden("lua_rows", format!("{stem}.json")),
            &serde_json::to_string_pretty(&rows).unwrap(),
        );
    }
}
