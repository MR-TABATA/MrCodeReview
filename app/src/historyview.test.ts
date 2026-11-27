// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { circleNumber, countsLine, dayKey, displayRecipeTitle, formatWhen, historyPanel, needsAttention, prunePicks, RECENT_KEEP, reviewName, splitHistory } from './historyview';
import type { ReviewSummary } from './types';

const S = (id: string, over: Partial<ReviewSummary> = {}): ReviewSummary => ({ id, created_at: 1_793_000_000_000, repo: '/Users/me/Git/MrkDown', recipe_title: '潜在バグ', provider_label: 'Claude', findings: 3, verified: 2, total_tokens: 1820, failed: false, counts: { open: 1, done: 1, wait: 0, dismiss: 1 }, ...over });
const ITEMS = [S('a'), S('b', { failed: true, total_tokens: null }), S('c')];
const acts = () => ({ open: vi.fn(), toggle: vi.fn(), toggleDay: vi.fn(), toggleAll: vi.fn(), report: vi.fn(), remove: vi.fn() });
const render = (picked: string[], a = acts(), openId: string | null = null, items = ITEMS, openDays: string[] = []) => ({ el: historyPanel(items, openId, new Set(picked), 'Git/MrkDown', '/Users/me/Git/MrkDown', 'まだありません', a, new Set(openDays)), a });

describe('reviewName and circleNumber', () => {
  it('1〜50 は Unicode 丸数字に変換し、51 以降は (n) にフォールバックする', () => {
    expect(circleNumber(1)).toBe('①');
    expect(circleNumber(20)).toBe('⑳');
    expect(circleNumber(21)).toBe('㉑');
    expect(circleNumber(35)).toBe('㉟');
    expect(circleNumber(36)).toBe('㊱');
    expect(circleNumber(50)).toBe('㊿');
    expect(circleNumber(51)).toBe('(51)');
  });

  it('reviewName は観点名と半角スペース + 丸数字を組み合わせる。番号なしは観点名のみ', () => {
    expect(reviewName('潜在バグ', 1)).toBe('潜在バグ ①');
    expect(reviewName('潜在バグ', 2)).toBe('潜在バグ ②');
    expect(reviewName('潜在バグ', null)).toBe('潜在バグ');
    expect(reviewName('潜在バグ', undefined)).toBe('潜在バグ');
    expect(reviewName('bugs', 3, 'en')).toBe('Latent Bugs ③');
  });
});

describe('prunePicks', () => {
  it('履歴に無い識別子は、選んだままにしない', () => {
    expect([...prunePicks(new Set(['a', 'zzz', 'c']), ITEMS)].sort()).toEqual(['a', 'c']);
    expect(prunePicks(new Set(), ITEMS).size).toBe(0);
    expect(prunePicks(new Set(['a']), []).size).toBe(0);
  });
});

