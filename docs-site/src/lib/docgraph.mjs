import { getCollection } from 'astro:content';
import { resolveDocLink } from './doc-links.mjs';

/**
 * The documentation's link graph, built from the prose itself.
 *
 * Edges come from cross-references authors actually wrote, resolved with the
 * same function the rehype plugin uses — so an edge here is always a link a
 * reader can follow. Sidebar nesting is deliberately NOT an edge source: the
 * sidebar is a filing decision, the prose links are where the ideas connect,
 * and only the second is worth drawing.
 *
 * Consumed by the hero constellation and the per-page neighbourhood widget.
 */

/** Matches markdown links, ignoring image embeds. */
const LINK_RE = /(?<!!)\[[^\]]*\]\(([^)\s]+)\)/g;
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
			// and to label the widget; 'overview' and other root pages get ''.
			section: slug.includes('/') ? slug.split('/')[0] : '',
			group: slug.split('/').slice(0, 2).join('/'),
			outbound: [],
			inbound: [],
		});
	}

	for (const entry of entries) {
		const sourceDir = entry.id.includes('/')
			? entry.id.split('/').slice(0, -1).join('/')
			: '.';

		const body = (entry.body ?? '').replace(FENCE_RE, '');
		const seen = new Set();

		for (const match of body.matchAll(LINK_RE)) {
			const target = resolveDocLink(sourceDir, match[1]);
			if (!target || target === entry.id) continue;
			if (!nodes.has(target)) continue; // points outside the docs
			if (seen.has(target)) continue; // one edge per pair, not per mention
			seen.add(target);

			nodes.get(entry.id).outbound.push(target);
			nodes.get(target).inbound.push(entry.id);
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
 * `related` is capped: a hub like the CLI index has dozens of inbound links and
 * a widget showing all of them is a wall, not a map. Highest-degree neighbours
 * win, since those are the pages most likely to be worth the next click.
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
	const mutual = decorate(node.outbound.filter((s) => node.inbound.includes(s)));

	return {
		node,
		outbound: outbound.slice(0, limit),
		inbound: inbound.slice(0, limit),
		mutual,
		totalOutbound: outbound.length,
		totalInbound: inbound.length,
	};
}
