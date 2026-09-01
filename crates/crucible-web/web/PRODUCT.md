# Product

<!-- impeccable:product-schema 1 -->

Scope: the web UI at `crates/crucible-web/web`. Users, purpose and positioning
come from the whole Crucible workspace, so design decisions stay grounded in the
product. The TUI and the CLI are context here, not design targets.

## Platform

web

## Users

**Primary:** power users and developers who write code and keep notes. They run
Crucible on their own machine. They use a terminal every day.

**Situation:** the user works inside one project. The user must read notes, edit
them, watch the graph, and talk to an agent at the same time.

**Job:** the user grounds an agent in a personal knowledge graph. The user then
keeps what the agent learns as more knowledge.

**Relation to the TUI:** the web UI is a peer of the TUI. It is not a lite view.
The same person uses both surfaces. The web UI owns the work that a terminal
renders badly — the graph, the editor, canvases, diffs and the file tree — and it
matches the TUI in depth.

**Later audience:** `docs/Meta/Product.md` records a broader, non-terminal
audience as a future phase. That audience does not constrain the design today.

## Product Purpose

Crucible is a knowledge-grounded agent runtime. Notes, sessions and wikilinks
form a knowledge graph. Agents read from that graph and write back to it.

The web UI gives direct manipulation of the same local daemon that the TUI
drives. It shows the graph, the notes and the agent session in one window.

**Success:** the user does a full task in the browser — read a note, edit it,
run a session against it, review the diff, and keep the result — without a fall
back to the terminal, and without a loss of the knowledge that the task made.

## Positioning

Memory and knowledge are part of the core, not an add-on. Four mechanisms carry
this, and a neighboring product cannot truthfully copy them together:

- Precognition injects relevant context before the first agent turn.
- A session saves as markdown in the same graph. It is linkable and greppable.
- Embeddings work at block level, so retrieval returns a paragraph.
- The SQLite index is optional acceleration. The files rebuild it.

For the web UI: the browser talks to a local daemon over HTTP, SSE and WS. It is
not a cloud dashboard. There is no account, and there is no server that the user
does not own.

## Operating Context

- The daemon owns all storage and all business logic. The web UI renders and
  takes input. If the web UI must duplicate daemon logic, the logic sits in the
  wrong crate.
- Development: `bun run dev` serves the UI on `localhost:5273` and proxies
  `/api/*` to the Axum server on port 3000.
- Production: `cru web` serves `dist/` through rust-embed. The bundle compiles
  into the binary.
- The shell is a window manager: two rails, edge panels, recursive split panes,
  tab groups, floating windows, a corner bar and a status bar.
- Desktop-first. The design targets a wide screen.
- The planned remote path is self-hosted access over Tailscale or a Cloudflare
  Tunnel, not a hosted service.
- Files stay on the user's disk as markdown.

## Capabilities and Constraints

**Stack constraints:** SolidJS, Tailwind CSS, Vite, and bun as the only package
manager. No SSR. The build is static. React idioms do not apply.

**Surfaces in the shell:** editor (CodeMirror, with a vim mode), reading view,
markdown live preview, graph (d3-force), canvas, terminal (xterm), chat and
session panels, diff and review, file tree, search, backlinks, inbox,
notifications, settings, skills, plugins, and a command palette.

**Rendering:** markdown-it with DOMPurify, KaTeX for math, Shiki for code, and
Mermaid for diagrams. Geist and Geist Mono ship as variable web fonts.

**Themes:** a dark theme and a light theme both ship (`lib/theme.ts`,
`components/editor/editor-theme.ts`, `index.css`).

**Terminology is fixed.** These words each carry one meaning, and a rename
breaks the product:

- **Project** — where work output goes.
- **Kiln** — where knowledge goes.
- **Workspace** — an instance of a project directory.
- **Review** — the agent's file edits. **Proposal** — a suggested note. They are
  never interchangeable.

**Undecided:** the mobile layout, and whether the PWA path ships.
`vite-plugin-pwa` is installed, but no small-screen form is settled.

## Brand Commitments

`assets/brand.md` is binding. It carries the durable marks; the palettes live in
`index.css` (app) and `docs-site/src/styles/tokens.css` (docs site).

- The wordmark is "Crucible". It carries no glyph prefix, no emoji and no icon
  lockup. The alembic text logo is retired.
- **Bodoni Moda is docs-site display type only.** The web app must not load it.
  Every app surface uses Geist, with Geist Mono for code.
- The favicon source is `assets/favicon.svg`. It deploys byte-identical to
  `crates/crucible-web/web/public/favicon.svg` and `docs-site/public/favicon.svg`.
