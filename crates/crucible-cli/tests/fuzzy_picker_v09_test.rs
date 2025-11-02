//! Integration test for nucleo-picker v0.9.0 API
//!
//! This test verifies that we can use the v0.9.0 Render trait API.
//! Expected to fail compilation until we upgrade from v0.3.

use nucleo_picker::{Picker, render::StrRenderer};

#[test]
fn test_picker_uses_render_trait() {
    // Test that we can create a picker with StrRenderer
    let mut picker: Picker<String, _> = Picker::new(StrRenderer);

    // Test that we can extend with a vector (v0.9 API)
    let files = vec![
        "file1.md".to_string(),
        "file2.md".to_string(),
        "file3.md".to_string(),
    ];

    picker.extend(files);

    // Test that injector.push() works without formatter parameter
    let injector = picker.injector();
    injector.push("file4.md".to_string());

    // This test just verifies the API compiles - actual picker behavior
    // will be tested in integration tests
    assert!(true);
}

#[test]
fn test_injector_push_no_formatter() {
    // Test that the new injector API works
    let picker: Picker<String, _> = Picker::new(StrRenderer);
    let injector = picker.injector();

    // v0.9 API: push takes just the item, no formatter
    injector.push("test.md".to_string());
    injector.push("another.md".to_string());

    assert!(true);
}
