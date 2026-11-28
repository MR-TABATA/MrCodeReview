// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { aggregateAspects } from './aspectstats';
import { aspectsPanel } from './aspectsview';

const rev = (title: string, findings: number, counts: { open: number; done: number; wait: number; dismiss: number } | null) => ({
  id: Math.random().toString(), created_at: 1, repo: '/r', recipe_title: title, provider_label: 'p', findings, verified: 0, total_tokens: null, failed: false, counts,
});

describe('観点のページ', () => {
  it('空なら、案内だけ', () => {
    const el = aspectsPanel(aggregateAspects([]), 0, 'ja', new Set(), vi.fn());
    expect(el.textContent).toContain('まだレビューがありません');
    expect(el.querySelector('.aspect-table')).toBeNull();
  });

  it('カード 4 枚・観点ごとの行・対象件数を出す。無視の割合が出せないときは「−」', () => {
    const stats = aggregateAspects([rev('潜在バグ', 3, { open: 3, done: 0, wait: 0, dismiss: 0 })]);
    const el = aspectsPanel(stats, 1, 'ja', new Set(), vi.fn());
    expect(el.querySelectorAll('.aspect-card').length).toBe(4);
    expect(el.querySelectorAll('.aspect-row[data-aspect]').length).toBe(1);
    expect(el.querySelector('.aspect-row[data-aspect] .aspect-num[title]')!.textContent).toBe('−');
    expect(el.querySelector('.aspects-scope')!.textContent).toContain('1 件');
    expect(el.querySelector('.aspects-unreadable')).toBeNull();
  });

  it('「履歴」を押すと、観点名が渡る。tool 型には印、読めない内訳があれば注記', () => {
    const stats = aggregateAspects([rev('潜在バグ', 3, null)]);
    const on = vi.fn();
    const el = aspectsPanel(stats, 1, 'ja', new Set(['潜在バグ']), on);
    (el.querySelector('.aspect-hist') as HTMLElement).click();
    expect(on).toHaveBeenCalledWith('潜在バグ');
    expect(el.querySelector('.badge')!.textContent).toBe('Tool');
    expect(el.querySelector('.aspects-unreadable')).not.toBeNull();
  });
});
