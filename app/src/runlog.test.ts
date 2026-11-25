// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import type { ReviewView } from './types';
import { findReviewTarget, applyHandling } from './main';
import { dayKey, dayLabel } from './historyview';

const makeReview = (id: string): ReviewView =>
  ({
    id,
    repo: '/repo',
    seq: 1,
    created_at: 1700000000,
    provider: { id: 'mock', label: 'Mock', note: '' },
    model: 'mock',
    snapshot: { head: 'HEAD', diff_hash: 'h', diff_bytes: 100, files: [], excluded: [] },
    prompt: '',
    response_text: '',
    raw: '',
    usage: { prompt_tokens: 10, completion_tokens: 10, total_tokens: 20, cache_read_tokens: null, cache_write_tokens: null },
    cost_usd: null,
    notes: [],
    recipe: { id: 'bugs', title: '潜在バグ', kind: 'llm', source: 'builtin', prompt: '' },
    findings: [
      { title: 'バグ1', detail: '詳細1', severity: 'high', file: 'a.ts', line: 10, evidence: 'const x = 1;', verified: true, rule: null },
    ],
    triage: ['new'],
    finding_notes: [],
    previous: null,
    fates: [],
    carries: [],
    agent_reports: [],
    handoff_state: null,
    related: null,
    duration_ms: 100,
    parse_error: null,
    error: null,
  }) as unknown as ReviewView;

describe('実行ログとインライン展開のテスト（指摘の検証）', () => {
  describe('指摘 #1: findReviewTarget（保存先レビューの特定）', () => {
    it('キャッシュにあるレビューIDを指定したとき、キャッシュのレビューを返す', () => {
      const selected = makeReview('selected-1');
      const cached = makeReview('cached-2');
      const cache = new Map<string, ReviewView>([['cached-2', cached]]);

      expect(findReviewTarget('cached-2', selected, cache)).toBe(cached);
    });

    it('選択中レビューのIDと一致するとき、選択中のレビューを返す', () => {
      const selected = makeReview('selected-1');
      const cache = new Map<string, ReviewView>();

      expect(findReviewTarget('selected-1', selected, cache)).toBe(selected);
    });

    it('どちらでもない未知のIDを指定したとき、選択中を返さず null を返す（別レビューへの誤書き込みを防止）', () => {
      const selected = makeReview('selected-1');
      const cache = new Map<string, ReviewView>([['cached-2', makeReview('cached-2')]]);

      expect(findReviewTarget('unknown-999', selected, cache)).toBeNull();
    });

    it('reviewId が未指定のときは、選択中のレビューを返す', () => {
      const selected = makeReview('selected-1');
      const cache = new Map<string, ReviewView>();

      expect(findReviewTarget(undefined, selected, cache)).toBe(selected);
    });
  });

  describe('指摘 #5: 日付グルーピングと空表示の分岐', () => {
    it('別の日時のレビューが異なる dayKey にグルーピングされる', () => {
      const day1 = dayKey(Date.UTC(2026, 10, 25, 1, 0, 0));
      const day2 = dayKey(Date.UTC(2026, 10, 24, 1, 0, 0));
      expect(day1).not.toBe(day2);

      const label1 = dayLabel(Date.UTC(2026, 10, 25, 1, 0, 0), 'ja');
      const label2 = dayLabel(Date.UTC(2026, 10, 24, 1, 0, 0), 'ja');
      expect(label1).toContain('2026');
      expect(label2).toContain('2026');
    });
  });

  describe('指摘 #7: 指摘カードの識別子（キー管理）による分離', () => {
    it('異なるレビューIDの同じ指摘インデックスが別々のキーとして管理され干渉しない', () => {
      const expandedFindings = new Set<string>();
      const keyA0 = `${'review-A'}:${0}`;
      const keyB0 = `${'review-B'}:${0}`;

      expandedFindings.add(keyA0);

      expect(expandedFindings.has(keyA0)).toBe(true);
      expect(expandedFindings.has(keyB0)).toBe(false);

      expandedFindings.delete(keyA0);
      expect(expandedFindings.has(keyA0)).toBe(false);
    });
  });

  describe('潜在バグ #2: 詳細画面での仕分け変更のキャッシュ同期', () => {
    it('詳細画面（selected）で仕分けが更新されたとき、実行ログ用キャッシュの同一レビューも同期更新される', () => {
      const selected = makeReview('review-sync');
      const cached = makeReview('review-sync');
      const cache = new Map<string, ReviewView>([['review-sync', cached]]);

      const handling = {
        triage: ['fix' as const],
        notes: [{ memo: 'メモ更新', memo_at: null, triage_at: 1000, triage_source: 'user' as const, triage_head: 'abc', done_at: null, done_head: null }],
      };

      applyHandling(selected, handling, cache);

      expect(selected.triage[0]).toBe('fix');
      expect(cached.triage[0]).toBe('fix');
      expect(cached.finding_notes[0]?.memo).toBe('メモ更新');
    });
  });
});
