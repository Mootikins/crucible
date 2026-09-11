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
