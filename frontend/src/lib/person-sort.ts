/**
 * The single ordering rule for people's names across the product: the speaker
 * panel's name list and the transcript chip's dropdown both sort with it.
 *
 * Groups come first — Cyrillic (А-Я), then Latin (A-Z), then everything else
 * (digits, other alphabets and punctuation). The group is decided by the first
 * character, so a name is listed where the eye looks for it. Inside a group the
 * comparison is case-insensitive so `анна` and `Анна` stand together; the
 * original spelling is the final tie-breaker so the result never depends on the
 * input order.
 *
 * The rule knows nothing about meetings or about "unnamed" speakers: that is
 * the chip's own extra rule, layered on top of this comparator.
 */

/** 0 for Cyrillic, 1 for Latin, 2 for anything else. */
function groupOf(name: string): 0 | 1 | 2 {
  const first = name.charAt(0);
  if (/[\u0400-\u04FF]/.test(first)) return 0;
  if (/[A-Za-z]/.test(first)) return 1;
  return 2;
}

/**
 * Case folding for the sort key. The fold happens here, in JS, and not in
 * SQLite: its `NOCASE` collation folds ASCII only, so Cyrillic names would
 * split by case.
 */
function fold(name: string): string {
  return name.toLowerCase();
}

/** Code-point comparison: Cyrillic А-Я and Latin A-Z are already contiguous. */
function byCodePoint(a: string, b: string): number {
  if (a === b) return 0;
  return a < b ? -1 : 1;
}

export function comparePersonNames(a: string, b: string): number {
  const group = groupOf(a) - groupOf(b);
  if (group !== 0) return group;

  const folded = byCodePoint(fold(a), fold(b));
  if (folded !== 0) return folded;

  // The original spelling decides between names that differ only by case, so
  // the two never swap places between renders.
  return byCodePoint(a, b);
}
