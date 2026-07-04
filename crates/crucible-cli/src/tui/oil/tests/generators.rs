// Test generators for property-based tests.
use proptest::prelude::*;

pub fn arb_short_text() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z]{5,30}")
        .unwrap()
        .prop_filter("non-empty", |s| !s.trim().is_empty())
}
