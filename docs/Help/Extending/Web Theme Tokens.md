---
title: Web Theme Tokens
description: Restyle the web UI from a plugin with one stylesheet of CSS custom properties
status: partial
tags:
  - extending
  - web
  - theme
  - css
  - plugins
aliases:
  - CSS Tokens
  - Web Theming
---

# Web Theme Tokens

The web UI reads every colour, radius, row height and type size from a CSS
custom property. The property names start with `--cru-`. They are public.
A plugin sets them in one stylesheet, and the whole app follows.

The contract has three promises:

1. A plugin sets a value. It never names a component class.
2. A plugin writes no `!important`. The cascade gives the plugin the win.
3. A name in the table below does not change. A new name can appear.

> [!warning] The delivery path is not built yet
> The token layer is in place: 85 names, in `@layer cru-theme` in
> `crates/crucible-web/web/src/index.css`, with tests that hold them there.
> The route that serves a plugin stylesheet is NOT. This page defines the
> names so that plugin authors and the app agree before the first theme
> ships. Read [[Meta/Analysis/Plugin Web Delivery]] for that design and its
> open questions.

## How an override wins

The app declares its defaults inside a CSS cascade layer named `cru-theme`:

```css
@layer cru-theme, theme, base, components, utilities;

@layer cru-theme {
  :root                     { /* dark defaults  */ }
  :root[data-theme='light'] { /* light defaults */ }
}
```

An unlayered rule beats a layered rule at any specificity. A plugin
stylesheet declares no layer, so its rules are unlayered. A plugin therefore
overrides both themes with a plain `:root` selector. No `!important` is
necessary, and none is permitted.

To restyle one theme only, add the theme selector:

```css
:root                      { --cru-color-primary: #3a7fe0; }  /* both themes */
:root[data-theme='light']  { --cru-color-primary: #1f4a91; }  /* light only  */
```

## Which tokens change with the theme

A colour and a shadow carry a value in EACH theme. A radius, a row height, a
type size and a measure do not: the app declares each of them once, in the
base block, and the light block does not repeat it. A test enforces the
split. A second copy of a value that never differs is a copy that can only
drift.

To re-value a radius, therefore, set it once on `:root`. To re-value a
colour, set it on `:root` for both themes, or under the theme selector for
one.

## Colour

Every colour token has a value in both themes.

### Surfaces

| Token | Role | Dark | Light |
|---|---|---|---|
| `--cru-color-shell-bg` | The window ground behind every panel | `#0e0d11` | `#edecf2` |
| `--cru-color-shell-panel` | A docked panel's own ground | `#141318` | `#f5f4f8` |
| `--cru-color-surface-base` | A card or a list at rest | `#141318` | `#f5f4f8` |
| `--cru-color-surface-elevated` | A card that lifts off the panel | `#1c1b22` | `#ffffff` |
| `--cru-color-surface-overlay` | A menu, a popover or a dialog | `#232128` | `#ffffff` |
| `--cru-color-control` | The fill of an input or a button | `#302e38` | `#e0dee6` |
| `--cru-color-hover-wash` | The wash that a pointer lays over a row | `rgba(255,255,255,.05)` | `rgba(23,22,28,.055)` |

### Ink

| Token | Role | Dark | Light |
|---|---|---|---|
| `--cru-color-ink` | A title or a primary label | `#e7e4df` | `#17161c` |
| `--cru-color-body` | Running text | `#c4c0ba` | `#34323c` |
| `--cru-color-muted` | A secondary label | `#9f9ba5` | `#4f4d57` |
| `--cru-color-muted-dark` | The quietest text that keeps 4.5:1 | `#8d8990` | `#66656e` |

### Lines

| Token | Role | Dark | Light |
|---|---|---|---|
| `--cru-color-hairline` | A 1px separator inside a surface | `#211f26` | `#dedde4` |
| `--cru-color-hairline-strong` | A 1px border that defines an edge | `#322f38` | `#c7c5ce` |

### Accent

| Token | Role | Dark | Light |
|---|---|---|---|
| `--cru-color-primary` | The ember. The one accent that means "act on this" | `#e0653a` | `#b04823` |
| `--cru-color-primary-hover` | The ember under a pointer | `#f08a5e` | `#963c1c` |
| `--cru-color-primary-active` | The ember while a user presses it | `#c4552e` | `#7d3116` |
| `--cru-color-on-primary` | Ink on a solid ember fill | `#0e0d11` | `#ffffff` |
| `--cru-color-focus-ring` | The keyboard focus ring | `var(--cru-color-primary)` | `var(--cru-color-primary)` |

The focus ring points at the accent. To move the ring with the accent, set
`--cru-color-primary` alone. To hold the ring still, set the ring token too.

### Status

Each status colour carries one meaning. Do not give two meanings to one
colour.

| Token | Role | Dark | Light |
|---|---|---|---|
| `--cru-color-error` | A failure | `#ef4444` | `#c02626` |
| `--cru-color-error-dark` | The wash or the border of a failure | `#991b1b` | `#8a1b1b` |
| `--cru-color-attention` | The app waits on the user | `#d4a72c` | `#866400` |
| `--cru-color-ok` | A thing that finished well | `#7bc47f` | `#3a763f` |
| `--cru-color-precog` | Precognition, and nothing else | `#a78bda` | `#6d51a8` |

