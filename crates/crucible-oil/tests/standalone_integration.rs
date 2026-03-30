use crucible_oil::{col, text, TestRuntime};

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

#[test]
fn basic_rendering() {
    let mut runtime = TestRuntime::new(80, 24);
    let tree = col([text("Hello"), text("World")]);

    runtime.render(&tree);
    let viewport = runtime.viewport_content();
    assert!(viewport.contains("Hello"));
    assert!(viewport.contains("World"));
}

#[test]
fn viewport_resize_preserves_content() {
    let mut runtime = TestRuntime::new(80, 24);
    let tree = col([text("Persist me"), text("live")]);

    runtime.render(&tree);
    assert!(runtime.viewport_content().contains("Persist me"));

    runtime.resize(80, 10);
    runtime.render(&tree);

    assert_eq!(runtime.height(), 10);
    assert!(runtime.viewport_content().contains("Persist me"));
    assert_eq!(
        count_occurrences(runtime.viewport_content(), "Persist me"),
        1
    );
}
