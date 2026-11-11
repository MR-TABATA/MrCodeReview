import { describe, it, expect } from 'vitest';
import { comboItems, recipeSummary, GROUP_LLM, GROUP_TOOL } from './recipeinfo';
import type { Recipe } from './types';

const r = (over: Partial<Recipe> & { id: string }): Recipe => ({ title: over.id, source: 'builtin', prompt: '', kind: 'llm', ...over });

describe('recipeSummary', () => {
  it('組み込みの観点は、決まった 1 行の説明を返す', () => {
    for (const id of ['bugs', 'errors', 'security', 'performance', 'tests', 'consistency', 'lint']) {
      expect(recipeSummary(r({ id })).length, id).toBeGreaterThan(5);
      expect(recipeSummary(r({ id }), 'en').length, id).toBeGreaterThan(5);
    }
    expect(recipeSummary(r({ id: 'security' }), 'en')).toBe('Find security holes in input validation, auth, and secret handling');
  });

  it('リポジトリ側のレシピは、見出しとコメントを飛ばして、本文の最初の 1 行を切って使う', () => {
    const long = 'あ'.repeat(80);
    const s = recipeSummary(r({ id: 'mine', source: 'repo', prompt: `# 見出し\n<!-- related -->\n\n${long}\n2 行目` }));
    expect(s).toBe(`${'あ'.repeat(44)}…`);
    expect(recipeSummary(r({ id: 'mine', prompt: '# 見出しだけ' }))).toBe('');
    expect(recipeSummary(r({ id: 'mine', prompt: '# t\n短い説明' }))).toBe('短い説明');
  });
});

describe('comboItems', () => {
  it('LLM の観点（組み込み → リポジトリ側）のあとに、リンターが来て、グループが分かれる', () => {
    const items = comboItems([
      r({ id: 'bugs', title: '潜在バグ' }),
      r({ id: 'mine', title: 'うちの観点', source: 'repo', prompt: '# t\n説明' }),
      r({ id: 'lint', title: 'リンター', kind: 'tool' }),
    ]);
    expect(items.map((i) => [i.value, i.group])).toEqual([
      ['bugs', GROUP_LLM],
      ['mine', GROUP_LLM],
      ['lint', GROUP_TOOL],
    ]);
    expect(items[1]?.badge).toBe('このリポジトリ');
    expect(items[0]?.badge).toBeUndefined();
  });
});
