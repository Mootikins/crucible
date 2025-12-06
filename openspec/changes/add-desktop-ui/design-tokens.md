# Design Tokens - Desktop UI

Extracted from `terminal-cli-interface.html` reference design.

## Color Palette

### Backgrounds
| Token | Value | Usage |
|-------|-------|-------|
| `bg_primary` | `#1a1a1a` / `rgba(26, 26, 26, 1)` | Main background |
| `bg_secondary` | `#2d2d2d` / `rgba(45, 45, 45, 1)` | Header, input area, cards |
| `bg_tertiary` | `#404040` / `rgba(64, 64, 64, 1)` | Status bar, table headers |
| `bg_hover` | `#404040` | Button/item hover state |
| `bg_code` | `#1a1a1a` | Code block background |

### Borders
| Token | Value | Usage |
|-------|-------|-------|
| `border_default` | `#404040` / `rgba(64, 64, 64, 1)` | Cards, dividers |
| `border_subtle` | `#555555` / `rgba(85, 85, 85, 1)` | Status bar top |

### Text
| Token | Value | Usage |
|-------|-------|-------|
| `text_primary` | `#f9f7f4` / `rgba(249, 247, 244, 1)` | Main text, headings |
| `text_secondary` | `#d4d2ce` / `rgba(212, 210, 206, 1)` | Body text, descriptions |
| `text_muted` | `#a3a3a3` / `rgba(163, 163, 163, 1)` | Paths, hints |
| `text_subtle` | `#737373` / `rgba(115, 115, 115, 1)` | Timestamps, metadata |

### Accent Colors
| Token | Value | Usage |
|-------|-------|-------|
| `accent_green` | `#3f6b21` / `rgba(63, 107, 33, 1)` | Prompt arrow, status indicators |
| `accent_green_bright` | `#76a544` / `rgba(118, 165, 68, 1)` | Links, file paths, headings |
| `accent_blue` | `#2e5c8a` / `rgba(46, 92, 138, 1)` | Code keywords (import/from) |
| `accent_brown` | `#8a5a2e` / `rgba(138, 90, 46, 1)` | Code comments |

## Typography

### Font Families

**Monospace (code, terminal):**
```
"Space Mono", "Fira Code", "Menlo", "Monaco", "Consolas", "Liberation Mono", "Courier New", monospace
```

**UI (system):**
```
-apple-system, BlinkMacSystemFont, "Segoe UI", "Helvetica Neue", Arial, sans-serif
```

**Emoji fallback:**
```
"Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji"
```

### Font Sizes
| Token | Value | Rem | Usage |
|-------|-------|-----|-------|
| `text_xs` | `0.75rem` | 12px | Metadata, status bar, code |
| `text_sm` | `0.875rem` | 14px | Body text, messages |
| `text_base` | `1rem` | 16px | Default |

### Font Weights
| Token | Value | Usage |
|-------|-------|-------|
| `font_normal` | 400 | Body text |
| `font_medium` | 500 | Labels, titles |
| `font_semibold` | 600 | Headings |

### Line Heights
| Token | Value | Usage |
|-------|-------|-------|
| `leading_tight` | 1.33333 | Small text, code |
| `leading_normal` | 1.42857 | Body text |
| `leading_relaxed` | 1.6 | Default body |

## Spacing

### Padding/Margin Scale
| Token | Value | Usage |
|-------|-------|-------|
| `space_1` | `0.25rem` | 4px - tight |
| `space_2` | `0.5rem` | 8px - compact |
| `space_3` | `0.75rem` | 12px - card padding |
| `space_4` | `1rem` | 16px - section padding |
| `space_6` | `1.5rem` | 24px - between sections |

### Gap Scale
| Token | Value | Usage |
|-------|-------|-------|
| `gap_1` | `0.25rem` | Icon + label |
| `gap_2` | `0.5rem` | Inline elements |
| `gap_4` | `1rem` | Section items |

## Component Patterns

### Header Bar
- Height: ~48px
- Background: `bg_secondary`
- Border bottom: 1px `border_default`
- Padding: `0.5rem 1rem`

### Status Bar (Footer)
- Height: ~24px
- Background: `bg_tertiary`
- Border top: 1px `border_subtle`
- Padding: `0.25rem 1rem`
- Font size: `text_xs`

### Input Area
- Background: `bg_secondary`
- Border top: 1px `border_default`
- Padding: `1rem`
- Prompt: `➜` in `accent_green`

### Message/Card
- Background: `bg_secondary`
- Border: 1px `border_default`
- Padding: `0.75rem`

### Code Block
- Background: `bg_primary` (darker than card)
- Border: 1px `border_default`
- Padding: `0.5rem`
- Font: monospace
- Size: `text_xs`

### Table
- Header bg: `bg_tertiary`
- Row border: 1px `border_default`
- Cell padding: `0.75rem`
- Grid: CSS grid with equal columns

### Status Indicator (dot)
- Size: `0.5rem` (8px)
- Color: `accent_green` for connected/active

## Icons

Using Lucide icons:
- `lucide:terminal` - App icon
- `lucide:settings` - Settings button
- `lucide:file-text` - File references
- `lucide:search` - Search command
- `lucide:file-plus` - New file command

Icon sizes:
- Small: `0.75rem` (12px)
- Default: `1rem` (16px)

## Ligatures Note

If using **Fira Code**, enable ligatures for:
- `->` → arrow
- `=>` → fat arrow
- `!=` → not equal
- `<=` → less than or equal
- `>=` → greater than or equal
- `==` → equality

**Space Mono** does not have ligatures but has excellent readability.

## GPUI Implementation

```rust
pub struct Theme {
    // Backgrounds
    pub bg_primary: Hsla,      // #1a1a1a
    pub bg_secondary: Hsla,    // #2d2d2d
    pub bg_tertiary: Hsla,     // #404040

    // Text
    pub text_primary: Hsla,    // #f9f7f4
    pub text_secondary: Hsla,  // #d4d2ce
    pub text_muted: Hsla,      // #a3a3a3
    pub text_subtle: Hsla,     // #737373

    // Accents
    pub accent_green: Hsla,    // #3f6b21
    pub accent_green_bright: Hsla, // #76a544
    pub accent_blue: Hsla,     // #2e5c8a
    pub accent_brown: Hsla,    // #8a5a2e

    // Borders
    pub border_default: Hsla,  // #404040
    pub border_subtle: Hsla,   // #555555

    // Fonts
    pub font_mono: &'static str,  // "Space Mono", "Fira Code", ...
    pub font_ui: &'static str,    // system fonts
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            bg_primary: rgb(0x1a1a1a).into(),
            bg_secondary: rgb(0x2d2d2d).into(),
            bg_tertiary: rgb(0x404040).into(),

            text_primary: rgb(0xf9f7f4).into(),
            text_secondary: rgb(0xd4d2ce).into(),
            text_muted: rgb(0xa3a3a3).into(),
            text_subtle: rgb(0x737373).into(),

            accent_green: rgb(0x3f6b21).into(),
            accent_green_bright: rgb(0x76a544).into(),
            accent_blue: rgb(0x2e5c8a).into(),
            accent_brown: rgb(0x8a5a2e).into(),

            border_default: rgb(0x404040).into(),
            border_subtle: rgb(0x555555).into(),

            font_mono: "Space Mono, Fira Code, Menlo, Monaco, monospace",
            font_ui: "-apple-system, BlinkMacSystemFont, Segoe UI, sans-serif",
        }
    }
}
```
