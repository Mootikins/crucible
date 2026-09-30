use super::super::config::ContextStrategy;

#[test]
fn test_context_strategy_display_and_parse() {
    assert_eq!(ContextStrategy::Truncate.to_string(), "truncate");
    assert_eq!(ContextStrategy::Summarize.to_string(), "summarize");

    assert_eq!(
        "truncate".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::Truncate
    );
    assert_eq!(
        "summarize".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::Summarize
    );
    assert_eq!(
        "SUMMARIZE".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::Summarize
    );
    assert!("nonsense".parse::<ContextStrategy>().is_err());
}

/// A name the enum used to know must be refused, not silently accepted.
///
/// `sliding_window` drained exactly what `Summarize` drains and left no marker
/// in the hole. A session config or `:set` carrying the old name now fails
/// loudly rather than falling back to the default, which would have changed
/// the strategy without saying so.
/// A session a daemon persisted before `ContextStrategy` carried
/// `#[serde(rename_all = "snake_case")]` wrote the derive's plain variant
/// name (`"Truncate"`/`"Summarize"`). The fixtures are byte-for-byte what
/// that old code wrote (`crates/crucible-core/tests/fixtures/wire_compat/
/// context_strategy_*_pre_step18.json`); this proves a daemon that resumes
/// such a session still loads it, even though a fresh write now uses the
/// lowercase spelling.
#[test]
fn old_pascal_case_records_still_load() {
    let truncate: ContextStrategy = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/wire_compat/context_strategy_truncate_pre_step18.json"
    ))
    .expect("an old `\"Truncate\"` record must still deserialize");
    assert_eq!(truncate, ContextStrategy::Truncate);

    let summarize: ContextStrategy = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/wire_compat/context_strategy_summarize_pre_step18.json"
    ))
    .expect("an old `\"Summarize\"` record must still deserialize");
    assert_eq!(summarize, ContextStrategy::Summarize);

    // The new spelling round-trips, and is what a fresh write now produces.
    assert_eq!(
        serde_json::to_string(&ContextStrategy::Truncate).unwrap(),
        "\"truncate\""
    );
    assert_eq!(
        serde_json::to_string(&ContextStrategy::Summarize).unwrap(),
        "\"summarize\""
    );
}

#[test]
fn the_removed_sliding_window_name_is_refused() {
    for spelling in ["sliding_window", "slidingwindow", "SLIDING_WINDOW"] {
        let err = spelling
            .parse::<ContextStrategy>()
            .expect_err("`{spelling}` must not parse");
        assert!(
            err.contains("truncate, summarize"),
            "the error must name what is valid now; got: {err}"
        );
    }
}