describe('historyPanel', () => {
  it('各行に、チェックの欄がある。選んだ行に印が付く。結果を開いている行にも印が付く', () => {
    const { el } = render(['b'], acts(), 'a');
    const rows = [...el.querySelectorAll('li')];
    expect(rows).toHaveLength(3);
    expect(rows.map((r) => (r.querySelector('.pick') as HTMLInputElement).checked)).toEqual([false, true, false]);
    expect(rows[0]?.className).toContain('on');
    expect(rows[1]?.className).toContain('picked');
    expect(rows[1]?.textContent).toContain('失敗');
  });

  it('1 行は、目次に絞る: 観点・日時・指摘の件数だけ。リポジトリ名・根拠あり・トークンは出さない', () => {
    const { el } = render([]);
    const row = el.querySelector('li') as HTMLElement;
    expect(row.querySelector('.ht')?.textContent).toBe('潜在バグ');
    expect(row.querySelector('.hwhen')?.textContent).toMatch(/^2026\/\d+\/\d+ /);
    expect(row.querySelector('.hfind')?.textContent).toBe('指摘 3 件');
    for (const w of ['トークン', '根拠あり', 'MrkDown', 'Git/']) expect(row.textContent, w).not.toContain(w);
    // 失敗の行でも、トークンの有無は出さない
    expect(render([]).el.querySelectorAll('li')[1]?.textContent).not.toContain('トークン');
  });

  it('仕分けの件数: 未・済み・待ち・無視の 4 つを、1 行に出す。足すと、指摘の数', () => {
    const { el } = render([]);
    const row = el.querySelector('li') as HTMLElement;
    expect([...row.querySelectorAll('.hcounts .hc')].map((c) => c.textContent)).toEqual(['未 1 件', '済み 1 件', '待ち 0 件', '無視 1 件']);
    expect(row.querySelectorAll('.hcounts'), '仕分けの件数は、同じ 1 行の中').toHaveLength(1);
  });

  it('0 件の項目は薄くする。指摘が 0 件のレビュー・失敗したレビューには、件数の行を出さない', () => {
    const line = countsLine({ open: 2, done: 0, wait: 0, dismiss: 3 });
    expect([...line.querySelectorAll('.hc')].map((c) => c.className.includes('zero'))).toEqual([false, true, true, false]);
    const items = [S('a', { findings: 0, counts: { open: 0, done: 0, wait: 0, dismiss: 0 } }), S('b', { findings: 0, failed: true, counts: { open: 0, done: 0, wait: 0, dismiss: 0 } })];
    const { el } = render([], acts(), null, items);
    expect(el.querySelector('.hcounts')).toBeNull();
  });

  it('件数が読み取れなかったとき（null）は、0 件に見せず、件数の行を出さない', () => {
    const { el } = render([], acts(), null, [S('a', { counts: null })]);
    expect(el.querySelector('.hcounts')).toBeNull();
    expect(el.querySelector('li')?.textContent).toContain('指摘 3 件');
  });

  it('1 行: 日時・観点・指摘の件数・仕分けの件数が、1 つの行に並ぶ。「詳細」のボタンは無い（行を押すと開く）', () => {
    const { el } = render([]);
    const row = el.querySelector('li') as HTMLElement;
    expect([...row.children].map((c) => c.className.split(' ').pop())).toEqual(['hlead', 'hwhen', 'h1', 'hfind', 'hcounts']);
    expect(row.querySelector('.hdetail')).toBeNull();
    expect(row.querySelector('.hbody')).toBeNull();
    expect(row.querySelector('.ht')?.textContent).toBe('潜在バグ');
    expect(row.querySelector('.hfind')?.textContent).toBe('指摘 3 件');
  });

  it('チェックを押すと toggle を呼び、行を開かない。行を押すと open を呼ぶ', () => {
    const { el, a } = render([]);
    const first = el.querySelector('li') as HTMLElement;
    (first.querySelector('.pick') as HTMLInputElement).click();
    expect(a.open).not.toHaveBeenCalled();
    (first.querySelector('.pick') as HTMLInputElement).dispatchEvent(new Event('change'));
    expect(a.toggle).toHaveBeenCalledExactlyOnceWith('a');
    first.querySelector('.ht')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(a.open).toHaveBeenCalledExactlyOnceWith('a');
  });

  it('「選んだ履歴を削除」は、1 件以上選んだときだけ出て、押すと remove が呼ばれる（確認は、呼ぶ側）', () => {
    const none = render([]);
    expect([...none.el.querySelectorAll('button')].some((b) => b.textContent === '選んだ履歴を削除')).toBe(false);
    const { el, a } = render(['a', 'c']);
    const btn = [...el.querySelectorAll('button')].find((b) => b.textContent === '選んだ履歴を削除') as HTMLButtonElement;
    expect(btn).toBeTruthy();
    btn.click();
    expect(a.remove).toHaveBeenCalledOnce();
    expect(a.report).not.toHaveBeenCalled();
  });

  it('選んだ件数と「レポートに出す」は、1 件以上選んだときだけ出る', () => {
    expect(render([]).el.querySelector('.pickbar')).toBeNull();
    const { el, a } = render(['a', 'c']);
    const bar = el.querySelector('.pickbar') as HTMLElement;
    expect(bar.textContent).toContain('2 件を選択中');
    (bar.querySelector('button') as HTMLElement).click();
    expect(a.report).toHaveBeenCalledOnce();
  });

  it('「すべて選ぶ」は、全部選んでいれば「すべて外す」になる。履歴が無ければ出ない', () => {
    const label = (picked: string[]) => (render(picked).el.querySelector('.hhead button') as HTMLElement).textContent;
    expect(label([])).toBe('すべて選ぶ');
    expect(label(['a'])).toBe('すべて選ぶ');
    expect(label(['a', 'b', 'c'])).toBe('すべて外す');
    const { el, a } = render([]);
    (el.querySelector('.hhead button') as HTMLElement).click();
    expect(a.toggleAll).toHaveBeenCalledOnce();
    const empty = render([], acts(), null, []).el;
    expect(empty.querySelector('.hhead button')).toBeNull();
    expect(empty.textContent).toContain('まだありません');
    expect(empty.querySelector('.pickbar')).toBeNull();
  });

  it('範囲（リポジトリ名）を出す', () => {
    expect(render([]).el.querySelector('.scope-line')?.textContent).toBe('Git/MrkDown');
  });

  it('isCollapsed が true のときは履歴の行や日別グループを出さず、ヘッダーのみ表示する', () => {
    const a = acts();
    const el = historyPanel(ITEMS, null, new Set(), 'Git/MrkDown', '/path', '空', a, new Set(), true);
    expect(el.querySelector('.history')).toBeNull();
    expect(el.querySelector('.scope-line')).toBeNull();
    const title = el.querySelector('.hhead-title') as HTMLElement;
    expect(title.textContent).toContain('履歴');
    expect(title.textContent).toContain('(3)');
  });
});

