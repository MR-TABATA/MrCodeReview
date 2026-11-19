import { describe, expect, it } from 'vitest';
import { continuityOf, filteredFates, flowOf, shownByContinuity } from './continuity';
import type { Carry, Fate, ReviewSummary, ReviewView } from './types';

const carry = (from_review: string, from_index = 0): Carry => ({ from_review, from_index, state: 'new', exact: true });
const fate = (from_index: number, f: Fate['fate'], from_review = 'p'): Fate => ({ from_review, from_index, title: `t${from_index}`, file: 'a.ts', state_before: 'fix', fate: f });

function view(o: { findings: number; carries: (Carry | null)[]; fates: Fate[]; previous?: ReviewView['previous'] }): ReviewView {
  return {
    id: 'cur',
    findings: Array.from({ length: o.findings }, () => ({})),
    carries: o.carries,
    fates: o.fates,
    previous: o.previous === undefined ? { id: 'p', created_at: 1, findings: 3, seq: 1 } : o.previous,
  } as unknown as ReviewView;
}

describe('continuityOf（前回 → 今回の内訳）', () => {
  it('前回 3 件・今回 2 件で 1 件が継続判定: 継続 1・新規 1・解消 1・再指摘なし 1', () => {
    const r = view({
      findings: 2,
      carries: [carry('p', 0), null],
      fates: [fate(0, 'persists'), fate(1, 'gone'), fate(2, 'not_reraised')],
    });
    const c = continuityOf(r)!;
    expect(c.persist).toEqual([0]);
    expect(c.isNew).toEqual([1]);
    expect(c.gone.map((x) => x.from_index)).toEqual([1]);
    expect(c.other.map((x) => x.from_index)).toEqual([2]);
  });

  it('前回が無い（初回・古い記録）ときは null', () => {
    expect(continuityOf(view({ findings: 1, carries: [], fates: [], previous: null }))).toBeNull();
  });

  it('前回とは別のレビューから引き継いだだけの指摘は、継続ではなく新規（前回の指摘ではない）', () => {
    const r = view({ findings: 2, carries: [carry('older'), carry('older')], fates: [] });
    const c = continuityOf(r)!;
    expect(c.persist).toEqual([]);
    expect(c.isNew).toEqual([0, 1]);
  });

  it('前回の指摘の行方は、前回のレビューのものだけ数える（別のレビューの行方は混ぜない）', () => {
    const r = view({ findings: 0, carries: [], fates: [fate(0, 'gone'), fate(1, 'gone', 'older')] });
    expect(continuityOf(r)!.gone).toHaveLength(1);
  });

  it('継続・新規・解消・再指摘なしの合計は、今回の指摘数と、前回の指摘の行方の数に合う', () => {
    const r = view({
      findings: 4,
      carries: [carry('p', 0), carry('p', 1), null, null],
      fates: [fate(0, 'persists'), fate(1, 'persists'), fate(2, 'gone'), fate(3, 'unknown'), fate(4, 'file_missing')],
    });
    const c = continuityOf(r)!;
    expect(c.persist.length + c.isNew.length).toBe(4);
    expect(c.persist.length + c.gone.length + c.other.length).toBe(5);
  });

  it('同じ記録には、同じ結果を返す（指摘ごとに計算し直さない）', () => {
    const r = view({ findings: 1, carries: [null], fates: [] });
    expect(continuityOf(r)).toBe(continuityOf(r));
  });
});

