// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { htmlNodes, newReportState, reportFileName, reportOptions, reportPanel, reportRange, ymd, type ReportState } from './reportview';
import type { ReportSummary } from './types';

const NOW = new Date(2026, 9, 27, 8, 30); // 2026-10-27 08:30（この Mac の時刻）
const day = (y: number, m: number, d: number): number => new Date(y, m - 1, d).getTime();

describe('reportRange', () => {
  const st = (over: Partial<ReportState>): ReportState => ({ ...newReportState(), ...over });

  it('すべては、限りなし。今日は、今日の 0 時から。7 日間・30 日間は、今日を含めて数える', () => {
    expect(reportRange(st({ period: 'all' }), NOW)).toEqual({ since: null, until: null, error: '' });
    expect(reportRange(st({ period: 'today' }), NOW)).toEqual({ since: day(2026, 10, 27), until: null, error: '' });
    expect(reportRange(st({ period: 'week' }), NOW).since).toBe(day(2026, 10, 21));
    expect(reportRange(st({ period: 'month' }), NOW).since, '月をまたぐ').toBe(day(2026, 9, 28));
    expect(reportRange(st({ period: 'week' }), new Date(2026, 0, 3)).since, '年をまたぐ').toBe(day(2025, 12, 28));
  });

  it('期間を指定: 終了の日は、その日の終わりまで含める。片方だけでもよい', () => {
    const r = reportRange(st({ period: 'custom', from: '2026-10-20', to: '2026-10-22' }), NOW);
    expect(r).toEqual({ since: day(2026, 10, 20), until: day(2026, 10, 23) - 1, error: '' });
    expect(reportRange(st({ period: 'custom', from: '2026-10-20' }), NOW)).toEqual({ since: day(2026, 10, 20), until: null, error: '' });
    expect(reportRange(st({ period: 'custom', to: '2026-10-22' }), NOW)).toEqual({ since: null, until: day(2026, 10, 23) - 1, error: '' });
    expect(reportRange(st({ period: 'custom' }), NOW)).toEqual({ since: null, until: null, error: '' });
    expect(reportRange(st({ period: 'custom', from: '2026-10-22', to: '2026-10-22' }), NOW).error).toBe('');
  });

  it('正しくない日付と、逆の順は、理由を返す', () => {
    expect(reportRange(st({ period: 'custom', from: '2026-02-30' }), NOW).error).toContain('開始');
    expect(reportRange(st({ period: 'custom', from: 'abc' }), NOW).error).toContain('開始');
    expect(reportRange(st({ period: 'custom', to: '2026-13-01' }), NOW).error).toContain('終了');
    expect(reportRange(st({ period: 'custom', from: '2026-10-23', to: '2026-10-22' }), NOW).error).toContain('より後');
  });
});

describe('reportOptions', () => {
  it('選んでいるリポジトリか、すべて。含める項目と時差を、そのまま渡す', () => {
    const st = { ...newReportState(), period: 'today' as const, evidence: false, prompt: true, tokens: false, log: false };
    const o = reportOptions(st, '/Users/me/Git/MrkDown', NOW);
    expect(o).toEqual({ repo: '/Users/me/Git/MrkDown', since_ms: day(2026, 10, 27), until_ms: null, include_evidence: false, include_prompt: true, include_tokens: false, include_log: false, tz_offset_min: -NOW.getTimezoneOffset(), review_ids: null, lang: 'ja' });
    expect(reportOptions({ ...st, allRepos: true }, '/x', NOW).repo).toBeNull();
    expect(reportOptions(st, '', NOW).repo, 'リポジトリを選んでいなければ、すべて').toBeNull();
  });
  it('履歴から選んだときは、識別子を渡し、リポジトリと期間は渡さない', () => {
    const st = { ...newReportState(), period: 'today' as const, reviewIds: ['a', 'b'] };
    const o = reportOptions(st, '/Users/me/Git/MrkDown', NOW);
    expect([o.review_ids, o.repo, o.since_ms, o.until_ms]).toEqual([['a', 'b'], null, null, null]);
    expect(reportOptions({ ...st, reviewIds: [] }, '/x', NOW).review_ids, '何も選んでいなければ、空の一覧（全件にしない）').toEqual([]);
  });
  it('既定は、引用あり・トークンあり・経過あり・送った文章の全文なし', () => {
    const s = newReportState();
    expect([s.evidence, s.tokens, s.log, s.prompt, s.allRepos, s.period]).toEqual([true, true, true, false, false, 'week']);
  });
});