### Terminal

The terminal panel paints 16 ANSI slots. A program writes these colours, so
they stay separate from the status family. Set
`--cru-color-term-<slot>` and `--cru-color-term-bright-<slot>`.

| Slot | Dark | Light | Bright dark | Bright light |
|---|---|---|---|---|
| `black` | `#2b2933` | `#3a3843` | `#6b6673` | `#7d7a85` |
| `red` | `#e8746e` | `#c02626` | `#f2938c` | `#9d1f1f` |
| `green` | `#9dcf85` | `#3a763f` | `#b7e0a1` | `#2b5c2f` |
| `yellow` | `#e0b24c` | `#866400` | `#ecc76e` | `#6a4f00` |
| `blue` | `#7fa7e0` | `#2a5db0` | `#a0c0ee` | `#1f4a91` |
| `magenta` | `#bd93e0` | `#7c3fa8` | `#d0b0ee` | `#63308a` |
| `cyan` | `#79c9c4` | `#12717a` | `#9adcd7` | `#0d585f` |
| `white` | `#c9c5bf` | `#6d6b75` | `#e7e4df` | `#17161c` |

### Canvas

A canvas card takes one of six preset colours. JSON Canvas numbers them 1 to
6. They are categorical. Slot 4 means "the green one". It never means "ok".

| Token | Canvas slot | Dark | Light |
|---|---|---|---|
| `--cru-color-canvas-red` | 1 | `#dd7a76` | `#b32d2d` |
| `--cru-color-canvas-orange` | 2 | `#cb9147` | `#8a5a14` |
| `--cru-color-canvas-yellow` | 3 | `#cdb75f` | `#6f5b0f` |
| `--cru-color-canvas-green` | 4 | `#8fc47f` | `#33713b` |
| `--cru-color-canvas-cyan` | 5 | `#6bbfbb` | `#0e6b73` |
| `--cru-color-canvas-purple` | 6 | `#ae90d6` | `#6f3fa0` |

## Callout

A callout takes one colour per kind. These 13 tokens carry an `r, g, b`
TRIPLE, not a hex: the rules wrap each one in `rgb()` and `rgba()` to build
a border, a wash and an icon mask from one value.

| Token | Dark | Light |
|---|---|---|
| `--cru-color-callout-note` | `96, 165, 250` | `37, 99, 235` |
| `--cru-color-callout-info` | follows `note` | follows `note` |
| `--cru-color-callout-todo` | follows `note` | follows `note` |
| `--cru-color-callout-abstract` | `45, 212, 191` | `13, 148, 136` |
| `--cru-color-callout-tip` | `45, 212, 191` | `13, 148, 136` |
| `--cru-color-callout-success` | `123, 196, 127` | `58, 118, 63` |
| `--cru-color-callout-question` | `212, 167, 44` | `134, 100, 0` |
| `--cru-color-callout-warning` | `237, 137, 54` | `154, 82, 12` |
| `--cru-color-callout-failure` | `239, 68, 68` | `192, 38, 38` |
| `--cru-color-callout-danger` | `239, 68, 68` | `192, 38, 38` |
| `--cru-color-callout-bug` | `239, 68, 68` | `192, 38, 38` |
| `--cru-color-callout-example` | `167, 139, 218` | `109, 81, 168` |
| `--cru-color-callout-quote` | `152, 147, 158` | `79, 77, 87` |

## Radius

The three role names are the contract. Set those. The legacy steps stay for
the classes that still name a step.

| Token | Role | Value |
|---|---|---|
| `--cru-radius-control` | An input, a chip or a button | `6px` |
| `--cru-radius-card` | A card, a popover or a menu | `10px` |
| `--cru-radius-composer` | The prompt field, at one line and at many lines | `18px` |
| `--cru-radius-sm` | Legacy step | `3px` |
| `--cru-radius-md` | Legacy step | `4px` |
| `--cru-radius-lg` | Legacy step | `6px` |
| `--cru-radius-xl` | Legacy step | `8px` |
| `--cru-radius-2xl` | Legacy step | `10px` |
| `--cru-radius-3xl` | Legacy step | `14px` |

## Row height

These three values set the density of every list and every tree.

| Token | Role | Value |
|---|---|---|
| `--cru-row-sm` | A desktop list row or tree row | `28px` |
| `--cru-row-md` | A palette row, and a tree row at touch density | `36px` |
| `--cru-row-touch` | A phone control row | `44px` |

## Type

| Token | Role | Value |
|---|---|---|
| `--cru-font-ui` | The interface family | `'Geist Variable', system-ui, …` |
| `--cru-font-mono` | The monospace family | `'Geist Mono Variable', ui-monospace, …` |
| `--cru-font-floor` | Timestamps, token counts and kiln names | `11px` |
| `--cru-font-reading` | The transcript, note prose, chips and tool rows | `13px` |
| `--cru-font-title` | An empty state title or a card title | `14px` |
| `--cru-leading-reading` | One leading for every block of running text | `1.6` |
| `--cru-font-reading-leading` | The leading of the reading step, which `text-xs` reads | `1.5` |

