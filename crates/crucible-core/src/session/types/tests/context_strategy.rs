use super::super::config::ContextStrategy;

#[test]
fn test_context_strategy_display_and_parse() {
    assert_eq!(ContextStrategy::Truncate.to_string(), "truncate");
    assert_eq!(ContextStrategy::SlidingWindow.to_string(), "sliding_window");
    assert_eq!(ContextStrategy::Summarize.to_string(), "summarize");

    assert_eq!(
        "truncate".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::Truncate
    );
    assert_eq!(
        "sliding_window".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::SlidingWindow
    );
    assert_eq!(
        "slidingwindow".parse::<ContextStrategy>().unwrap(),
        ContextStrategy::SlidingWindow
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
