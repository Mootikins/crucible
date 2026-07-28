import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { slugifyRelPath } from './kiln-links.mjs';

/**
 * Publishing the kiln's `.canvas` boards as read-only pages.
 *
 * A canvas is a document like any other note, and one that exists only inside
 * the app is a document the docs cannot point at. These pages are deliberately
 * a *view*: no editing, no live web embeds, no daemon. The site is static, so
 * everything here happens at build time.
 *
 * Geometry and edge routing are imported from the app itself
 * (`crucible-web/web/src/lib/canvas-types.ts`, which has no imports of its own)
 * rather than reimplemented. Two renderers drifting apart would show the same
 * board differently in the app and on the site, and edge routing is exactly the
 * fiddly arithmetic nobody would notice going wrong.
 */

const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
const KILN = path.join(ROOT, 'docs');

/**
 * Directories whose canvases are not published.
 *
 * `Meta/` holds architecture notes and contributor material — the note loader
 * excludes it for that reason and canvases follow the same rule, or a board
 * about internals would appear in user documentation.
 */
const PRIVATE_DIRS = new Set(['Meta']);

function walk(dir, out = []) {
	for (const name of readdirSync(dir)) {
		const full = path.join(dir, name);
		if (statSync(full).isDirectory()) {
			if (!PRIVATE_DIRS.has(path.relative(KILN, full))) walk(full, out);
			continue;
		}
		// `_name.canvas` is a draft, matching the note loader's `[^_]*` glob.
		if (name.endsWith('.canvas') && !name.startsWith('_')) out.push(full);
	}
	return out;
}

/** Note directories the site publishes — the note loader's globs. */
const PUBLISHED_DIRS = ['Help', 'Guides'];

/**
 * Where a `file` reference points on the site.
 *
 * An EXACT path lookup, deliberately not `resolveWikilink`. A wikilink is a
 * fuzzy target — it falls back to matching a bare filename anywhere in the kiln
 * — but a canvas `file` node stores a real kiln-relative path, and running it
 * through the fuzzy resolver silently pointed a card at a different note:
 * `Index.md`, which the site does not publish, resolved to `Help/CLI/index.md`
 * and rendered as a confident link to an unrelated page.
 *
 * Only notes under Help/ and Guides/ are published, so a card referencing
 * anything else — an image, a note in Meta/, a file that was deleted — has no
 * page to open, and says so by not being a link.
 */
export function referenceHref(relPath, base) {
	if (!/\.md$/i.test(relPath)) return null;
	const segments = relPath.split('/');
	if (!PUBLISHED_DIRS.includes(segments[0])) return null;
	// Matches the loader's `[^_]*.md`: a draft is not a page.
	if (segments.some((s) => s.startsWith('_'))) return null;
	// A reference to a note that no longer exists stays legal in the document —
	// it just has nothing to open.
	if (!existsSync(path.join(KILN, relPath))) return null;

	const slug = slugifyRelPath(relPath.replace(/\.md$/i, ''));
	return `${base}/${slug}/`.replace(/([^:])\/{2,}/g, '$1/');
}

/** Every publishable canvas, parsed. */
export function listCanvases() {
	return walk(KILN).map((full) => {
		const rel = path.relative(KILN, full);
		const raw = readFileSync(full, 'utf-8');
		let doc;
		try {
			doc = JSON.parse(raw);
		} catch (cause) {
			// Loud rather than skipped. A canvas that silently fails to publish
			// is a 404 nobody notices until a reader finds it; the kiln is also
			// a test fixture, so a malformed board is a real defect.
			throw new Error(`Canvas is not valid JSON: docs/${rel}`, { cause });
		}
		return {
			rel,
			slug: slugifyRelPath(rel.replace(/\.canvas$/i, '')),
			title: path.basename(rel, '.canvas'),
			doc: { nodes: doc.nodes ?? [], edges: doc.edges ?? [] },
		};
	});
}
