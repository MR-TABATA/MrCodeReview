import { describe, expect, it } from 'vitest';
import { ut } from './uitext';

/** 日本語の文字（かな・漢字・全角の記号。`〜` や `（` だけの文言も、日本語として数える） */
const JAPANESE = /[　-ヿ一-鿿＀-￯]/;
/** 日本語と英語で、同じ文字列のままでよいもの（固有名詞・略語）。ここに無いのに同じなら、訳し忘れ */
const SAME_IN_BOTH = new Set(['detail.llm', 'detail.head']);

type Leaf = [path: string, value: string];

/** 辞書のすべての文字列（関数は、ASCII の見本の引数で呼んだ結果）。引数の見本は、引数の数だけ、数と文字を交互に。 */
function leaves(lang: 'ja' | 'en'): Leaf[] {
  const out: Leaf[] = [];
  const walk = (o: unknown, path: string): void => {
    if (typeof o === 'string') out.push([path, o]);
    else if (typeof o === 'function') {
      const fn = o as (...a: unknown[]) => unknown;
      const args = Array.from({ length: fn.length }, (_, i) => (i % 2 === 0 ? i + 2 : 'ab'));
      const v = fn(...args);
      expect(typeof v, `${lang} ${path}`).toBe('string');
      out.push([`${path}()`, v as string]);
      // 条件分岐（0 件・1 件・複数）がある文も、通す
      const args2 = Array.from({ length: fn.length }, (_, i) => (i % 2 === 0 ? 0 : ''));
      out.push([`${path}(0)`, fn(...args2) as string]);
    } else if (o && typeof o === 'object') for (const [k, v] of Object.entries(o)) walk(v, path ? `${path}.${k}` : k);
  };
  walk(ut(lang), '');
  return out;
}

const keysOf = (lang: 'ja' | 'en') => leaves(lang).map(([p]) => p);

describe('uitext（画面の文言・日英）', () => {
  it('日本語と英語で、キーが同じ', () => {
    expect(keysOf('en')).toEqual(keysOf('ja'));
  });

  it('空の文言が無く、「undefined」「NaN」「[object」が混ざらない', () => {
    for (const lang of ['ja', 'en'] as const) {
      for (const [p, v] of leaves(lang)) {
        expect(v.trim(), `${lang} ${p}`).not.toBe('');
        expect(v, `${lang} ${p}`).not.toMatch(/undefined|NaN|\[object/);
      }
    }
  });

  it('英語の文言に、日本語が混ざらない', () => {
    for (const [p, v] of leaves('en')) expect(v, `en ${p}`).not.toMatch(JAPANESE);
  });

  it('日本語の文言は、日本語を含む（固有名詞だけのものを除く）', () => {
    for (const [p, v] of leaves('ja')) {
      if (SAME_IN_BOTH.has(p)) continue;
      expect(v, `ja ${p}`).toMatch(JAPANESE);
    }
  });

  it('英語は、日本語のコピーのままではない（訳し忘れを見つける）', () => {
    const ja = new Map(leaves('ja'));
    for (const [p, v] of leaves('en')) {
      if (SAME_IN_BOTH.has(p.replace(/\(0?\)$/, ''))) continue;
      expect(v, `en ${p}`).not.toBe(ja.get(p));
    }
  });

  it('関数は、渡した引数を、文に入れる', () => {
    for (const lang of ['ja', 'en'] as const) {
      const t = ut(lang);
      expect(t.detail.resultFailed('boom')).toContain('boom');
      expect(t.detail.specChars('doc', '1,234')).toContain('1,234');
      expect(t.batch.runN(7)).toContain('7');
      expect(t.batch.summary(5, 3, 1, 1)).toContain('5');
      expect(t.batch.summary(5, 3, 0, 0)).not.toBe(t.batch.summary(5, 3, 2, 0)); // 失敗の有無で、文が変わる
      expect(t.duration.minSec(2, 5)).toContain('2');
      expect(t.agent.foldTitleTag('mrcr:x#1')).toContain('mrcr:x#1');
    }
  });
});
