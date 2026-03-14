#!/usr/bin/env node
/**
 * convert-docs.mjs
 *
 * Converts docs/Help/ and docs/Guides/ to Starlight-compatible markdown.
 * Creates COPIES in docs-site/src/content/docs/ — never modifies originals.
 * Transforms [[wikilinks]] to standard markdown links.
 *
 * Usage: node scripts/convert-docs.mjs
 */

import { readFileSync, writeFileSync, mkdirSync, readdirSync, existsSync, rmSync } from 'node:fs';
import { join, relative, dirname, resolve } from 'node:path';

const ROOT = resolve(import.meta.dirname, '..');
const DOCS_ROOT = join(ROOT, 'docs');
const SITE_DOCS = join(ROOT, 'docs-site', 'src', 'content', 'docs');
const CONVERT_DIRS = ['Help', 'Guides'];

// ─── Slugification ───────────────────────────────────────────

function slugify(name) {
  return name
    .toLowerCase()
    .replace(/&/g, '-and-')
    .replace(/\s+/g, '-')
    .replace(/[^a-z0-9\-]/g, '')
    .replace(/-+/g, '-')
    .replace(/^-|-$/g, '');
}

function slugifyPath(relPath) {
  return relPath
    .split('/')
    .map((part, i, arr) => {
      if (i === arr.length - 1 && part.endsWith('.md')) {
        return slugify(part.slice(0, -3)) + '.md';
      }
      return slugify(part);
    })
    .join('/');
}

// ─── File Map ────────────────────────────────────────────────

function buildFileMap() {
  const map = new Map();

  function walk(dir) {
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const fullPath = join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(fullPath);
      } else if (entry.name.endsWith('.md')) {
        const relPath = relative(DOCS_ROOT, fullPath);
        const slugPath = slugifyPath(relPath);
        const nameNoExt = entry.name.slice(0, -3);
        const relNoExt = relPath.slice(0, -3);

        const info = { srcPath: fullPath, slugPath, relPath };

        // Full relative path key: "Help/Concepts/Kilns"
        map.set(relNoExt, info);

        // Short name key: "Kilns" (first match wins)
        if (!map.has(nameNoExt)) {
          map.set(nameNoExt, info);
        }
      }
    }
  }

  for (const dir of CONVERT_DIRS) {
    const p = join(DOCS_ROOT, dir);
    if (existsSync(p)) walk(p);
  }

  return map;
}

// ─── Wikilink Resolution ─────────────────────────────────────

function resolveWikilink(target, fileMap) {
  // 1. Exact path match: "Help/Concepts/Kilns"
  if (fileMap.has(target)) return fileMap.get(target);

  // 2. Try with directory prefixes
  for (const prefix of ['Help/', 'Guides/']) {
    if (fileMap.has(prefix + target)) return fileMap.get(prefix + target);
  }

  // 3. Short name (filename only): "Kilns"
  const shortName = target.split('/').pop();
  if (fileMap.has(shortName)) return fileMap.get(shortName);

  return null;
}

// ─── Wikilink Conversion ─────────────────────────────────────

function convertWikilinksInLine(line, currentSlugPath, fileMap) {
  // Protect inline code spans by replacing with placeholders
  const codeSpans = [];
  let processed = line.replace(/(`+)([\s\S]*?)\1/g, (match) => {
    codeSpans.push(match);
    return `\x00CODE${codeSpans.length - 1}\x00`;
  });

  // Convert wikilinks: !?[[target|alias]]
  processed = processed.replace(/(!?)\[\[([^\]]+)\]\]/g, (_match, _embed, inner) => {
    let [targetRaw, alias] = inner.split('|').map((s) => s.trim());
    let heading = '';

    // Split heading/block reference
    if (targetRaw.includes('#')) {
      const idx = targetRaw.indexOf('#');
      heading = targetRaw.slice(idx + 1);
      targetRaw = targetRaw.slice(0, idx);
    }

    const info = resolveWikilink(targetRaw, fileMap);

    if (!info) {
      // Target not in converted docs — render as plain text
      return alias || targetRaw.split('/').pop();
    }

    // Compute relative path from current file to target
    const currentDir = dirname(currentSlugPath);
    const targetNoExt = info.slugPath.slice(0, -3);
    let rel = relative(currentDir, targetNoExt);
    if (!rel.startsWith('.')) rel = './' + rel;

    // Append heading anchor or trailing slash
    if (heading) {
      const anchor = heading
        .replace(/^\^/, '') // strip block-ref prefix
        .toLowerCase()
        .replace(/\s+/g, '-')
        .replace(/[^a-z0-9\-]/g, '');
      rel += '/#' + anchor;
    } else {
      rel += '/';
    }

    const displayText = alias || targetRaw.split('/').pop();
    return `[${displayText}](${rel})`;
  });

  // Restore inline code spans
  processed = processed.replace(/\x00CODE(\d+)\x00/g, (_, idx) => codeSpans[parseInt(idx)]);

  return processed;
}

function convertWikilinks(content, currentSlugPath, fileMap) {
  const lines = content.split('\n');
  let inCodeBlock = false;

  return lines
    .map((line) => {
      const trimmed = line.trimStart();
      if (/^(`{3,}|~{3,})/.test(trimmed)) {
        inCodeBlock = !inCodeBlock;
        return line;
      }
      if (inCodeBlock) return line;
      return convertWikilinksInLine(line, currentSlugPath, fileMap);
    })
    .join('\n');
}

