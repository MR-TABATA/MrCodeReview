import { describe, it, expect } from 'vitest';
import { findingOrder, isDoneFinding } from './findingorder';
import type { AgentReport, Note } from './types';

const note = (done_at: number | null): Note => ({ memo: '', memo_at: null, done_at, done_head: null, triage_at: null, triage_head: null, triage_source: null });
const rep = (verdict: 'did' | 'skipped'): AgentReport => ({ verdict, text: 'x', reported_at: 1, source: 's' });

describe('isDoneFinding（履歴の「済み」と同じ定義）', () => {
  it('「対応」で、対応済みの印がある、または、エージェントが「対応した」と申告したもの', () => {
    expect(isDoneFinding('fix', note(5), null)).toBe(true);
    expect(isDoneFinding('fix', note(null), rep('did'))).toBe(true);
    expect(isDoneFinding('fix', undefined, rep('did'))).toBe(true);
  });
  it('「対応しなかった」の申告・印も申告も無いもの・対応以外の仕分けは、済みではない', () => {
    expect(isDoneFinding('fix', note(null), rep('skipped'))).toBe(false);
    expect(isDoneFinding('fix', note(null), null)).toBe(false);
    expect(isDoneFinding('fix', undefined, undefined)).toBe(false);
    // 済みの印があっても、仕分けを「待ち」「無視」「未」に変えたものは、済みに数えない
    for (const st of ['wait', 'dismiss', 'new']) expect(isDoneFinding(st, note(5), rep('did')), st).toBe(false);
  });
});

describe('findingOrder', () => {
  it('対応済みは、あと。同じ組の中は、元の順を保つ', () => {
    const done = new Set([0, 2]);
    expect(findingOrder(5, (i) => done.has(i))).toEqual([1, 3, 4, 0, 2]);
  });
  it('済みが無ければ、元の順。全部済みでも、元の順', () => {
    expect(findingOrder(3, () => false)).toEqual([0, 1, 2]);
    expect(findingOrder(3, () => true)).toEqual([0, 1, 2]);
    expect(findingOrder(0, () => false)).toEqual([]);
  });
  it('「対応済みにする」を押すと、その指摘が、あとへ移る（押した直後）', () => {
    const done = new Set<number>();
    expect(findingOrder(3, (i) => done.has(i))).toEqual([0, 1, 2]);
    done.add(0);
    expect(findingOrder(3, (i) => done.has(i))).toEqual([1, 2, 0]);
    done.delete(0);
    expect(findingOrder(3, (i) => done.has(i)), '戻すと、元の位置').toEqual([0, 1, 2]);
  });
  describe('重要度の順', () => {
    const sev = (list: string[]) => (i: number) => list[i];

    it('high → medium → low の順。同じ重要度の中は、元の順', () => {
      //                 0       1         2       3        4
      const list = ['low', 'medium', 'high', 'medium', 'high'];
      expect(findingOrder(5, () => false, sev(list))).toEqual([2, 4, 1, 3, 0]);
    });

    it('対応済みは、重要度に関わらず、あと。その中も、重要度の順', () => {
      const list = ['high', 'low', 'medium', 'high'];
      const done = new Set([0, 3, 1]);
      expect(findingOrder(4, (i) => done.has(i), sev(list))).toEqual([2, 0, 3, 1]);
    });

    it('知らない重要度は、いちばん後ろ（消えない）', () => {
      expect(findingOrder(4, () => false, sev(['?', 'low', 'high', '']))).toEqual([2, 1, 0, 3]);
    });

    it('重要度を渡さなければ、これまでどおり、元の順', () => {
      expect(findingOrder(3, () => false)).toEqual([0, 1, 2]);
    });

    it('並べ替えても、指摘の番号は、変わらない（全部が、1 回ずつ出る）', () => {
      const order = findingOrder(6, (i) => i === 2, sev(['low', 'high', 'medium', 'low', 'high', 'medium']));
      expect([...order].sort()).toEqual([0, 1, 2, 3, 4, 5]);
    });
  });
});
