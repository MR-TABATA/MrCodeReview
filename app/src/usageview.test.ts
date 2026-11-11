// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { barPct, periodSince, usagePanel, type UsageState } from './usageview';
import type { UsageRow, UsageSummary } from './types';

const row = (key: string, input: number, output: number, over: Partial<UsageRow> = {}): UsageRow => ({
  key, reviews: 1, failed: 0, unreported: 0, input, output, cache_read: 0, cache_write: 0, ...over,
});

describe('periodSince', () => {
  const now = new Date(2026, 9, 26, 15, 30); // 2026-10-26 15:30（ローカル）
  it('今日は、今日の 0 時。7 日間は、6 日前の 0 時。すべては null', () => {
    expect(periodSince('today', now)).toBe(new Date(2026, 9, 26).getTime());
    expect(periodSince('week', now)).toBe(new Date(2026, 9, 20).getTime());
    expect(periodSince('all', now)).toBeNull();
  });
  it('月をまたいでも、正しい日付になる', () => {
    expect(periodSince('week', new Date(2026, 10, 3, 9, 0))).toBe(new Date(2026, 9, 28).getTime());
  });
});

describe('barPct', () => {
  it('最大の行が 100。全部 0 なら 0（0 で割らない）', () => {
    const rows = [row('a', 300, 100), row('b', 100, 0), row('c', 0, 0)];
    expect(rows.map((r) => barPct(r, rows))).toEqual([100, 25, 0]);
    expect(barPct(row('z', 0, 0), [row('z', 0, 0)])).toBe(0);
  });
});

describe('usagePanel', () => {
  const data: UsageSummary = {
    since: null,
    total: row('合計', 1500, 200, { reviews: 4, failed: 1, unreported: 1, cache_read: 50 }),
    by_recipe: [row('潜在バグ', 1500, 200, { reviews: 3, unreported: 1 }), row('リンター', 0, 0)],
    by_repo: [row('/Users/me/Git/MrkDown', 1500, 200)],
    by_model: [row('リンター（LLM を使わない）', 0, 0)],
    cost_usd: 0.25,
    cost_reviews: 1,
    unreadable: 0,
  };
  const st = (over: Partial<UsageState> = {}): UsageState => ({ period: 'today', data, loading: false, error: '', ...over });

  it('合計・報告なしの件数・金額（参考）を出し、報告なしを 0 として足したとは言わない', () => {
    const el = usagePanel(st(), { setPeriod: vi.fn(), close: vi.fn() });
    const text = el.textContent ?? '';
    expect(text).toContain('1,500');
    expect(text).toContain('レビュー 4 件');
    expect(text).toContain('失敗 1 件は、トークンを足していません');
    expect(text).toContain('トークンの報告が無かったレビュー 1 件');
    expect(text).toContain('$0.2500');
    expect(text).toContain('＋報告なし 1 件');
    expect(text).toContain('MrkDown'); // リポジトリは、末尾 2 階層に縮めて出す
    expect(text).toContain('測っての 0');
  });

  it('英語モードで英語タイトル、カード、ボタンが表示される', () => {
    const el = usagePanel(st(), { setPeriod: vi.fn(), close: vi.fn() }, 'en');
    const text = el.textContent ?? '';
    expect(text).toContain('Token Usage');
    expect(text).toContain('Input');
    expect(text).toContain('Output');
    expect(text).toContain('Cache Read');
    expect(text).toContain('4 reviews');
    expect(text).toContain('Close');
    expect(text).toContain('By Review Angle');
    expect(text).toContain('Latent Bugs');
  });

  it('期間のボタンを押すと、その期間で setPeriod を呼ぶ。いまの期間に印が付く', () => {
    const setPeriod = vi.fn();
    const el = usagePanel(st({ period: 'week' }), { setPeriod, close: vi.fn() });
    const btns = [...el.querySelectorAll('.dmode button')] as HTMLElement[];
    expect(btns.map((b) => b.textContent)).toEqual(['今日', '7 日間', 'すべて']);
    expect(btns[1]?.className).toBe('on');
    btns[2]?.click();
    expect(setPeriod).toHaveBeenCalledExactlyOnceWith('all');
  });

  it('読み込み中・エラー・記録なしを、それぞれ出す。金額が無ければ、金額の行を出さない', () => {
    const noop = { setPeriod: vi.fn(), close: vi.fn() };
    expect(usagePanel(st({ data: null, loading: true }), noop).textContent).toContain('集計しています');
    expect(usagePanel(st({ data: null, error: '壊れた' }), noop).querySelector('.err')?.textContent).toBe('壊れた');
    const empty: UsageSummary = { since: 1, total: row('合計', 0, 0, { reviews: 0 }), by_recipe: [], by_repo: [], by_model: [], cost_usd: null, cost_reviews: 0, unreadable: 0 };
    const t = usagePanel(st({ data: empty }), noop).textContent ?? '';
    expect(t).toContain('この期間の記録はありません');
    expect(t).not.toContain('提供元が報告した金額');
  });

  it('壊れて読めなかった記録があれば、件数を言う（黙って抜かない）。無ければ、出さない', () => {
    const noop = { setPeriod: vi.fn(), close: vi.fn() };
    const base = { since: 1, total: row('合計', 1, 1, { reviews: 1 }), by_recipe: [], by_repo: [], by_model: [], cost_usd: null, cost_reviews: 0 };
    const bad = usagePanel(st({ data: { ...base, unreadable: 2 } }), noop).textContent ?? '';
    expect(bad).toContain('壊れていて読めなかった記録が 2 件あり、この集計に含まれていません');
    expect(usagePanel(st({ data: { ...base, unreadable: 0 } }), noop).textContent).not.toContain('壊れていて読めなかった');
  });

  it('閉じるボタンで close を呼ぶ', () => {
    const close = vi.fn();
    const el = usagePanel(st(), { setPeriod: vi.fn(), close });
    ([...el.querySelectorAll('button')].find((b) => b.textContent === '閉じる') as HTMLElement).click();
    expect(close).toHaveBeenCalledOnce();
  });
});
