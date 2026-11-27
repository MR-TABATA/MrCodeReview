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
    state.backTo = 'log';
    state.history = [];
    state.runLogReviewCache.clear();
    state.expandedRunLogs.clear();
    state.loadingRunLogs.clear();
    state.runLogLoadErrors.clear();
    state.openRunLogDiffs.clear();
  });

  it('新規レビューのページ: 実行する場所だけ。結果も、実行ログも、ここには出ない', () => {
    render();
    expect(selected()).toEqual(['newreview']);
    expect(document.querySelector('.page-title')?.textContent).toBe('新規レビュー');
    expect(document.querySelector('.runlog-feed')).toBeNull();
    expect(document.querySelector('.back-to-log-btn')).toBeNull();
    expect(document.querySelector('.left')).toBeNull();
    expect(document.querySelector('.right')).toBeNull();
  });

  it('リポジトリ・履歴・ログの各タブを押すと、そのページの中身に変わる（左右の 3 カラムは、もう無い）', () => {
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

    tab('log').click();
    expect(state.page).toBe('log');
    expect(document.querySelector('.runlog-feed')).not.toBeNull();
    expect(document.querySelector('.review-empty')).toBeNull();

    tab('newreview').click();
    expect(state.page).toBe('newreview');
    expect(document.querySelector('.runlog-feed')).toBeNull();
  });

  it('4 つのページは、どれも、先頭に、同じ見出し（ページ名）が付く', () => {
    const titles: Record<string, string> = { repos: 'リポジトリ', history: '履歴', log: 'ログ', newreview: '新規レビュー' };
    for (const page of ['repos', 'history', 'log', 'newreview'] as const) {
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

  it('ログ: フックの実行とレビューが、1 つの時系列（新しい順）に、同じ書式の行で並ぶ', () => {
    state.history = [{ id: 'a', created_at: 1_795_775_717_000, repo: '/r', recipe_title: 'バグ', provider_label: 'Mock', findings: 2, verified: 2, total_tokens: null, failed: false, counts: null }] as never;
    state.hook.status = {
      recent: [
        { run: 'h1', started_at_ms: 1_795_775_800_000, commit: 'abc1234', subject: '件名', recipes: ['bugs'], steps: [], state: 'done', code: 1, message: null, findings: 1, ms: 2000 },
      ],
    } as never;
    state.page = 'log';
    render();
    const rows = [...document.querySelectorAll('.runlog-feed ul.history > li')];
    expect(rows.length).toBe(2);
    expect(rows[0].classList.contains('hookrun')).toBe(true); // 新しいほうが上
    expect(rows[0].querySelector('.badge')?.textContent).toBe('フック');
    expect(rows[1].querySelector('.ht')?.textContent).toBe('バグ');
    // 同じ列
    for (const r of rows) {
      expect(r.querySelector('.hwhen')).not.toBeNull();
      expect(r.querySelector('.hfind')).not.toBeNull();
    }
    // フックの行を押すと、内訳が開く
    (rows[0] as HTMLElement).click();
    expect(document.querySelector('.hookrun-detail')).not.toBeNull();
  });

  it('履歴の行と、ログの行は、同じ書式（同じ列の並び。ログに、開閉の印は無い）', () => {
    state.history = [{ id: 'a', created_at: 1_795_775_717_000, repo: '/r', recipe_title: 'バグ', provider_label: 'Mock', findings: 2, verified: 2, total_tokens: null, failed: false, counts: { open: 2, done: 0, wait: 0, dismiss: 0 } }] as never;
    const cols = (li: Element) => [...li.children].map((c) => c.className.split(' ').pop());
    state.page = 'history';
    render();
    const h1 = document.querySelector('.page-aside ul.history > li')!;
    state.page = 'log';
    render();
    const l1 = document.querySelector('.runlog-feed ul.history > li')!;
    expect(cols(l1)).toEqual(cols(h1));
    expect(cols(h1)).toEqual(['hlead', 'hwhen', 'h1', 'hfind', 'hcounts']);
    expect(document.querySelector('.runlog-feed')?.textContent).not.toMatch(/[▸▾]/);
    expect(l1.querySelector('.hlead')?.children.length).toBe(0);
    expect(h1.querySelector('.hlead .pick')).not.toBeNull();
  });

  it('履歴から開いた結果: 見出しは「履歴」で、履歴のタブが選ばれたまま。タブ（指摘・差分・記録）も、「すべて開く」「取り込む」の行も無い。結果が無くなれば、履歴へ戻る', () => {
    state.selected = fakeReview('x');
    state.backTo = 'history';
    state.page = 'result';
    render();
    expect(selected()).toEqual(['history']);
    expect(document.querySelectorAll('.pagetabs button').length).toBe(4);
    expect(document.querySelector('.page-title')?.textContent).toBe('履歴');
    expect(document.querySelector('.recipe-bar-top')).toBeNull();
    expect(document.querySelector('.result-compact')).not.toBeNull();
    expect(document.querySelector('.result nav.tabs')).toBeNull();
    expect(document.querySelector('.result')?.textContent).not.toContain('すべて開く');
    expect(document.querySelector('.result')?.textContent).not.toContain('まとめて CLI に渡す');
    expect(document.querySelector('.back-to-log-btn')?.textContent).toBe('← 履歴');
    state.selected = null;
    render();
    expect(state.page).toBe('history');
    expect(selected()).toEqual(['history']);
  });

  it('ログから開いた結果: 見出しは「ログ」で、ログのタブが選ばれたまま', () => {
    state.selected = fakeReview('y');
    state.backTo = 'log';
    state.page = 'result';
    render();
    expect(selected()).toEqual(['log']);
    expect(document.querySelector('.page-title')?.textContent).toBe('ログ');
    expect(document.querySelector('.result nav.tabs')).toBeNull();
  });

  it('実行した直後の結果（新規レビューの下）には、これまでどおり、タブがある', () => {
    state.selected = fakeReview('z');
    state.backTo = 'newreview';
    state.page = 'newreview';
    render();
    expect(document.querySelector('.result-anchor nav.tabs')).not.toBeNull();
    expect(document.querySelector('.result-compact')).toBeNull();
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

  it('結果の「戻る」は、来たページ（履歴・ログ）へ戻る', async () => {
    invokeMock.mockImplementation(async (cmd: string, args: { id?: string }) => {
      if (cmd === 'get_review') return fakeReview(args.id ?? 'x');
      if (cmd === 'build_handoff') return '';
      return null;
    });
    state.history = [{ id: 'a', created_at: 1_793_000_000_000, repo: '/r', recipe_title: 'バグ', provider_label: 'Mock', findings: 0, verified: 0, total_tokens: null, failed: false, counts: null }] as never;
    // 履歴のページから開く → 戻ると、履歴
    state.page = 'history';
    render();
    document.querySelector<HTMLElement>('.page-aside .history li, .page-aside .history button')!.click();
    await flush();
    expect(state.page).toBe('result');
    expect(state.selected?.id).toBe('a');
    const back = document.querySelector<HTMLElement>('.back-to-log-btn')!;
    expect(back.textContent).toBe('← 履歴');
    back.click();
    expect(state.page).toBe('history');
    expect(state.selected).toBeNull();

    // ログのページの行を押す → 実行の詳細が、その場で開く（指摘は出ない）。「結果を開く」で結果のページ。戻ると、ログ
    state.page = 'log';
    render();
    document.querySelector<HTMLElement>('.runlog-feed li')!.click();
    await flush();
    expect(state.page).toBe('log');
    expect(document.querySelector('.runlog-feed li .rld')).not.toBeNull();
    expect(document.querySelector('.runlog-feed li .finding')).toBeNull();
    const rows = [...document.querySelectorAll('.runlog-feed li .rld-row')].map((r) => r.textContent);
    expect(rows.some((t) => t?.includes('観点') && t.includes('バグ'))).toBe(true);
    expect(rows.some((t) => t?.includes('比較元') && t.includes('未コミットの変更'))).toBe(true);
    expect(rows.some((t) => t?.includes('差分') && t.includes('0 ファイル'))).toBe(true);
    document.querySelector<HTMLElement>('.runlog-feed li .rld-foot button')!.click();
    await flush();
    expect(state.page).toBe('result');
    expect(state.selected).not.toBeNull();
    const back2 = document.querySelector<HTMLElement>('.back-to-log-btn')!;
    expect(back2.textContent).toBe('← 実行ログ一覧');
    back2.click();
    expect(state.page).toBe('log');
    expect(state.selected).toBeNull();
  });

  describe('結果を開いたときの、戻り先', () => {
    const setup = () =>
      invokeMock.mockImplementation(async (cmd: string, args: { id?: string }) => {
        if (cmd === 'get_review') return fakeReview(args.id ?? 'x');
        if (cmd === 'build_handoff') return '';
        return null;
      });

    it('リポジトリのページ（履歴でもログでもない所）から開くと、戻り先はログ。結果のページになる', async () => {
      setup();
      state.page = 'repos';
      state.backTo = 'history';
      await select('a');
      expect(state.page).toBe('result');
      expect(state.backTo).toBe('log');
    });

    it('履歴・ログから開くと、そのページが戻り先になる', async () => {
      setup();
      state.page = 'history';
      await select('a');
      expect([state.page, state.backTo]).toEqual(['result', 'history']);
      state.page = 'log';
      await select('b');
      expect([state.page, state.backTo]).toEqual(['result', 'log']);
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

  it('結果のページで仕分け・メモを変えて、ログへ戻っても、ログの詳細は、変えた状態で出る（古いキャッシュを使わない）', async () => {
    const withFinding = (id: string): ReviewView => ({
      ...fakeReview(id),
      findings: [{ severity: 'medium', title: '指摘', file: 'a.ts', line: 1, detail: '本文', evidence: 'x', verified: true, rule: null }],
      triage: ['new'],
      finding_notes: [{ memo: '', memo_at: null, done_at: null, done_head: null, triage_at: null, triage_head: null, triage_source: null }],
    }) as unknown as ReviewView;
    invokeMock.mockImplementation(async (cmd: string, args: { id?: string }) => (cmd === 'get_review' ? withFinding(args.id ?? 'a') : cmd === 'build_handoff' ? '' : null));
    state.history = [{ id: 'a', created_at: 1_795_775_717_000, repo: '/r', recipe_title: 'バグ', provider_label: 'Mock', findings: 1, verified: 1, total_tokens: null, failed: false, counts: null }] as never;
    // ログの行を開く（記録が、キャッシュに入る）
    state.page = 'log';
    render();
    document.querySelector<HTMLElement>('.runlog-feed ul.history > li')!.click();
    await flush();
    expect(document.querySelector('.rld-finding')?.textContent).toContain('（記入なし）');
    // 「結果を開く」で、結果のページへ。そこで、仕分けとメモを変える（開いている記録を、直接書き換える）
    document.querySelector<HTMLElement>('.rld-foot button')!.click();
    await flush();
    expect(state.page).toBe('result');
    state.selected!.triage[0] = 'fix';
    state.selected!.finding_notes[0].memo = '直しました';
    // ログへ戻る
    document.querySelector<HTMLElement>('.back-to-log-btn')!.click();
    expect(state.page).toBe('log');
    const f = document.querySelector('.rld-finding')!;
    expect(f.textContent).toContain('直しました');
    expect(f.textContent).not.toContain('（記入なし）');
  });

  describe('ログの行を開いたとき、詳細を読み込めない', () => {
    const openFirstRow = async () => {
      state.history = [{ id: 'a', created_at: 1_795_775_717_000, repo: '/r', recipe_title: 'バグ', provider_label: 'Mock', findings: 0, verified: 0, total_tokens: null, failed: false, counts: null }] as never;
      state.page = 'log';
      render();
      document.querySelector<HTMLElement>('.runlog-feed ul.history > li')!.click();
      await flush();
    };

    it('記録が無い（null）とき: 取得できなかった旨と理由が出て、読み込み中の表示は消える', async () => {
      invokeMock.mockImplementation(async () => null);
      await openFirstRow();
      const d = document.querySelector('.runlog-card-detail')!;
      expect(d.classList.contains('loading')).toBe(false);
      expect(d.textContent).toContain('レビューの詳細を取得できませんでした。');
      expect(d.textContent).toContain('レビューの記録が見つかりません');
      expect(state.loadingRunLogs.size).toBe(0);
    });

    it('例外が出たとき: 読み込み中の状態が解除され、理由が画面に出る（握りつぶさない）', async () => {
      invokeMock.mockImplementation(async () => {
        throw new Error('記録が壊れています');
      });
      await openFirstRow();
      const d = document.querySelector('.runlog-card-detail')!;
      expect(d.classList.contains('loading')).toBe(false);
      expect(d.textContent).toContain('記録が壊れています');
      expect(state.loadingRunLogs.size).toBe(0);
    });

    it('閉じて、開き直すと、もう一度読み込む（前の失敗を引きずらない）', async () => {
      let n = 0;
      invokeMock.mockImplementation(async (cmd: string, args: { id?: string }) => {
        if (cmd !== 'get_review') return null;
        n++;
        if (n === 1) throw new Error('一時的な失敗');
        return fakeReview(args.id ?? 'x');
      });
      await openFirstRow();
      expect(document.querySelector('.runlog-card-detail')?.textContent).toContain('一時的な失敗');
      document.querySelector<HTMLElement>('.runlog-feed ul.history > li')!.click(); // 閉じる
      document.querySelector<HTMLElement>('.runlog-feed ul.history > li')!.click(); // 開き直す
      await flush();
      expect(document.querySelector('.runlog-card-detail.rld')).not.toBeNull();
      expect(state.runLogLoadErrors.size).toBe(0);
    });
  });
});
