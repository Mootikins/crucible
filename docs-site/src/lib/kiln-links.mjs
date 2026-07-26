import { readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/**
 * Resolving `[[Wikilinks]]` to page slugs.
 *
 * Shared by the remark plugin that rewrites links for the reader and the doc
 * graph that draws them. If those two ever disagree the graph starts drawing
 * edges for links nobody can follow, so they resolve through one function.
 */

const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
const KILN = path.join(ROOT, 'docs');
const DIRS = ['Help', 'Guides'];

/** Kiln filenames are Title Case with spaces; URLs are not. */
export function slugifySegment(name) {
	return name
		.toLowerCase()
		.replace(/&/g, '-and-')
		.replace(/\s+/g, '-')
		.replace(/[^a-z0-9\-]/g, '')
		.replace(/-+/g, '-')
		.replace(/^-|-$/g, '');
}

export function slugifyRelPath(relNoExt) {
	return relNoExt
		.split(/[\\/]/)
		.map(slugifySegment)
		.filter(Boolean)
		.join('/')
		.replace(/(^|\/)index$/, '');
}

let fileMap = null;

/** target-as-written -> slug. Built once; the kiln does not change mid-build. */
function buildFileMap() {
	const map = new Map();

	const walk = (dir) => {
		for (const name of readdirSync(dir)) {
			const full = path.join(dir, name);
			if (statSync(full).isDirectory()) {
				walk(full);
				continue;
			}
			if (!name.endsWith('.md')) continue;

			const relNoExt = path.relative(KILN, full).replace(/\.md$/, '');
			const slug = slugifyRelPath(relNoExt);

			// Full path key: "Help/Concepts/Kilns"
			map.set(relNoExt.split(path.sep).join('/'), slug);
			// Bare filename: "Kilns". First match wins, matching the kiln's own
			// resolution order.
			const bare = path.basename(relNoExt);
			if (!map.has(bare)) map.set(bare, slug);
		}
	};

	for (const d of DIRS) {
		const p = path.join(KILN, d);
		try {
			if (statSync(p).isDirectory()) walk(p);
		} catch {
			/* directory absent — nothing to map */
		}
	}
	return map;
}

/** @returns the target's slug, or null when it resolves to nothing. */
export function resolveWikilink(target) {
	fileMap ??= buildFileMap();
	if (fileMap.has(target)) return fileMap.get(target);
	for (const prefix of ['Help/', 'Guides/']) {
		if (fileMap.has(prefix + target)) return fileMap.get(prefix + target);
	}
	const bare = target.split('/').pop();
	if (fileMap.has(bare)) return fileMap.get(bare);
	return null;
}

/** Matches `[[Target]]`, `[[Target|Alias]]`, and the `![[embed]]` form. */
export const WIKILINK_RE = /!?\[\[([^\]]+)\]\]/g;

/** Splits `Target#Heading` into its parts. */
export function splitTarget(inner) {
	let [target, alias] = inner.split('|').map((s) => s.trim());
	let heading = '';
	if (target.includes('#')) {
		const at = target.indexOf('#');
		heading = target.slice(at + 1);
		target = target.slice(0, at);
	}
	return { target, alias, heading };
}
