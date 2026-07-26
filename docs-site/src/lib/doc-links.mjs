import path from 'node:path';

/**
 * Resolve a link as it was authored — file-relative — into a page slug.
 *
 * Shared by the rehype plugin (which rewrites hrefs at build) and the doc graph
 * (which needs the same edges the reader will actually be able to follow). If
 * these two ever disagree, the graph starts drawing edges for links that 404.
 *
 * @param sourceDir directory of the source file, relative to src/content/docs
 * @param href      the raw href as written in the markdown
 * @returns the target slug ('help/concepts/kilns'), or null if not resolvable
 */
export function resolveDocLink(sourceDir, href) {
	if (typeof href !== 'string') return null;
	if (!href.startsWith('./') && !href.startsWith('../')) return null;

	const target = href.split('#')[0];
	if (!target) return null;

	let slug = path.posix.normalize(path.posix.join(sourceDir, target));
	slug = slug.replace(/\.mdx?$/, '').replace(/\/+$/, '');
	slug = slug.replace(/(^|\/)index$/, ''); // help/cli/index -> help/cli
	slug = slug.replace(/^\.\/?/, '').replace(/^\/+/, '');

	// Climbs above the content root — a content bug. Return null so callers
	// leave it alone rather than rewriting it into something that resolves
	// but points somewhere the author never meant.
	if (slug.startsWith('..')) return null;

	return slug;
}

/** Split an href into its path and its trailing '#anchor' (if any). */
export function splitHash(href) {
	const at = href.indexOf('#');
	return at === -1 ? [href, ''] : [href.slice(0, at), href.slice(at)];
}
