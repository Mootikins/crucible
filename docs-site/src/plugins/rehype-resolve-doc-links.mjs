import path from 'node:path';
import { visit } from 'unist-util-visit';

/**
 * Rewrites file-relative links in doc bodies into root-absolute site URLs.
 *
 * Content is authored the way the `docs/` kiln is authored: `./semantic-search/`
 * means "the sibling note", resolved against the *file*. Astro serves that file
 * at a directory URL with a trailing slash, so a browser resolves the same href
 * against the *page directory* and lands one level too deep:
 *
 *     page   /crucible/help/concepts/precognition/
 *     href   ./the-knowledge-graph/
 *     browser /crucible/help/concepts/precognition/the-knowledge-graph/   404
 *     wanted  /crucible/help/concepts/the-knowledge-graph/
 *
 * This silently broke 240 of 257 in-content cross-references. Rewriting the
 * content to absolute URLs would fix it too, but would hardcode the base path
 * into 85 files and drift from the kiln's authoring style — so resolve at build
 * instead and leave the prose portable.
 *
 * Only `./` and `../` hrefs are touched. Absolute, external, anchor-only,
 * and protocol-relative hrefs are left exactly as written.
 */
export function rehypeResolveDocLinks({ contentDir, base }) {
	const basePrefix = base.endsWith('/') ? base.slice(0, -1) : base;

	return function transformer(tree, file) {
		// The source file's directory, relative to src/content/docs — this is
		// the frame the author actually wrote against.
		const sourceDir = path.dirname(path.relative(contentDir, file.path));

		visit(tree, 'element', (node) => {
			if (node.tagName !== 'a') return;
			const href = node.properties?.href;
			if (typeof href !== 'string') return;
			if (!href.startsWith('./') && !href.startsWith('../')) return;

			const hashAt = href.indexOf('#');
			const hash = hashAt === -1 ? '' : href.slice(hashAt);
			let target = hashAt === -1 ? href : href.slice(0, hashAt);
			if (!target) return; // bare "#anchor" after a ./ — nothing to resolve

			// Resolve as a file path, then normalise to a page slug.
			let slug = path.posix.normalize(path.posix.join(sourceDir, target));
			slug = slug.replace(/\.mdx?$/, '').replace(/\/+$/, '');
			slug = slug.replace(/(^|\/)index$/, ''); // help/cli/index -> help/cli
			slug = slug.replace(/^\.\/?/, '').replace(/^\/+/, '');

			// A link that climbs above the content root is a content bug; leave
			// it untouched so it surfaces in the link check rather than being
			// silently rewritten into something that resolves but is wrong.
			if (slug.startsWith('..')) return;

			node.properties.href = slug ? `${basePrefix}/${slug}/${hash}` : `${basePrefix}/${hash}`;
		});
	};
}

export default rehypeResolveDocLinks;