describe('reportFileName', () => {
  it('リポジトリ名と日付。ファイル名に使えない文字は、_ にする', () => {
    expect(reportFileName('/Users/me/Git/MrkDown', false, 'md', NOW)).toBe('レビュー記録_MrkDown_2026-10-27.md');
    expect(reportFileName('/Users/me/Git/Mrk Down: v2?', false, 'xlsx', NOW)).toBe('レビュー記録_Mrk_Down_v2_2026-10-27.xlsx');
    expect(reportFileName('/x/y', true, 'md', NOW)).toBe('レビュー記録_すべて_2026-10-27.md');
    expect(reportFileName('', false, 'md', NOW)).toBe('レビュー記録_すべて_2026-10-27.md');
    expect(reportFileName('/', false, 'md', NOW)).toContain('repo');
    expect(reportFileName('/x/y', false, 'xlsx', NOW, 3)).toBe('レビュー記録_選択3件_2026-10-27.xlsx');
    expect(reportFileName('/x/y', false, 'md', NOW, 0)).toBe('レビュー記録_y_2026-10-27.md');
    expect(ymd(new Date(2026, 0, 5))).toBe('2026-01-05');
  });
});

describe('htmlNodes', () => {
  it('スクリプト・外部の読み込み・イベント・javascript: を落とし、文章と構造は残す', () => {
    const html =
      '<article class="report"><h1>題</h1><script>window.__x=1</script><p onclick="alert(1)">本文 <b>太</b></p><a href="javascript:alert(1)">l</a>' +
      '<iframe src="https://example.com"></iframe><img src="x" onerror="alert(2)"><style>body{display:none}</style><form action="/x"><input></form><code>a&lt;b</code></article>';
    const host = document.createElement('div');
    host.append(...htmlNodes(html));
    expect(host.querySelector('script, iframe, style, form')).toBeNull();
    expect(host.querySelector('[onclick], [onerror]')).toBeNull();
    expect(host.querySelector('a')?.hasAttribute('href')).toBe(false);
    expect(host.querySelector('img')?.hasAttribute('src')).toBe(false);
    expect(host.querySelector('h1')?.textContent).toBe('題');
    expect(host.querySelector('p')?.textContent).toBe('本文 太');
    expect(host.querySelector('article')?.className).toBe('report');
    expect(host.querySelector('code')?.textContent, 'エスケープされた文字は、文字のまま').toBe('a<b');
    expect((window as unknown as { __x?: number }).__x).toBeUndefined();
  });
});

