// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { currentRecipes, enableBlock, hookSection, newHookUi, recipesChanged, selectableRecipes, type HookActions, type HookUiState } from './hookview';
import type { HookStatus, Recipe } from './types';

const recipe = (id: string, over: Partial<Recipe> = {}): Recipe => ({ id, title: `観点 ${id}`, source: 'builtin', prompt: '', kind: 'llm', ...over });
const status = (over: Partial<HookStatus> = {}): HookStatus => ({
  repo: '/r',
  enabled: false,
  recipes: ['bugs'],
  installed: false,
  cli: '/opt/homebrew/bin/mrcr',
  blocked: null,
  target_missing: false,
  last_run: null,
  ...over,
});
const ui = (over: Partial<HookUiState> = {}): HookUiState => ({ ...newHookUi(), status: status(), ...over });
const noop: HookActions = { toggle: () => {}, saveRecipes: () => {}, pick: () => {} };
const text = (el: HTMLElement): string => el.textContent ?? '';
const buttons = (el: HTMLElement): HTMLButtonElement[] => [...el.querySelectorAll('button')];

describe('selectableRecipes', () => {
  it('LLM の観点だけで、設計書を要る観点と linter は選べない', () => {
    const list = [recipe('bugs'), recipe('spec', { needs_spec: true }), recipe('eslint', { kind: 'tool' })];
    expect(selectableRecipes(list).map((r) => r.id)).toEqual(['bugs']);
  });
});

describe('recipes draft', () => {
  it('draft が無ければ保存済みの観点のまま。順序が違うだけなら、変更とは見なさない', () => {
    const st = ui({ status: status({ recipes: ['bugs', 'errors'] }) });
    expect(currentRecipes(st)).toEqual(['bugs', 'errors']);
    expect(recipesChanged(st)).toBe(false);
    expect(recipesChanged({ ...st, draft: ['errors', 'bugs'] })).toBe(false);
    expect(recipesChanged({ ...st, draft: ['bugs'] })).toBe(true);
  });
});

describe('enableBlock（ON にできない理由）', () => {
  it('押せる状態なら null', () => expect(enableBlock(ui())).toBeNull());
  it('読み込み前・処理中は busy', () => {
    expect(enableBlock({ ...newHookUi() })).toBe('busy');
    expect(enableBlock(ui({ busy: true }))).toBe('busy');
  });
  it('CLI が無い・置き場に書けない・観点が空は、それぞれ別の理由', () => {
    expect(enableBlock(ui({ status: status({ cli: null }) }))).toBe('cli');
    expect(enableBlock(ui({ status: status({ blocked: 'husky' }) }))).toBe('blocked');
    expect(enableBlock(ui({ draft: [] }))).toBe('no-recipes');
  });
});

describe('hookSection', () => {
  it('リポジトリが選ばれていないときは、設定を出さない', () => {
    const el = hookSection(ui(), false, [recipe('bugs')], noop, 'ja');
    expect(text(el)).toContain('リポジトリを選ぶと');
    expect(buttons(el)).toHaveLength(0);
  });

  it('読み込み前は読み取り中、失敗したら理由を出す', () => {
    expect(text(hookSection(newHookUi(), true, [], noop, 'ja'))).toContain('読み取っています');
    expect(text(hookSection({ ...newHookUi(), error: 'boom' }, true, [], noop, 'ja'))).toContain('boom');
  });

  it('OFF: 「ON にする」を押すと、いま選んでいる観点で toggle(true)', () => {
    const calls: unknown[] = [];
    const el = hookSection(ui(), true, [recipe('bugs'), recipe('errors')], { ...noop, toggle: (e, r) => calls.push([e, r]) }, 'ja');
    const on = buttons(el).find((b) => b.textContent === 'ON にする')!;
    expect(on.disabled).toBe(false);
    on.click();
    expect(calls).toEqual([[true, ['bugs']]]);
  });

  it('CLI が無いと「ON にする」は押せず、案内（brew install …）を出す', () => {
    const el = hookSection(ui({ status: status({ cli: null }) }), true, [recipe('bugs')], noop, 'ja');
    expect(buttons(el).find((b) => b.textContent === 'ON にする')!.disabled).toBe(true);
    expect(text(el)).toContain('brew install mr-tabata/tap/mrcr');
    expect(text(el)).toContain('npm i -g @mr-tabata/mrcr');
    expect(text(el)).toContain('npx');
  });

  it('フックが呼ぶ mrcr が消えているときは、入れ直しを案内する', () => {
    const el = hookSection(ui({ status: status({ enabled: true, installed: true, cli: null, target_missing: true }) }), true, [recipe('bugs')], noop, 'ja');
    expect(text(el)).toContain('フックが呼ぶ mrcr が見つかりません');
  });

  it('フックを置けないリポジトリは、理由を出して押せない', () => {
    const el = hookSection(ui({ status: status({ blocked: 'core.hooksPath' }) }), true, [recipe('bugs')], noop, 'ja');
    expect(text(el)).toContain('core.hooksPath');
    expect(buttons(el).find((b) => b.textContent === 'ON にする')!.disabled).toBe(true);
  });

  it('ON: 「OFF にする」があり、観点を変えたときだけ「観点を保存」が出る', () => {
    const on = ui({ status: status({ enabled: true, installed: true }) });
    const labels = (st: HookUiState) => buttons(hookSection(st, true, [recipe('bugs'), recipe('errors')], noop, 'ja')).map((b) => b.textContent);
    expect(labels(on)).toEqual(['OFF にする']);
    expect(labels({ ...on, draft: ['bugs', 'errors'] })).toEqual(['OFF にする', '観点を保存']);
  });

  it('観点のチェックを変えると pick に、新しい選択が渡る', () => {
    const picked: string[][] = [];
    const el = hookSection(ui(), true, [recipe('bugs'), recipe('errors')], { ...noop, pick: (r) => picked.push(r) }, 'ja');
    const boxes = [...el.querySelectorAll<HTMLInputElement>('input[type=checkbox]')];
    expect(boxes.map((b) => b.checked)).toEqual([true, false]);
    boxes[1].checked = true;
    boxes[1].dispatchEvent(new Event('change'));
    expect(picked).toEqual([['bugs', 'errors']]);
  });

  it('直近の実行: 失敗は理由つきで出し、まだ無ければ、そう言う', () => {
    expect(text(hookSection(ui(), true, [], noop, 'ja'))).toContain('まだ実行されていません');
    const failed = ui({ status: status({ last_run: { at_ms: 1_700_000_000_000, code: 2, message: 'API キーがありません' } }) });
    const t = text(hookSection(failed, true, [], noop, 'ja'));
    expect(t).toContain('失敗');
    expect(t).toContain('API キーがありません');
  });

  it('英語でも出る', () => {
    expect(text(hookSection(ui(), true, [recipe('bugs')], noop, 'en'))).toContain('Turn ON');
  });
});