Note prose takes `--cru-font-reading` as its root, and the headings scale in
`em` from it. To enlarge the headings, raise the reading size. There is no
separate prose token, because a second name for the same 13px invites the
two to drift apart. Hierarchy above the reading size moves on weight, not on
size.

## Measure

| Token | Role | Value |
|---|---|---|
| `--cru-measure-chat` | The chat column. The transcript, the composer and the chip row stop at this edge | `64rem` |
| `--cru-measure-empty` | The body line of an empty state | `36ch` |
| `--cru-measure-tab` | The cap on a tab title before the middle ellipsis | `200px` |

## Elevation

The geometry stays the same in both themes. Only the ink changes.

| Token | Dark | Light |
|---|---|---|
| `--cru-shadow-sm` | `0 1px 2px rgba(0,0,0,.3)` | `0 1px 2px rgba(23,22,28,.08)` |
| `--cru-shadow-md` | `0 2px 8px -2px rgba(0,0,0,.4)` | `0 2px 8px -2px rgba(23,22,28,.11)` |
| `--cru-shadow-lg` | `0 6px 20px -6px rgba(0,0,0,.5)` | `0 6px 20px -6px rgba(23,22,28,.14)` |
| `--cru-shadow-xl` | `0 12px 32px -8px rgba(0,0,0,.55)` | `0 12px 32px -8px rgba(23,22,28,.17)` |
| `--cru-shadow-2xl` | `0 18px 46px -10px rgba(0,0,0,.6)` | `0 18px 46px -10px rgba(23,22,28,.2)` |

## A worked example

This stylesheet gives the app a cool blue accent and square chrome. It
changes 9 values and touches no component.

```css
/* theme.css — a plugin's whole web theme. */

:root {
  /* One accent, three states. The app derives every hover and press from
     these, so a chip, a button and a link all follow. */
  --cru-color-primary:        #3a7fe0;
  --cru-color-primary-hover:  #5e9bf0;
  --cru-color-primary-active: #2e63b4;

  /* Ink on a solid accent fill. A light blue needs dark ink. */
  --cru-color-on-primary:     #0b0f17;

  /* Square the chrome. A radius does not change with the theme, so it is set
     once here and the light block below does not repeat it. The composer
     keeps a small radius: a zero-radius prompt field reads as a text editor
     rather than as a prompt. */
  --cru-radius-control: 0px;
  --cru-radius-card:    2px;
  --cru-radius-composer: 4px;
}

/* The light theme needs a darker accent. A #3a7fe0 fill on white measures
   3.1:1, which fails as text and as a button. */
:root[data-theme='light'] {
  --cru-color-primary:        #1f4a91;
  --cru-color-primary-hover:  #17376d;
  --cru-color-primary-active:  #102850;
  --cru-color-on-primary:     #ffffff;
}
```

The focus ring turns blue with no further work. `--cru-color-focus-ring`
points at `--cru-color-primary` by default.

### Check the contrast

The app tests its own defaults against WCAG 2.1 AA. It does not test a
plugin's values. A theme that fails contrast produces an app that a user
cannot read. Check two ratios before you ship a theme:

- `--cru-color-on-primary` against `--cru-color-primary` needs 4.5:1.
- `--cru-color-focus-ring` against every surface needs 3:1.

## What the app does with a value you set

Most of the app reads a token through a CSS rule, so a new value paints on
the next frame. Three places do not, and a plugin author should know which:

- **The terminal and the knowledge graph paint on a canvas.** A canvas
  cannot follow a custom property, so both read the tokens once and repaint
  on a theme switch. A stylesheet that loads later needs that same repaint.
- **The editor compiles its syntax colours at configuration time.** Prose,
  wikilinks and the editor chrome follow the tokens. The code syntax palette
  is a CodeMirror highlight style and this contract does not publish it.
- **Elevation reaches `.shadow-lg` through an alias.** It works, and a test
  proves it, but it is the one family where a change in the app's own
  stylesheet could break a plugin override without breaking the app.

## Rules and limits

- **Set a value. Do not name a class.** A rule that targets
  `.composer-surface` styles an implementation detail. The next release
  breaks it.
- **Do not write `!important`.** The layer gives an unlayered plugin rule the
  win already. An `!important` blocks the user's own later override.
- **Do not add a colour.** A plugin cannot add a seventh canvas slot or a
  second accent. It gives a new value to a role that exists.
- **Two themes conflict by install order.** The app appends one stylesheet per
  plugin. The last stylesheet wins.
- **The layout does not move.** No token changes a flex direction, a panel
  position or the window manager.

## Related

- [[Help/Extending/Creating Plugins]] builds the plugin that carries the
  stylesheet.
- [[Help/Extending/Scripted UI]] themes the TUI. The TUI and the web UI use
  separate theme systems, and a Lua theme does not reach the browser.
- [[Meta/Analysis/Plugin Web Delivery]] records the design for plugin web
  assets and its open questions.
- [[Help/Config/web]] configures the web server.
