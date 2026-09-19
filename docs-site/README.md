# Crucible documentation site

The published documentation at <https://mootikins.github.io/crucible/>: Astro +
Starlight, deployed to GitHub Pages by `.github/workflows/pages.yml`.

## Where the content comes from

**The repository is the source.** `src/lib/kiln-loader.mjs` reads the kiln
directly, so the pages are `docs/Help/**` and `docs/Guides/**` plus this site's
own MDX under `src/content/docs/`. There is no committed copy to hand-edit and
no conversion step: a note edited under `docs/Help/` is the page that ships.

`docs/Meta/**` is deliberately unpublished — architecture notes and contributor
material. Canvases are the exception to the directory rule:
`src/lib/canvas-pages.mjs` publishes every `.canvas` under `docs/` except
`Meta/`, so `docs/Canvas Tour.canvas` has a page.

Two remark plugins adapt kiln conventions rather than rewriting 80 files:
`remark-kiln-wikilinks` turns `[[Note Name]]` into a site-absolute link, and
`remark-strip-title-heading` drops each note's leading H1 because Starlight
renders the frontmatter title as the page heading.

## Commands

Run from this directory; the package manager is **bun**.

| Command | Action |
| --- | --- |
| `bun install` | Install dependencies |
| `bun run dev` | Development server on `localhost:4321` |
| `bun run check:sidebar` | Check every sidebar slug against the content — run before `build` |
| `bun run build` | Build the site into `dist/` |
| `bun run check:links` | Check every internal link against `dist/` — run it after a build |
| `bun run preview` | Serve a built site locally |

Both checks are blocking in CI, and both exist because a green build proves
nothing: Starlight fails on the *first* stale sidebar slug (so `check:sidebar`
runs first and names them all), and 240 of 257 in-content cross-references once
404'd through a build that succeeded (which is `check:links`).

## Sidebar

`astro.config.mjs` addresses pages by slug, so **a new note under `docs/Help/`
or `docs/Guides/` needs a sidebar entry there or it is reachable only by search.**
Slugs come from `generateId` in `kiln-loader.mjs` (lower-case, spaces to
hyphens, `index` dropped).