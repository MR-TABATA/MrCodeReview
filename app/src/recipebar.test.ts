// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { recipeBar, type RecipeBarState } from './recipebar';
import type { Recipe } from './types';

describe('recipeBar', () => {
  const sampleRecipes: Recipe[] = [
    { id: 'bugs', title: '潜在バグ', kind: 'llm', source: 'builtin', prompt: '' },
    { id: 'security', title: 'セキュリティ', kind: 'llm', source: 'builtin', prompt: '' },
    { id: 'lint', title: 'リンター', kind: 'tool', source: 'builtin', prompt: '' },
  ];

  const acts = () => ({
    onToggle: vi.fn(),
    onSelectAll: vi.fn(),
    onClearAll: vi.fn(),
    onRun: vi.fn(),
    onCancel: vi.fn(),
    onExpand: vi.fn(),
    onToggleShowAll: vi.fn(),
  });

  it('チェックが0件のときはボタンが無効で案内が出る', () => {
    const a = acts();
    const st: RecipeBarState = {
      recipes: sampleRecipes,
      checkedIds: new Set(),
      running: '',
    };
    const el = recipeBar(st, a);
    const runBtn = el.querySelector('.main-run-btn') as HTMLButtonElement;
    expect(runBtn.disabled).toBe(true);
    expect(runBtn.textContent).toBe('観点を選択してください');
  });

  it('1件選択時のボタンラベル', () => {
    const a = acts();
    const st: RecipeBarState = {
      recipes: sampleRecipes,
      checkedIds: new Set(['bugs']),
      running: '',
    };
    const el = recipeBar(st, a);
    const runBtn = el.querySelector('.main-run-btn') as HTMLButtonElement;
    expect(runBtn.disabled).toBe(false);
    expect(runBtn.textContent).toBe('「潜在バグ」を実行');
  });

  it('複数選択時のボタンラベル', () => {
    const a = acts();
    const st: RecipeBarState = {
      recipes: sampleRecipes,
      checkedIds: new Set(['bugs', 'security']),
      running: '',
    };
    const el = recipeBar(st, a);
    const runBtn = el.querySelector('.main-run-btn') as HTMLButtonElement;
    expect(runBtn.disabled).toBe(false);
    expect(runBtn.textContent).toBe('選択した 2 件を実行');
  });

  it('チェックボックスを切り替えると onToggle が呼ばれる', () => {
    const a = acts();
    const st: RecipeBarState = {
      recipes: sampleRecipes,
      checkedIds: new Set(['bugs']),
      running: '',
    };
    const el = recipeBar(st, a);
    const cbs = el.querySelectorAll('input[type="checkbox"]');
    (cbs[1] as HTMLInputElement).click();
    expect(a.onToggle).toHaveBeenCalledWith('security', true);
  });

  it('すべて選択・全解除ボタンが正しく発火する', () => {
    const a = acts();
    const st: RecipeBarState = {
      recipes: sampleRecipes,
      checkedIds: new Set(['bugs']),
      running: '',
    };
    const el = recipeBar(st, a);
    const btns = el.querySelectorAll('.link-btn');
    (btns[0] as HTMLButtonElement).click();
    expect(a.onSelectAll).toHaveBeenCalled();
    (btns[1] as HTMLButtonElement).click();
    expect(a.onClearAll).toHaveBeenCalled();
  });

  it('実行中は中止ボタンが表示され、実行ボタンは disabled になる', () => {
    const a = acts();
    const st: RecipeBarState = {
      recipes: sampleRecipes,
      checkedIds: new Set(['bugs', 'security']),
      running: '潜在バグ',
      runIndex: 0,
      totalRuns: 2,
    };
    const el = recipeBar(st, a);
    const runBtn = el.querySelector('.main-run-btn') as HTMLButtonElement;
    expect(runBtn.disabled).toBe(true);
    expect(runBtn.textContent).toContain('実行中… (1/2)');

    const cancelBtn = el.querySelector('.cancel-btn') as HTMLButtonElement;
    expect(cancelBtn).not.toBeNull();
    cancelBtn.click();
    expect(a.onCancel).toHaveBeenCalled();
  });

  it('英語モードで英語ラベルが表示される', () => {
    const a = acts();
    const st: RecipeBarState = {
      recipes: sampleRecipes,
      checkedIds: new Set(['bugs']),
      running: '',
    };
    const el = recipeBar(st, a, 'en');
    const runBtn = el.querySelector('.main-run-btn') as HTMLButtonElement;
    expect(runBtn.textContent).toBe('Run "Latent Bugs"');
    const chipTitle = el.querySelector('.recipe-title');
    expect(chipTitle?.textContent).toBe('Latent Bugs');
    const btns = el.querySelectorAll('.link-btn');
    expect(btns[0].textContent).toBe('Select All');
    expect(btns[1].textContent).toBe('Deselect All');
  });

  it('折り畳み時は新しいレビューを開く1行だけを表示する', () => {
    const a = acts();
    const el = recipeBar(
      { recipes: sampleRecipes, checkedIds: new Set(['bugs']), running: '', collapsed: true },
      a,
    );
    expect(el.querySelectorAll('.recipe-chip')).toHaveLength(0);
    expect(el.querySelector('.bar-foot')).toBeNull();
    const expand = el.querySelector('.recipe-bar-expand') as HTMLButtonElement;
    expect(expand.textContent).toBe('▸ 新しいレビュー');
    expand.click();
    expect(a.onExpand).toHaveBeenCalled();
  });

  it('実行中でも、折り畳みでなければ、観点を出したまま進捗をボタンに出す', () => {
    const el = recipeBar(
      { recipes: sampleRecipes, checkedIds: new Set(['bugs', 'security']), running: 'bugs', runIndex: 1, totalRuns: 2 },
      acts(),
    );
    expect(el.querySelectorAll('.recipe-chip')).toHaveLength(sampleRecipes.length);
    expect(el.querySelector('.main-run-btn')?.textContent).toContain('実行中… (2/2)');
    expect(el.querySelector('.main-run-btn')?.textContent).not.toMatch(/\d:\d\d/);
    expect(el.querySelector('.cancel-btn')).not.toBeNull();
  });

  it('隠している観点があるときだけ「すべての観点を見る」を出し、押すと切り替える', () => {
    const a = acts();
    const none = recipeBar({ recipes: sampleRecipes, checkedIds: new Set(), running: '' }, a);
    expect(none.textContent).not.toContain('すべての観点を見る');
    const el = recipeBar({ recipes: sampleRecipes, checkedIds: new Set(), running: '', hiddenCount: 3 }, a);
    const btn = [...el.querySelectorAll('button')].find((b) => b.textContent === 'すべての観点を見る（他に 3 件）') as HTMLButtonElement;
    expect(btn).toBeDefined();
    btn.click();
    expect(a.onToggleShowAll).toHaveBeenCalled();
    const open = recipeBar({ recipes: sampleRecipes, checkedIds: new Set(), running: '', hiddenCount: 0, showAll: true }, a);
    expect(open.textContent).toContain('この差分に向く観点だけ');
  });

  it('Terraform の観点は、見出し「Terraform」つきの別の塊にし、実行ボタンは右寄せの recipe-run-row に配置される', () => {
    const recipes = [...sampleRecipes, { id: 'tf-state', title: '状態・アドレス', source: 'builtin' as const, prompt: '', kind: 'llm' as const }];
    const el = recipeBar({ recipes, checkedIds: new Set(), running: '' }, acts());
    const group = el.querySelector('.recipe-group') as HTMLElement;
    expect(group.querySelector('.recipe-group-label')?.textContent).toBe('Terraform');
    expect(group.querySelectorAll('.recipe-chip')).toHaveLength(1);
    expect(group.querySelector('.main-run-btn')).toBeNull(); // チップグリッド内には入らない
    expect(el.querySelector('.recipe-run-row .main-run-btn')).not.toBeNull();
    expect(el.querySelectorAll('.recipe-chip')).toHaveLength(recipes.length);
    const only = recipeBar({ recipes: recipes.slice(-1), checkedIds: new Set(), running: '' }, acts());
    expect(only.querySelectorAll('.recipe-chips')).toHaveLength(1);
    expect(only.querySelectorAll('.main-run-btn')).toHaveLength(1);
  });

  it('文書の観点は、見出し「文書」（英語は Documents）つきの別の塊にし、実行ボタンは右寄せの recipe-run-row に配置される', () => {
    const doc = (id: string, title: string): Recipe => ({ id, title, source: 'builtin', prompt: '', kind: 'llm' });
    const tf: Recipe = { id: 'tf-state', title: '状態・アドレス', source: 'builtin', prompt: '', kind: 'llm' };
    const recipes = [...sampleRecipes, doc('doc-claims', '根拠のない主張・数字'), doc('doc-steps', '手順・前提・決定の抜け')];
    const el = recipeBar({ recipes, checkedIds: new Set(), running: '' }, acts());
    const groups = el.querySelectorAll('.recipe-group');
    expect(groups).toHaveLength(1);
    expect(groups[0].querySelector('.recipe-group-label')?.textContent).toBe('文書');
    expect(groups[0].querySelectorAll('.recipe-chip')).toHaveLength(2);
    expect(groups[0].querySelector('.main-run-btn')).toBeNull(); // チップグリッド内には入らない
    expect(el.querySelector('.recipe-run-row .main-run-btn')).not.toBeNull();
    expect(el.querySelectorAll('.main-run-btn')).toHaveLength(1);
    expect(el.querySelectorAll('.recipe-chip')).toHaveLength(recipes.length);
    expect(recipeBar({ recipes, checkedIds: new Set(), running: '' }, acts(), 'en').querySelector('.recipe-group-label')?.textContent).toBe('Documents');

    // Terraform と文書の両方: 塊は Terraform → 文書の順で、実行ボタンは右寄せの recipe-run-row に出る
    const both = recipeBar({ recipes: [...recipes, tf], checkedIds: new Set(), running: '' }, acts());
    const g = both.querySelectorAll('.recipe-group');
    expect([...g].map((x) => x.querySelector('.recipe-group-label')?.textContent)).toEqual(['Terraform', '文書']);
    expect(g[0].querySelector('.main-run-btn')).toBeNull();
    expect(g[1].querySelector('.main-run-btn')).toBeNull();
    expect(both.querySelector('.recipe-run-row .main-run-btn')).not.toBeNull();
    expect(both.querySelectorAll('.main-run-btn')).toHaveLength(1);

    // 文書の観点だけ（ほかが無い）でも、実行ボタンが 1 つ出る
    const only = recipeBar({ recipes: recipes.slice(-2), checkedIds: new Set(), running: '' }, acts());
    expect(only.querySelectorAll('.main-run-btn')).toHaveLength(1);
  });

  it('実行中は、走っている観点にスピナー、終わった観点に ✓、待っている観点に待ちの印を出す', () => {
    const el = recipeBar(
      { recipes: sampleRecipes, checkedIds: new Set(['bugs', 'security', 'lint']), running: sampleRecipes[1].title, runIndex: 1, totalRuns: 3, doneIds: new Set(['bugs']) },
      acts(),
    );
    const chip = (i: number) => el.querySelectorAll('.recipe-chip')[i] as HTMLElement;
    expect(chip(0).classList.contains('done')).toBe(true);
    expect(chip(0).querySelector('.chip-done')).not.toBeNull();
    expect(chip(1).classList.contains('running')).toBe(true);
    expect(chip(1).querySelector('.chip-spin')).not.toBeNull();
    expect(chip(2).classList.contains('waiting')).toBe(true);
  });

  it('specSlot（仕様書フォーム）があるときは、フォームの直下に右寄せで実行ボタンを配置する', () => {
    const specSlot = document.createElement('div');
    specSlot.className = 'test-spec-slot';

    // 1. グループなし（コード観点のみ）の場合
    const elPlain = recipeBar(
      { recipes: sampleRecipes, checkedIds: new Set(['bugs']), running: '', specSlot },
      acts(),
    );
    expect(elPlain.querySelector('.recipe-chips .main-run-btn')).toBeNull();
    const childrenPlain = Array.from(elPlain.children);
    const specIdxPlain = childrenPlain.indexOf(specSlot);
    const runContainerPlain = elPlain.querySelector('.recipe-run-row') as HTMLElement;
    const runIdxPlain = childrenPlain.indexOf(runContainerPlain);
    expect(specIdxPlain).toBeGreaterThanOrEqual(0);
    expect(runIdxPlain).toBe(specIdxPlain + 1); // フォームの直後に実行ボタン

    // 2. グループあり（文書観点など）の場合
    const doc = (id: string, title: string): Recipe => ({ id, title, source: 'builtin', prompt: '', kind: 'llm' });
    const recipesWithDoc = [...sampleRecipes, doc('doc-claims', '根拠のない主張・数字')];
    const elGroup = recipeBar(
      { recipes: recipesWithDoc, checkedIds: new Set(['bugs']), running: '', specSlot },
      acts(),
    );
    expect(elGroup.querySelector('.recipe-group .main-run-btn')).toBeNull();
    const childrenGroup = Array.from(elGroup.children);
    const specIdxGroup = childrenGroup.indexOf(specSlot);
    const runContainerGroup = elGroup.querySelector('.recipe-run-row') as HTMLElement;
    const runIdxGroup = childrenGroup.indexOf(runContainerGroup);
    expect(runIdxGroup).toBe(specIdxGroup + 1);
  });
});
