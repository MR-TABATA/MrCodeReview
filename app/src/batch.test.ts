// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { batchEstimate, batchPanel, batchSummary, newBatchUi, pickedRecipes, STOPPED, type BatchContext, type BatchUi } from './batch';
import type { DurationStats } from './duration';
import type { Recipe } from './types';

const R = (id: string, title: string, kind: 'llm' | 'tool' = 'llm'): Recipe => ({ id, title, source: 'builtin', prompt: 'p', kind });
const RECIPES = [R('bug', '潜在バグ'), R('sec', 'セキュリティ'), R('perf', 'パフォーマンス'), R('lint', 'リンター', 'tool')];
const S = (n: number, min: number, median: number, max: number): DurationStats => ({ n, min_ms: min, median_ms: median, max_ms: max });
const ids = (...x: string[]) => new Set(x);
const btn = (el: HTMLElement, label: string | RegExp) => [...el.querySelectorAll('button')].find((b) => (typeof label === 'string' ? b.textContent === label : label.test(b.textContent ?? ''))) as HTMLButtonElement | undefined;

describe('pickedRecipes', () => {
  it('観点の並びの順に返す（クリックした順ではなく、いつも同じ順に実行する）', () => {
    expect(pickedRecipes(RECIPES, ids('perf', 'bug')).map((r) => r.id)).toEqual(['bug', 'perf']);
    expect(pickedRecipes(RECIPES, ids('nope'))).toEqual([]);
  });
});

describe('batchEstimate（実行前の見積もり）', () => {
  const two = pickedRecipes(RECIPES, ids('bug', 'sec'));
  it('送る量は、1 回の概算 × LLM を使う回数。リンターは、数えない', () => {
    const e = batchEstimate(pickedRecipes(RECIPES, ids('bug', 'sec', 'lint')), {}, 35_000, false);
    expect([e.count, e.llm]).toEqual([3, 2]);
    expect(e.tokensText).toBe('送る量: 約 35,000 トークン × 2 回 ＝ 約 70,000 トークン（概算。入力だけ）');
    expect(batchEstimate(pickedRecipes(RECIPES, ids('lint')), {}, 35_000, false).tokensText).toBe('LLM は使いません。トークンは 0 です。');
    expect(batchEstimate(two, {}, null, false).tokensText).toBe('LLM に 2 回、送ります。');
  });

  it('時間は、観点ごとの過去の実績（中央値と幅）を足す', () => {
    const e = batchEstimate(two, { bug: S(3, 10_000, 15_000, 20_000), sec: S(2, 20_000, 30_000, 40_000) }, 1, false);
    expect(e.timeText).toBe('目安: 約 45 秒（30 秒〜1 分）');
    // 幅が無い（最小＝最大）ときは、幅を付けない
    expect(batchEstimate(two, { bug: S(1, 10_000, 10_000, 10_000), sec: S(1, 20_000, 20_000, 20_000) }, 1, false).timeText).toBe('目安: 約 30 秒');
  });

  it('実績の無い観点は、「実績なし」と言う（推定で埋めない）。全部無ければ、そう言う', () => {
    const part = batchEstimate(two, { bug: S(3, 10_000, 15_000, 20_000), sec: S(0, 0, 0, 0) }, 1, false);
    expect(part.timeText).toBe('目安: 実績のある 1 観点で 約 15 秒（10 秒〜20 秒）。ほか 1 観点は、実績なし');
    expect(batchEstimate(two, { bug: S(0, 0, 0, 0), sec: S(0, 0, 0, 0) }, 1, false).timeText).toContain('実績なし');
  });

  it('実績を読み込み中（未取得）のものがあれば、時間は出さない。モックは、時間を測らないので、出さない。選んでいなければ、出さない', () => {
    expect(batchEstimate(two, { bug: S(3, 1, 2, 3) }, 1, false).timeText).toBe('');
    expect(batchEstimate(two, { bug: S(3, 1, 2, 3), sec: S(3, 1, 2, 3) }, 1, true).timeText).toBe('');
    expect(batchEstimate([], {}, 1, false)).toMatchObject({ count: 0, timeText: '' });
  });
});

