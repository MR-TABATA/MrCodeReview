// @vitest-environment jsdom
// 上部タブでページを切り替える流れ（タブを押す → state.page → 描画）を、画面の DOM で確かめる。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: vi.fn(), message: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { changeRepo, pollOpenReviewReports, REPORT_POLL_MS, refreshHistory, loadHook, loadRepoStatus, render, select, state, wantsHook } from './main';
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
    // 既定の答え（リポジトリのページを開くと、状態・データ量・ブランチ・フックを読みに行く）。ほかは、null
    const base: Record<string, unknown> = { repo_status: [], repo_data: [], repo_branches: {}, repo_hooks: {}, list_reviews: [] };
    invokeMock.mockImplementation(async (cmd: string) => base[cmd] ?? null);
    state.repo = '/r';
    state.settings = null;
    state.usage = null;
    state.report = null;
    state.selected = null;
    state.hook = newHookUi();
    state.page = 'newreview';
    state.history = [];
    state.repoBranches = {};
    state.historyQuery = '';
    state.picked = new Set();
    state.historyLimit = 100;
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

  it('タブは 4 つ（リポジトリ・観点・履歴・ログ・新規レビュー）。「ログ」だけのタブは無い', () => {
    render();
    expect([...document.querySelectorAll('.pagetabs button')].map((b) => b.textContent)).toEqual(['リポジトリ', '観点', '履歴・ログ', '新規レビュー']);
  });

  it('ヘッダーのリポジトリの横に、いまのブランチ名が出る。無い（detached HEAD・Git ではない）ときは、出ない', () => {
    state.repoBranches = { '/r': 'feature/x-y' };
    render();
    expect(document.querySelector('.topbar .repo-branch')?.textContent).toBe('⎇ feature/x-y');
    state.repoBranches = { '/r': null };
    render();
    expect(document.querySelector('.topbar .repo-branch')).toBeNull();
    state.repoBranches = {};
    render();
    expect(document.querySelector('.topbar .repo-branch')).toBeNull();
  });

  it('リポジトリのページの行を押すと、そのリポジトリを選んで、「履歴・ログ」のページへ移る', async () => {
    const answers: Record<string, unknown> = {
      list_reviews: [],
      hook_status: { recent: [], log_unreadable: 0 },
      list_recipes: { recipes: [], warnings: [] },
      list_bases: [],
      preview_snapshot: null,
      find_repo_icon: null,
      repo_status: [],
      repo_data: [],
      repo_branches: {},
      repo_hooks: {},
    };
    invokeMock.mockImplementation(async (cmd: string) => answers[cmd] ?? null);
    state.registeredRepos = [
      { id: '1', path: '/r' },
      { id: '2', path: '/other' },
    ];
    state.page = 'repos';
    render();
    const rows = document.querySelectorAll<HTMLElement>('.page-aside .repo-item');
    expect(rows.length).toBe(2);
    rows[1].click();
    await flush();
    expect(state.repo).toBe('/other');
    expect(state.page).toBe('history');
    expect(selected()).toEqual(['history']);
    state.page = 'newreview'; // ポーリングを止める
    state.registeredRepos = [];
  });

  describe('リポジトリのページ', () => {
    const answers = (over: Record<string, unknown> = {}): Record<string, unknown> => ({
      list_reviews: [],
      hook_status: { recent: [], log_unreadable: 0 },
      list_recipes: { recipes: [], warnings: [] },
      list_bases: [],
      preview_snapshot: null,
      find_repo_icon: null,
      repo_status: [{ path: '/r', exists: true, is_git: true }],
      repo_data: [{ path: '/r', reviews: 2, findings: 4, triaged: 0, memos: 0, open: 4, last_at: 1_795_775_717_000 }],
      repo_branches: { '/r': 'main', '/other': 'dev' },
      repo_hooks: { '/r': { enabled: true, installed: true }, '/other': { enabled: false, installed: false } },
      ...over,
    });
    const setup = (over: Record<string, unknown> = {}) => {
      const a = answers(over);
      invokeMock.mockImplementation(async (cmd: string) => {
        const v = a[cmd];
        if (v instanceof Error) throw v;
        return v ?? null;
      });
      state.registeredRepos = [
        { id: '1', path: '/r' },
        { id: '2', path: '/other' },
      ];
      state.repoAuxErrors = [];
      state.repoMsg = { text: '', kind: '' };
    };
    const callsOf = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
    afterEach(() => {
      state.registeredRepos = [];
      state.repoBranches = {};
      state.repoHooks = {};
      state.repoAuxErrors = [];
      state.repoMsg = { text: '', kind: '' };
      state.page = 'newreview';
    });

    it('行を押したとき、切り替えに失敗したら、ページは変えず、理由を出す（前のリポジトリの履歴を、新しい名前で出さない）', async () => {
      setup({ list_reviews: new Error('履歴を読めません') });
      state.page = 'repos';
      render();
      document.querySelectorAll<HTMLElement>('.page-aside .repo-item')[1].click();
      await flush();
      expect(state.page, 'ページは、そのまま').toBe('repos');
      expect(selected()).toEqual(['repos']);
      const err = document.querySelector('.repo-page-error');
      expect(err?.textContent).toContain('切り替えられませんでした');
      expect(err?.textContent).toContain('履歴を読めません');
    });

    it('切り替えが成功したら、終わってから、履歴・ログへ移る。前の失敗の表示は、消える', async () => {
      setup();
      state.repoMsg = { text: '前の失敗', kind: 'err' };
      state.page = 'repos';
      render();
      document.querySelectorAll<HTMLElement>('.page-aside .repo-item')[1].click();
      await flush();
      expect(state.repo).toBe('/other');
      expect(state.page).toBe('history');
      expect(state.repoMsg.text).toBe('');
      state.page = 'newreview';
    });

    it('ブランチ名だけ読めなくても、状態・データ量・フックの更新は、止まらない。前のブランチ名は残り、理由が出る', async () => {
      setup({ repo_branches: new Error('git を起動できません') });
      state.repoBranches = { '/r': 'old-branch' };
      state.page = 'repos';
      render();
      await loadRepoStatus(false);
      expect(state.repoData['/r'].open, '主要なデータは、更新される').toBe(4);
      expect(state.repoHooks['/r']?.installed, 'フックも、更新される').toBe(true);
      expect(state.repoBranches['/r'], '前のブランチ名は、残る').toBe('old-branch');
      expect(state.repoAuxErrors.some((e) => e.includes('ブランチ名') && e.includes('git を起動できません'))).toBe(true);
      expect(document.querySelector('.repo-page-error')?.textContent).toContain('git を起動できません');
    });

    it('フックの状態だけ読めなくても、ブランチ名・データ量は、更新される。前のフックの状態は残り、理由が出る', async () => {
      setup({ repo_hooks: new Error('設定ファイルが壊れています') });
      state.repoHooks = { '/r': { enabled: false, installed: false } };
      state.page = 'repos';
      render();
      await loadRepoStatus(false);
      expect(state.repoBranches['/r']).toBe('main');
      expect(state.repoData['/r'].reviews).toBe(2);
      expect(state.repoHooks['/r'], '前の値は、残る').toEqual({ enabled: false, installed: false });
      expect(state.repoAuxErrors.some((e) => e.includes('Git フック') && e.includes('壊れています'))).toBe(true);
      // 次に読めたら、理由は、消える
      invokeMock.mockImplementation(async (cmd: string) => answers()[cmd] ?? null);
      await loadRepoStatus(false);
      expect(state.repoAuxErrors).toEqual([]);
      expect(state.repoHooks['/r']?.installed).toBe(true);
    });

    it('窓に戻ったときの読み直しは、リポジトリのページ以外では、選んだ 1 つのブランチだけ。フックの状態は、読まない', async () => {
      setup();
      state.repo = '/r';
      state.page = 'newreview';
      render();
      invokeMock.mockClear();
      await loadRepoStatus(false);
      expect(callsOf('repo_branches').map((c) => (c[1] as { paths: string[] }).paths)).toEqual([['/r']]);
      expect(callsOf('repo_hooks').length, 'フックの状態は、読まない').toBe(0);
      state.page = 'repos';
      render(); // ページが変わった描き直しで、1 回、読む
      await flush();
      invokeMock.mockClear();
      await loadRepoStatus(false);
      expect(callsOf('repo_branches').map((c) => (c[1] as { paths: string[] }).paths)).toEqual([['/r', '/other']]);
      expect(callsOf('repo_hooks').map((c) => (c[1] as { paths: string[] }).paths)).toEqual([['/r', '/other']]);
    });
  });

  it('新規レビューのページ: 実行する場所だけ。結果も、履歴も、ここには出ない', () => {
    render();
    expect(selected()).toEqual(['newreview']);
    expect(document.querySelector('.page-title')?.textContent).toBe('新規レビュー');
    expect(document.querySelector('.history')).toBeNull();
    expect(document.querySelector('.back-to-log-btn')).toBeNull();
    expect(document.querySelector('.run-form, .recipe-bar-top')).not.toBeNull();
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
    state.page = 'newreview';
    render();
    expect(selected()).toEqual(['newreview']);
    expect(document.querySelector('.run-form, .recipe-bar-top')).not.toBeNull();
    expect(document.querySelector('.result-anchor .d-head')).not.toBeNull();
    expect(document.querySelector('.result-anchor nav.tabs')).not.toBeNull(); // 実行直後は、これまでどおり、タブがある
    const close = document.querySelector<HTMLElement>('.back-to-log-btn')!;
    expect(close.textContent).toBe('結果を閉じる'); // 先頭の「✕」は、アイコン
    expect(close.title).toBe('✕ 結果を閉じる');
    close.click();
    expect(state.page).toBe('newreview');
    expect(state.selected).toBeNull();
    expect(document.querySelector('.result-anchor')).toBeNull();
    expect(document.querySelector('.run-form, .recipe-bar-top')).not.toBeNull();
  });

  it('ページを変えたら、開いている結果は閉じる。新規レビューのタブに、履歴で開いた結果は出ない', () => {
    state.history = [summary('old1')] as never;
    state.selected = fakeReview('old1');
    state.page = 'history';
    render();
    expect(document.querySelector('.hdetail')).not.toBeNull();
    tab('newreview').click();
    expect(state.selected).toBeNull();
    expect(document.querySelector('.result-anchor')).toBeNull();
    expect(document.querySelector('.hdetail')).toBeNull();
    expect(document.querySelector('.run-form, .recipe-bar-top')).not.toBeNull();
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

  describe('フックの状態を読むタイミング', () => {
    const hookCalls = () => invokeMock.mock.calls.filter((c) => c[0] === 'hook_status').length;
    const okImpl = async (cmd: string) => (cmd === 'hook_status' ? { recent: [hookRun()], log_unreadable: 0 } : cmd === 'list_reviews' ? [] : null);

    afterEach(() => {
      vi.useRealTimers();
      state.page = 'newreview'; // ポーリングを止める
      state.settings = null;
      state.usage = null;
      state.report = null;
    });

    it('設定を開いているか、「履歴・ログ」のページを開いている（レポート・使用量のモードではない）ときだけ、読む', () => {
      state.page = 'newreview';
      expect(wantsHook(), '新規レビューを開いているだけでは、読まない').toBe(false);
      state.page = 'history';
      expect(wantsHook()).toBe(true);
      state.report = {} as never;
      expect(wantsHook(), 'レポートを開いているあいだは、読まない').toBe(false);
      state.report = null;
      state.usage = {} as never;
      expect(wantsHook(), '使用量を開いているあいだは、読まない').toBe(false);
      state.usage = null;
      state.page = 'newreview';
      state.settings = { view: {}, draft: {} } as never;
      expect(wantsHook(), '設定を開いているときは、どのページでも、読む').toBe(true);
    });

    it('レポート・使用量を開いているときと、新規レビューのページでは、loadHook を呼んでも、hook_status を呼ばない。履歴・ログのページ、設定を開いているときは、呼ぶ', async () => {
      invokeMock.mockImplementation(okImpl);
      state.page = 'history';
      state.report = {} as never;
      await loadHook();
      state.report = null;
      state.usage = {} as never;
      await loadHook();
      state.usage = null;
      state.page = 'newreview';
      await loadHook();
      expect(hookCalls()).toBe(0);
      state.page = 'history';
      await loadHook();
      expect(hookCalls()).toBe(1);
      state.page = 'newreview';
      state.settings = { view: {}, draft: {} } as never;
      await loadHook();
      expect(hookCalls()).toBe(2);
    });

    it('定期の読み直しは、別のページへ移ると、止まる（時間を進めても、hook_status を呼ばない）', async () => {
      vi.useFakeTimers();
      invokeMock.mockImplementation(okImpl);
      state.page = 'newreview';
      render();
      tab('history').click();
      await vi.advanceTimersByTimeAsync(0);
      expect(hookCalls()).toBe(1);
      // 履歴・ログに居るあいだは、一定時間ごとに、読み直す（実行中でなければ 8 秒）
      await vi.advanceTimersByTimeAsync(8_000);
      expect(hookCalls()).toBe(2);
      // 別のページへ移ると、止まる
      tab('newreview').click();
      await vi.advanceTimersByTimeAsync(60_000);
      expect(hookCalls(), '移ったあとは、時間が経っても、増えない').toBe(2);
    });

    it('読み取りに失敗したら、「実行なし」と見分けがつくように、フックの欄に理由が出る。次に読めたら、消える', async () => {
      let fail = true;
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'hook_status') {
          if (fail) throw new Error('記録を読めません');
          return { recent: [], log_unreadable: 0 };
        }
        return cmd === 'list_reviews' ? [] : null;
      });
      state.page = 'newreview';
      render();
      tab('history').click();
      await flush();
      const err = document.querySelector('.hookruns-section .hooklog-load-error');
      expect(err?.textContent).toContain('記録を読めません');
      expect(document.querySelector('.hookruns-section li'), '実行の行は、まだ無い').toBeNull();
      // 読めるようになったら、消える（実行が 0 件なら、フックの欄ごと消える）
      fail = false;
      [...document.querySelectorAll<HTMLElement>('.page-head button')].find((b) => b.textContent === '更新')!.click();
      await flush();
      expect(document.querySelector('.hooklog-load-error')).toBeNull();
      expect(document.querySelector('.hookruns-section')).toBeNull();
    });
  });

  describe('「履歴・ログ」の絞り込み（入力欄 1 つ・複数語）', () => {
    const type = (value: string) => {
      const input = document.getElementById('history-query') as HTMLInputElement;
      input.value = value;
      input.dispatchEvent(new Event('input', { bubbles: true }));
    };
    const reviewTitles = () => [...document.querySelectorAll('.right ul.history > li .ht')].map((e) => e.textContent);
    const hookCount = () => document.querySelectorAll('.hookruns-section li.hookrun').length;
    const seed = () => {
      state.history = [
        summary('a', { recipe_title: '潜在バグ', seq: 1 }),
        summary('b', { recipe_title: 'セキュリティ', seq: 1 }),
        summary('c', { recipe_title: '潜在バグ', seq: 2, counts: { open: 0, done: 2, wait: 0, dismiss: 0 } }),
      ] as never;
      state.hook.status = { recent: [hookRun({ run: 'h1', subject: 'タブの骨格' }), hookRun({ run: 'h2', state: 'failed', subject: '直した' })] } as never;
      state.historyQuery = '';
      state.picked = new Set();
      state.page = 'history';
      render();
    };

    it('入力欄は、1 つだけ（ドロップダウンは、置かない）', () => {
      seed();
      expect(document.querySelectorAll('.hsearch input, .hsearch select').length).toBe(1);
      expect(document.querySelectorAll('.page-aside select').length).toBe(0);
    });

    it('語を入れると、当てはまる行だけが残る。語を増やすと、さらに絞れる（合わせ技）。フックの行にも効く', () => {
      seed();
      expect(reviewTitles().length).toBe(3);
      expect(hookCount()).toBe(2);
      type('潜在バグ');
      expect(reviewTitles().length).toBe(2);
      expect(hookCount(), 'フックの行には、「潜在バグ」は当てはまらない').toBe(0);
      type('潜在バグ 未対応');
      expect(reviewTitles().length).toBe(1);
      type('フック 失敗');
      expect(reviewTitles().length).toBe(0);
      expect(hookCount()).toBe(1);
      expect(document.querySelector('.hsearch-count')?.textContent).toBe('1 件 / 全 5 件');
    });

    it('解除（✕）で、すべて戻る。当てはまるものが無いときは、その旨', () => {
      seed();
      type('存在しない語');
      expect(document.querySelector('.right')?.textContent).toContain('当てはまるものがありません');
      document.querySelector<HTMLElement>('.hsearch-clear')!.click();
      expect(state.historyQuery).toBe('');
      expect(reviewTitles().length).toBe(3);
      expect(document.querySelector('.hsearch-clear')).toBeNull();
    });

    it('日本語入力の変換中は、絞り込まない。確定（compositionend）してから', () => {
      seed();
      const input = document.getElementById('history-query') as HTMLInputElement;
      input.value = 'せきゅ';
      input.dispatchEvent(new InputEvent('input', { bubbles: true, isComposing: true }));
      expect(state.historyQuery, '変換中は、そのまま').toBe('');
      expect(reviewTitles().length).toBe(3);
      input.value = 'セキュリティ';
      input.dispatchEvent(new Event('compositionend', { bubbles: true }));
      expect(state.historyQuery).toBe('セキュリティ');
      expect(reviewTitles().length).toBe(1);
    });

    it('入力中は、描き直したあとも、フォーカスとカーソルの位置が、戻る', () => {
      seed();
      const input = document.getElementById('history-query') as HTMLInputElement;
      input.focus();
      input.value = '潜在バグ 未';
      input.setSelectionRange(2, 2);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      const again = document.getElementById('history-query') as HTMLInputElement;
      expect(again).not.toBe(input); // 描き直されて、別の要素になっている
      expect(document.activeElement).toBe(again);
      expect(again.value).toBe('潜在バグ 未');
      expect([again.selectionStart, again.selectionEnd]).toEqual([2, 2]);
    });

    it('「すべて選ぶ」は、見えている行だけを選ぶ。隠れている行は、選ばない', () => {
      seed();
      type('セキュリティ');
      document.querySelector<HTMLElement>('.hselectall')!.click();
      expect([...state.picked]).toEqual(['b']);
      // もう一度で、見えている分を外す
      document.querySelector<HTMLElement>('.hselectall')!.click();
      expect(state.picked.size).toBe(0);
    });

    it('絞り込みを変えて、見えなくなった行の選択は、外れる（見えない行を、レポート・削除に入れない）', () => {
      seed();
      document.querySelector<HTMLElement>('.hselectall')!.click();
      expect(state.picked.size).toBe(3);
      type('セキュリティ');
      expect([...state.picked]).toEqual(['b']);
    });

    it('手で選んだ a と b のうち、b だけに絞ると、a の選択が外れ、✕ で解除しても、a は戻らない（b だけが残る）', () => {
      seed();
      document.querySelectorAll<HTMLInputElement>('.right ul.history li .pick').forEach((box, i) => {
        if (i < 2) box.dispatchEvent(new Event('change'));
      });
      expect([...state.picked].sort()).toEqual(['a', 'b']);
      type('セキュリティ'); // b だけ
      expect([...state.picked]).toEqual(['b']);
      document.querySelector<HTMLElement>('.hsearch-clear')!.click();
      expect([...state.picked], '解除しても、外れた a は、戻らない').toEqual(['b']);
      expect(reviewTitles().length).toBe(3);
    });

    it('件数表示: フックの記録が無く、レビューだけのとき、「1 件 / 全 26 件」', () => {
      state.history = Array.from({ length: 26 }, (_, i) => summary(`r${i}`, { recipe_title: i === 7 ? 'セキュリティ' : '潜在バグ' })) as never;
      state.hook.status = null;
      state.historyQuery = '';
      state.page = 'history';
      render();
      type('セキュリティ');
      expect(document.querySelector('.hsearch-count')?.textContent).toBe('1 件 / 全 26 件');
    });

    it('当てはまる行が多いときは、最初の 100 件だけ描き、「さらに表示」で増やす。「すべて選ぶ」は、出している行だけ', () => {
      state.history = Array.from({ length: 250 }, (_, i) => summary(`m${i}`, { recipe_title: '潜在バグ', created_at: 1_795_775_717_000 - i * 1000 })) as never;
      state.hook.status = null;
      state.historyQuery = '';
      state.page = 'history';
      render();
      type('潜在バグ');
      expect(reviewTitles().length).toBe(100);
      expect(document.querySelector('.hmore')?.textContent).toBe('さらに表示（残り 150 件）');
      expect(document.querySelector('.hsearch-count')?.textContent, '件数は、当てはまる全部').toBe('250 件 / 全 250 件');
      document.querySelector<HTMLElement>('.hselectall')!.click();
      expect(state.picked.size, '出している 100 件だけ').toBe(100);
      document.querySelector<HTMLElement>('.hmore')!.click();
      expect(reviewTitles().length).toBe(200);
      expect(document.querySelector('.hmore')?.textContent).toBe('さらに表示（残り 50 件）');
      document.querySelector<HTMLElement>('.hmore')!.click();
      expect(reviewTitles().length).toBe(250);
      expect(document.querySelector('.hmore')).toBeNull();
      // 入力を変えると、100 件に戻る
      type('潜在');
      expect(reviewTitles().length).toBe(100);
    });

    it('絞り込んでいるあいだは、古い日付のグループに隠れず、当てはまる行が、平らに並ぶ', () => {
      const day = 86_400_000;
      const many = Array.from({ length: 25 }, (_, i) => summary(`n${i}`, { recipe_title: '潜在バグ', created_at: 1_795_775_717_000 - i * 1000 }));
      const old = summary('old', { recipe_title: 'パフォーマンス', created_at: 1_795_775_717_000 - 90 * day, counts: { open: 0, done: 2, wait: 0, dismiss: 0 } });
      state.history = [...many, old] as never;
      state.hook.status = null;
      state.historyQuery = '';
      state.page = 'history';
      render();
      expect(reviewTitles().some((t) => t?.includes('パフォーマンス')), '絞り込まないときは、古い日付に畳まれている').toBe(false);
      type('パフォーマンス');
      expect(reviewTitles().length).toBe(1);
      expect(document.querySelector('.hday-h')).toBeNull();
    });
  });

  describe('履歴から開いた結果（行の下に開くアコーディオン）', () => {
    const open = (id: string, over: Partial<ReviewView> = {}) => {
      state.history = [summary(id)] as never;
      state.selected = { ...fakeReview(id), ...over } as ReviewView;
      state.page = 'history';
      render();
    };

    it('押した行の下に開く。ページは、「履歴・ログ」のまま。タブ（指摘・差分・記録）も、「すべて開く」「取り込む」の行も、まとめて渡す欄も、戻るボタンも無い', () => {
      open('x');
      expect(selected()).toEqual(['history']);
      expect(document.querySelectorAll('.pagetabs button').length).toBe(4);
      expect(document.querySelector('.page-title')?.textContent).toContain('履歴・ログ'); // 件数つき「履歴・ログ (1)」
      expect(document.querySelector('.run-form, .recipe-bar-top')).toBeNull();
      expect(document.querySelector('li .hdetail .result-compact')).not.toBeNull(); // 行（li）の中、行の下
      expect(document.querySelector('.result nav.tabs')).toBeNull();
      expect(document.querySelector('.result')?.textContent).not.toContain('すべて開く');
      expect(document.querySelector('.result')?.textContent).not.toContain('まとめて CLI に渡す');
      expect(document.querySelector('.back-to-log-btn')).toBeNull(); // 行を押せば、閉じる
      expect(document.querySelector('.hdetail .d-head h2')).toBeNull(); // 名前は、行に出ている
      expect(document.querySelector('.hdetail .report-this-btn')).not.toBeNull();
    });

    it('同時に開くのは 1 件だけ。開いている行をもう一度押すと閉じる。別の行を押すと、前の行は閉じる', async () => {
      invokeMock.mockImplementation(async (cmd: string, args: { id?: string }) => (cmd === 'get_review' ? fakeReview(args.id ?? 'x') : cmd === 'build_handoff' ? '' : null));
      state.history = [summary('a'), summary('b', { created_at: 1_795_775_717_000 - 1000 })] as never;
      state.page = 'history';
      render();
      const rows = () => [...document.querySelectorAll<HTMLElement>('.right ul.history li')];
      rows()[0].querySelector<HTMLElement>('.ht')!.click();
      await flush();
      expect(state.page).toBe('history'); // ページは、変わらない
      expect(state.selected?.id).toBe('a');
      expect(document.querySelectorAll('.hdetail').length).toBe(1);
      expect(rows()[0].querySelector('.hdetail')).not.toBeNull();
      rows()[1].querySelector<HTMLElement>('.ht')!.click(); // 別の行
      await flush();
      expect(state.selected?.id).toBe('b');
      expect(document.querySelectorAll('.hdetail').length).toBe(1);
      expect(rows()[1].querySelector('.hdetail')).not.toBeNull();
      expect(rows()[0].querySelector('.hdetail')).toBeNull();
      rows()[1].querySelector<HTMLElement>('.ht')!.click(); // 同じ行: 閉じる
      await flush();
      expect(state.selected).toBeNull();
      expect(document.querySelector('.hdetail')).toBeNull();
    });

    it('開いた詳細の中を押しても、行は閉じない（カード・ボタンの操作で、閉じてしまわない）', () => {
      open('x');
      (document.querySelector('.hdetail') as HTMLElement).click();
      expect(state.selected?.id).toBe('x');
      expect(document.querySelector('.hdetail')).not.toBeNull();
    });

    it('下に、折りたたみの「実行の記録」。開いたときにだけ、中身（実行・入力・差分）を作る。差分も、その欄を開いたときにだけ', () => {
      open('x', { prompt: '前置き\n\n## 差分\n\n```diff\n+追加した行\n```\n' } as Partial<ReviewView>);
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
      expect(det.querySelector('.rld-finding')).toBeNull();
    });

    it('「前回」へ移ったとき、その行が、折りたたまれた日付の中にあっても、開く（日付を開く）', async () => {
      const old = { created_at: new Date(2026, 9, 1, 10, 0, 0).getTime() };
      invokeMock.mockImplementation(async (cmd: string, args: { id?: string }) => (cmd === 'get_review' ? fakeReview(args.id ?? 'x') : cmd === 'build_handoff' ? '' : null));
      state.history = [summary('new1'), ...Array.from({ length: 8 }, (_, i) => summary(`o${i}`, old))] as never;
      state.page = 'history';
      state.historyDays = new Set();
      await select('o3');
      expect(state.historyDays.size).toBe(1);
      expect(document.querySelectorAll('.hdetail').length).toBe(1);
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

  describe('観点のページ', () => {
    it('タブで開くと、観点ごとの表が出る。「履歴」を押すと、履歴・ログへ移り、絞り込みに観点名が入る', async () => {
      state.history = [summary('a', { recipe_title: 'セキュリティ' }), summary('b'), summary('c')] as never;
      render();
      tab('aspects').click();
      await flush();
      expect(selected()).toEqual(['aspects']);
      expect(document.querySelectorAll('.aspect-row[data-aspect]').length).toBe(2);
      (document.querySelector('.aspect-row[data-aspect="セキュリティ"] .aspect-hist') as HTMLElement).click();
      await flush();
      expect(state.page).toBe('history');
      expect(selected()).toEqual(['history']);
      expect(state.historyQuery).toBe('セキュリティ');
      expect((document.getElementById('history-query') as HTMLInputElement).value).toBe('セキュリティ');
    });

    it('履歴が空なら、案内だけ（表は出さない）', () => {
      state.history = [];
      state.page = 'aspects';
      render();
      expect(document.querySelector('.aspects-empty')).not.toBeNull();
      expect(document.querySelector('.aspect-table')).toBeNull();
    });
  });

  describe('ブランチ・フックの読み直し', () => {
    const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd).length;

    it('窓に戻っただけの読み直しは、15 秒以内なら、ブランチ・フックを読み直さない。ページを開いたときの読み込みは、いつも読む', async () => {
      state.registeredRepos = [{ path: '/r' }, { path: '/s' }] as never;
      state.page = 'repos';
      await loadRepoStatus(false);
      expect([calls('repo_branches'), calls('repo_hooks')]).toEqual([1, 1]);
      await loadRepoStatus(false, true);
      expect([calls('repo_branches'), calls('repo_hooks')]).toEqual([1, 1]);
      expect(calls('repo_status')).toBe(2);
      await loadRepoStatus(false);
      expect([calls('repo_branches'), calls('repo_hooks')]).toEqual([2, 2]);
    });

    const withBase = (over: (cmd: string, args: { paths?: string[] }) => unknown | undefined) => {
      const base: Record<string, unknown> = { repo_status: [], repo_data: [], repo_branches: {}, repo_hooks: {}, list_reviews: [], list_recipes: { recipes: [], warnings: [] }, list_bases: [] };
      invokeMock.mockImplementation(async (cmd: string, args: { paths?: string[] }) => {
        const o = over(cmd, args);
        if (o instanceof Error) throw o;
        return o !== undefined ? o : (base[cmd] ?? null);
      });
    };

    it('間引いた窓復帰では、前の失敗の理由を消さない（窓に戻っただけで、エラー表示が消えない）', async () => {
      state.registeredRepos = [{ path: '/r' }] as never;
      state.page = 'repos';
      withBase((cmd) => (cmd === 'repo_branches' ? new Error('git が見つかりません') : undefined));
      await loadRepoStatus(false);
      expect(state.repoAuxErrors.join('')).toContain('git が見つかりません');
      await loadRepoStatus(false, true); // 15 秒以内の窓復帰: 読み直さない
      expect(state.repoAuxErrors.join('')).toContain('git が見つかりません');
    });

    it('15 秒を過ぎた窓復帰では、ブランチ・フックを読み直す（境界は、ちょうど 15 秒から）', async () => {
      vi.useFakeTimers({ toFake: ['Date'] });
      try {
        vi.setSystemTime(new Date(2026, 10, 28, 12, 0, 0));
        state.registeredRepos = [{ path: '/r' }] as never;
        state.page = 'repos';
        withBase(() => undefined);
        await loadRepoStatus(false);
        expect(calls('repo_branches')).toBe(1);
        vi.setSystemTime(new Date(2026, 10, 28, 12, 0, 14, 999));
        await loadRepoStatus(false, true);
        expect(calls('repo_branches')).toBe(1);
        vi.setSystemTime(new Date(2026, 10, 28, 12, 0, 15, 0));
        await loadRepoStatus(false, true);
        expect(calls('repo_branches')).toBe(2);
        expect(calls('repo_hooks')).toBe(2);
      } finally {
        vi.useRealTimers();
      }
    });

    it('間引いた窓復帰は、読んだ時刻を進めない（15 秒の起点は、実際に読んだ時刻）。読んだ直後のページ移動では、また読む', async () => {
      vi.useFakeTimers({ toFake: ['Date'] });
      try {
        vi.setSystemTime(new Date(2026, 10, 28, 14, 0, 0));
        state.registeredRepos = [{ path: '/r' }] as never;
        state.page = 'repos';
        withBase(() => undefined);
        await loadRepoStatus(false); // 読む（起点 14:00:00）
        expect(calls('repo_branches')).toBe(1);
        vi.setSystemTime(new Date(2026, 10, 28, 14, 0, 10));
        await loadRepoStatus(false, true); // 間引く（起点は、進めない）
        expect(calls('repo_branches')).toBe(1);
        vi.setSystemTime(new Date(2026, 10, 28, 14, 0, 11));
        await loadRepoStatus(false); // 読んだ直後のページ移動（窓復帰ではない）: また読む（起点 14:00:11）
        expect(calls('repo_branches')).toBe(2);
        vi.setSystemTime(new Date(2026, 10, 28, 14, 0, 16));
        await loadRepoStatus(false, true); // 起点 14:00:11 から 5 秒: 間引く
        expect(calls('repo_branches')).toBe(2);
        vi.setSystemTime(new Date(2026, 10, 28, 14, 0, 26));
        await loadRepoStatus(false, true); // 起点から 15 秒: 読む
        expect(calls('repo_branches')).toBe(3);
        vi.setSystemTime(new Date(2026, 10, 28, 13, 59, 0));
        await loadRepoStatus(false, true); // 時計が戻った（差が負）: 間引かず、読む
        expect(calls('repo_branches')).toBe(4);
      } finally {
        vi.useRealTimers();
      }
    });

    it('リポジトリを切り替えると、前のリポジトリの履歴を空にする。ブランチを読めなかったときは理由を出す。登録外のパスは読まない', async () => {
      state.registeredRepos = [{ path: '/r' }, { path: '/s' }] as never;
      state.history = [{ id: 'old', created_at: 1, repo: '/r', recipe_title: 'バグ', provider_label: 'p', findings: 1, verified: 0, total_tokens: null, failed: false, counts: null }] as never;
      let reviewsSeen = -1;
      withBase((cmd) => {
        if (cmd === 'repo_branches') return new Error('読めません');
        if (cmd === 'list_reviews') {
          reviewsSeen = state.history.length; // 履歴を読んでいる最中は、前のリポジトリの履歴が残っていない
          return [];
        }
        return undefined;
      });
      await changeRepo('/s');
      await flush();
      expect(reviewsSeen).toBe(0);
      expect(state.history).toEqual([]);
      expect(state.repoAuxErrors.join('')).toContain('読めません');
      invokeMock.mockClear();
      await changeRepo('/not-registered');
      await flush();
      expect(calls('repo_branches')).toBe(0);
    });

    it('切り替えが続いたとき、遅れて返った前のリポジトリの失敗は、いまのリポジトリの表示（ブランチ・エラー）を変えない', async () => {
      state.registeredRepos = [{ path: '/r' }, { path: '/s' }] as never;
      state.repoAuxErrors = [];
      let failR: (e: Error) => void = () => {};
      withBase((cmd, args) => {
        if (cmd === 'repo_branches' && args.paths?.[0] === '/r') return new Promise((_, rej) => (failR = rej)) as never;
        if (cmd === 'repo_branches') return { '/s': 'main' };
        return undefined;
      });
      const first = changeRepo('/r');
      await changeRepo('/s');
      await first;
      await flush();
      expect(state.repoBranches['/s']).toBe('main');
      failR(new Error('/r の遅い失敗'));
      await flush();
      expect(state.repo).toBe('/s');
      expect(state.repoBranches['/s']).toBe('main');
      expect(state.repoAuxErrors.join('')).not.toContain('/r の遅い失敗');
    });

    it('観点のページでも、履歴を読めなかった理由が出る。切り替えの失敗のあと、前のリポジトリの履歴は、残らない', async () => {
      state.registeredRepos = [{ path: '/r' }, { path: '/s' }] as never;
      state.history = [{ id: 'old', created_at: 1, repo: '/r', recipe_title: 'バグ', provider_label: 'p', findings: 1, verified: 0, total_tokens: null, failed: false, counts: null }] as never;
      state.page = 'aspects';
      withBase((cmd) => (cmd === 'list_reviews' ? new Error('DB が開けません') : undefined));
      await expect(changeRepo('/s')).rejects.toThrow('DB が開けません');
      await flush();
      expect(document.querySelector('.history-load-error')?.textContent).toContain('DB が開けません');
      expect(state.history).toEqual([]);
      expect(document.querySelector('.aspect-table')).toBeNull(); // 前のリポジトリの集計は、出ない
    });

    it('履歴のページで読んだ 5 秒後に、リポジトリのページを開いたときの読み込みは、間引かれず、全リポジトリのブランチとフックを読む', async () => {
      vi.useFakeTimers({ toFake: ['Date'] });
      try {
        vi.setSystemTime(new Date(2026, 10, 28, 13, 0, 0));
        state.registeredRepos = [{ path: '/r' }, { path: '/s' }] as never;
        state.page = 'history';
        withBase(() => undefined);
        await loadRepoStatus(false, true); // 履歴のページでの窓復帰: 選んだ 1 つのブランチだけ
        expect([calls('repo_branches'), calls('repo_hooks')]).toEqual([1, 0]);
        vi.setSystemTime(new Date(2026, 10, 28, 13, 0, 5));
        state.page = 'repos';
        await loadRepoStatus(false); // ページを開いたとき（窓復帰ではない）
        expect([calls('repo_branches'), calls('repo_hooks')]).toEqual([2, 1]);
        const branchCalls = invokeMock.mock.calls.filter((c) => c[0] === 'repo_branches');
        const last = branchCalls[branchCalls.length - 1][1] as { paths: string[] };
        expect(last.paths).toEqual(['/r', '/s']);
      } finally {
        vi.useRealTimers();
      }
    });

    it('履歴を読めなかったときは、空の履歴と区別できるよう、理由を画面に出す。読めたら消える', async () => {
      state.registeredRepos = [{ path: '/s' }] as never;
      state.page = 'history';
      withBase((cmd) => (cmd === 'list_reviews' ? new Error('DB が開けません') : undefined));
      await expect(changeRepo('/s')).rejects.toThrow('DB が開けません');
      await flush();
      expect(document.querySelector('.history-load-error')?.textContent).toContain('DB が開けません');
      expect(document.querySelector('.history-load-error')?.textContent).toContain('履歴が無い');
      withBase(() => undefined);
      await changeRepo('/s');
      await flush();
      expect(document.querySelector('.history-load-error')).toBeNull();
      expect(state.historyError).toBe('');
    });

    it('リポジトリを切り替えたら、そのリポジトリのブランチ名を、すぐ読む（ページ変更・窓復帰を待たない）', async () => {
      state.registeredRepos = [{ path: '/r' }, { path: '/s' }] as never;
      state.page = 'newreview';
      const base: Record<string, unknown> = { repo_status: [], repo_data: [], repo_hooks: {}, list_reviews: [], list_recipes: { recipes: [], warnings: [] }, list_bases: [] };
      invokeMock.mockImplementation(async (cmd: string, args: { paths?: string[] }) => (cmd === 'repo_branches' ? { [args.paths![0]]: 'feature/x' } : (base[cmd] ?? null)));
      await changeRepo('/s');
      await flush();
      expect(state.repoBranches['/s']).toBe('feature/x');
    });
  });

  describe('見る範囲（比較元）の置き場所', () => {
    const rangeChips = () => document.querySelectorAll('.combo').length;

    it('新規レビューのページの先頭にだけ出る。ヘッダーにも、ほかのページにも、出ない', () => {
      state.page = 'newreview';
      render();
      expect(document.querySelectorAll('.run-target .combo').length).toBe(1);
      expect(document.querySelectorAll('.topbar .combo').length).toBe(0);
      const first = document.querySelector('.detail.page')!.children;
      expect([...first].findIndex((el) => el.classList.contains('run-target'))).toBeLessThan([...first].findIndex((el) => el.classList.contains('run-form')));
      for (const p of ['repos', 'aspects', 'history'] as const) {
        state.page = p;
        render();
        expect(document.querySelector('.run-target'), p).toBeNull();
        expect(document.querySelectorAll('.topbar .combo').length, p).toBe(0);
      }
      void rangeChips;
    });

    it('名前で入力する欄（baseForm）は、新規レビューのページの中に出て、タブの下には出ない', () => {
      state.page = 'newreview';
      state.baseCustom = true;
      render();
      expect(document.querySelector('.run-target .baseform')).not.toBeNull();
      expect(document.querySelector('.top > .baseform')).toBeNull();
      state.page = 'history';
      render();
      expect(document.querySelector('.baseform')).toBeNull();
      state.baseCustom = false;
    });
  });

  describe('開いているレビューの、エージェントの報告', () => {
    it('履歴を読み直したとき、開いているカードも、最新の報告に追いつく（件数だけ新しい、にならない）', async () => {
      const f = (n: number) => ({ file: `a${n}.rs`, line: 1, severity: 'medium', title: `見出し${n}`, detail: '', evidence: '', verified: true });
      const open = { ...fakeReview('rv'), findings: [f(0), f(1)], triage: ['fix', 'fix'], finding_notes: [null, null], agent_reports: [null, null] } as unknown as ReviewView;
      const reported = [
        { verdict: 'did', text: '直した', reported_at: 1, source: 'cli:agent' },
        { verdict: 'skipped', text: '誤検知です', reported_at: 2, source: 'cli:agent' },
      ];
      const base: Record<string, unknown> = { repo_status: [], repo_data: [], repo_branches: {}, repo_hooks: {}, list_reviews: [], list_recipes: { recipes: [], warnings: [] }, list_bases: [] };
      invokeMock.mockImplementation(async (cmd: string) => (cmd === 'get_agent_reports' ? reported : (base[cmd] ?? null)));
      state.page = 'history';
      state.selected = open;
      await refreshHistory();
      await flush();
      expect(state.selected!.agent_reports).toEqual(reported);
      // 読むのは、報告だけ（レビュー本体を、取り直さない）。指摘の件数を、渡す
      expect(invokeMock.mock.calls.some((c) => c[0] === 'get_review')).toBe(false);
      expect(invokeMock.mock.calls.find((c) => c[0] === 'get_agent_reports')![1]).toEqual({ id: 'rv', count: 2 });
    });

    it('別のレビューへ移っていたら、読み終わった報告を、いまのレビューに入れない', async () => {
      const a = { ...fakeReview('a'), agent_reports: [null] } as unknown as ReviewView;
      const b = { ...fakeReview('b'), agent_reports: [null] } as unknown as ReviewView;
      let release: (v: unknown) => void = () => {};
      const base: Record<string, unknown> = { repo_status: [], repo_data: [], list_reviews: [], list_recipes: { recipes: [], warnings: [] }, list_bases: [] };
      invokeMock.mockImplementation((cmd: string) => (cmd === 'get_agent_reports' ? new Promise((r) => (release = r)) : Promise.resolve(base[cmd] ?? null)));
      state.selected = a;
      const done = refreshHistory();
      await flush();
      state.selected = b;
      release([{ verdict: 'did', text: 'x', reported_at: 1, source: 's' }]);
      await done;
      await flush();
      expect(state.selected!.agent_reports).toEqual([null]);
    });
  });

  describe('「誤検知」ボタン', () => {
    const f = { file: 'a.rs', line: 1, severity: 'medium', title: '見出し', detail: '詳細', evidence: 'x', verified: true };
    const show = (status: string, agent: unknown, memo = '') => {
      state.selected = { ...fakeReview('fp'), findings: [f], triage: [status], finding_notes: [{ memo, done_at: null, done_head: null }], agent_reports: [agent] } as unknown as ReviewView;
      state.history = [summary('fp')] as never;
      state.page = 'history';
      state.openCards = new Set([0]);
      state.expandedFindings = new Set(['fp:0']); // カードを開いておく
      render();
    };
    const skipped = { verdict: 'skipped', text: '誤検知です', reported_at: 1, source: 'cli:agent' };

    it('エージェントが「対応しなかった」と申告した、「対応」の指摘にだけ出る', () => {
      show('fix', skipped);
      expect(document.querySelectorAll('.false-positive').length).toBe(1);
      expect(document.querySelector('.false-positive')?.textContent).toBe('誤検知');
      show('fix', null);
      expect(document.querySelector('.false-positive')).toBeNull(); // 報告なし
      show('fix', { ...skipped, verdict: 'did' });
      expect(document.querySelector('.false-positive')).toBeNull(); // 対応した
      show('dismiss', skipped);
      expect(document.querySelector('.false-positive')).toBeNull(); // もう「対応」ではない
      show('wait', skipped);
      expect(document.querySelector('.false-positive')).toBeNull();
    });

    it('「確認して対応しなかった」と申告された指摘には、「未対応」の印を出さない（申告の印だけ）。直す対象と読めないように', () => {
      show('fix', skipped);
      const chips = [...document.querySelectorAll('.finding .chip')].map((c) => c.textContent);
      expect(chips).toContain('🤖 確認して対応しなかった');
      expect(chips).not.toContain('未対応');
      // 申告が無ければ、これまでどおり「未対応」
      show('fix', null);
      expect([...document.querySelectorAll('.finding .chip')].map((c) => c.textContent)).toContain('未対応');
    });

    it('「確認して対応しなかった」の申告があっても、あなたが「対応済みにする」を付けたあとは、「対応済み」の印を出す（一覧の「済み」と食い違わない）', () => {
      state.selected = { ...fakeReview('fp'), findings: [f], triage: ['fix'], finding_notes: [{ memo: '', done_at: 1_795_775_717_000, done_head: 'abc1234' }], agent_reports: [skipped] } as unknown as ReviewView;
      state.history = [summary('fp')] as never;
      state.page = 'history';
      state.openCards = new Set([0]);
      state.expandedFindings = new Set(['fp:0']);
      render();
      const chips = [...document.querySelectorAll('.finding .chip')].map((c) => c.textContent);
      expect(chips).toContain('🤖 確認して対応しなかった');
      expect(chips).toContain('対応済み');
      expect(chips).not.toContain('未対応');
    });

    it('押すと、仕分けを「無視」にして、メモに「誤検知」と残す。すでにあるメモは、消さずにあとに足す', async () => {
      const done = (triage: string[], memo: string) => ({ triage, notes: [{ memo, done_at: null, done_head: null }] });
      invokeMock.mockImplementation(async (cmd: string, args: { memo?: string }) => {
        if (cmd === 'set_triage') return done(['dismiss'], '');
        if (cmd === 'set_memo') return done(['dismiss'], args.memo ?? '');
        return null;
      });
      show('fix', skipped);
      (document.querySelector('.false-positive') as HTMLElement).click();
      await flush();
      const calls = invokeMock.mock.calls.filter((c) => c[0] === 'set_triage' || c[0] === 'set_memo');
      expect(calls.map((c) => [c[0], (c[1] as { status?: string }).status, (c[1] as { memo?: string }).memo])).toEqual([
        ['set_triage', 'dismiss', undefined],
        ['set_memo', undefined, '誤検知'],
      ]);
      expect(state.selected!.triage).toEqual(['dismiss']);

      invokeMock.mockClear();
      show('fix', skipped, '前に書いたメモ');
      (document.querySelector('.false-positive') as HTMLElement).click();
      await flush();
      const memoCall = invokeMock.mock.calls.find((c) => c[0] === 'set_memo');
      expect((memoCall![1] as { memo: string }).memo).toBe('前に書いたメモ ／ 誤検知');
    });
  });

  describe('「誤検知」ボタンの、途中の失敗', () => {
    const f = { file: 'a.rs', line: 1, severity: 'medium', title: '見出し', detail: '詳細', evidence: 'x', verified: true };
    const skipped = { verdict: 'skipped', text: '誤検知です', reported_at: 1, source: 'cli:agent' };
    const show = (memo = '') => {
      state.handlingError = '';
      state.selected = { ...fakeReview('fp'), findings: [f], triage: ['fix'], finding_notes: [{ memo, done_at: null, done_head: null }], agent_reports: [skipped] } as unknown as ReviewView;
      state.history = [summary('fp')] as never;
      state.page = 'history';
      state.openCards = new Set([0]);
      state.expandedFindings = new Set(['fp:0']);
      render();
    };
    const handling = (triage: string, memo: string) => ({ triage: [triage], notes: [{ memo, done_at: null, done_head: null }] });
    const names = () => invokeMock.mock.calls.filter((c) => c[0] === 'set_triage' || c[0] === 'set_memo').map((c) => `${c[0]}:${(c[1] as { status?: string }).status ?? ''}`);

    it('仕分けに失敗したら、メモは書かない。理由が、履歴・ログの画面に出る', async () => {
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'set_triage') throw new Error('仕分けを保存できません');
        return null;
      });
      show();
      (document.querySelector('.false-positive') as HTMLElement).click();
      await flush();
      expect(names()).toEqual(['set_triage:dismiss']); // set_memo は、呼ばない
      expect(state.selected!.triage).toEqual(['fix']);
      expect(document.querySelector('.handling-error')?.textContent).toContain('仕分けを保存できません');
    });

    it('メモの保存に失敗したら、仕分けを「対応」に戻す（理由のない「無視」を残さない）。理由は、画面に出る', async () => {
      invokeMock.mockImplementation(async (cmd: string, args: { status?: string }) => {
        if (cmd === 'set_triage') return handling(args.status ?? 'fix', '');
        if (cmd === 'set_memo') throw new Error('メモを保存できません');
        return null;
      });
      show();
      (document.querySelector('.false-positive') as HTMLElement).click();
      await flush();
      expect(names()).toEqual(['set_triage:dismiss', 'set_memo:', 'set_triage:fix']);
      expect(state.selected!.triage).toEqual(['fix']);
      expect(document.querySelector('.handling-error')?.textContent).toContain('メモを保存できません');
    });

    it('メモに、すでに「誤検知」とあるときは、二重に足さない（仕分けだけ「無視」にする）', async () => {
      invokeMock.mockImplementation(async (cmd: string, args: { status?: string }) => (cmd === 'set_triage' ? handling(args.status ?? 'fix', '誤検知でした') : null));
      show('誤検知でした');
      (document.querySelector('.false-positive') as HTMLElement).click();
      await flush();
      expect(names()).toEqual(['set_triage:dismiss']);
    });
  });

  describe('仕分け・メモの保存の失敗の理由（実行のエラーとは別）', () => {
    const f = { file: 'a.rs', line: 1, severity: 'medium', title: '見出し', detail: '詳細', evidence: 'x', verified: true };
    const show = () => {
      state.handlingError = '';
      state.runError = '';
      state.selected = { ...fakeReview('he'), findings: [f], triage: ['new'], finding_notes: [null], agent_reports: [null] } as unknown as ReviewView;
      state.history = [summary('he')] as never;
      state.page = 'history';
      state.expandedFindings = new Set(['he:0']);
      render();
    };
    const handling = (triage: string) => ({ triage: [triage], notes: [{ memo: '', done_at: null, done_head: null }] });

    it('保存に失敗した理由は、次に保存できたら、消える（成功しているのに、赤字が残り続けない）', async () => {
      let fail = true;
      invokeMock.mockImplementation(async (cmd: string, args: { status?: string }) => {
        if (cmd === 'set_triage') {
          if (fail) throw new Error('保存できません');
          return handling(args.status ?? 'new');
        }
        return null;
      });
      show();
      (document.querySelector('.t-wait') as HTMLElement).click();
      await flush();
      expect(document.querySelector('.handling-error')?.textContent).toContain('保存できません');
      fail = false;
      (document.querySelector('.t-wait') as HTMLElement).click(); // 今度は、成功する
      await flush();
      expect(document.querySelector('.handling-error')).toBeNull();
      expect(state.handlingError).toBe('');
    });

    it('実行のエラー（runError）は、結果の画面に出さない。別のレビューを開いたら、前の失敗の理由は消える', async () => {
      show();
      state.runError = 'API キーがありません';
      state.handlingError = '前の保存の失敗';
      render();
      expect(document.querySelector('.handling-error')?.textContent).toBe('前の保存の失敗');
      expect(document.body.textContent).not.toContain('API キーがありません');
      invokeMock.mockImplementation(async (cmd: string) => (cmd === 'get_review' ? fakeReview('other') : cmd === 'build_handoff' ? '' : null));
      await select('other');
      expect(state.handlingError).toBe('');
      expect(document.querySelector('.handling-error')).toBeNull();
    });
  });

  describe('開いているレビューの報告の読み直し（失敗・変化なし）', () => {
    const rep = [{ verdict: 'did', text: '直した', reported_at: 1, source: 'cli:agent' }];
    const open = () => {
      const o = { ...fakeReview('rv'), findings: [{ file: 'a.rs', line: 1, severity: 'medium', title: 't', detail: '', evidence: '', verified: true }], triage: ['fix'], finding_notes: [null], agent_reports: rep } as unknown as ReviewView;
      state.history = [summary('rv')] as never;
      state.selected = o;
      state.page = 'history';
      return o;
    };
    const base: Record<string, unknown> = { repo_status: [], repo_data: [], list_reviews: [], list_recipes: { recipes: [], warnings: [] }, list_bases: [] };

    it('報告を読めなかったとき、いまの表示（報告）を、空で上書きしない', async () => {
      const o = open();
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === 'get_agent_reports') throw new Error('読み取れません');
        return base[cmd] ?? null;
      });
      await refreshHistory();
      await flush();
      expect(state.selected!.agent_reports).toEqual(rep);
      expect(state.selected).toBe(o);
    });

    it('定期の読み直し: 開いているレビューがあり、窓が見えているときだけ、報告を読む。フォーカスが無くても読む。変わったら、カードが追いつく', async () => {
      expect(REPORT_POLL_MS).toBe(5000);
      const o = open();
      const reported = [{ verdict: 'skipped', text: '誤検知です', reported_at: 2, source: 'cli:agent' }];
      invokeMock.mockImplementation(async (cmd: string) => (cmd === 'get_agent_reports' ? reported : (base[cmd] ?? null)));
      const hidden = vi.spyOn(document, 'hidden', 'get');
      try {
        hidden.mockReturnValue(true); // 窓が隠れている: 読まない
        expect(await pollOpenReviewReports()).toBe(false);
        expect(invokeMock.mock.calls.some((c) => c[0] === 'get_agent_reports')).toBe(false);
        hidden.mockReturnValue(false); // 見えている（フォーカスは、問わない）
        expect(await pollOpenReviewReports()).toBe(true);
        await flush();
        expect(o.agent_reports).toEqual(reported);
        expect(await pollOpenReviewReports()).toBe(false); // 変わっていない: 描き直さない
        // メモなどを入力している最中は、読まない（描き直すと、入力途中の文字が消える）
        const input = document.createElement('input');
        document.body.append(input);
        input.focus();
        invokeMock.mockClear();
        expect(await pollOpenReviewReports()).toBe(false);
        expect(invokeMock.mock.calls.length).toBe(0);
        input.remove();
        state.selected = null; // 開いていない: 読まない
        invokeMock.mockClear();
        expect(await pollOpenReviewReports()).toBe(false);
        expect(invokeMock.mock.calls.length).toBe(0);
      } finally {
        hidden.mockRestore();
      }
    });

    it('報告が、変わっていないときは、渡す文面を作り直さない（描き直さない）', async () => {
      open();
      invokeMock.mockImplementation(async (cmd: string) => (cmd === 'get_agent_reports' ? JSON.parse(JSON.stringify(rep)) : (base[cmd] ?? null)));
      await refreshHistory();
      await flush();
      expect(invokeMock.mock.calls.filter((c) => c[0] === 'build_handoff').length).toBe(0);
    });
  });

  describe('結果の指摘の表（step 5）', () => {
    const mk = (n: number, over: Record<string, unknown> = {}) => {
      const sev = ['low', 'high', 'medium'];
      const findings = Array.from({ length: n }, (_, i) => ({ file: `src/f${i}.rs`, line: i + 1, severity: sev[i % 3], title: `見出し${i}`, detail: `詳細${i}`, evidence: `x${i}`, verified: true }));
      return { ...fakeReview('tb'), findings, triage: findings.map(() => 'new'), finding_notes: findings.map(() => null), agent_reports: findings.map(() => null), ...over } as unknown as ReviewView;
    };
    const show = (r: ReviewView, page: 'history' | 'newreview' = 'history') => {
      state.history = [summary('tb')] as never;
      state.selected = r;
      state.page = page;
      state.expandedFindings = new Set();
      state.expanded = new Set();
      state.openCards = new Set();
      render();
    };

    it('指摘は表で出る。履歴の行の下には、集計の 1 行。カード（fc-head）と、閉じたままの仕分けのボタンは、出ない', () => {
      show(mk(3));
      expect(document.querySelectorAll('.hdetail .ftable .fitem').length).toBe(3);
      expect(document.querySelector('.hdetail .fsum-compact .fsum-line')?.textContent).toBe('指摘 3 件 ・ high 1 ・ medium 1 ・ low 1');
      expect(document.querySelector('.hdetail .fsum-cards')).toBeNull();
      expect(document.querySelector('.hdetail .fc-head')).toBeNull();
      expect(document.querySelector('.hdetail .ftable .triage')).toBeNull(); // 一覧で、押し間違えない。仕分けは、開いた詳細の中
      expect([...document.querySelectorAll('.hdetail .ftable .ftitle')].map((t) => t.textContent)).toEqual(['見出し1', '見出し2', '見出し0']); // 重大度の高い順
    });

    it('新規レビューの下の結果は、4 枚の集計と表。指摘が 1 件でも、表', () => {
      show(mk(1), 'newreview');
      expect(document.querySelectorAll('.result-anchor .fsum-card').length).toBe(4);
      expect(document.querySelectorAll('.result-anchor .ftable .fitem').length).toBe(1);
    });

    it('行を押すと、その行の下に詳細（仕分けのボタン・全文・引用）が開く。同時に開くのは 1 件だけ。もう一度押すと、閉じる', () => {
      show(mk(3));
      const rows = () => [...document.querySelectorAll<HTMLElement>('.hdetail .ftable .fitem')];
      rows()[1].querySelector<HTMLElement>('.frow')!.click();
      expect(rows()[1].querySelector('.fdetail .triage')).not.toBeNull();
      expect(rows()[1].querySelector('.fdetail .fc-body')?.textContent).toContain('詳細2');
      expect(document.querySelectorAll('.fdetail').length).toBe(1);
      rows()[0].querySelector<HTMLElement>('.frow')!.click(); // 別の行
      expect(document.querySelectorAll('.fdetail').length).toBe(1);
      expect(rows()[0].querySelector('.fdetail')).not.toBeNull();
      expect(rows()[1].querySelector('.fdetail')).toBeNull();
      rows()[0].querySelector<HTMLElement>('.frow')!.click(); // 同じ行: 閉じる
      expect(document.querySelectorAll('.fdetail').length).toBe(0);
      expect(state.selected?.id).toBe('tb'); // 結果（履歴の行の詳細）は、閉じない
    });

    it('対応済みの指摘（エージェントが「対応した」）を開くと、仕分けのボタンは出ない（読むだけ）。済みでない指摘では、出る', () => {
      const r = mk(2, { triage: ['fix', 'fix'], agent_reports: [{ verdict: 'did', text: '直した', reported_at: 1, source: 'c' }, null] });
      show(r);
      const rows = () => [...document.querySelectorAll<HTMLElement>('.hdetail .ftable .fitem')];
      state.expandedFindings = new Set(['tb:0']); // 対応済み（済みは、下に並ぶ）
      state.expanded = new Set([0]);
      render();
      const done = rows().find((x) => x.querySelector('.fc-no')?.textContent === '#1')!;
      expect(done.querySelector('.fdetail')).not.toBeNull();
      expect(done.querySelector('.fdetail .triage')).toBeNull();
      expect(done.querySelector('.fdetail .handling .done-row')).toBeNull(); // 「対応済みにする」「誤検知」も、出ない（申告が、記録）
      state.expandedFindings = new Set(['tb:1']);
      state.expanded = new Set([1]);
      render();
      const undone = rows().find((x) => x.querySelector('.fc-no')?.textContent === '#2')!;
      expect(undone.querySelector('.fdetail .triage')).not.toBeNull();
    });

    it('判断の列に、仕分けの印と、申告の印が出る。対応済みの行は、薄くして、下に並ぶ', () => {
      const r = mk(3, {
        triage: ['fix', 'fix', 'new'],
        finding_notes: [null, null, null],
        agent_reports: [{ verdict: 'skipped', text: '誤検知です', reported_at: 1, source: 'c' }, { verdict: 'did', text: '直した', reported_at: 1, source: 'c' }, null],
      });
      show(r);
      const items = [...document.querySelectorAll<HTMLElement>('.hdetail .ftable .fitem')];
      // 並び: 済みでない（#3 → 未仕分け、#1 → 判断待ち）、済み（#2）は下
      expect(items.map((i) => i.querySelector('.fc-no')?.textContent)).toEqual(['#3', '#1', '#2']);
      expect(items[2].classList.contains('fdone')).toBe(true);
      expect(items[1].querySelector('.fstat')?.textContent).toContain('対応');
      expect(items[1].querySelector('.fstat')?.textContent).toContain('確認して対応しなかった');
      expect(items[1].querySelector('.fstat')?.textContent).not.toContain('未対応');
      expect(items[0].querySelector('.fpill')?.textContent).toBe('未仕分け');
      expect(items[2].querySelector('.fstat')?.textContent).toContain('対応した');
    });
  });

  describe('結果のページ（「ページで開く」）', () => {
    const open = () => {
      state.history = [summary('pg')] as never;
      state.selected = { ...fakeReview('pg'), model: 'claude-sonnet-5-5', duration_ms: 14000, usage: { input_tokens: 18420, output_tokens: 1, cache_read_tokens: null, cache_write_tokens: null } } as unknown as ReviewView;
      state.page = 'history';
      render();
    };

    it('履歴の行の下の結果に、「ページで開く」がある。押すと、結果のページ（実行の情報 5 枚・指摘の集計・表）になる。タブは、履歴・ログのまま', () => {
      open();
      (document.querySelector('.hdetail .open-page-btn') as HTMLElement).click();
      expect(state.page).toBe('result');
      expect(state.selected?.id).toBe('pg');
      expect(selected()).toEqual(['history']);
      expect(document.querySelector('.hdetail')).toBeNull(); // 行の下ではなく、ページ
      expect(document.querySelector('.page-title')?.textContent).toContain('履歴・ログ');
      const info = [...document.querySelectorAll('.fsum-info .fsum-card')].map((c) => c.textContent);
      expect(info).toEqual(['状態完了', 'モデルclaude-sonnet-5-5', '観点バグ', '入力トークン18,420', '所要時間14 秒']);
      expect(document.querySelector('.back-to-log-btn')?.textContent).toBe('履歴・ログ');
      expect(document.querySelector('.back-to-log-btn')?.getAttribute('title')).toBe('← 履歴・ログ');
      expect(document.querySelector('.result nav.tabs')).not.toBeNull(); // フル版（タブ・指摘・差分・記録）
    });

    it('戻るボタンで、履歴・ログへ戻る。結果は、開いたまま（行の下に、また開いている）', () => {
      open();
      (document.querySelector('.hdetail .open-page-btn') as HTMLElement).click();
      (document.querySelector('.back-to-log-btn') as HTMLElement).click();
      expect(state.page).toBe('history');
      expect(state.selected?.id).toBe('pg');
      expect(document.querySelector('.hdetail .result-compact')).not.toBeNull();
    });

    it('見出しは、パンくず（履歴・ログ › 観点名）。「履歴・ログ」を押すと、戻る', () => {
      open();
      (document.querySelector('.hdetail .open-page-btn') as HTMLElement).click();
      const crumb = document.querySelector('.page-title .crumb')!;
      expect(crumb.querySelector('.crumb-link')?.textContent).toBe('履歴・ログ');
      expect(crumb.querySelector('.crumb-here')?.textContent).toBe('バグ');
      (crumb.querySelector('.crumb-link') as HTMLElement).click();
      expect(state.page).toBe('history');
      expect(state.selected?.id).toBe('pg');
    });

    it('履歴・ログと結果のページの行き来だけ、動きでつなぐ（進む・戻る）。その描画だけに付き、次の描き直しでは、付かない。ほかのタブの切り替えは、動かさない', () => {
      const enter = () => document.querySelector('.detail')?.getAttribute('data-enter') ?? null;
      open();
      expect(enter()).toBeNull();
      (document.querySelector('.hdetail .open-page-btn') as HTMLElement).click();
      expect(enter()).toBe('forward');
      render(); // 中の操作などによる描き直し: 動かさない
      expect(enter()).toBeNull();
      (document.querySelector('.back-to-log-btn') as HTMLElement).click();
      expect(enter()).toBe('back');
      render();
      expect(enter()).toBeNull();
      tab('aspects').click();
      expect(enter()).toBeNull(); // ほかのタブ: 動かさない
    });

    it('戻ったとき、さっきの行を光らせ、開いたまま。「ページで開く」の前の、スクロール位置に戻す', () => {
      open();
      const main = document.querySelector('.detail') as HTMLElement;
      main.scrollTop = 240;
      (document.querySelector('.hdetail .open-page-btn') as HTMLElement).click();
      expect((document.querySelector('.detail') as HTMLElement).scrollTop).toBe(0);
      (document.querySelector('.back-to-log-btn') as HTMLElement).click();
      expect(document.querySelector('.history li.on.flash')).not.toBeNull();
      expect((document.querySelector('.detail') as HTMLElement).scrollTop).toBe(240);
      render(); // 次の描き直しでは、光らせない
      expect(document.querySelector('.history li.flash')).toBeNull();
    });

    it('実行に失敗したレビューをページで開くと、状態カードは「失敗」。入力トークンが無いときは「—」', () => {
      state.history = [summary('pg')] as never;
      state.selected = { ...fakeReview('pg'), error: 'API キーがありません', usage: { input_tokens: null, output_tokens: null, cache_read_tokens: null, cache_write_tokens: null } } as unknown as ReviewView;
      state.page = 'history';
      render();
      (document.querySelector('.hdetail .open-page-btn') as HTMLElement).click();
      const info = [...document.querySelectorAll('.fsum-info .fsum-card')].map((c) => c.textContent);
      expect(info[0]).toBe('状態失敗');
      expect(info[3]).toBe('入力トークン—');
    });

    it('行を開いて「ページで開く」→ 戻る、のあとも、同じ行だけが開いている。もう一度押すと、閉じる', () => {
      const sev = ['high', 'medium', 'low'];
      const findings = [0, 1, 2].map((i) => ({ file: `src/f${i}.rs`, line: i + 1, severity: sev[i], title: `見出し${i}`, detail: `詳細${i}`, evidence: `x${i}`, verified: true }));
      state.history = [summary('pg')] as never;
      state.selected = { ...fakeReview('pg'), findings, triage: ['new', 'new', 'new'], finding_notes: [null, null, null], agent_reports: [null, null, null] } as unknown as ReviewView;
      state.page = 'history';
      state.expandedFindings = new Set();
      state.expanded = new Set();
      render();
      const rows = () => [...document.querySelectorAll<HTMLElement>('.ftable .fitem')];
      const openRows = () => rows().map((r, i) => (r.querySelector('.fdetail') ? i : -1)).filter((i) => i >= 0);
      rows()[1].querySelector<HTMLElement>('.frow')!.click();
      expect(openRows()).toEqual([1]);
      (document.querySelector('.hdetail .open-page-btn') as HTMLElement).click(); // 結果のページ
      expect(openRows()).toEqual([1]); // 同じ行が、開いている
      (document.querySelector('.back-to-log-btn') as HTMLElement).click(); // 戻る
      expect(openRows()).toEqual([1]); // 行 1 だけ
      rows()[1].querySelector<HTMLElement>('.frow')!.click();
      expect(openRows()).toEqual([]); // 閉じる
      expect(state.expandedFindings.size + state.expanded.size).toBe(0); // 2 つの開閉の状態が、そろって空
    });

    it('結果が無くなれば（消した・リポジトリを替えた）、履歴・ログへ戻る。タブを押しても、履歴・ログへ移る（結果は閉じる）', () => {
      open();
      state.page = 'result';
      state.selected = null;
      render();
      expect(state.page).toBe('history');
      open();
      state.page = 'result';
      render();
      tab('history').click();
      expect(state.page).toBe('history');
      expect(state.selected).toBeNull();
    });
  });

  describe('結果を開いたときの、ページ', () => {
    const setup = () =>
      invokeMock.mockImplementation(async (cmd: string, args: { id?: string }) => {
        if (cmd === 'get_review') return fakeReview(args.id ?? 'x');
        if (cmd === 'build_handoff') return '';
        return null;
      });

    for (const page of ['history', 'newreview'] as const) {
      it(`${page}: 結果を開いても、ページは変わらない（別のレビュー〈「前回」など〉へ移っても）`, async () => {
        setup();
        state.page = page;
        await select('a');
        expect(state.page).toBe(page);
        await select('prev');
        expect(state.page).toBe(page);
        expect(state.selected?.id).toBe('prev');
      });
    }
  });
});