// ── 新しい 5 件と「未」のあるものだけを出し、古いものは、年月日ごとにまとめる ──
const DAY = 86_400_000;
/** 手元の時刻の、ある日の昼（時差に依らず、日付が決まるように） */
const noon = (y: number, m: number, d: number) => new Date(y, m - 1, d, 12).getTime();
const DONE = { open: 0, done: 3, wait: 0, dismiss: 0 };
const OPEN = { open: 2, done: 1, wait: 0, dismiss: 0 };
const at = (id: string, ms: number, over: Partial<ReviewSummary> = {}) => S(id, { created_at: ms, counts: DONE, ...over });

describe('needsAttention（「未」が残っているか）', () => {
  it('未が 1 件以上ある指摘つきのレビューだけ。指摘が 0 件・失敗・全部済みは、対象外', () => {
    expect(needsAttention(S('a', { counts: OPEN }))).toBe(true);
    expect(needsAttention(S('a', { counts: DONE }))).toBe(false);
    expect(needsAttention(S('a', { findings: 0, counts: { open: 0, done: 0, wait: 0, dismiss: 0 } }))).toBe(false);
  });
  it('件数が読めなかったとき（null）は、隠さない（出す）。ただし、指摘が 0 件なら、対象外', () => {
    expect(needsAttention(S('a', { counts: null }))).toBe(true);
    expect(needsAttention(S('a', { findings: 0, counts: null }))).toBe(false);
  });
});

describe('splitHistory', () => {
  const day = (n: number) => noon(2026, 10, 27) - n * DAY;
  it('新しい 5 件は、いつも出す。それより古いもので、未が無いものは、日付ごとにまとめる（新しい日から）', () => {
    const items = [...Array(5)].map((_, i) => at(`n${i}`, day(0) - i * 1000)).concat([at('d1a', day(1)), at('d1b', day(1) - 1000), at('d3', day(3))]);
    const { shown, groups } = splitHistory(items);
    expect(shown.map((s) => s.id)).toEqual(['n0', 'n1', 'n2', 'n3', 'n4']);
    expect(groups.map((g) => [g.key, g.label, g.items.map((s) => s.id)])).toEqual([
      ['2026-10-26', '2026年10月26日', ['d1a', 'd1b']],
      ['2026-10-24', '2026年10月24日', ['d3']],
    ]);
    expect(RECENT_KEEP).toBe(5);
  });

  it('古くても、未が残っているものは、いつも出す（新しい順の並びのまま）', () => {
    const items = [...Array(5)].map((_, i) => at(`n${i}`, day(0) - i * 1000)).concat([at('old-open', day(9), { counts: OPEN }), at('old-done', day(9) - 1000)]);
    const { shown, groups } = splitHistory(items);
    expect(shown.map((s) => s.id)).toEqual(['n0', 'n1', 'n2', 'n3', 'n4', 'old-open']);
    expect(groups.flatMap((g) => g.items.map((s) => s.id))).toEqual(['old-done']);
  });

  it('5 件以下なら、全部出す。グループは、無い', () => {
    const { shown, groups } = splitHistory([at('a', day(0)), at('b', day(30))]);
    expect(shown).toHaveLength(2);
    expect(groups).toEqual([]);
  });

  it('日付の区切りは、手元の時刻の、年月日（同じ日は、1 つのグループ）', () => {
    expect(dayKey(noon(2026, 1, 5))).toBe('2026-01-05');
    const items = [...Array(5)].map((_, i) => at(`n${i}`, day(0))).concat([at('x', noon(2026, 3, 9)), at('y', new Date(2026, 2, 9, 1).getTime()), at('z', new Date(2026, 2, 8, 23).getTime())]);
    expect(splitHistory(items).groups.map((g) => [g.key, g.items.length])).toEqual([['2026-03-09', 2], ['2026-03-08', 1]]);
  });
});

