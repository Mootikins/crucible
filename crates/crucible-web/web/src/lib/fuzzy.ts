// Scored fuzzy matching for the omnibox. Deliberately small: subsequence
// acceptance with bonuses for the things users actually perceive as "better"
// — substring runs, word-start hits, and earlier positions. Not a general
// fzf clone; deterministic so section ordering stays stable.

const SUBSTRING_BASE = 1000;
const WORD_START_BONUS = 200;
const CONTIGUOUS_BONUS = 10;
const SUBSEQ_WORD_START_BONUS = 20;

const isWordStart = (s: string, i: number): boolean =>
  i === 0 || !/[a-z0-9]/i.test(s[i - 1]);

/**
 * Score `query` against `haystack`. Higher is better; `null` means no match.
 * Empty queries match with a neutral 0 so callers can skip filtering.
 */
export function fuzzyScore(haystack: string, query: string): number | null {
  if (!query) return 0;
  const h = haystack.toLowerCase();
  const q = query.toLowerCase();

  // Substring run: dominant score class, earlier + word-start preferred.
  const idx = h.indexOf(q);
  if (idx !== -1) {
    return SUBSTRING_BASE + (isWordStart(h, idx) ? WORD_START_BONUS : 0) - idx;
  }

  // Subsequence walk: every query char must appear in order.
  let score = 0;
  let hi = 0;
  let prevHit = -2;
  for (const ch of q) {
    const found = h.indexOf(ch, hi);
    if (found === -1) return null;
    if (found === prevHit + 1) score += CONTIGUOUS_BONUS;
    if (isWordStart(h, found)) score += SUBSEQ_WORD_START_BONUS;
    score -= Math.floor(found / 10); // gentle penalty for late hits
    prevHit = found;
    hi = found + 1;
  }
  return score;
}
