import { describe, expect, it } from 'vitest';
import { findingStats } from './findingstats';
import type { AgentReport, Note, TriageState } from './types';

const f = (severity: string) => ({ severity });
const rep = (verdict: 'did' | 'skipped'): AgentReport => ({ verdict, text: 'x', reported_at: 1, source: 's' });
const note = (done: boolean): Note => ({ memo: '', memo_at: null, done_at: done ? 1 : null, done_head: null, triage_at: null, triage_head: null, triage_source: null }) as Note;

describe('指摘の集計（結果の帯）', () => {
  it('重大度ごとの件数と、判断の内訳（履歴の行と同じ数え方）。5 つを足すと、指摘の数になる', () => {
    const triage: TriageState[] = ['new', 'fix', 'fix', 'fix', 'wait', 'dismiss', 'fix'];
    const r = {
      findings: [f('high'), f('high'), f('medium'), f('medium'), f('low'), f('low'), f('weird')],
      triage,
      // 0 未仕分け／1 対応（未）／2 対応＋did → 済み／3 対応＋skipped → 判断待ち／4 待ち／5 無視／6 対応＋skipped＋対応済みの印 → 済み
      finding_notes: [note(false), note(false), note(false), note(false), note(false), note(false), note(true)],
      agent_reports: [null, null, rep('did'), rep('skipped'), null, null, rep('skipped')],
    };
    const s = findingStats(r);
    expect(s.total).toBe(7);
    expect(s.severity).toEqual({ high: 2, medium: 2, low: 2, other: 1 });
    expect(s.counts).toEqual({ open: 2, pending: 1, done: 2, wait: 1, dismiss: 1 });
    expect(Object.values(s.counts).reduce((a, b) => a + b, 0)).toBe(7);
  });

  it('待ち・無視に、対応しなかった申告があっても、判断待ちに入れない。指摘が 0 件なら、全部 0', () => {
    const s = findingStats({ findings: [f('low'), f('low')], triage: ['wait', 'dismiss'], agent_reports: [rep('skipped'), rep('skipped')] });
    expect(s.counts).toEqual({ open: 0, pending: 0, done: 0, wait: 1, dismiss: 1 });
    expect(findingStats({ findings: [], triage: [] })).toEqual({ total: 0, severity: { high: 0, medium: 0, low: 0, other: 0 }, counts: { open: 0, pending: 0, done: 0, wait: 0, dismiss: 0 } });
  });
});
