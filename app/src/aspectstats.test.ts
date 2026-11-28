import { describe, expect, it } from 'vitest';
import { aggregateAspects, ignorePct } from './aspectstats';
import type { ReviewSummary, TriageCounts } from './types';

let n = 0;
const rev = (title: string, findings: number, counts: TriageCounts | null, failed = false): ReviewSummary => ({
  id: `r${n++}`,
  created_at: 1,
  repo: '/r',
  recipe_title: title,
  provider_label: 'p',
  findings,
  verified: 0,
  total_tokens: null,
  failed,
  counts,
});
const c = (open: number, done: number, wait: number, dismiss: number): TriageCounts => ({ open, done, wait, dismiss });

describe('観点ごとの集計', () => {
  it('履歴が空なら、行も合計も空', () => {
    const s = aggregateAspects([]);
    expect(s.rows).toEqual([]);
    expect(s.totals).toEqual({ aspects: 0, runs: 0, findings: 0, ignorePct: null });
    expect(s.unreadable).toBe(0);
  });

  it('観点名で束ね、実行・指摘・内訳を足す。合計は、履歴の件数・指摘の合計と一致する', () => {
    const h = [rev('潜在バグ', 3, c(1, 1, 0, 1)), rev('潜在バグ', 2, c(0, 0, 1, 1)), rev('セキュリティ', 4, c(4, 0, 0, 0))];
    const s = aggregateAspects(h);
    expect(s.rows.map((r) => [r.title, r.runs, r.findings])).toEqual([['潜在バグ', 2, 5], ['セキュリティ', 1, 4]]);
    const bug = s.rows[0];
    expect([bug.open, bug.done, bug.wait, bug.dismiss]).toEqual([1, 1, 1, 2]);
    expect(s.totals.runs).toBe(h.length);
    expect(s.totals.findings).toBe(h.reduce((a, x) => a + x.findings, 0));
    expect(s.totals.aspects).toBe(2);
  });

  it('無視の割合は、未を分母に入れない。判断が 0 件なら null', () => {
    expect(ignorePct(1, 1, 2)).toBe(50);
    expect(ignorePct(0, 0, 0)).toBeNull();
    const s = aggregateAspects([rev('A', 5, c(5, 0, 0, 0)), rev('B', 4, c(2, 1, 0, 1))]);
    expect(s.rows.find((r) => r.title === 'A')!.ignorePct).toBeNull();
    expect(s.rows.find((r) => r.title === 'B')!.ignorePct).toBe(50);
    expect(s.totals.ignorePct).toBe(50);
  });

  it('失敗した実行も、実行の回数に数える。失敗の数は別に持つ', () => {
    const s = aggregateAspects([rev('A', 0, c(0, 0, 0, 0), true), rev('A', 2, c(2, 0, 0, 0))]);
    expect(s.rows[0].runs).toBe(2);
    expect(s.rows[0].failed).toBe(1);
  });

  it('内訳を読めなかった記録（counts が null で、指摘がある）は、指摘に入れ、内訳には入れず、数を返す', () => {
    const s = aggregateAspects([rev('A', 3, null), rev('A', 2, c(1, 1, 0, 0)), rev('A', 0, null, true)]);
    expect(s.unreadable).toBe(1);
    expect(s.rows[0].findings).toBe(5);
    expect(s.rows[0].open + s.rows[0].done).toBe(2);
  });

  it('並びは、実行の多い順。同数なら観点名の順', () => {
    const s = aggregateAspects([rev('B', 0, c(0, 0, 0, 0)), rev('A', 0, c(0, 0, 0, 0)), rev('C', 0, c(0, 0, 0, 0)), rev('C', 0, c(0, 0, 0, 0))]);
    expect(s.rows.map((r) => r.title)).toEqual(['C', 'A', 'B']);
  });

  it('判断待ち（pending）は、観点ごとに別に数える。無視の割合の分母には、入れない', () => {
    const s = aggregateAspects([rev('A', 4, { open: 1, pending: 2, done: 1, wait: 0, dismiss: 0 } as TriageCounts)]);
    const a = s.rows[0];
    expect([a.open, a.pending, a.done]).toEqual([1, 2, 1]);
    expect(a.ignorePct).toBe(0); // 判断済みは 1 件（済み）で、無視は 0
    expect(a.open + a.pending + a.done + a.wait + a.dismiss).toBe(a.findings);
  });
});
