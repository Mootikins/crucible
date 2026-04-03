mod generators;
pub(crate) mod helpers;
mod layout_test_helpers;
pub(crate) mod vt100_runtime;

// Surviving framework tests
mod component_isolation_tests;
mod event_loop_tests;
mod event_tests;
mod focus_tests;
mod layout_tests;
mod markdown_fuzz_tests;
mod node_tests;
mod popup_tests;
mod property_tests;
mod render_tests;

// Phase 7: Component model tests
mod container_snapshot_tests;
mod e2e_debug_test;
mod fixture_replay_tests;
mod graduation_tests;
mod message_routing_tests;
mod rendering_regression_tests;
mod inter_frame_invariant_tests;
mod spacing_tests;
mod visual_inspection;
