#!/usr/bin/env node
/**
 * Static internal-link check over the built site.
 *
 * Exists because 240 of 257 in-content cross-references were silently 404ing:
 * they resolved, they just resolved one directory too deep, so nothing failed
 * loudly. A build that succeeds is not evidence that its links work.
 *
 * Runs against dist/ with no server, so it works the same locally and in CI.
 *
 *   bun run check:links
 */
import { readFileSync, existsSync, statSync } from 'node:fs';
import { readdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
const DIST = path.join(ROOT, 'dist');
const BASE = '/crucible';

if (!existsSync(DIST)) {
	console.error('dist/ not found — run `bun run build` first.');
	process.exit(2);
}

async function htmlFiles(dir) {
	const out = [];
	for (const entry of await readdir(dir, { withFileTypes: true })) {
		const full = path.join(dir, entry.name);
		if (entry.isDirectory()) out.push(...(await htmlFiles(full)));
		else if (entry.name.endsWith('.html')) out.push(full);
	}
	return out;
}

/** Does a site-absolute URL correspond to something in dist/? */
function resolves(urlPath) {
	// Anything outside the configured base is broken in production, whatever
	// happens to exist in dist/. Treating a base-less path as site-relative made
	// the checker pass links that 404 once deployed — the precise class of bug
	// it exists to catch.
	if (urlPath !== BASE && !urlPath.startsWith(`${BASE}/`)) return false;
	const rel = urlPath.slice(BASE.length);
	const clean = decodeURIComponent(rel).replace(/^\/+/, '').replace(/\/+$/, '');
	const candidates = [
		path.join(DIST, clean, 'index.html'),
		path.join(DIST, `${clean}.html`),
		path.join(DIST, clean),
	];
	return candidates.some((c) => existsSync(c) && statSync(c).isFile());
}

const pages = await htmlFiles(DIST);
const broken = [];
const missingBody = [];
let checked = 0;

for (const file of pages) {
	const html = readFileSync(file, 'utf8');

	// On Starlight pages, only the rendered doc body. Sidebar and header links
	// are Starlight's own output — if those break the problem is upstream, and
	// including them drowns the signal we care about in duplicates.
	let body = html.match(/<div class="sl-markdown-content">([\s\S]*)$/)?.[1] ?? '';

	if (!body) {
		// The landing page bypasses Starlight and has no content block, but its
		// links still need checking — there is no sidebar to drown them.
		const isStarlightPage = /data-has-sidebar|class="sl-/.test(html);
		if (isStarlightPage) {
			// Fail loudly. Skipping silently means a Starlight markup change
			// turns this into a checker that inspects nothing and still reports
			// success, which is worse than not having it at all.
			missingBody.push(path.relative(DIST, file));
			continue;
		}
		body = html.match(/<body[^>]*>([\s\S]*)<\/body>/)?.[1] ?? '';
		if (!body) {
			missingBody.push(path.relative(DIST, file));
			continue;
		}
	}

	// The page's own URL, needed to resolve any link still written relatively.
	const pageUrl = `${BASE}/${path
		.relative(DIST, file)
		.replace(/index\.html$/, '')
		.replace(/\.html$/, '/')}`;

	for (const m of body.matchAll(/href="([^"]+)"/g)) {
		const href = m[1];
		if (/^(https?:|mailto:|#|\/\/)/.test(href)) continue;

		const resolved = new URL(href, `http://x${pageUrl}`).pathname;
		checked++;
		if (!resolves(resolved)) {
			broken.push({ page: pageUrl, href, resolved });
		}
	}
}

if (missingBody.length) {
	console.error(
		`\n${missingBody.length} page(s) had no .sl-markdown-content block, so their links were never checked:\n`
	);
	for (const f of missingBody.slice(0, 10)) console.error(`  ${f}`);
	if (missingBody.length > 10) console.error(`  ... and ${missingBody.length - 10} more`);
	console.error('\nStarlight\'s content markup probably changed; update the selector.');
	process.exit(1);
}

if (broken.length) {
	console.error(`\n${broken.length} broken internal link(s) of ${checked} checked:\n`);
	for (const b of broken) {
		console.error(`  ${b.page}`);
		console.error(`    href="${b.href}"  ->  ${b.resolved}\n`);
	}
	process.exit(1);
}

console.log(`All ${checked} internal body links resolve across ${pages.length} pages.`);
