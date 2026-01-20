//! Lua view registration and discovery
//!
//! Provides view discovery from Lua/Fennel files with `@view` annotations.
//!
//! ## Usage in Lua
//!
//! ```lua
//! --- Graph visualization
//! -- @view name="graph" desc="Knowledge graph view"
//! function graph_view(ctx)
//!     return cru.oil.col({gap = 1},
//!         cru.oil.text("Graph", {bold = true}),
//!         cru.oil.text("Width: " .. ctx.width)
//!     )
//! end
//!
//! --- Handle keyboard events
//! -- @view.handler name="graph"
//! function graph_keypress(key, ctx)
//!     if key == "q" then ctx:close_view() end
//! end
//! ```

use crate::annotations::{AnnotationParser, DiscoveredView};
use crate::error::LuaError;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

pub async fn discover_views_from(dirs: &[PathBuf]) -> Result<Vec<DiscoveredView>, LuaError> {
    let mut all_views = Vec::new();
    let parser = AnnotationParser::new();

    for dir in dirs {
        match discover_views_in_dir(&parser, dir).await {
            Ok(views) => {
                if !views.is_empty() {
                    info!(
                        "Discovered {} Lua views from {}",
                        views.len(),
                        dir.display()
                    );
                }
                all_views.extend(views);
            }
            Err(e) => {
                warn!("Failed to discover views from {}: {}", dir.display(), e);
            }
        }
    }

    Ok(all_views)
}

async fn discover_views_in_dir(
    parser: &AnnotationParser,
    dir: &Path,
) -> Result<Vec<DiscoveredView>, LuaError> {
    if !dir.exists() {
        debug!("View directory does not exist: {}", dir.display());
        return Ok(Vec::new());
    }

    let mut views = Vec::new();
    let mut entries = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        let is_lua_or_fennel = path.extension().is_some_and(|e| e == "lua" || e == "fnl");

        if !is_lua_or_fennel {
            continue;
        }

        match discover_views_in_file(parser, &path).await {
            Ok(file_views) => {
                for view in file_views {
                    debug!("Discovered view: {} from {}", view.name, path.display());
                    views.push(view);
                }
            }
            Err(e) => {
                warn!("Failed to parse views in {}: {}", path.display(), e);
            }
        }
    }

    Ok(views)
}

async fn discover_views_in_file(
    parser: &AnnotationParser,
    path: &Path,
) -> Result<Vec<DiscoveredView>, LuaError> {
    let source = tokio::fs::read_to_string(path).await?;
    parser.parse_views(&source, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_discover_views_from_directory() {
        let dir = tempfile::tempdir().unwrap();

        let script_content = r#"
--- Graph visualization
-- @view name="graph" desc="Knowledge graph view"
function graph_view(ctx)
    return cru.oil.text("Graph")
end
"#;
        std::fs::write(dir.path().join("graph.lua"), script_content).unwrap();

        let views = discover_views_from(&[dir.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(views.len(), 1);
        assert_eq!(views[0].name, "graph");
        assert_eq!(views[0].description, "Knowledge graph view");
        assert_eq!(views[0].view_fn, "graph_view");
    }

    #[tokio::test]
    async fn test_discover_view_with_handler() {
        let dir = tempfile::tempdir().unwrap();

        let script_content = r#"
--- Graph visualization
-- @view name="graph"
function graph_view(ctx)
    return cru.oil.text("Graph")
end

--- Handle keyboard events
-- @view.handler name="graph"
function graph_keypress(key, ctx)
end
"#;
        std::fs::write(dir.path().join("graph.lua"), script_content).unwrap();

        let views = discover_views_from(&[dir.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(views.len(), 1);
        assert_eq!(views[0].name, "graph");
        assert_eq!(views[0].handler_fn, Some("graph_keypress".to_string()));
    }

    #[tokio::test]
    async fn test_discover_views_empty_directory() {
        let dir = tempfile::tempdir().unwrap();

        let views = discover_views_from(&[dir.path().to_path_buf()])
            .await
            .unwrap();

        assert!(views.is_empty());
    }

    #[tokio::test]
    async fn test_discover_views_nonexistent_directory() {
        let views = discover_views_from(&[PathBuf::from("/nonexistent/path")])
            .await
            .unwrap();

        assert!(views.is_empty());
    }

    #[tokio::test]
    async fn test_discover_multiple_views_same_file() {
        let dir = tempfile::tempdir().unwrap();

        let script_content = r#"
--- First view
-- @view name="view1"
function view1(ctx)
    return cru.oil.text("One")
end

--- Second view
-- @view name="view2"
function view2(ctx)
    return cru.oil.text("Two")
end
"#;
        std::fs::write(dir.path().join("multi.lua"), script_content).unwrap();

        let views = discover_views_from(&[dir.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(views.len(), 2);
        let names: Vec<_> = views.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"view1"));
        assert!(names.contains(&"view2"));
    }

    #[tokio::test]
    async fn test_discover_views_from_multiple_directories() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();

        std::fs::write(
            dir1.path().join("a.lua"),
            r#"
--- View A
-- @view name="view_a"
function view_a(ctx) end
"#,
        )
        .unwrap();

        std::fs::write(
            dir2.path().join("b.lua"),
            r#"
--- View B
-- @view name="view_b"
function view_b(ctx) end
"#,
        )
        .unwrap();

        let views = discover_views_from(&[dir1.path().to_path_buf(), dir2.path().to_path_buf()])
            .await
            .unwrap();

        assert_eq!(views.len(), 2);
        let names: Vec<_> = views.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"view_a"));
        assert!(names.contains(&"view_b"));
    }
}
