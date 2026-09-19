#!/usr/bin/env node
/**
 * Does every Starlight sidebar entry name a page the kiln actually provides?
 *
 * Starlight fails the whole site build on the FIRST stale slug, so one deleted
 * note costs a red Deploy Docs and names one line of the config — and the next
 * stale entry is only discovered on the following run. This walks both sides
 * (the `slug:` values in `astro.config.mjs`, and the content the loader really
 * collects) and names every stale entry at once.
 *
 * It reads the same slug rule the loader and the wikilink plugin use
 * (`slugifyRelPath`), so a rename that both sides follow cannot make this
 * disagree with the build it guards.
 *
 *   bun run check:sidebar
 *
 * Exit 0 when every entry resolves, 1 with the list when any does not.
 */
import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { slugifyRelPath } from '../src/lib/kiln-links.mjs';

const SITE = fileURLToPath(new URL('..', import.meta.url));
const REPO = fileURLToPath(new URL('../..', import.meta.url));
const KILN = path.join(REPO, 'docs');
const SITE_PAGES = path.join(SITE, 'src/content/docs');

/** Every file under `dir`, recursively. */
function walk(dir, out = []) {
	for (const entry of readdirSync(dir)) {
		const full = path.join(dir, entry);
		if (statSync(full).isDirectory()) walk(full, out);
		else out.push(full);
	}
	return out;
}

/** The slugs the content collection exposes, from the loader's own roots. */
function kilnSlugs() {
	const slugs = new Set();

	// `docs/{Help,Guides}/**/[^_]*.md` — Meta/ is deliberately unpublished, and
	// a leading `_` marks a draft, exactly as the loader's glob says.
	for (const dir of ['Help', 'Guides']) {
		for (const file of walk(path.join(KILN, dir))) {
			if (!file.endsWith('.md')) continue;
			if (path.basename(file).startsWith('_')) continue;
			slugs.add(slugifyRelPath(path.relative(KILN, file).replace(/\.md$/, '')));
		}
	}

	// `docs-site/src/content/docs/*.{md,mdx}` — the site's own pages.
	for (const file of walk(SITE_PAGES)) {
		if (!/\.mdx?$/.test(file)) continue;
		slugs.add(slugifyRelPath(path.relative(SITE_PAGES, file).replace(/\.mdx?$/, '')));
	}

	return slugs;
}

const sidebar = [...readFileSync(path.join(SITE, 'astro.config.mjs'), 'utf8').matchAll(/slug:\s*'([^']+)'/g)].map(
	(m) => m[1],
);
const slugs = kilnSlugs();
const stale = sidebar.filter((slug) => !slugs.has(slug));

if (stale.length) {
	console.error(
		`${stale.length} sidebar slug(s) name no page:\n  - ${stale.join('\n  - ')}\n\n` +
			`Either the note was deleted or its slug changed; fix docs-site/astro.config.mjs.`,
	);
	process.exit(1);
}

console.log(`${sidebar.length} sidebar entries resolve against ${slugs.size} pages.`);
