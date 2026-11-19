/**
 * The single ordering rule for people's names across the product: the speaker
 * panel's name list and the transcript chip's dropdown both sort with it.
 * It also owns the single case fold for names, so "same person, different
 * case" means the same thing in the panel, the picker and the sort key.
 *
 * Groups come first — Cyrillic (А-Я), then Latin (A-Z), then everything else
 * (digits, other alphabets and punctuation). The group is decided by the first
 * character, so a name is listed where the eye looks for it. Inside a group the
 * comparison is case-insensitive so `анна` and `Анна` stand together; the
 * original spelling is the final tie-breaker so the result never depends on the
 * input order.
 *
 * Inside a letter group the order is the familiar alphabet, spelled out below
 * as a static string. It is deliberately not `Intl.Collator`: that reads the
 * host's ICU data, which differs between WebView2, WebKitGTK and WKWebView, so
 * the same directory could order differently per OS. The alphabet here is the
 * same on every host and needs no locale data at all.
 *
 * The rule knows nothing about meetings or about "unnamed" speakers: that is
 * the chip's own extra rule (`orderMeetingSpeakersForChip` in
 * `transcript-format.ts`), layered on top of this comparator.
 */

/**
 * The Russian alphabet in the order it is recited: `ё` right after `е` (not at
 * the end of the code-point range where `U+0451` sits) and `й` after `и`.
 * The rank of a character is its position here.
 */
const CYRILLIC_ALPHABET = 'абвгдеёжзийклмнопрстуфхцчшщъыьэюя';

/** Latin A-Z in order; plain code points already give this, but a rank keeps both groups symmetric. */
const LATIN_ALPHABET = 'abcdefghijklmnopqrstuvwxyz';

/**
 * Character ranks per group, built once. A character missing from the group's
 * alphabet has no rank and falls back to its code point in the comparison.
 */
function rankMap(alphabet: string): Map<string, number> {
  const ranks = new Map<string, number>();
  for (const [index, letter] of [...alphabet].entries()) {
    ranks.set(letter, index);
  }
  return ranks;
}

const CYRILLIC_RANKS = rankMap(CYRILLIC_ALPHABET);
const LATIN_RANKS = rankMap(LATIN_ALPHABET);

/** 0 for Cyrillic, 1 for Latin, 2 for anything else. */
function groupOf(name: string): 0 | 1 | 2 {
  const first = name.charAt(0);
  if (/[\u0400-\u04FF]/.test(first)) return 0;
  if (/[A-Za-z]/.test(first)) return 1;
  return 2;
}

/**
 * Case folding for every name comparison in the product: the sort key, the
 * "name is taken" check and the picker's filter all fold through here.
 *
 * The fold happens in JS, and not in SQLite: its `NOCASE` collation folds
 * ASCII only, so Cyrillic names would split by case. It is `toLowerCase()`,
 * never `toLocaleLowerCase()`: a name is not a locale-specific string, and the
 * directory has to compare the same way whatever the machine's locale is.
 */
export function foldPersonName(name: string): string {
  return name.toLowerCase();
}

/**
 * The identity key of a person's name: trimmed, case-folded, and `null` when
 * the name is blank. Two names share a key exactly when the product treats
 * them as the same person. Nothing else — no accent folding, no collapsing of
 * inner spaces — so a stored spelling is never silently rewritten.
 */
export function personNameKey(name: string | null | undefined): string | null {
  const folded = name?.trim();
  if (!folded) return null;
  return foldPersonName(folded);
}

/** Code-point comparison: deterministic and collation-free. */
function byCodePoint(a: string, b: string): number {
  if (a === b) return 0;
  return a < b ? -1 : 1;
}

/**
 * Compares two folded strings character by character. A known character beats
 * an unknown one (letters come before digits and punctuation); two unknown
 * characters fall back to their code points, so a character outside the group's
 * alphabet never breaks the result.
 */
function compareAlphabetically(
  a: string,
  b: string,
  ranks: ReadonlyMap<string, number>,
): number {
  const length = Math.min(a.length, b.length);
  for (let index = 0; index < length; index += 1) {
    const left = a.charAt(index);
    const right = b.charAt(index);
    if (left === right) continue;

    const leftRank = ranks.get(left);
    const rightRank = ranks.get(right);
    if (leftRank !== undefined && rightRank !== undefined) return leftRank - rightRank;
    if (leftRank !== undefined) return -1;
    if (rightRank !== undefined) return 1;
    return byCodePoint(left, right);
  }

  // A prefix sorts before what extends it, like any dictionary.
  return a.length - b.length;
}

export function comparePersonNames(a: string, b: string): number {
  const group = groupOf(a);
  const otherGroup = groupOf(b);
  if (group !== otherGroup) return group - otherGroup;

  // Digits and other alphabets have no familiar order to preserve, so their
  // code points keep the result deterministic.
  const folded =
    group === 2
      ? byCodePoint(foldPersonName(a), foldPersonName(b))
      : compareAlphabetically(
          foldPersonName(a),
          foldPersonName(b),
          group === 0 ? CYRILLIC_RANKS : LATIN_RANKS,
        );
  if (folded !== 0) return folded;

  // The original spelling decides between names that differ only by case, so
  // the two never swap places between renders.
  return byCodePoint(a, b);
}