describe('batchSummary', () => {
  it('成功・失敗・中止を、件数で言う', () => {
    const ok = { id: 'a', title: 'A', ok: true, note: '' };
    const bad = { id: 'b', title: 'B', ok: false, note: 'タイムアウト' };
    const stopped = { id: 'c', title: 'C', ok: false, note: STOPPED };
    expect(batchSummary([ok, ok])).toBe('2 件中 2 件成功');
    expect(batchSummary([ok, bad, stopped])).toBe('3 件中 1 件成功・1 件失敗・1 件は、中止して実行せず');
  });
});

describe('batchPanel', () => {
  const ctx = (over: Partial<BatchContext> = {}): BatchContext => ({ stats: {}, inputTokens: 35_000, isMock: false, reason: '', elapsed: '12 秒', lintNote: '', ...over });
  const acts = () => ({ toggle: vi.fn(), pick: vi.fn(), start: vi.fn(), confirm: vi.fn(), cancelConfirm: vi.fn(), stop: vi.fn() });
  const ui = (over: Partial<BatchUi> = {}): BatchUi => ({ ...newBatchUi(), open: true, ...over });

  it('畳んでいるときは、見出しだけ。押すと開閉する', () => {
    const a = acts();
    const el = batchPanel(ui({ open: false }), RECIPES, ctx(), a);
    expect(el.querySelector('.batchlist')).toBeNull();
    btn(el, 'まとめて実行 ▸')!.click();
    expect(a.toggle).toHaveBeenCalledOnce();
    expect(btn(batchPanel(ui(), RECIPES, ctx(), a), 'まとめて実行 ▾')).toBeDefined();
  });

  it('観点が、チェックの一覧で出る。選ぶと pick を呼ぶ。リンターには「LLM を使わない」と添える', () => {
    const a = acts();
    const el = batchPanel(ui({ picked: ids('bug') }), RECIPES, ctx(), a);
    const boxes = [...el.querySelectorAll('input[type=checkbox]')] as HTMLInputElement[];
    expect(boxes.map((b) => b.checked)).toEqual([true, false, false, false]);
    boxes[1]!.dispatchEvent(new Event('change'));
    expect(a.pick).toHaveBeenCalledExactlyOnceWith('sec');
    expect(el.querySelectorAll('li')[3]?.textContent).toContain('（LLM を使わない）');
  });

  it('選ぶと、件数・送る量・目安の時間が出て、「N 観点を、順に実行」が押せる。選んでいなければ、押せない', () => {
    const a = acts();
    const el = batchPanel(ui({ picked: ids('bug', 'sec') }), RECIPES, ctx({ stats: { bug: S(3, 10_000, 15_000, 20_000), sec: S(3, 10_000, 15_000, 20_000) } }), a);
    expect(el.textContent).toContain('2 観点を選択中。送る量: 約 35,000 トークン × 2 回 ＝ 約 70,000 トークン');
    expect(el.querySelector('.est')?.textContent).toBe('目安: 約 30 秒（20 秒〜40 秒）');
    btn(el, '2 観点を、順に実行')!.click();
    expect(a.start).toHaveBeenCalledOnce();
    const none = batchPanel(ui(), RECIPES, ctx(), acts());
    expect(btn(none, '順に実行')!.disabled).toBe(true);
    expect(none.textContent).toContain('観点を選ぶと、送る量と目安の時間が出ます');
  });

  it('実行できない理由があれば、ボタンを押せず、理由を書く', () => {
    const el = batchPanel(ui({ picked: ids('bug') }), RECIPES, ctx({ reason: '送る差分がありません（未コミットの変更が無い）' }), acts());
    expect(btn(el, '1 観点を、順に実行')!.disabled).toBe(true);
    expect(el.textContent).toContain('送る差分がありません');
  });

  it('リンターを選んだときだけ、リンターの注意（許可が要るものを除く）を出す', () => {
    const note = 'リンターは、許可が要るもの（ESLint）を除いて実行します。';
    expect(batchPanel(ui({ picked: ids('lint') }), RECIPES, ctx({ lintNote: note }), acts()).textContent).toContain(note);
    expect(batchPanel(ui({ picked: ids('bug') }), RECIPES, ctx({ lintNote: note }), acts()).textContent).not.toContain(note);
  });

  it('確認を出しているときは、送る回数と量を言い、「実行する」「やめる」を出す。実行ボタンは、出さない', () => {
    const a = acts();
    const el = batchPanel(ui({ picked: ids('bug', 'sec', 'perf'), confirming: true }), RECIPES, ctx(), a);
    expect(el.querySelector('.confirm')?.textContent).toContain('LLM に 3 回、送ります');
    expect(el.querySelector('.confirm')?.textContent).toContain('約 105,000 トークン');
    expect(btn(el, /順に実行/)).toBeUndefined();
    btn(el, '実行する')!.click();
    btn(el, 'やめる')!.click();
    expect(a.confirm).toHaveBeenCalledOnce();
    expect(a.cancelConfirm).toHaveBeenCalledOnce();
  });

  it('実行中は、何番目か・いまの観点・経過時間を出し、チェックと実行ボタンを止める。「残りを中止」を押すと stop。押したあとは、止めると言う', () => {
    const a = acts();
    const run = { ids: ['bug', 'sec', 'perf'], index: 1, stop: false, results: [] };
    const el = batchPanel(ui({ picked: ids('bug', 'sec', 'perf'), run }), RECIPES, ctx(), a);
    expect(el.querySelector('.note [data-elapsed]')?.textContent).toBe('12 秒');
    expect(el.textContent).toContain('2 / 3　セキュリティ を実行中…');
    expect([...el.querySelectorAll('input[type=checkbox]')].every((b) => (b as HTMLInputElement).disabled)).toBe(true);
    expect(btn(el, /順に実行/)).toBeUndefined();
    btn(el, '残りを中止')!.click();
    expect(a.stop).toHaveBeenCalledOnce();
    const stopping = batchPanel(ui({ run: { ...run, stop: true } }), RECIPES, ctx(), a);
    expect(btn(stopping, '現在の実行が終わったら、止めます')!.disabled).toBe(true);
  });

  it('畳んでいても、実行中なら、進み具合は見える', () => {
    const run = { ids: ['bug'], index: 0, stop: false, results: [] };
    expect(batchPanel(ui({ open: false, run }), RECIPES, ctx(), acts()).textContent).toContain('1 / 1　潜在バグ を実行中…');
  });

  it('終わったあと: 件数と、失敗の理由・中止を出す。実行中は出さない', () => {
    const last = [
      { id: 'bug', title: '潜在バグ', ok: true, note: '' },
      { id: 'sec', title: 'セキュリティ', ok: false, note: 'タイムアウト' },
      { id: 'perf', title: 'パフォーマンス', ok: false, note: STOPPED },
    ];
    const el = batchPanel(ui({ last }), RECIPES, ctx(), acts());
    expect(el.querySelector('.batchlast')?.textContent).toContain('3 件中 1 件成功・1 件失敗・1 件は、中止して実行せず');
    expect([...el.querySelectorAll('.batchlast .err')].map((e) => e.textContent)).toEqual(['セキュリティ: タイムアウト', `パフォーマンス: ${STOPPED}`]);
    const running = batchPanel(ui({ last, run: { ids: ['bug'], index: 0, stop: false, results: [] } }), RECIPES, ctx(), acts());
    expect(running.querySelector('.batchlast')).toBeNull();
  });
});
