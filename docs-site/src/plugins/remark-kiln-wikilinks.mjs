import { visit } from 'unist-util-visit';
import { resolveWikilink, splitTarget, WIKILINK_RE } from '../lib/kiln-links.mjs';

/**
 * Rewrites `[[Wikilinks]]` into site links.
 *
 * Replaces the wikilink half of the old convert-docs.mjs. Two things are better
 * here than in the copy-and-rewrite script it replaces:
 *
 * 1. It emits ROOT-ABSOLUTE urls (`/crucible/help/concepts/kilns/`). The old
 *    script emitted file-relative ones (`./kilns/`), which the browser resolved
 *    against the page's directory URL and landed one level too deep — 240 of 257
 *    in-content links 404'd. That also makes rehype-resolve-doc-links redundant.
 *
 * 2. It works on the mdast, so fenced and inline code are skipped structurally.
 *    The old script scanned lines and tracked fence state by hand, which is why
 *    91 wikilinks inside code blocks had to be excused in its output.
 */
export function remarkKilnWikilinks({ base = '/crucible' } = {}) {
	const basePrefix = base.endsWith('/') ? base.slice(0, -1) : base;

	return (tree) => {
		visit(tree, 'text', (node, index, parent) => {
			if (!parent || index === null) return;
			// A wikilink inside a markdown link label would splice a link into a
			// link, which serialises as nested <a> and is invalid HTML.
			if (parent.type === 'link' || parent.type === 'linkReference') return;
			WIKILINK_RE.lastIndex = 0;
			if (!WIKILINK_RE.test(node.value)) return;
			WIKILINK_RE.lastIndex = 0;

			const out = [];
			let last = 0;
			let match;

			while ((match = WIKILINK_RE.exec(node.value)) !== null) {
				if (match.index > last) {
					out.push({ type: 'text', value: node.value.slice(last, match.index) });
				}
				last = match.index + match[0].length;

				const { target, alias, heading } = splitTarget(match[1]);
				const slug = resolveWikilink(target);
				const label = alias || target.split('/').pop();

				if (!slug) {
					// Unresolvable target: render the label as plain text rather
					// than a link to nowhere. An unlinked phrase is a much
					// cheaper failure than a 404.
					out.push({ type: 'text', value: label });
					continue;
				}

				const anchor = heading
					? '#' +
						heading
							.replace(/^\^/, '')
							.toLowerCase()
							.replace(/\s+/g, '-')
							.replace(/[^a-z0-9\-]/g, '')
					: '';

				out.push({
					type: 'link',
					url: `${basePrefix}/${slug}/${anchor}`,
					children: [{ type: 'text', value: label }],
				});
			}

			if (last < node.value.length) {
				out.push({ type: 'text', value: node.value.slice(last) });
			}

			parent.children.splice(index, 1, ...out);
			return index + out.length;
		});
	};
}

export default remarkKilnWikilinks;
