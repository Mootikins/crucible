# Ink Architecture Reference

Reference notes from analyzing [vadimdemedes/ink](https://github.com/vadimdemedes/ink) for our Rust TUI implementation.

## Core Architecture

### Layout Engine: Yoga
Ink uses **Yoga** (Facebook's flexbox implementation) for layout. This gives them:
- Full flexbox semantics (row/column, wrap, grow/shrink, align, justify)
- Absolute/relative positioning
- Padding, margin, borders, gaps

**Rust equivalent**: `taffy` crate (pure Rust Yoga port)

### Rendering Pipeline
1. **React reconciler** builds virtual DOM tree
2. **Yoga** computes layout (x, y, width, height for each node)
3. **Output** class builds a 2D character grid
4. **log-update** diffs and writes to terminal

### Output Class (`output.ts`)
Virtual framebuffer approach:
- Pre-allocate `height × width` grid of `StyledChar`
- Write operations record position + text + transformers
- `get()` renders all operations to final string
- Handles clipping regions for overflow

### Incremental Rendering (`log-update.ts`)
Key optimization - **line-level diffing**:
```typescript
for (let i = 0; i < visibleCount; i++) {
  if (nextLines[i] === previousLines[i]) {
    buffer.push(ansiEscapes.cursorNextLine);  // Skip unchanged
    continue;
  }
  // Write changed line
  buffer.push(cursorTo(0) + nextLines[i] + eraseEndLine + '\n');
}
```

This prevents flickering by only rewriting changed lines.

## Components

### Box (`Box.tsx`)
Flexbox container, maps directly to Yoga node:
- `flexDirection: 'row' | 'column'`
- `flexGrow`, `flexShrink`, `flexBasis`
- `padding*`, `margin*`, `gap`
- `border` (with box-drawing characters)
- `overflow: 'hidden' | 'visible'`

### Text (`Text.tsx`)
Styled inline text:
- `color`, `backgroundColor`
- `bold`, `italic`, `underline`, `strikethrough`, `inverse`
- `dimColor`
- `wrap` (text wrapping mode)

Uses `internal_transform` function to apply ANSI styles.

### Static (`Static.tsx`)
Scrollback/graduated content:
- `items` array of data
- Renders with `position: absolute`
- Uses `internal_static` flag
- Rendered ONCE, then skipped in subsequent renders
- `useLayoutEffect` tracks which items have been rendered

Key insight: Static tracks rendered count via React state:
```typescript
const [index, setIndex] = useState(0);
const itemsToRender = items.slice(index);  // Only new items
useLayoutEffect(() => setIndex(items.length), [items.length]);
```

## Styles System (`styles.ts`)

Full flexbox vocabulary:
- `position: 'absolute' | 'relative'`
- `margin`, `marginX`, `marginY`, `marginTop/Bottom/Left/Right`
- `padding` (same pattern)
- `gap`, `columnGap`, `rowGap`
- `flexDirection`, `flexWrap`, `flexGrow`, `flexShrink`, `flexBasis`
- `alignItems`, `alignSelf`, `justifyContent`
- `width`, `height`, `minWidth`, `minHeight`, `maxWidth`, `maxHeight`
- `overflow`, `overflowX`, `overflowY`
- `display: 'flex' | 'none'`
- `borderStyle`, `borderColor`, `borderTop/Bottom/Left/Right`

## Hooks

- `useInput(handler)` - keyboard input handling
- `useApp()` - access app context (exit, etc.)
- `useFocus()` / `useFocusManager()` - focus management
- `useStdin()`, `useStdout()`, `useStderr()` - stream access

## Key Differences from Our Current Impl

| Ink | Our Current | Should Adopt? |
|-----|-------------|---------------|
| Yoga layout engine | Manual col/row | Yes - use `taffy` |
| 2D char grid output | String concatenation | Yes - cleaner clipping/overflow |
| Line-level diffing | Clear + redraw | Yes - reduces flicker |
| React reconciler | Direct tree build | No - keep simple |
| Transform functions | Style struct | Keep ours |

## Migration Path

1. **Add `taffy` for layout** - replace manual Direction::Column/Row
2. **Implement Output buffer** - 2D grid of styled chars
3. **Add line diffing** - track previous lines, skip unchanged
4. **Static component** - track rendered index, skip rerenders

## References

- Yoga: https://yogalayout.dev/
- taffy (Rust): https://github.com/DioxusLabs/taffy
- cli-boxes: https://github.com/sindresorhus/cli-boxes
- ansi-escapes: https://github.com/sindresorhus/ansi-escapes
