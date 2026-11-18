import { describe, expect, it } from 'vitest';
import { msg } from './i18n';
import type { Lang } from './i18n';

const JAPANESE = /[぀-ヿ一-鿿]/;
const langs: Lang[] = ['ja', 'en'];

/** 関数の文言を呼ぶときの引数の例（ASCII だけ。英語の文言に日本語が混ざったら、文言のほうの誤り）。新しい関数を足したら、ここにも足す（足さないと、テストが失敗する）。 */
const ARGS: Record<string, unknown[][]> = {
  carryNote: [['Fix', false], ['Fix', true]],
  prevTitle: [['11/18']],
  prevRun: [['Bugs_1', 2]],
  thisRun: [['Bugs_2', 3]],
  flowLabel: [['Bugs']],
  flowItem: [[2, false, 5], [2, true, 0]],
  flowOpenTitle: [['11/18']],
  panelTitle: [['Bugs_1']],
  fateBefore: [['Fix', 'a.ts']],
  continueBtn: [['Bugs_1', 3]],
  continueTitle: [['11/18']],
  continueFailed: [['reason']],
};

const keysOf = (o: unknown, prefix = ''): string[] =>
  o && typeof o === 'object' ? Object.entries(o as Record<string, unknown>).flatMap(([k, v]) => (v && typeof v === 'object' ? keysOf(v, `${prefix}${k}.`) : [`${prefix}${k}`])) : [];

/** 文言の表の、すべての文字列（関数は、`ARGS` の例で呼んで、出た文字列）。 */
function texts(lang: Lang): [string, string][] {
  const out: [string, string][] = [];
  const walk = (o: unknown, path: string, key: string): void => {
    if (typeof o === 'string') out.push([path, o]);
    else if (typeof o === 'function') {
      const calls = ARGS[key];
      if (!calls) throw new Error(`${path}: 引数の例が、ARGS に無い（新しい文言の関数を足したら、このテストの表にも足す）`);
      for (const args of calls) {
        const v = (o as (...a: unknown[]) => unknown)(...args);
        expect(typeof v, `${lang} ${path}(${args.join(',')})`).toBe('string');
        out.push([`${path}(${args.join(',')})`, v as string]);
      }
    } else if (o && typeof o === 'object') for (const [k, v] of Object.entries(o)) walk(v, `${path}.${k}`, k);
  };
  walk(msg(lang).mainUi.continuity, 'continuity', 'continuity');
  return out;
}

describe('「前回 → 今回」まわりの文言（日英）', () => {
  it('日本語と英語で、キーが同じ（片方だけ抜けない）', () => {
    expect(keysOf(msg('en').mainUi.continuity).sort()).toEqual(keysOf(msg('ja').mainUi.continuity).sort());
  });

  it('すべての文言が、空でなく、「undefined」「NaN」「[object」を含まない', () => {
    for (const lang of langs) {
      for (const [path, v] of texts(lang)) {
        expect(v.trim(), `${lang} ${path}`).not.toBe('');
        expect(v, `${lang} ${path}`).not.toMatch(/undefined|NaN|\[object/);
      }
    }
  });

  it('関数は、渡した引数を、文に入れる', () => {
    for (const lang of langs) {
      const t = msg(lang).mainUi.continuity;
      expect(t.carryNote('Fix', false)).toContain('Fix');
      expect(t.carryNote('Fix', true)).not.toBe(t.carryNote('Fix', false)); // 「引用がほぼ同じ」の有無で、文が変わる
      expect(t.flowItem(2, false, 5)).toContain('2');
      expect(t.flowItem(2, false, 5)).toContain('5');
      expect(t.flowItem(2, true, 0)).not.toBe(t.flowItem(2, false, 0)); // 失敗は、件数と別の表示
      expect(t.continueBtn('Bugs_1', 3)).toContain('Bugs_1');
      expect(t.panelTitle('Bugs_3')).toContain('Bugs_3');
      expect(t.fateBefore('Fix', 'a.ts')).toContain('a.ts');
    }
  });

  it('英語の文言に、日本語が混ざらない', () => {
    for (const [path, v] of texts('en')) expect(v, `en ${path}`).not.toMatch(JAPANESE);
  });

  it('日本語の文言は、日本語を含む（英語のまま移し忘れない）', () => {
    for (const [path, v] of texts('ja')) expect(v, `ja ${path}`).toMatch(JAPANESE);
  });
});