describe('絞り込み', () => {
  const r = view({
    findings: 4,
    carries: [carry('p', 0), null, carry('p', 1), null],
    fates: [fate(0, 'persists'), fate(1, 'persists'), fate(2, 'gone'), fate(3, 'not_reraised')],
  });
  const c = continuityOf(r);
  const shown = (k: Parameters<typeof shownByContinuity>[1]) => [0, 1, 2, 3].filter((i) => shownByContinuity(c, k, i));

  it('絞り込みなしは全部、継続・新規は添字が合う、解消・再指摘なしは今回の指摘を出さない', () => {
    expect(shown(null)).toEqual([0, 1, 2, 3]);
    expect(shown('persist')).toEqual([0, 2]);
    expect(shown('new')).toEqual([1, 3]);
    expect(shown('gone')).toEqual([]);
    expect(shown('other')).toEqual([]);
  });

  it('前回が無いときは、絞り込みがあっても全部出す', () => {
    expect(shownByContinuity(null, 'persist', 0)).toBe(true);
  });

  it('前回の指摘の行方の一覧は、選んだ種類の分だけ', () => {
    expect(filteredFates(r, c, null)).toHaveLength(4);
    expect(filteredFates(r, c, 'gone').map((x) => x.from_index)).toEqual([2]);
    expect(filteredFates(r, c, 'other').map((x) => x.from_index)).toEqual([3]);
    expect(filteredFates(r, c, 'persist')).toEqual([]);
    expect(filteredFates(r, c, 'new')).toEqual([]);
  });
});

describe('flowOf（同じ観点の流れ）', () => {
  const S = (id: string, seq: number | null | undefined, over: Partial<ReviewSummary> = {}): ReviewSummary =>
    ({ id, created_at: 1, repo: '/r', recipe_title: '潜在バグ', provider_label: 'x', findings: 2, verified: 0, total_tokens: null, failed: false, counts: null, seq, ...over }) as ReviewSummary;
  const cur = (id: string) => ({ id, repo: '/r', recipe: { title: '潜在バグ' } });

  it('番号の無い記録は流れに入れない（0 や undefined を出さない）。失敗した実行は、失敗として残る', () => {
    const f = flowOf([S('a', 1), S('old', null), S('older', undefined), S('zero', 0), S('b', 2, { failed: true }), S('c', 3)], cur('c'))!;
    expect(f.items.map((i) => i.seq)).toEqual([1, 2, 3]);
    expect(f.items.map((i) => i.id)).toEqual(['a', 'b', 'c']);
    expect(f.items.some((i) => !i.seq)).toBe(false);
    expect(f.items.map((i) => i.failed)).toEqual([false, true, false]);
    expect(f.items.map((i) => i.now)).toEqual([false, false, true]);
  });

  it('作成時刻の無い記録は流れに入れず、いまのレビューがそれなら出さない', () => {
    const f = flowOf([S('a', 1, { created_at: 10 }), S('z', 1, { created_at: 0 }), S('b', 2, { created_at: 20 })], cur('b'))!;
    expect(f.items.map((i) => i.id)).toEqual(['a', 'b']);
    expect(flowOf([S('a', 1, { created_at: 10 }), S('b', 2, { created_at: 20 }), S('z', 3, { created_at: 0 })], cur('z'))).toBeNull();
  });

  it('番号のある記録が 2 件に満たなければ、出さない', () => {
    expect(flowOf([S('a', 1), S('x', null)], cur('a'))).toBeNull();
    expect(flowOf([], cur('a'))).toBeNull();
  });

  it('別のリポジトリ・別の観点は入れない。番号の順に並べる', () => {
    const f = flowOf([S('c', 3), S('o', 1, { repo: '/other' }), S('e', 1, { recipe_title: '例外処理' }), S('a', 1), S('b', 2)], cur('b'))!;
    expect(f.items.map((i) => i.seq)).toEqual([1, 2, 3]);
  });

  it('長いときは、いまのレビューの近くの max 件だけ。前後に続きがあるかを返す', () => {
    const h = Array.from({ length: 12 }, (_, i) => S(`r${i + 1}`, i + 1));
    const last = flowOf(h, cur('r12'), 8)!;
    expect([last.items[0]?.seq, last.items[last.items.length - 1]?.seq, last.hasBefore, last.hasAfter]).toEqual([5, 12, true, false]);
    const first = flowOf(h, cur('r1'), 8)!;
    expect([first.items[0]?.seq, first.items[first.items.length - 1]?.seq, first.hasBefore, first.hasAfter]).toEqual([1, 8, false, true]);
    const mid = flowOf(h, cur('r6'), 8)!;
    expect(mid.items).toHaveLength(8);
    expect(mid.items.some((i) => i.now)).toBe(true);
  });
});
