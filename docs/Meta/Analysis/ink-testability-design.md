---
status: complete
completed: 2025 (approximate)
---

# Ink TUI Testability Design

> **Status: COMPLETE** — All 5 phases implemented. This document is now a historical reference.
> See: `graduation.rs` (RenderFilter), `planning.rs` (FramePlanner), `component.rs` (Component trait),
> `test_harness.rs` (AppHarness), `terminal.rs` (Terminal refactor).

> Design for improving testability and scrollback preservation in the Crucible ink TUI framework.

## Goals

1. **Component Testability** - Test components in isolation without Terminal
2. **Scrollback Assertions** - Assert which keys graduated on which frame
3. **Unified Test/Production Behavior** - Same logic in TestRuntime and Terminal
4. **Preserve Graduation Innovation** - Keep the scrollback concept

## Current Problems

1. Graduation decision happens during render (mixed concerns)
2. `filter_graduated()` rebuilds entire tree every frame
3. No way to assert "this content graduated on this frame"
4. `AppHarness` doesn't use same filtering as real Terminal
5. Spacing/boundary between scrollback and viewport is implicit

## Design Overview

### Phase 1: RenderFilter (eliminate tree rebuilding)

Instead of rebuilding a filtered `Node`, add a filter hook to rendering:

```rust
// crates/crucible-cli/src/tui/ink/render.rs
pub trait RenderFilter {
    fn skip_static(&self, key: &str) -> bool;
}

pub struct NoFilter;
impl RenderFilter for NoFilter {
    fn skip_static(&self, _key: &str) -> bool { false }
}

pub fn render_with_cursor_filtered(
    node: &Node,
    width: usize,
    filter: &dyn RenderFilter,
) -> RenderResult {
    // same as render_with_cursor, but pass `filter` down
}
```

In `Node::Static` branch:

```rust
Node::Static(s) => {
    if filter.skip_static(&s.key) {
        return;
    }
    for child in &s.children {
        render_node_filtered(child, width, filter, output, cursor_info);
    }
}
```

Implement filter on `GraduationState`:

```rust
impl RenderFilter for GraduationState {
    fn skip_static(&self, key: &str) -> bool {
        self.is_graduated(key)
    }
}
```

**Benefits:**
- Removes `Terminal::filter_graduated()` and `TestRuntime::filter_graduated()`
- No tree rebuild cost every frame
- No divergence risk between prod/test

### Phase 2: FramePlanner (separate decisions from I/O)

```rust
// crates/crucible-cli/src/tui/ink/planning.rs

#[derive(Debug, Clone)]
pub struct FrameTrace {
    pub frame_no: u64,
    pub graduated_keys: Vec<String>,
    pub boundary_lines: usize,
    pub viewport_visual_rows: usize,
}

#[derive(Debug, Clone)]
pub struct FramePlan {
    pub frame_no: u64,
    pub graduated: Vec<GraduatedContent>,
    pub boundary_lines: usize,
    pub viewport: RenderResult,
    pub trace: FrameTrace,
}

#[derive(Debug, Clone)]
pub struct FrameSnapshot {
    pub plan: FramePlan,
    pub stdout_delta: String,
}

pub struct FramePlanner {
    width: u16,
    height: u16,
    frame_no: u64,
    graduation: GraduationState,
    boundary_default: usize, // usually 1
}

impl FramePlanner {
    pub fn new(width: u16, height: u16) -> Self {
        Self { 
            width, height, 
            frame_no: 0, 
            graduation: GraduationState::new(), 
            boundary_default: 1 
        }
    }

    pub fn plan(&mut self, tree: &Node) -> FrameSnapshot {
        self.frame_no += 1;

        // Phase A: decide graduation (pure)
        let graduated = self.graduation.plan_graduation(tree);

        // Phase B: explicit boundary decision
        let boundary_lines = if graduated.is_empty() { 0 } else { self.boundary_default };

        // Phase C: render viewport with filter (no tree rebuild)
        let viewport = render_with_cursor_filtered(
            tree,
            self.width as usize,
            &self.graduation,
        );

        // Phase D: build stdout delta (no actual IO)
        let stdout_delta = self.graduation.format_stdout_delta(&graduated, boundary_lines);

        // Phase E: commit graduation state
        self.graduation.commit_graduation(&graduated);

        let trace = FrameTrace {
            frame_no: self.frame_no,
            graduated_keys: graduated.iter().map(|g| g.key.clone()).collect(),
            boundary_lines,
            viewport_visual_rows: visual_rows(&viewport.content, self.width as usize),
        };

        FrameSnapshot {
            plan: FramePlan { frame_no: self.frame_no, graduated, boundary_lines, viewport, trace },
            stdout_delta,
        }
    }
}
```

**Key insight:** `FramePlanner::plan()` does no terminal I/O. Returns explicit trace for assertions.

### Phase 3: Component Trait

```rust
// crates/crucible-cli/src/tui/ink/component.rs
pub trait Component {
    fn view(&self, ctx: &ViewContext<'_>) -> Node;
}

impl<F> Component for F
where
    F: Fn(&ViewContext<'_>) -> Node,
{
    fn view(&self, ctx: &ViewContext<'_>) -> Node {
        (self)(ctx)
    }
}
```

Component harness for isolated testing:

