import { glob } from 'astro/loaders';
import { slugifySegment } from './kiln-links.mjs';

/**
 * Loads the docs collection from the `docs/` kiln directly.
 *
 * There used to be a second copy: `scripts/convert-docs.mjs` rm -rf'd
 * `docs-site/src/content/docs/{help,guides}` and rewrote 80 files into it, and
 * that copy was committed. A generated tree you can hand-edit gets hand-edited —
 * fixes landed in the copy and were wiped by the next CI build, the copy drifted
 * from its source, and a stale page for a deleted feature stayed published for
 * two commits after the source was removed. Reading the source is the only
 * arrangement where none of that is possible.
 *
 * `docsLoader()` cannot be reused: it hardcodes its base to `src/content/docs`
 * and exposes only `generateId`. This is the same underlying `glob()` loader
 * with a base that reaches the kiln.
 *
 * The base is the repo root so one glob can cover both the kiln and the site's
 * own MDX pages. Chaining two loaders is not an option — glob deletes every
 * entry it did not touch on each run, so the second would evict the first.
 */

/**
 * `docs/Help/Concepts/Block References.md` -> `help/concepts/block-references`
 * `docs-site/src/content/docs/overview.mdx` -> `overview`
 *
 * Must match the old converter's slugs exactly: these become the URLs, and the
 * sidebar in astro.config.mjs addresses pages by them.
 */
export function generateId({ entry }) {
	const path = entry
		.replace(/^docs-site\/src\/content\/docs\//, '')
		.replace(/^docs\//, '')
		.replace(/\.mdx?$/, '');

	return path
		.split('/')
		.map(slugifySegment)
		.filter(Boolean)
		.join('/')
		.replace(/(^|\/)index$/, '');
}

export function kilnDocsLoader() {
	return {
		name: 'crucible-kiln-loader',
		load: (context) =>
			glob({
				base: new URL('../../../', import.meta.url),
				// Only Help and Guides are user documentation. Meta/ holds
				// architecture notes and contributor material and is deliberately
				// not published.
				pattern: [
					'docs/{Help,Guides}/**/[^_]*.md',
					'docs-site/src/content/docs/*.{md,mdx}',
				],
				generateId,
			}).load(context),
	};
}