describe('historyPanel の日付のグループ', () => {
  const day = (n: number) => noon(2026, 10, 27) - n * DAY;
  const many = () => [...Array(5)].map((_, i) => at(`n${i}`, day(0) - i * 1000)).concat([at('d1a', day(1)), at('d1b', day(1) - 1000), at('d3', day(3))]);

  it('古いものは、日付の見出し（件数つき）だけを出し、閉じておく。押すと toggleDay を呼ぶ', () => {
    const { el, a } = render([], acts(), null, many());
    expect(el.querySelectorAll('ul.history > li')).toHaveLength(5);
    const heads = [...el.querySelectorAll('.hday-h')];
    expect(heads.map((b) => b.textContent)).toEqual(['▸ 2026年10月26日（2 件）', '▸ 2026年10月24日（1 件）']);
    expect(el.querySelectorAll('.hday ul')).toHaveLength(0);
    (heads[0] as HTMLElement).click();
    expect(a.toggleDay).toHaveBeenCalledExactlyOnceWith('2026-10-26');
  });

  it('開いているグループは、行を出す。結果を開いているレビューが入っているグループは、開く', () => {
    const open = render([], acts(), null, many(), ['2026-10-26']).el;
    expect([...open.querySelectorAll('.hday')[0]!.querySelectorAll('li')].map((li) => li.querySelector('.ht')?.textContent)).toHaveLength(2);
    expect(open.querySelectorAll('.hday-h')[0]?.textContent).toBe('▾ 2026年10月26日（2 件）');
    const auto = render([], acts(), 'd3', many()).el;
    expect(auto.querySelectorAll('.hday-h')[1]?.textContent).toBe('▾ 2026年10月24日（1 件）');
    expect(auto.querySelectorAll('.hday-h')[0]?.textContent, '関係のないグループは、閉じたまま').toBe('▸ 2026年10月26日（2 件）');
  });

  it('グループの中の行も、同じ操作ができる（開く）', () => {
    const { el, a } = render([], acts(), null, many(), ['2026-10-26']);
    const row = el.querySelector('.hday li') as HTMLElement;
    row.querySelector('.ht')!.dispatchEvent(new MouseEvent('click', { bubbles: true }));
    expect(a.open).toHaveBeenCalledExactlyOnceWith('d1a');
  });

  it('古くても、未が残っているものは、グループに入れず、上に出す', () => {
    const items = [...Array(5)].map((_, i) => at(`n${i}`, day(0) - i * 1000)).concat([at('old-open', day(20), { counts: OPEN, recipe_title: '未が残る' })]);
    const { el } = render([], acts(), null, items);
    expect(el.querySelectorAll('ul.history > li')).toHaveLength(6);
    expect(el.querySelector('.hday')).toBeNull();
  });

  it('displayRecipeTitle は英語表示時に組み込み観点とリンターを Linters に揃えて返す', () => {
    expect(displayRecipeTitle('リンター', 'en')).toBe('Linters');
    expect(displayRecipeTitle('lint', 'en')).toBe('Linters');
    expect(displayRecipeTitle('他ファイルとの不整合', 'en')).toBe('Cross-file Inconsistencies');
    expect(displayRecipeTitle('リンター', 'ja')).toBe('リンター');
  });
});

describe('formatWhen（日時の表示は、言語に従う）', () => {
  const ms = Date.UTC(2026, 10, 18, 12, 0, 0);
  it('日本語は日本語の書式、英語は英語の書式。同じ日時で、書式が違う', () => {
    const ja = formatWhen(ms, 'ja');
    const en = formatWhen(ms, 'en');
    expect(ja).toBe(new Date(ms).toLocaleString('ja-JP'));
    expect(en).toBe(new Date(ms).toLocaleString('en-US'));
    expect(ja).toMatch(/^2026\/\d+\/\d+/); // 年/月/日（日付は、タイムゾーンで 1 日ずれうる）
    expect(en).toMatch(/AM|PM/); // 12 時間表記
    expect(ja).not.toBe(en);
  });
});
