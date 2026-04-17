use super::{create_plugin_with_deps, create_test_plugin};
use crate::lifecycle::{LifecycleError, PluginManager};
use tempfile::TempDir;

#[test]
fn test_dependency_order() {
    let temp = TempDir::new().unwrap();
    create_test_plugin(temp.path(), "base", "1.0.0");
    create_plugin_with_deps(temp.path(), "dependent", &["base"]);

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();

    let loaded = manager.load_all().unwrap();
    let base_idx = loaded.iter().position(|n| n == "base").unwrap();
    let dep_idx = loaded.iter().position(|n| n == "dependent").unwrap();
    assert!(base_idx < dep_idx, "base should load before dependent");
}

#[test]
fn test_missing_dependency_error() {
    let temp = TempDir::new().unwrap();
    create_plugin_with_deps(temp.path(), "orphan", &["missing"]);

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();

    let result = manager.load("orphan");
    assert!(matches!(
        result,
        Err(LifecycleError::DependencyNotSatisfied { .. })
    ));
}

#[test]
fn test_circular_dependency_detection() {
    let temp = TempDir::new().unwrap();
    create_plugin_with_deps(temp.path(), "a", &["b"]);
    create_plugin_with_deps(temp.path(), "b", &["a"]);

    let mut manager = PluginManager::new().with_search_paths(vec![temp.path().to_path_buf()]);
    manager.discover().unwrap();

    let result = manager.load_all();
    assert!(matches!(result, Err(LifecycleError::CircularDependency(_))));
}
