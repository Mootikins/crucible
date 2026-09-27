// Evaluate each Bases JavaScript-semantics case in Node and record the result.
// Obsidian 1.14.2 captures stay the reference where they exist; these cases
// cover the JavaScript behavior that the captures do not reach.
import { readFile, writeFile } from 'node:fs/promises';
const root = new URL('../assets/fixtures/bases/', import.meta.url);
const corpus = JSON.parse(await readFile(new URL('js-semantics.json', root), 'utf8'));
// JSON has no NaN or Infinity, so non-finite numbers keep their JavaScript text.
const plain = value => typeof value === 'number' && !Number.isFinite(value) ? String(value)
  : Array.isArray(value) ? value.map(plain) : value;
const cases = corpus.cases.map(item => item.js === undefined ? item
  : { ...item, expected: plain((0, eval)(item.js)) });
await writeFile(new URL('js-reference.json', root),
  JSON.stringify({ node: process.version, cases }, null, 2) + '\n');
console.log(`Recorded ${cases.length} JavaScript reference cases with Node ${process.version}`);
