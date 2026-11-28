// @vitest-environment jsdom
// 上部タブでページを切り替える流れ（タブを押す → state.page → 描画）を、画面の DOM で確かめる。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: vi.fn(), message: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { loadHook, loadRepoStatus, render, select, state, wantsHook } from './main';
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
    state.backTo = 'history';
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
      expect(cols(hookRow).slice(0, -1)).toEqual(['hlead', 'hwhen', 'h1', 'hbranch', 'hcommit', 'hfind']);
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

  describe('履歴から開いた結果（掘り下げのページ）', () => {
    it('見出しは「履歴・ログ」で、そのタブが選ばれたまま。タブ（指摘・差分・記録）も、「すべて開く」「取り込む」の行も、まとめて渡す欄も無い', () => {
      state.selected = fakeReview('x');
      state.backTo = 'history';
      state.page = 'result';
      render();
      expect(selected()).toEqual(['history']);
      expect(document.querySelectorAll('.pagetabs button').length).toBe(4);
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
