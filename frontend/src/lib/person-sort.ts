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

/**
 * Russian collation, so the order is the familiar alphabet (`Ё` belongs after
 * `Е`, not at the end of the code-point range where `U+0451` would put it).
 * Only the folded keys of the two letter groups are collated this way.
 */
const collator = new Intl.Collator('ru');

/** Code-point comparison: deterministic and collation-free. */
function byCodePoint(a: string, b: string): number {
  if (a === b) return 0;
  return a < b ? -1 : 1;
}

export function comparePersonNames(a: string, b: string): number {
  const group = groupOf(a);
  const otherGroup = groupOf(b);
  if (group !== otherGroup) return group - otherGroup;

  // Digits and other alphabets have no familiar order to preserve, so their
  // code points keep the result deterministic without collation data.
  const compare = group === 2 ? byCodePoint : collator.compare.bind(collator);

  const folded = compare(fold(a), fold(b));
  if (folded !== 0) return folded;

  // The original spelling decides between names that differ only by case, so
  // the two never swap places between renders.
  return byCodePoint(a, b);
}
