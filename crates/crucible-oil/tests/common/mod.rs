//! Shared helpers for crucible-oil proptest integration tests.
//!
//! Property tests in this crate read their case budget through
//! [`default_cases`] so the suite has one knob — `CRUCIBLE_PROPTEST_CASES` —
//! that lets CI run more cases (256) than the local default (64) without
//! editing individual files. Per-file `.max(N)` floors applied at each
//! `proptest!` block preserve the heavyweight proofs that need MORE than the
//! default; `default_cases().max(N)` ensures the floor always wins regardless
//! of what the env var says.

/// Resolve the per-property case budget from the environment.
///
/// - `CRUCIBLE_PROPTEST_CASES=<n>` with `n > 0` (parses as `u32`) → `n`.
/// - unset, empty, `0`, non-numeric, or parse error → `64`.
///
/// `u32` (not `usize`) matches proptest 1.x's `Config::cases` field type, so
/// the value can be handed straight to `ProptestConfig::with_cases` without a
/// cast.
pub fn default_cases() -> u32 {
    std::env::var("CRUCIBLE_PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n: &u32| n > 0)
        .unwrap_or(64)
}