describe('reportPanel', () => {
  const SUM: ReportSummary = { reviews: 3, failed: 1, findings: 4, fix: 1, wait: 1, dismiss: 1, new: 1, done: 1, input: 1500, output: 320, cache_read: 0, cache_write: 0, unreported: 1 };
  const acts = () => ({ changed: vi.fn(), save: vi.fn(), print: vi.fn(), clearSelection: vi.fn(), close: vi.fn() });
  const buttons = (el: HTMLElement) => [...el.querySelectorAll('.rout button')] as HTMLButtonElement[];

  it('プレビューと件数を出し、3 つの出力ボタンが押せる。押すと、その形式で呼ぶ', () => {
    const st = { ...newReportState(), preview: { html: '<article class="report"><h1>レビューの記録</h1></article>', summary: SUM } };
    const a = acts();
    const el = reportPanel(st, a, true);
    expect(el.querySelector('.rpreview h1')?.textContent).toBe('レビューの記録');
    expect(el.querySelector('.rcount')?.textContent).toBe('レビュー 3 件・指摘 4 件（対応 1・待ち 1・無視 1・未仕分け 1）');
    const [md, xlsx, pdf] = buttons(el);
    expect([md?.textContent, xlsx?.textContent, pdf?.textContent]).toEqual(['Markdown で保存', 'Excel で保存', 'PDF で出力']);
    expect([md?.disabled, xlsx?.disabled, pdf?.disabled]).toEqual([false, false, false]);
    md?.click();
    xlsx?.click();
    pdf?.click();
    expect(a.save.mock.calls).toEqual([['md'], ['xlsx']]);
    expect(a.print).toHaveBeenCalledOnce();
    expect(el.textContent).toContain('秘密の値');
  });

  it('レビューが 0 件・集めている最中・出力中・期間の誤りのときは、出力できない', () => {
    const none = reportPanel({ ...newReportState(), preview: { html: '<p>なし</p>', summary: { ...SUM, reviews: 0 } } }, acts(), true);
    expect(buttons(none).every((b) => b.disabled)).toBe(true);
    expect(buttons(reportPanel({ ...newReportState(), loading: true }, acts(), true)).every((b) => b.disabled)).toBe(true);
    const ok = { ...newReportState(), preview: { html: '<p>x</p>', summary: SUM } };
    expect(buttons(reportPanel({ ...ok, busy: 'xlsx' }, acts(), true)).every((b) => b.disabled)).toBe(true);
    const bad = reportPanel({ ...ok, period: 'custom', from: '2026-10-23', to: '2026-10-22' }, acts(), true);
    expect(bad.querySelector('.err')?.textContent).toContain('より後');
    expect(buttons(bad).every((b) => b.disabled)).toBe(true);
  });

  it('期間・リポジトリ・項目を変えると、changed を呼ぶ。期間を指定すると、日付の欄が出る', () => {
    const a = acts();
    const st = { ...newReportState(), preview: { html: '<p>x</p>', summary: SUM } };
    const el = reportPanel(st, a, true);
    expect(el.querySelector('.rdate')).toBeNull();
    ([...el.querySelectorAll('.dmode button')].find((b) => b.textContent === '今日') as HTMLElement).click();
    expect(st.period).toBe('today');
    ([...el.querySelectorAll('.dmode button')].find((b) => b.textContent === 'すべてのリポジトリ') as HTMLElement).click();
    expect(st.allRepos).toBe(true);
    const box = el.querySelector('.rcheck input') as HTMLInputElement; // 引用
    box.checked = false;
    box.dispatchEvent(new Event('change'));
    expect(st.evidence).toBe(false);
    expect(a.changed).toHaveBeenCalledTimes(3);
    const custom = reportPanel({ ...st, period: 'custom' }, a, true);
    expect(custom.querySelectorAll('.rdate')).toHaveLength(2);
  });

  it('履歴から選んだレビューのときは、件数と「期間の指定に戻す」を出し、期間とリポジトリの欄は出さない', () => {
    const a = acts();
    const st = { ...newReportState(), reviewIds: ['a', 'b', 'c'], preview: { html: '<p>x</p>', summary: SUM } };
    const el = reportPanel(st, a, true);
    expect(el.querySelector('.pickedinfo')?.textContent).toContain('3 件');
    expect(el.querySelector('.dmode'), '期間・リポジトリの選択は、出さない').toBeNull();
    ([...el.querySelectorAll('.pickedinfo button')].find((b) => b.textContent === '期間の指定に戻す') as HTMLElement).click();
    expect(a.clearSelection).toHaveBeenCalledOnce();
    ([...el.querySelectorAll('.pickedinfo button')].find((b) => b.textContent === '閉じる') as HTMLElement).click();
    expect(a.close).toHaveBeenCalledOnce();
    expect(buttons(el).every((b) => !b.disabled), '選んだレビューも、出力できる').toBe(true);
    // 選んでいないときは、出ない
    const normal = reportPanel(newReportState(), acts(), true);
    expect(normal.querySelector('.pickedinfo')).toBeNull();
    expect(normal.querySelector('.dmode')).not.toBeNull();
  });

  it('リポジトリを選んでいないときは、「選んでいるリポジトリ」を選べない', () => {
    const el = reportPanel(newReportState(), acts(), false);
    const first = [...el.querySelectorAll('.dmode button')].find((b) => b.textContent === '選んでいるリポジトリ') as HTMLButtonElement;
    expect(first.disabled).toBe(true);
  });

  it('英語モードで英語タイトル、ボタン、チェックボックスが表示される', () => {
    const st = { ...newReportState(), preview: { html: '<p>x</p>', summary: SUM } };
    const el = reportPanel(st, acts(), true, 'en');
    const text = el.textContent ?? '';
    expect(text).toContain('Report');
    expect(text).toContain('Close');
    expect(text).toContain('Selected repository');
    expect(text).toContain('All repositories');
    expect(text).toContain('Include code snippets');
    expect(text).toContain('Include token usage');
    expect(text).toContain('Save as Markdown');
    expect(text).toContain('Save as Excel');
    expect(text).toContain('Export as PDF');
  });
});
