# OIL Buffer-Based Layer System

## Problem

Current overlay compositing replaces entire lines instead of merging content:

```
base line:    "▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄"  (input box border)
overlay line: "         ▌ Notification"
result:       "         ▌ Notification"  // base destroyed!
```

The notification overlay wipes out the input box borders because `composite_overlays()` does line-level replacement, not cell-level merging.

## Solution: Cell-Based Buffer

### Core Types

```rust
/// A single character cell with styling
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub char: char,
    pub style: Style,
}

impl Cell {
    pub const EMPTY: Cell = Cell { char: ' ', style: Style::EMPTY };
    
    /// A transparent cell that won't overwrite during compositing
    pub fn is_transparent(&self) -> bool {
        self.char == ' ' && self.style == Style::EMPTY
    }
}

/// A 2D grid of cells
#[derive(Clone, Debug)]
pub struct Buffer {
    cells: Vec<Cell>,  // row-major: cells[y * width + x]
    width: u16,
    height: u16,
}

impl Buffer {
    pub fn new(width: u16, height: u16) -> Self { ... }
    pub fn get(&self, x: u16, y: u16) -> &Cell { ... }
    pub fn set(&mut self, x: u16, y: u16, cell: Cell) { ... }
    
    /// Write a string at position, applying style
    pub fn set_string(&mut self, x: u16, y: u16, s: &str, style: Style) { ... }
    
    /// Composite another buffer on top, skipping transparent cells
    pub fn merge(&mut self, other: &Buffer, offset_x: u16, offset_y: u16) {
        for y in 0..other.height {
            for x in 0..other.width {
                let cell = other.get(x, y);
                if !cell.is_transparent() {
                    let target_x = offset_x + x;
                    let target_y = offset_y + y;
                    if target_x < self.width && target_y < self.height {
                        self.set(target_x, target_y, cell.clone());
                    }
                }
            }
        }
    }
    
    /// Convert to Vec<String> with ANSI codes for terminal output
    pub fn to_lines(&self) -> Vec<String> { ... }
}
```

### Rendering Changes

**Before (string-based):**
```rust
fn render_to_string(node: &Node, width: usize) -> String
fn composite_overlays(base: &[String], overlays: &[Overlay], width: usize) -> Vec<String>
```

**After (buffer-based):**
```rust
fn render_to_buffer(node: &Node, buffer: &mut Buffer, x: u16, y: u16)
fn composite_layers(base: &mut Buffer, overlays: &[OverlayBuffer])
```

### Compositing Flow

```
1. Create base Buffer (viewport size)
2. Render main tree to base buffer
3. For each overlay:
   a. Create overlay Buffer
   b. Render overlay content to overlay buffer
   c. Calculate position based on anchor (FromBottom, FromBottomRight)
   d. Merge overlay buffer onto base buffer (transparent cells pass through)
4. Convert final buffer to strings with ANSI codes
5. Output to terminal
```

### Correct Behavior

```
Base buffer (input box):
  ▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
  > hello                            
  ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀

Overlay buffer (notification, spaces=transparent):
            ▗▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
             ▌ ✓ Ctrl+C again to quit
                                    ▘

After merge (overlay overwrites only non-transparent cells):
  ▄▄▄▄▄▄▄▄▄▄▄▗▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄▄
  > hello     ▌ ✓ Ctrl+C again to quit
  ▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▀▘
```

## Implementation Phases

### Phase 1: Add Buffer Types (Additive)
- [ ] Create `buffer.rs` with `Cell` and `Buffer` types
- [ ] Add `Buffer::merge()` for layer compositing
- [ ] Add `Buffer::to_lines()` for terminal output
- [ ] Unit tests for buffer operations

### Phase 2: Buffer-Based Overlay Compositing
- [ ] Add `render_overlay_to_buffer()` 
- [ ] Modify `composite_overlays()` to use buffer merge
- [ ] Keep string-based main rendering (for now)
- [ ] Test notification overlay preserves input borders

### Phase 3: Full Buffer Rendering (Optional, Larger Refactor)
- [ ] Convert `render_to_string()` → `render_to_buffer()`
- [ ] Update all node rendering functions
- [ ] Update graduation system for buffers
- [ ] Performance optimization (diff buffers)

## Phase 2 Design (Recommended Starting Point)

Minimal change to fix the overlay bug:

```rust
// overlay.rs - new function
pub fn composite_overlays_buffered(
    base_lines: &[String], 
    overlays: &[Overlay], 
    width: usize
) -> Vec<String> {
    // 1. Convert base_lines to Buffer
    let mut buffer = Buffer::from_lines(base_lines, width);
    
    // 2. For each overlay, render to temp buffer and merge
    for overlay in overlays {
        let overlay_buf = Buffer::from_lines(&overlay.lines, width);
        let (offset_x, offset_y) = calculate_overlay_position(&overlay.anchor, &buffer);
        buffer.merge(&overlay_buf, offset_x, offset_y);
    }
    
    // 3. Convert back to lines
    buffer.to_lines()
}
```

This keeps the existing string-based rendering but fixes compositing.

## Files to Modify

| File | Changes |
|------|---------|
| `oil/buffer.rs` | NEW - Buffer and Cell types |
| `oil/mod.rs` | Add `mod buffer` |
| `oil/overlay.rs` | Use buffer-based compositing |
| `oil/planning.rs` | Use new compositing function |

## Testing Strategy

1. Unit tests for `Buffer` operations
2. Test `composite_overlays_buffered()` preserves base content
3. Snapshot test: notification + input box shows both
4. Existing tests should continue passing

## Success Criteria

- [ ] Notification snapshot shows input box borders preserved
- [ ] All existing overlay tests pass
- [ ] All existing graduation tests pass
- [ ] `cargo nextest run -p crucible-cli --profile ci` passes
