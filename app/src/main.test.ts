// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}));
vi.mock('@tauri-apps/plugin-dialog', () => ({
  confirm: vi.fn(),
  message: vi.fn(),
  open: vi.fn(),
  save: vi.fn(),
}));

import { fixAllBySeverity, handoffBar, state } from './main';
import { getAppLang, msg } from './i18n';
import type { ReviewView, Finding, Handling, Note } from './types';

const makeFinding = (severity: string, title = 'finding'): Finding => ({
  severity,
  title,
  file: 'test.ts',
  line: 1,
  detail: 'detail',
  evidence: 'evidence',
  verified: true,
  rule: null,
});

const makeNote = (): Note => ({
  memo: '',
  memo_at: null,
  done_at: null,
  done_head: null,
  triage_at: null,
  triage_head: null,
  triage_source: null,
});

const makeReview = (over: Partial<ReviewView> = {}): ReviewView =>
  ({
    id: 'rev-test',
    repo: '/repo',
    recipe: { id: 'bug', title: 'バグ', source: 'builtin', prompt: '', kind: 'llm' },
    provider: { id: 'mock', name: 'Mock', models: [] },
    model: 'mock',
    created_at: 1000,
    duration_ms: 50,
    prompt: '',
    response_text: '',
    raw: '',
    usage: { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 },
    cost_usd: null,
    carries: [],
    fates: [],
    notes: [],
    related: null,
    parse_error: null,
    error: null,
    snapshot: { head: 'abc', diff_hash: '123', diff_bytes: 10, files: [], excluded: [] },
    findings: [],
    triage: [],
    finding_notes: [],
    agent_reports: [],
    ...over,
  }) as ReviewView;

