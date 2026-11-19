import { describe, expect, test } from 'bun:test';
import { Glob } from 'bun';

import en from '../../src/i18n/locales/en/translation.json';
import ru from '../../src/i18n/locales/ru/translation.json';

/**
 * Usage-vs-catalog guard.
 *
 * The parity test compares `en` against `ru` and cannot see a key that code
 * calls but NEITHER catalog defines — i18next then silently renders the raw
 * key path (`speakers.title` on screen). This test scans the sources for
 * literal `t('a.b')` / `i18n.t('a.b')` key strings and requires every one of
 * them to exist in both catalogs.
 *
 * Keys built at runtime (template literals, variables, ternaries mixing
 * both) are exempt: the guard validates every string literal it can find in
 * the first argument — a ternary of two literals is checked on both halves.
 */

const PLURAL_SUFFIXES = ['_zero', '_one', '_two', '_few', '_many', '_other'] as const;

/**
 * Whether `key` exists in `catalog`: as a leaf value, or as the base of a
 * plural family (`allDone` matches `allDone_one/_few/_many/...`, the way
 * i18next resolves `t(key, { count })`).
 */
function catalogHas(catalog: unknown, key: string): boolean {
  const parts = key.split('.');
  const leaf = parts[parts.length - 1];
  let parent: unknown = catalog;
  for (const part of parts.slice(0, -1)) {
    if (parent === null || typeof parent !== 'object') return false;
    parent = (parent as Record<string, unknown>)[part];
  }
  if (parent === null || typeof parent !== 'object') return false;
  const entries = parent as Record<string, unknown>;
  const present = (value: unknown) =>
    typeof value === 'string'
    || (value !== null && typeof value === 'object' && Object.keys(value).length > 0);
  return present(entries[leaf])
    || PLURAL_SUFFIXES.some(suffix => present(entries[`${leaf}${suffix}`]));
}

/**
 * Blanks comment bodies (outside string literals) so commented-out `t()`
 * examples cannot produce false positives. Comments are replaced by spaces to
 * keep offsets and line counts stable.
 */
function stripComments(source: string): string {
  const out = source.split('');
  let state: 'code' | 'line' | 'block' | 'single' | 'double' | 'template' = 'code';
  for (let i = 0; i < out.length; i++) {
    const ch = out[i];
    const next = out[i + 1];
    switch (state) {
      case 'code':
        if (ch === '/' && next === '/') { state = 'line'; out[i] = ' '; out[i + 1] = ' '; i++; }
        else if (ch === '/' && next === '*') { state = 'block'; out[i] = ' '; out[i + 1] = ' '; i++; }
        else if (ch === "'") state = 'single';
        else if (ch === '"') state = 'double';
        else if (ch === '`') state = 'template';
        break;
      case 'line':
        if (ch === '\n') state = 'code'; else out[i] = ' ';
        break;
      case 'block':
        if (ch === '*' && next === '/') { state = 'code'; out[i] = ' '; out[i + 1] = ' '; i++; }
        else if (ch !== '\n') out[i] = ' ';
        break;
      case 'single':
        if (ch === '\\') i++; else if (ch === "'" || ch === '\n') state = 'code';
        break;
      case 'double':
        if (ch === '\\') i++; else if (ch === '"' || ch === '\n') state = 'code';
        break;
      case 'template':
        if (ch === '\\') i++; else if (ch === '`') state = 'code';
        break;
    }
  }
  return out.join('');
}

/** Closes a balanced run starting after `open`, skipping string contents. */
function matchBalanced(source: string, open: number, openChar: string, closeChar: string): number {
  let depth = 0;
  let inString: string | null = null;
  for (let i = open; i < source.length; i++) {
    const ch = source[i];
    if (inString) {
      if (ch === '\\') i++;
      else if (ch === inString) inString = null;
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') inString = ch;
    else if (ch === openChar) depth++;
    else if (ch === closeChar) {
      depth--;
      if (depth === 0) return i;
    }
  }
  return source.length;
}

/** The first argument of a call, with the argument list already balanced. */
function firstArgument(args: string): string {
  let depth = 0;
  let inString: string | null = null;
  for (let i = 0; i < args.length; i++) {
    const ch = args[i];
    if (inString) {
      if (ch === '\\') i++;
      else if (ch === inString) inString = null;
      continue;
    }
    if (ch === "'" || ch === '"' || ch === '`') inString = ch;
    else if (ch === '(' || ch === '[' || ch === '{') depth++;
    else if (ch === ')' || ch === ']' || ch === '}') depth--;
    else if (ch === ',' && depth === 0) return args.slice(0, i);
  }
  return args;
}

/** A translation key: dotted segments of identifier characters. */
const KEY_RE = /^[A-Za-z0-9_]+(\.[A-Za-z0-9_]+)+$/;
/** `t(` or `i18n.t(` call sites; not `.t(` on other receivers. */
const CALL_RE = /(?<![\w$.])(?:i18n\.)?t\s*\(/g;

type UsedKey = { key: string; where: string };

async function scanSources(): Promise<UsedKey[]> {
  const used: UsedKey[] = [];
  const files = Array.from(new Glob('src/**/*.{ts,tsx}').scanSync())
    .filter(file => !file.startsWith('src/i18n/locales'))
    .sort();
  expect(files.length).toBeGreaterThan(50); // the glob must actually see the app

  for (const file of files) {
    const source = stripComments(await Bun.file(file).text());
    let match: RegExpExecArray | null;
    CALL_RE.lastIndex = 0;
    while ((match = CALL_RE.exec(source))) {
      const open = match.index + match[0].length - 1;
      const close = matchBalanced(source, open, '(', ')');
      const args = firstArgument(source.slice(open + 1, close));
      for (const literal of args.matchAll(/(['"])([^'"\n]*)\1/g)) {
        const key = literal[2];
        if (!KEY_RE.test(key)) continue; // options objects, plain strings, etc.
        const line = source.slice(0, match.index).split('\n').length;
        used.push({ key, where: `${file}:${line}` });
      }
    }
  }
  return used;
}

const usedKeys = await scanSources();

describe('translation key usage', { timeout: 30_000 }, () => {
  test('the scan sees a meaningful share of the catalog', () => {
    // A broken regex or glob would make the guard vacuously pass.
    expect(usedKeys.length).toBeGreaterThan(200);
    const distinct = new Set(usedKeys.map(usage => usage.key));
    expect(distinct.size).toBeGreaterThan(150);
  });

  test('every literal t() key exists in the English catalog', () => {
    const missing = usedKeys
      .filter(usage => !catalogHas(en, usage.key))
      .map(usage => `${usage.key} (${usage.where})`);
    expect(missing).toEqual([]);
  });

  test('every literal t() key exists in the Russian catalog', () => {
    const missing = usedKeys
      .filter(usage => !catalogHas(ru, usage.key))
      .map(usage => `${usage.key} (${usage.where})`);
    expect(missing).toEqual([]);
  });
});