```rust
// crates/crucible-cli/src/tui/ink/component_harness.rs
pub struct ComponentHarness {
    focus: FocusContext,
    planner: FramePlanner,
}

impl ComponentHarness {
    pub fn new(width: u16, height: u16) -> Self {
        Self { 
            focus: FocusContext::new(), 
            planner: FramePlanner::new(width, height) 
        }
    }

    pub fn render_component(&mut self, c: &impl Component) -> FrameSnapshot {
        let ctx = ViewContext::new(&self.focus);
        let tree = c.view(&ctx);
        self.planner.plan(&tree)
    }
}
```

### Phase 4: Unified AppHarness

```rust
pub struct AppHarness<A: App> {
    app: A,
    focus: FocusContext,
    planner: FramePlanner,
    last_snapshot: Option<FrameSnapshot>,
}

impl<A: App> AppHarness<A> {
    pub fn render(&mut self) -> &mut Self {
        let ctx = ViewContext::new(&self.focus);
        let tree = self.app.view(&ctx);
        self.last_snapshot = Some(self.planner.plan(&tree));
        self
    }

    pub fn trace(&self) -> &FrameTrace {
        &self.last_snapshot.as_ref().unwrap().plan.trace
    }

    pub fn viewport(&self) -> &str {
        &self.last_snapshot.as_ref().unwrap().plan.viewport.content
    }

    pub fn stdout_delta(&self) -> &str {
        &self.last_snapshot.as_ref().unwrap().stdout_delta
    }

    pub fn screen(&self) -> String {
        let snap = self.last_snapshot.as_ref().unwrap();
        format!("{}{}", snap.stdout_delta, snap.plan.viewport.content)
    }
}
```

### Phase 5: Refactor Terminal

```rust
impl Terminal {
    pub fn render(&mut self, tree: &Node) -> io::Result<()> {
        let snapshot = self.planner.plan(tree);
        self.apply(&snapshot)
    }

    fn apply(&mut self, snapshot: &FrameSnapshot) -> io::Result<()> {
        execute!(self.stdout, Hide)?;

        if !snapshot.plan.graduated.is_empty() {
            self.output.clear()?;
            write!(self.stdout, "{}", snapshot.stdout_delta)?;
            self.stdout.flush()?;
            self.output.force_redraw();
            self.last_cursor = None;
        }

        self.output.render_with_cursor_restore(
            &snapshot.plan.viewport.content,
            self.last_cursor.as_ref().map(|c| c.row_from_end).unwrap_or(0),
        )?;

        // cursor positioning...
        Ok(())
    }
}
```

## Test Examples

### Assert graduation per frame

```rust
#[test]
fn user_message_graduates_on_submit() {
    let mut h = AppHarness::new(InkChatApp::default(), 80, 24);
    
    h.send_key(KeyCode::Char('h'));
    h.send_key(KeyCode::Char('i'));
    h.render();
    
    assert!(h.trace().graduated_keys.is_empty(), "nothing graduated yet");
    
    h.send_key(KeyCode::Enter);
    h.app.on_message(ChatAppMsg::UserMessage("hi".into()));
    h.render();
    
    assert_eq!(h.trace().graduated_keys, vec!["user-1"]);
    assert_eq!(h.trace().boundary_lines, 1);
}
```

### Snapshot combined screen

```rust
#[test]
fn screen_shows_graduated_plus_viewport() {
    let mut h = AppHarness::new(InkChatApp::default(), 80, 24);
    h.app.on_message(ChatAppMsg::UserMessage("hello".into()));
    h.render();
    
    insta::assert_snapshot!(h.screen());
}
```

### Test component in isolation

```rust
#[test]
fn status_bar_shows_mode() {
    let mut h = ComponentHarness::new(80, 1);
    
    let status = StatusBar { mode: ChatMode::Plan, status: "Ready" };
    let snap = h.render_component(&status);
    
    assert!(snap.plan.viewport.content.contains("PLAN"));
}
```

## Migration Path

1. **Add RenderFilter trait** - No breaking changes, just new capability
2. **Add FramePlanner** - New module, doesn't touch existing code yet
3. **Update render.rs** - Add `render_with_cursor_filtered`, keep old function
4. **Update TestRuntime** - Use FramePlanner internally
5. **Update AppHarness** - Use FramePlanner, add new assertion helpers
6. **Update Terminal** - Split into plan + apply
7. **Remove filter_graduated** - Once all callers migrated
8. **Add Component trait** - Optional, for new components

## Watch Out For

- **`pending_newline` semantics**: Move behind `format_stdout_delta()` 
- **Width=10000 magic**: Make it a named constant on FramePlanner
- **Cursor restore vs boundary insertion**: Make boundary a first-class plan decision

## Effort Estimate

- Phase 1 (RenderFilter): 2-4 hours
- Phase 2 (FramePlanner): 4-6 hours  
- Phase 3 (Component trait): 1-2 hours
- Phase 4 (AppHarness): 2-4 hours
- Phase 5 (Terminal refactor): 2-4 hours
- Total: 1-2 days

## Future Considerations

- **Partial invalidation**: Only if profiling shows planning/render is bottleneck
- **ratatui Buffer**: Could simplify viewport math but larger refactor
- **Taffy layout**: Could replace custom layout logic for flexbox