describe('一括「対応」と handoffBar（レビュー指摘の修正）', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    state.runError = '';
    state.selected = null;
    state.expanded = new Set();
    document.body.innerHTML = '<div id="app"></div>';
  });

  describe('mrcr:1793608390178-43127d18#1: 一括「対応」が途中で失敗したときの挙動', () => {
    it('high 3件中2件目が失敗したとき、1件目のみfixになり、state.runErrorにエラーが入り、3件目は呼ばれず、refreshHandoffとrenderは呼ばれる', async () => {
      const review = makeReview({
        id: 'rev-1',
        findings: [makeFinding('high', 'h1'), makeFinding('high', 'h2'), makeFinding('high', 'h3')],
        triage: ['new', 'new', 'new'],
        finding_notes: [makeNote(), makeNote(), makeNote()],
      });
      state.selected = review;

      invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
        if (cmd === 'set_triage') {
          if (args.index === 0) {
            return Promise.resolve<Handling>({
              triage: ['fix', 'new', 'new'],
              notes: [makeNote(), makeNote(), makeNote()],
            });
          }
          if (args.index === 1) {
            return Promise.reject(new Error('2件目のset_triageで障害が発生'));
          }
          return Promise.resolve<Handling>({
            triage: ['fix', 'fix', 'fix'],
            notes: [makeNote(), makeNote(), makeNote()],
          });
        }
        if (cmd === 'build_handoff') {
          return Promise.resolve('handoff prompt text');
        }
        return Promise.resolve();
      });

      await fixAllBySeverity('high');

      // 1件目だけ triage が fix になる
      expect(state.selected.triage[0]).toBe('fix');
      expect(state.selected.triage[1]).toBe('new');
      expect(state.selected.triage[2]).toBe('new');

      // state.runError にエラーの文字列が入る
      expect(state.runError).toContain('2件目のset_triageで障害が発生');

      // 3件目の invoke は呼ばれない（index 0 と 1 のみ呼ばれる）
      const setTriageCalls = invokeMock.mock.calls.filter((c) => c[0] === 'set_triage');
      expect(setTriageCalls).toHaveLength(2);
      expect(setTriageCalls[0][1]).toEqual({ id: 'rev-1', index: 0, status: 'fix' });
      expect(setTriageCalls[1][1]).toEqual({ id: 'rev-1', index: 1, status: 'fix' });

      // refreshHandoff と render は呼ばれる（build_handoff の invoke と #app へのレンダリング確認）
      expect(invokeMock).toHaveBeenCalledWith('build_handoff', { id: 'rev-1' });
      expect(state.handoff).toBe('handoff prompt text');
      expect(document.getElementById('app')?.children.length).toBeGreaterThan(0);
    });
  });

  describe('mrcr:1793608390178-43127d18#2: 一括「対応」の対象の絞り込み（重大度・対応済み・triage 未設定）', () => {
    it('findings が [high(triage 未設定), high(fix), medium(new), low(new)] のとき、fixAllBySeverity("high") で set_triage が index 0 で 1 回だけ呼ばれる', async () => {
      const review = makeReview({
        id: 'rev-filter',
        findings: [
          makeFinding('high', 'high-unassigned'),
          makeFinding('high', 'high-already-fix'),
          makeFinding('medium', 'medium-new'),
          makeFinding('low', 'low-new'),
        ],
        // index 0 は undefined（triage 未設定）、index 1 はすでに 'fix'
        triage: [undefined as unknown as string, 'fix', 'new', 'new'] as any,
        finding_notes: [makeNote(), makeNote(), makeNote(), makeNote()],
      });
      state.selected = review;

      invokeMock.mockImplementation((cmd: string) => {
        if (cmd === 'set_triage') {
          return Promise.resolve<Handling>({
            triage: ['fix', 'fix', 'new', 'new'],
            notes: [makeNote(), makeNote(), makeNote(), makeNote()],
          });
        }
        if (cmd === 'build_handoff') return Promise.resolve('');
        return Promise.resolve();
      });

      await fixAllBySeverity('high');

      const setTriageCalls = invokeMock.mock.calls.filter((c) => c[0] === 'set_triage');
      // set_triage が index 0 で 1 回だけ呼ばれる
      expect(setTriageCalls).toHaveLength(1);
      expect(setTriageCalls[0][1]).toEqual({ id: 'rev-filter', index: 0, status: 'fix' });
    });
  });

  describe('mrcr:1793608390178-43127d18#3: 重大度ごとのボタンの件数と、出す・出さないの条件', () => {
    it('[high(new), high(fix), medium(fix), low(new)] で handoffBar(r, true) を描くとボタンは 2 つ（high を「対応」に・low を「無視」に）で、withExpandAll=false のときはボタンが 0 個', () => {
      const m = msg(getAppLang()).mainUi;
      const review = makeReview({
        findings: [
          makeFinding('high', 'h-new'),
          makeFinding('high', 'h-fix'),
          makeFinding('medium', 'm-fix'),
          makeFinding('low', 'l-new'),
        ],
        triage: ['new', 'fix', 'fix', 'new'],
        finding_notes: [makeNote(), makeNote(), makeNote(), makeNote()],
      });

      // withExpandAll = true
      const barWithExpand = handoffBar(review, true);
      const allButtons = [...barWithExpand.querySelectorAll('button')];
      // 重大度別の一括ボタン（high, medium, low）のみを抽出
      const sevButtons = allButtons.filter((b) =>
        ['high', 'medium', 'low'].some((sev) => b.textContent?.includes(sev) || b.textContent === m.fixAllSevBtn(sev, 1)),
      );

      // high を「対応」に（1）と、low を「無視」に（1）の 2 つ。medium は、すでに「対応」なので出ない
      expect(sevButtons.map((b) => b.textContent)).toEqual([m.fixAllSevBtn('high', 1), m.dismissAllSevBtn('low', 1)]);

      // withExpandAll = false
      const barWithoutExpand = handoffBar(review, false);
      const allButtonsNoExpand = [...barWithoutExpand.querySelectorAll('button')];
      const sevButtonsNoExpand = allButtonsNoExpand.filter((b) =>
        ['high', 'medium', 'low'].some((sev) => b.textContent?.includes(sev) || b.textContent === m.fixAllSevBtn(sev, 1)),
      );

      // 同じデータで withExpandAll=false のときは、ボタンが 0 個
      expect(sevButtonsNoExpand).toHaveLength(0);
    });
  });

  describe('mrcr:1793608390178-43127d18#4: 重大度ボタンだけがあるときにツール欄（.fc-tools）を出すテスト', () => {
    it('expandAll が null（指摘1件）で info.fix が 0 のとき、重大度ボタンがあれば .fc-tools が描画されボタンが含まれる', () => {
      const m = msg(getAppLang()).mainUi;
      const review = makeReview({
        findings: [makeFinding('high', 'h-1')],
        triage: ['new'],
        finding_notes: [makeNote()],
      });

      // withExpandAll = true だが findings.length === 1 なので expandAll は null。
      // info.fix = 0 なので fetchBar も null。
      // sevBtns には high のボタンが 1 つ入る。
      const bar = handoffBar(review, true);
      const tools = bar.querySelector('.fc-tools');
      expect(tools).not.toBeNull();

      const btn = tools?.querySelector('button');
      expect(btn).not.toBeNull();
      expect(btn?.textContent).toBe(m.fixAllSevBtn('high', 1));
    });
  });
});
