# Crucible Brand

> The durable, cross-surface marks only. The implementation palettes live in
> code — `crates/crucible-web/web/src/index.css` for the app, and
> `docs-site/src/styles/tokens.css` for the docs site. This file does not repeat
> them. It states what both must obey.

Reviewed 2026-08-29.

## Wordmark

**Crucible**. No emoji, no glyph prefix, no icon lockup. The word stands alone.

The alembic character (U+2697) was a text logo. It is retired: it renders
differently on every platform, it carries a colour we do not control, and it
made the product name look like a chat message.

**Bodoni Moda is display type for the docs site only.** The web app does not
load it, and it must not. The app sets every surface in IBM Plex Sans, because
the app is a tool and a Didone is fragile at UI sizes. Read the `opsz` note in
`tokens.css` before you set Bodoni anywhere.

Body and code are IBM Plex Sans and IBM Plex Mono on every surface.

## Favicon

`assets/favicon.svg` — a stylized alembic flask with ember liquid. This is the
one place the alembic survives. It is artwork here, not text.

Two copies deploy from it, byte-identical:

- `crates/crucible-web/web/public/favicon.svg`
- `docs-site/public/favicon.svg`

To change the icon, edit `assets/favicon.svg`, then copy it to both paths.

## The ember ramp

Ember is the one loud colour. It marks heat: a primary action, an active state,
a live thing. It is never body text.

**Canonical primary: `#E0653A`.**

| Field | Primary   | Hover     | Active    |
|-------|-----------|-----------|-----------|
| Dark  | `#E0653A` | `#F08A5E` | `#C4552E` |
| Light | `#B04823` | `#963C1C` | `#7D3116` |

Hover moves *away* from the field. It lightens on dark, and it darkens on light.

Ember at full strength fails contrast as text on a light field. The light ramp
is therefore the same colour, burnt down until it passes.

## Neutrals are warm

Every neutral carries a few points of red. A pure grey or a cool grey next to
this much ember reads blue. The app's field runs `#0E0D11` to `#302E38`. The
docs site's field runs `#0A0A0C` to `#1E1D22`. Both shift warm on purpose.

Slate blue is **not** a brand colour. It was the secondary once. The docs site
keeps `#64748B` for one job — terminal chrome and window dots, where a cool
colour reads as "machine". The app does not use it at all.

## Retired

Do not reintroduce these. They survive in old screenshots and old copy.

| Colour                | Was                            | Status |
|-----------------------|--------------------------------|--------|
| `#FF8C42`             | Crucible Amber, primary        | Superseded by `#E0653A`. |
| `#22D3EE`             | Cyan, "links and interactive"  | Retired. The app carries meaning in semantic colour instead — see `--color-attention`, `--color-ok` and `--color-precog` in `index.css`. |
| `#64748B` / `#334155` | Slate, secondary               | Demoted to docs-site machine chrome. |
| `#FFDD57`             | Bubble Gold                    | Retired. |
| `#FF6B1A` / `#FFAA33` | Flame Dark / Flame Light       | Retired with the old primary. |

## Voice

Plain, factual and evidence-first. Name the thing. Do not sell it.

`docs/Meta/Product.md` sets the bar: a shipped claim carries a **Proof** line
that names a test, and a claim that nothing demonstrates is marked in italics as
`_none — …_` instead of stated. UI copy and marketing copy hold the same bar.
Never write a capability that the tree does not demonstrate.

## Open

**The docs site still ships the retired primary.** It cites this file as its
palette origin while it disagrees with it. The migration to `#E0653A` touches
six declarations in four files, and every one needs a new contrast check against
its field:

| File | What holds the retired amber |
|------|------------------------------|
| `docs-site/src/styles/tokens.css` | `--cru-ember`, and the ramp derived from it: `--cru-forge`, `--cru-flame`, `--cru-gold`, `--cru-ember-on-paper` |
| `docs-site/src/styles/tokens.css` | the three washes, as literal `rgba(255, 140, 66, …)` — `--cru-ember-wash`, `--cru-ember-edge`, `--cru-ember-halo` |
| `docs-site/src/styles/custom.css` | `--sl-color-bg-inline-code` |
| `docs-site/src/pages/index.astro` | a hard-coded glow in the hero shadow stack |
| `docs-site/src/components/HeroGraph.astro` | `const EMBER = [255, 140, 66]`, painted to canvas |

The three `rgba()` forms and the canvas constant do not match a search for the
hex. Search for `255, 140, 66` as well, or the migration leaves half the site on
the old amber.
