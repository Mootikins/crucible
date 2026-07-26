import { getCollection } from 'astro:content';
import { resolveWikilink, splitTarget, WIKILINK_RE } from './kiln-links.mjs';

/**
 * The documentation's link graph, built from the prose itself.
 *
 * Edges come from the wikilinks authors actually wrote, resolved through the
 * same function the remark plugin uses — so an edge here is always a link a
 * reader can follow. Sidebar nesting is deliberately NOT an edge source: the
 * sidebar is a filing decision, the prose links are where the ideas connect,
 * and only the second is worth drawing.
 *
 * Note this reads `entry.body`, which is the RAW kiln source: wikilinks, not
 * the markdown links the remark plugin emits downstream. Parsing for `[](...)`
 * here silently produced an empty graph and a missing widget on every page.
 *
 * Consumed by the hero constellation and the per-page neighbourhood widget.
 */

/** Fenced and inline code — links inside are examples, not references. */
const FENCE_RE = /```[\s\S]*?```|~~~[\s\S]*?~~~|`[^`\n]*`/g;

let cached = null;

export async function getDocGraph() {
	if (cached) return cached;

	const entries = await getCollection('docs');

	const nodes = new Map();
	for (const entry of entries) {
		const slug = entry.id;
		nodes.set(slug, {
			slug,
			title: entry.data.title ?? slug,
			// Top-level directory. Used to colour and cluster the constellation
			// and to label the widget; root pages get ''.
			section: slug.includes('/') ? slug.split('/')[0] : '',
			outbound: [],
			inbound: [],
		});
	}

	for (const entry of entries) {
		const body = (entry.body ?? '').replace(FENCE_RE, '');
		const seen = new Set();

		WIKILINK_RE.lastIndex = 0;
		let match;
		while ((match = WIKILINK_RE.exec(body)) !== null) {
			const { target } = splitTarget(match[1]);
			const to = resolveWikilink(target);
			if (!to || to === entry.id) continue;
			if (!nodes.has(to)) continue; // resolves outside the published set
			if (seen.has(to)) continue; // one edge per pair, not per mention
			seen.add(to);

			nodes.get(entry.id).outbound.push(to);
			nodes.get(to).inbound.push(entry.id);
		}
	}

	const list = [...nodes.values()];
	cached = {
		nodes: list,
		bySlug: nodes,
		edges: list.flatMap((n) => n.outbound.map((to) => ({ from: n.slug, to }))),
	};
	return cached;
}

/**
 * The pages one hop from `slug`, split by direction.
 *
 * `related` is capped: a hub like the plugin guide has dozens of inbound links
 * and a widget showing all of them is a wall, not a map. Highest-degree
 * neighbours win, since those are the pages most likely to be worth the next
 * click.
 */
export async function getNeighbourhood(slug, limit = 8) {
	const { bySlug } = await getDocGraph();
	const node = bySlug.get(slug);
	if (!node) return null;

	const degree = (s) => {
		const n = bySlug.get(s);
		return n ? n.outbound.length + n.inbound.length : 0;
	};
	const decorate = (slugs) =>
		[...new Set(slugs)]
			.map((s) => bySlug.get(s))
			.filter(Boolean)
			.sort((a, b) => degree(b.slug) - degree(a.slug));

	const outbound = decorate(node.outbound);
	const inbound = decorate(node.inbound.filter((s) => !node.outbound.includes(s)));

	return {
		node,
		outbound: outbound.slice(0, limit),
		inbound: inbound.slice(0, limit),
		totalOutbound: outbound.length,
		totalInbound: inbound.length,
	};
}