// ─── Frontmatter Conversion ─────────────────────────────────

function convertFile(content) {
  const fmMatch = content.match(/^---\n([\s\S]*?)\n---\n/);
  let fm = '';
  let body = content;

  if (fmMatch) {
    fm = fmMatch[1];
    body = content.slice(fmMatch[0].length);
  }

  // Extract title from first H1
  const h1Match = body.match(/^#\s+(.+)$/m);
  const title = h1Match ? h1Match[1].trim() : 'Untitled';

  // Parse frontmatter fields we want to keep
  const descMatch = fm.match(/^description:\s*(.+)$/m);
  const orderMatch = fm.match(/^order:\s*(\d+)$/m);

  // Build Starlight-compatible frontmatter
  const newFmLines = [`title: "${title.replace(/"/g, '\\"')}"`];

  if (descMatch) {
    let desc = descMatch[1].trim();
    // Ensure proper quoting
    if (!desc.startsWith('"') && !desc.startsWith("'")) {
      desc = `"${desc.replace(/"/g, '\\"')}"`;
    }
    newFmLines.push(`description: ${desc}`);
  }

  if (orderMatch) {
    newFmLines.push(`sidebar:\n  order: ${orderMatch[1]}`);
  }

  // Remove H1 from body (Starlight auto-renders title as H1)
  let newBody = body.replace(/^\s*#\s+.+\n\n?/, '');

  return `---\n${newFmLines.join('\n')}\n---\n\n${newBody.trimStart()}`;
}

// ─── Main ────────────────────────────────────────────────────

console.log('🔥 Crucible docs converter\n');
console.log('Building file map...');
const fileMap = buildFileMap();

// Count unique files (full-path entries only)
const uniqueFiles = new Set();
for (const [key] of fileMap) {
  if (key.includes('/')) uniqueFiles.add(key);
}
console.log(`Found ${uniqueFiles.size} files to convert\n`);

// Clean existing converted content
for (const dir of CONVERT_DIRS) {
  const outDir = join(SITE_DOCS, slugify(dir));
  if (existsSync(outDir)) {
    rmSync(outDir, { recursive: true });
    console.log(`Cleaned ${outDir}`);
  }
}

let converted = 0;
let totalWikilinksBefore = 0;
let totalWikilinksAfter = 0;
const processedPaths = new Set();

for (const [key, info] of fileMap) {
  // Only process full-path entries (skip short-name aliases)
  if (!key.includes('/')) continue;
  if (processedPaths.has(info.srcPath)) continue;
  processedPaths.add(info.srcPath);

  let content = readFileSync(info.srcPath, 'utf-8');

  // Count wikilinks before conversion
  const beforeCount = (content.match(/\[\[[^\]]+\]\]/g) || []).length;
  totalWikilinksBefore += beforeCount;

  // Convert frontmatter
  content = convertFile(content);

  // Convert wikilinks
  content = convertWikilinks(content, info.slugPath, fileMap);

  // Count wikilinks after conversion
  const afterCount = (content.match(/\[\[[^\]]+\]\]/g) || []).length;
  totalWikilinksAfter += afterCount;

  // Write output file
  const destPath = join(SITE_DOCS, info.slugPath);
  mkdirSync(dirname(destPath), { recursive: true });
  writeFileSync(destPath, content);

  converted++;
  const status = afterCount > 0 ? ` (${afterCount} in code blocks)` : '';
  console.log(`  ${info.relPath} → ${info.slugPath}${status}`);
}

console.log(`\n✓ Converted ${converted} files`);
console.log(`  Wikilinks found:     ${totalWikilinksBefore}`);
console.log(`  Wikilinks remaining: ${totalWikilinksAfter} (expected: only inside code blocks)`);

if (totalWikilinksAfter > 0) {
  console.log('\nRemaining wikilinks are inside code blocks (intentional examples).');
}
