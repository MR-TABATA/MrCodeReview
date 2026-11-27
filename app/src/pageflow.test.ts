// @vitest-environment jsdom
// 上部タブでページを切り替える流れ（タブを押す → state.page → 描画）を、画面の DOM で確かめる。
import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: vi.fn(), message: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { render, select, state } from './main';
import { newHookUi } from './hookview';
import type { ReviewView } from './types';

const fakeReview = (id: string): ReviewView =>
  ({
    id,
    repo: '/r',
    recipe: { id: 'bug', title: 'バグ', source: 'builtin', prompt: '', kind: 'llm' },
    provider: { id: 'mock', name: 'Mock', label: 'Mock', note: '', models: [] },
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
  }) as ReviewView;

const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
  await new Promise((r) => setTimeout(r, 0));
};

const tab = (page: string) => document.querySelector<HTMLElement>(`.pagetabs [data-page="${page}"]`)!;
const selected = () => [...document.querySelectorAll('.pagetabs [aria-selected="true"]')].map((b) => (b as HTMLElement).dataset.page);

describe('上部タブでページを切り替える', () => {
  beforeEach(() => {
    document.body.innerHTML = '<div id="app"></div>';
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(null);
    state.repo = '/r';
    state.settings = null;
    state.usage = null;
    state.report = null;
    state.selected = null;
    state.hook = newHookUi();
    state.page = 'newreview';
    state.backTo = 'history';
    state.history = [];
    state.expandedHookRuns.clear();
  });

  const summary = (id: string, over: Record<string, unknown> = {}) => ({
    id,
    created_at: 1_795_775_717_000,
    repo: '/r',
    recipe_title: 'バグ',
    provider_label: 'Mock',
    findings: 2,
    verified: 2,
    total_tokens: null,
    failed: false,
    counts: { open: 2, done: 0, wait: 0, dismiss: 0 },
    head: 'abcdef0123456',
    ...over,
  });
  const hookRun = (over: Record<string, unknown> = {}) => ({
    run: 'h1',
    started_at_ms: 1_795_775_800_000,
    commit: 'abc1234',
    subject: '件名',
    recipes: ['bugs'],
    steps: [{ recipe: 'bugs', index: 1, total: 1, ok: true, findings: 1, ms: 2000, error: null }],
    state: 'done',
    code: 1,
    message: '完了',
    findings: 1,
    ms: 2000,
    ...over,
  });
  const cols = (li: Element) => [...li.children].map((c) => c.className.split(' ')[0]);

  it('タブは 3 つ（リポジトリ・履歴・ログ・新規レビュー）。「ログ」だけのタブは無い', () => {
    render();
    expect([...document.querySelectorAll('.pagetabs button')].map((b) => b.textContent)).toEqual(['リポジトリ', '履歴・ログ', '新規レビュー']);
  });

  it('新規レビューのページ: 実行する場所だけ。結果も、履歴も、ここには出ない', () => {
    render();
    expect(selected()).toEqual(['newreview']);
    expect(document.querySelector('.page-title')?.textContent).toBe('新規レビュー');
    expect(document.querySelector('.history')).toBeNull();
    expect(document.querySelector('.back-to-log-btn')).toBeNull();
    expect(document.querySelector('.recipe-bar-top')).not.toBeNull();
    expect(document.querySelector('.left')).toBeNull();
    expect(document.querySelector('.right')).toBeNull();
  });

  it('各タブを押すと、そのページの中身に変わる（左右の 3 カラムは、もう無い）', () => {
    render();
    tab('repos').click();
    expect(state.page).toBe('repos');
    expect(selected()).toEqual(['repos']);
    expect(document.querySelector('.page-aside > .left')).not.toBeNull();
    expect(document.querySelector('.right')).toBeNull();

    tab('history').click();
    expect(state.page).toBe('history');
    expect(document.querySelector('.page-aside > .right')).not.toBeNull();
    expect(document.querySelector('.left')).toBeNull();

    tab('newreview').click();
    expect(state.page).toBe('newreview');
    expect(document.querySelector('.right')).toBeNull();
  });

  it('3 つのページは、どれも、先頭に、同じ見出し（ページ名）が付く', () => {
    const titles: Record<string, string> = { repos: 'リポジトリ', history: '履歴・ログ', newreview: '新規レビュー' };
    for (const page of ['repos', 'history', 'newreview'] as const) {
      state.page = page;
      render();
      const heads = document.querySelectorAll('.detail.page > .page-head');
      expect(heads.length, page).toBe(1);
      expect(heads[0].querySelector('.page-title')?.textContent, page).toBe(titles[page]);
      expect(document.querySelector('.detail.page')?.firstElementChild, page).toBe(heads[0]);
    }
  });

  it('実行した直後の結果は、新規レビューの実行バーの下に出る。「結果を閉じる」で、新規レビューのまま、結果だけが消える', () => {
    state.selected = fakeReview('run1');
    state.backTo = 'newreview';
    state.page = 'newreview';
    render();
    expect(selected()).toEqual(['newreview']);
    expect(document.querySelector('.recipe-bar-top')).not.toBeNull();
    expect(document.querySelector('.result-anchor .d-head')).not.toBeNull();
    expect(document.querySelector('.result-anchor nav.tabs')).not.toBeNull(); // 実行直後は、これまでどおり、タブがある
    const close = document.querySelector<HTMLElement>('.back-to-log-btn')!;
    expect(close.textContent).toBe('✕ 結果を閉じる');
    close.click();
    expect(state.page).toBe('newreview');
    expect(state.selected).toBeNull();
    expect(document.querySelector('.result-anchor')).toBeNull();
    expect(document.querySelector('.recipe-bar-top')).not.toBeNull();
  });

  it('履歴から開いた結果は、新規レビューのタブには出ない（実行する場所に、過去の結果を混ぜない）', () => {
    state.selected = fakeReview('old1');
    state.backTo = 'history';
    state.page = 'newreview';
    render();
    expect(document.querySelector('.result-anchor')).toBeNull();
    expect(document.querySelector('.recipe-bar-top')).not.toBeNull();
  });

  describe('「履歴・ログ」のページ', () => {
    it('フックの実行（上）と、レビューの履歴（下）が、同じ書式の行（同じ列の並び）で並ぶ', () => {
      state.history = [summary('a')] as never;
      state.hook.status = { recent: [hookRun()] } as never;
      state.page = 'history';
      render();
      const hookRow = document.querySelector('.hookruns-section ul.history > li')!;
      const reviewRow = document.querySelector('.right ul.history > li')!;
      expect(hookRow.classList.contains('hookrun')).toBe(true);
      expect(hookRow.querySelector('.badge')?.textContent).toBe('フック');
      // 列の並びは同じ。右端の欄だけ、中身が違う（レビューは仕分けの件数、フックは所要時間）
      expect(cols(hookRow).slice(0, -1)).toEqual(cols(reviewRow).slice(0, -1));
      expect(cols(hookRow).slice(0, -1)).toEqual(['hlead', 'hwhen', 'h1', 'hcommit', 'hfind']);
      expect(reviewRow.lastElementChild?.classList.contains('hcounts')).toBe(true);
      expect(hookRow.lastElementChild?.classList.contains('hdur')).toBe(true);
      expect(hookRow.querySelector('.hcommit')?.textContent).toBe('abc1234');
      expect(reviewRow.querySelector('.hcommit')?.textContent).toBe('abcdef0');
      expect(hookRow.querySelector('.hcounts-empty')).not.toBeNull();
      // フックの行には、チェックが無い。レビューの行には、ある
      expect(hookRow.querySelector('.hlead .pick')).toBeNull();
      expect(reviewRow.querySelector('.hlead .pick')).not.toBeNull();
    });

    it('フックの行を押すと、内訳がその場で開く。もう一度で、閉じる', () => {
      state.hook.status = { recent: [hookRun()] } as never;
      state.page = 'history';
      render();
      document.querySelector<HTMLElement>('.hookruns-section li')!.click();
      expect(document.querySelector('.hookrun-detail')).not.toBeNull();
      document.querySelector<HTMLElement>('.hookruns-section li')!.click();
      expect(document.querySelector('.hookrun-detail')).toBeNull();
    });

    it('フックの実行が無ければ、フックの欄は出ない', () => {
      state.history = [summary('a')] as never;
      state.page = 'history';
      render();
      expect(document.querySelector('.hookruns-section')).toBeNull();
    });

    it('読めない行があれば、その知らせが出る（フックの実行が 0 件でも）', () => {
      state.hook.status = { recent: [], log_unreadable: 3 } as never;
      state.page = 'history';
      render();
      expect(document.querySelector('.hooklog-unreadable')?.textContent).toContain('3');
    });

    it('ページを開くと、フックの状態と実行ログを読む（設定を開いていなくても）。「更新」でも読み直す', async () => {
      invokeMock.mockImplementation(async (cmd: string) => (cmd === 'hook_status' ? { recent: [hookRun()], log_unreadable: 0 } : cmd === 'list_reviews' ? [] : null));
      state.page = 'newreview';
      render();
      const calls = () => invokeMock.mock.calls.filter((c) => c[0] === 'hook_status').length;
      expect(calls()).toBe(0); // 新規レビューでは、読まない
      tab('history').click();
      await flush();
      expect(calls()).toBe(1);
      expect(document.querySelector('.hookruns-section li.hookrun')).not.toBeNull();
      [...document.querySelectorAll<HTMLElement>('.page-head button')].find((b) => b.textContent === '更新')!.click();
      await flush();
      expect(calls()).toBe(2);
      state.page = 'newreview'; // ポーリングを止める
    });
  });

  describe('履歴から開いた結果（掘り下げのページ）', () => {
    it('見出しは「履歴・ログ」で、そのタブが選ばれたまま。タブ（指摘・差分・記録）も、「すべて開く」「取り込む」の行も、まとめて渡す欄も無い', () => {
      state.selected = fakeReview('x');
      state.backTo = 'history';
      state.page = 'result';
      render();
      expect(selected()).toEqual(['history']);
      expect(document.querySelectorAll('.pagetabs button').length).toBe(3);
      expect(document.querySelector('.page-title')?.textContent).toBe('履歴・ログ');
      expect(document.querySelector('.recipe-bar-top')).toBeNull();
      expect(document.querySelector('.result-compact')).not.toBeNull();
      expect(document.querySelector('.result nav.tabs')).toBeNull();
      expect(document.querySelector('.result')?.textContent).not.toContain('すべて開く');
      expect(document.querySelector('.result')?.textContent).not.toContain('まとめて CLI に渡す');
      expect(document.querySelector('.back-to-log-btn')?.textContent).toBe('← 履歴・ログ');
    });

    it('結果が無くなれば（履歴から消した・リポジトリを替えた）、履歴・ログへ戻る', () => {
      state.selected = fakeReview('x');
      state.backTo = 'history';
      state.page = 'result';
      render();
      state.selected = null;
      render();
      expect(state.page).toBe('history');
      expect(selected()).toEqual(['history']);
    });

    it('下に、折りたたみの「実行の記録」。開いたときにだけ、中身（実行・入力・差分）を作る。差分も、その欄を開いたときにだけ', () => {
      const r = { ...fakeReview('x'), prompt: '前置き\n\n## 差分\n\n```diff\n+追加した行\n```\n' } as ReviewView;
      state.selected = r;
      state.backTo = 'history';
      state.page = 'result';
      render();
      const det = document.querySelector('details.rld-record') as HTMLDetailsElement;
      expect(det.querySelector('.rld')).toBeNull(); // 閉じているあいだは、作らない
      det.open = true;
      det.dispatchEvent(new Event('toggle'));
      const text = det.textContent ?? '';
      expect(text).toContain('実行');
      expect(text).toContain('入力');
      expect(text).toContain('比較元');
      expect(det.querySelector('.rld-diff-pre'), '差分の欄は、まだ閉じている').toBeNull();
      const diff = det.querySelector('details.rld-diff') as HTMLDetailsElement;
      diff.open = true;
      diff.dispatchEvent(new Event('toggle'));
      expect(det.querySelector('.rld-diff-pre')?.textContent).toContain('+追加した行');
      // 指摘（結果）は、ここには出ない（結果のページ本体にある）
      expect(det.querySelector('.rld-finding')).toBeNull();
    });
  });

  it('設定を開いているあいだは、どのタブも選ばれない。タブを押すと、設定が閉じて、そのページへ移る', () => {
    state.settings = {
      view: { values: { language: 'ja' }, meta: [], path: '/d/settings.json', exists: false, error: null, repo: { path: '/r', keys: [], error: null } },
      draft: {},
      confirmClose: false,
    } as never;
    render();
    expect(selected()).toEqual([]);
    tab('history').click();
    expect(state.settings).toBeNull();
    expect(state.page).toBe('history');
    expect(selected()).toEqual(['history']);
  });

  it('履歴の行を押すと、結果のページが開き、戻るで、履歴・ログへ戻る', async () => {
    invokeMock.mockImplementation(async (cmd: string, args: { id?: string }) => (cmd === 'get_review' ? fakeReview(args.id ?? 'x') : cmd === 'build_handoff' ? '' : null));
    state.history = [summary('a')] as never;
    state.page = 'history';
    render();
    document.querySelector<HTMLElement>('.right ul.history li .ht')!.click();
    await flush();
    expect(state.page).toBe('result');
    expect(state.selected?.id).toBe('a');
    const back = document.querySelector<HTMLElement>('.back-to-log-btn')!;
    expect(back.textContent).toBe('← 履歴・ログ');
    back.click();
    expect(state.page).toBe('history');
    expect(state.selected).toBeNull();
  });

  describe('結果を開いたときの、戻り先', () => {
    const setup = () =>
      invokeMock.mockImplementation(async (cmd: string, args: { id?: string }) => {
        if (cmd === 'get_review') return fakeReview(args.id ?? 'x');
        if (cmd === 'build_handoff') return '';
        return null;
      });

    it('リポジトリのページ（履歴でも新規レビューでもない所）から開くと、結果のページになり、戻り先は履歴・ログ', async () => {
      setup();
      state.page = 'repos';
      state.backTo = 'newreview';
      await select('a');
      expect([state.page, state.backTo]).toEqual(['result', 'history']);
    });

    it('履歴・ログから開くと、結果のページになり、戻り先は履歴・ログ', async () => {
      setup();
      state.page = 'history';
      await select('a');
      expect([state.page, state.backTo]).toEqual(['result', 'history']);
    });

    it('結果のページの中で、別のレビュー（「前回」など）へ移っても、戻り先は変わらない', async () => {
      setup();
      state.page = 'result';
      state.backTo = 'history';
      await select('prev');
      expect([state.page, state.backTo]).toEqual(['result', 'history']);
    });

    it('新規レビューの下の結果から、別のレビューへ移っても、新規レビューのまま（結果のページに移らない）', async () => {
      setup();
      state.page = 'newreview';
      state.backTo = 'newreview';
      await select('prev');
      expect([state.page, state.backTo]).toEqual(['newreview', 'newreview']);
    });
  });
});