- **Ember is the one loud colour.** The canonical primary is `#E0653A`. On a
  dark field the ramp is `#E0653A` / hover `#F08A5E` / active `#C4552E`; on a
  light field it is `#B04823` / `#963C1C` / `#7D3116`. Hover moves away from the
  field. Ember is never body text.
- **Neutrals shift warm.** A cool grey next to this much ember reads blue. Slate
  blue is not a brand colour.
- Retired, and not to be reintroduced: `#FF8C42`, `#22D3EE`, `#FFDD57`,
  `#FF6B1A`, `#FFAA33`, and slate as a secondary. Meaning now rides on semantic
  colour — `--color-attention`, `--color-ok`, `--color-precog`.

**Voice:** plain, factual and evidence-first. `docs/Meta/Product.md` marks an
unproved claim in italics instead of a claim of the feature. The UI must hold
the same standard.

## Evidence on Hand

- `docs/Meta/Product.md` — the capability map. Each shipped entry carries a
  "Gets you" line and a "Proof" line that names a test.
- `README.md` — positioning and a comparison table.
- `assets/` — `demo.gif`, `cru-overview.gif`, `delegation-demo.gif`,
  `chat-demo.png`, `chat-response.png`, and `brand.md`.
- Tests: Vitest units, Playwright `chromium` mocked specs, Playwright story
  suites with committed screenshot baselines under
  `crates/crucible-web/web/e2e/__screenshots__/stories`, and a live tier that
  runs against a real daemon and a temporary kiln.
- The docs site is at `mootikins.github.io/crucible`.
- Accessibility signals in code: 51 source files use `aria-` attributes, and
  `index.css` honors `prefers-reduced-motion`.

**Absences that future work must not invent:** there are no customers, no
testimonials, no benchmarks, no pricing, no license tiers and no user counts.
The README states that the project is in early development.

## Product Principles

1. **The daemon owns the logic.** The web UI renders state and sends input.
2. **Files are the truth.** Every action lands as markdown bytes on disk.
3. **Show the proof.** The UI shows real state. It never implies work that the
   system did not do.
4. **The web UI is a peer, not a lite view.** Do not remove depth to make a
   screen calm.
5. **Keep the terminology exact.** Project, Kiln, Workspace, review and proposal
   each hold one meaning.

## Accessibility & Inclusion

**The target is WCAG 2.1 Level AA.** The decision is on record; it is not open.
New work meets AA, and a change that lowers a ratio below AA is a defect.

**Verified mechanically.** `src/lib/__tests__/contrast.test.ts` parses the token
blocks in `index.css` and computes WCAG contrast from the parsed values. It
holds four floors, in BOTH themes:

- every ink weight — `shell-ink`, `shell-body`, `muted`, `muted-dark` — clears
  4.5:1 on every panel surface, including the lightest one;
- `shell-ink`, `shell-body` and `muted` clear 4.5:1 on `--color-control`;
  `muted-dark` is too faint for a control fill and is not allowed there;
- `--color-on-primary` clears 4.5:1 on a solid ember fill, at rest and on hover;
- `--color-focus-ring` clears the 3:1 non-text floor on every surface.

The test also holds the two themes to the same `--color-*` token set, and holds
the ink ramp to four steps that stay apart. It never greps for a hex literal.

**Verified by construction.** One focus treatment, the `focus-ring` utility in
`index.css`. It suppresses the outline for a pointer focus and draws a 2px ember
outline for `:focus-visible`. It replaces `focus:outline-none`; the two are never
written together.

**A floor for functional text: 11px.** Fifty `text-[10px]` labels moved up to
it. Four files still hold the old size; they are listed below.

**Not verified mechanically, and not claimed.** No axe or Lighthouse run is
wired into CI. Screen-reader behaviour, focus ORDER, live-region announcements,
target size and reflow are unmeasured. `aria-` attributes appear in 51 source
files, which is a signal, not a result.

**Known open failures**, measured and recorded rather than quietly carried:

- `bg-error` with white text is 3.76:1 on the dark theme (5.92:1 light). The
  error ramp is untouched so far.
- The command palette marks its selected row with `bg-primary/15`, which is
  1.21:1 against the surface behind it — under the 3:1 a state indicator owes.
  An ember outline now rides along with that tint, so the state is carried by
  something that clears the floor.
- ~~`text-[10px]` survives in `ToolCard.tsx`, `ChangesPanel.tsx`, `EmptyPane.tsx`
  and `EdgePanel.tsx` (8px there).~~ Closed 2026-08-31: all of it now reads
  `--text-floor`, and the notification badge was resized to hold its count.

Two more facts, unchanged:

- `index.css` honors `prefers-reduced-motion`.
- The audience lives in a terminal, so full keyboard operation matters. A
  command palette and vim keybindings are present.
