/**
 * Small fuzzy matcher for the command palette.
 *
 * Subsequence match, case-insensitive, with bonuses for word starts and
 * contiguous runs. Deterministic and cheap enough to run over a few thousand
 * nodes on every keystroke.
 */

export interface FuzzyMatch {
  score: number;
  /** Indices in the haystack that matched, for highlighting. */
  indices: number[];
}

const BONUS_WORD_START = 8;
const BONUS_CAMEL = 6;
const BONUS_CONSECUTIVE = 5;
const PENALTY_GAP = 1;
const BONUS_EXACT_PREFIX = 12;

function isBoundary(text: string, index: number): boolean {
  if (index === 0) return true;
  const previous = text[index - 1] ?? "";
  const current = text[index] ?? "";
  if (/[\s\-_/.]/.test(previous)) return true;
  // camelCase / PascalCase boundaries: "LayerTree" matches "lt".
  if (/[a-z0-9]/.test(previous) && /[A-Z]/.test(current)) return true;
  return false;
}

export function fuzzyMatch(query: string, text: string): FuzzyMatch | null {
  const q = query.trim();
  if (q === "") return { score: 0, indices: [] };

  const lowerQuery = q.toLowerCase();
  const lowerText = text.toLowerCase();
  if (lowerQuery.length > lowerText.length) return null;

  // Fast path: exact substring is always a strong match.
  const exact = lowerText.indexOf(lowerQuery);
  const indices: number[] = [];
  let score = 0;
  let searchFrom = 0;
  let previousIndex = -1;

  for (const char of lowerQuery) {
    const found = lowerText.indexOf(char, searchFrom);
    if (found === -1) return null;
    indices.push(found);
    if (previousIndex !== -1 && found === previousIndex + 1) score += BONUS_CONSECUTIVE;
    else if (previousIndex !== -1) score -= Math.min(10, (found - previousIndex - 1) * PENALTY_GAP);
    if (isBoundary(text, found)) score += BONUS_WORD_START;
    else if (/[A-Z]/.test(text[found] ?? "")) score += BONUS_CAMEL;
    previousIndex = found;
    searchFrom = found + 1;
  }

  if (exact === 0) score += BONUS_EXACT_PREFIX;
  else if (exact > 0) score += 4;
  score -= Math.max(0, text.length - lowerQuery.length) * 0.02;

  return { score, indices };
}

export function fuzzyScore(query: string, text: string): number | null {
  return fuzzyMatch(query, text)?.score ?? null;
}

/** Filter + stably sort items by fuzzy score; empty query keeps input order. */
export function fuzzySort<T>(items: readonly T[], query: string, getText: (item: T) => string): T[] {
  if (query.trim() === "") return [...items];
  const scored: { item: T; score: number; index: number }[] = [];
  items.forEach((item, index) => {
    const match = fuzzyMatch(query, getText(item));
    if (match) scored.push({ item, score: match.score, index });
  });
  scored.sort((a, b) => (b.score === a.score ? a.index - b.index : b.score - a.score));
  return scored.map((entry) => entry.item);
}

/** Split text into highlighted / plain runs for palette result labels. */
export function highlightRuns(
  text: string,
  indices: readonly number[],
): { text: string; match: boolean }[] {
  if (indices.length === 0) return [{ text, match: false }];
  const set = new Set(indices);
  const runs: { text: string; match: boolean }[] = [];
  let current = { text: "", match: set.has(0) };
  for (let i = 0; i < text.length; i += 1) {
    const matches = set.has(i);
    if (matches !== current.match) {
      if (current.text) runs.push(current);
      current = { text: text[i] ?? "", match: matches };
    } else {
      current.text += text[i] ?? "";
    }
  }
  if (current.text) runs.push(current);
  return runs;
}
