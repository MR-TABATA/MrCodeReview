// 「まとめて実行」: 観点を複数選んで、順に 1 つずつ実行する。
//
// - 観点ごとに、別のレビューとして記録する（履歴に 1 件ずつ出て、仕分けの引き継ぎも、観点ごとに効く）。LLM は、同時に走らせない。
// - 同じ差分を、観点の数だけ、LLM に送る。実行前に、送る量（概算トークン × 回数）と、目安の時間（観点ごとの過去の実績を足したもの）を出す。
//   実績の無い観点は、「実績なし」と言う（推定で埋めない）。
// - LLM を使う観点が 3 つ以上のときは、実行前に、1 回、確認する（トークンが、観点の数だけ増えるため）。
// - 途中で失敗しても、残りは続ける。最後に、失敗を言う。「残りを中止」は、いま実行中の 1 件が終わってから、止める
//   （実行中の LLM を、途中で止める仕組みは、まだ無い）。

import { h } from './dom';
import { fmtSeconds, type DurationStats } from './duration';
import type { Recipe } from './types';

/** LLM を使う観点が、これ以上なら、実行前に確認する */
export const CONFIRM_FROM = 3;

export interface BatchResult {
  id: string;
  title: string;
  ok: boolean;
  /** 失敗の理由・中止の印（成功のときは空） */
  note: string;
}

export interface BatchRun {
  ids: string[];
  /** いま実行している観点の番号（0 始まり） */
  index: number;
  /** 「残りを中止」を押した（いまの 1 件が終わったら、止める） */
  stop: boolean;
  results: BatchResult[];
}

export interface BatchUi {
  open: boolean;
  picked: Set<string>;
  /** 実行前の確認を出している */
  confirming: boolean;
  run: BatchRun | null;
  /** 直前のまとめて実行の結果 */
  last: BatchResult[] | null;
}

export const newBatchUi = (): BatchUi => ({ open: false, picked: new Set(), confirming: false, run: null, last: null });

const nf = new Intl.NumberFormat('ja-JP');

/** 選んだ観点（観点の並びの順。クリックした順ではなく、いつも同じ順に実行する） */
export const pickedRecipes = (recipes: Recipe[], picked: Set<string>): Recipe[] => recipes.filter((r) => picked.has(r.id));

export interface BatchEstimate {
  count: number;
  /** LLM を使う観点の数 */
  llm: number;
  tokensText: string;
  timeText: string;
}

/**
 * 実行前の見積もり。
 * `stats`: 観点の id → 過去の実績。まだ読み込んでいないものは、undefined。
 * `inputTokens`: 1 回に送る、概算の入力トークン（まだ分からなければ null）。
 */
export function batchEstimate(chosen: Recipe[], stats: Record<string, DurationStats | undefined>, inputTokens: number | null, isMock: boolean): BatchEstimate {
  const llm = chosen.filter((r) => r.kind !== 'tool').length;
  const tokensText =
    llm === 0
      ? 'LLM は使いません。トークンは 0 です。'
      : inputTokens === null
        ? `LLM に ${llm} 回、送ります。`
        : `送る量: 約 ${nf.format(inputTokens)} トークン × ${llm} 回 ＝ 約 ${nf.format(inputTokens * llm)} トークン（概算。入力だけ）`;

  let timeText = '';
  if (!isMock && chosen.length > 0 && chosen.every((r) => stats[r.id] !== undefined)) {
    const known = chosen.map((r) => stats[r.id]!).filter((s) => s.n > 0);
    const unknown = chosen.length - known.length;
    if (known.length === 0) {
      timeText = '目安: 実績なし（この LLM・この観点では、まだ測っていません）';
    } else {
      const sum = (f: (s: DurationStats) => number) => known.reduce((a, s) => a + f(s), 0);
      const [lo, mid, hi] = [sum((s) => s.min_ms), sum((s) => s.median_ms), sum((s) => s.max_ms)];
      const range = lo === hi ? '' : `（${fmtSeconds(lo)}〜${fmtSeconds(hi)}）`;
      timeText = unknown === 0 ? `目安: 約 ${fmtSeconds(mid)}${range}` : `目安: 実績のある ${known.length} 観点で 約 ${fmtSeconds(mid)}${range}。ほか ${unknown} 観点は、実績なし`;
    }
  }
  return { count: chosen.length, llm, tokensText, timeText };
}

/** 終わったあとの 1 文。失敗と中止は、件数で言う。 */
export function batchSummary(results: BatchResult[]): string {
  const ok = results.filter((r) => r.ok).length;
  const stopped = results.filter((r) => !r.ok && r.note === STOPPED).length;
  const failed = results.length - ok - stopped;
  return `${results.length} 件中 ${ok} 件成功${failed ? `・${failed} 件失敗` : ''}${stopped ? `・${stopped} 件は、中止して実行せず` : ''}`;
}

export const STOPPED = '中止して実行していない';

export interface BatchContext {
  stats: Record<string, DurationStats | undefined>;
  /** 1 回に送る、概算の入力トークン */
  inputTokens: number | null;
  isMock: boolean;
  /** 実行できない理由（無ければ空）。リポジトリ未選択・差分なし・LLM 未選択など */
  reason: string;
  /** 実行中の経過時間の文字（実行中の 1 件のぶん） */
  elapsed: string;
  /** 「リンター」について、添える注意（許可が要る linter を除く、など）。無ければ空 */
  lintNote: string;
}

export interface BatchActions {
  toggle(): void;
  pick(id: string): void;
  /** 「順に実行」を押した（3 つ以上なら、確認を出す） */
  start(): void;
  /** 確認で「実行する」を押した */
  confirm(): void;
  cancelConfirm(): void;
  stop(): void;
}

export function batchPanel(ui: BatchUi, recipes: Recipe[], ctx: BatchContext, act: BatchActions): HTMLElement {
  const running = ui.run !== null;
  const chosen = pickedRecipes(recipes, ui.picked);
  const est = batchEstimate(chosen, ctx.stats, ctx.inputTokens, ctx.isMock);

  const list = h(
    'ul',
    { class: 'batchlist' },
    ...recipes.map((r) =>
      h(
        'li',
        {},
        h(
          'label',
          {},
          h('input', { type: 'checkbox', disabled: running, ...(ui.picked.has(r.id) ? { checked: true } : {}), onchange: () => act.pick(r.id) }),
          ` ${r.title}`,
          r.kind === 'tool' ? h('span', { class: 'muted' }, '（LLM を使わない）') : null,
        ),
      ),
    ),
  );

  const run = ui.run;
  const progress = run
    ? h(
        'div',
        {},
        h('p', { class: 'note' }, `${run.index + 1} / ${run.ids.length}　${recipes.find((r) => r.id === run.ids[run.index])?.title ?? ''} を実行中… `, h('span', { 'data-elapsed': '' }, ctx.elapsed)),
        h('button', { class: 'small', disabled: run.stop, onclick: () => act.stop() }, run.stop ? '現在の実行が終わったら、止めます' : '残りを中止'),
      )
    : null;

  const confirmBox = ui.confirming
    ? h(
        'div',
        { class: 'confirm' },
        h('p', {}, `LLM に ${est.llm} 回、送ります。同じ差分を、観点の数だけ、送ります。`),
        h('p', { class: 'muted' }, est.tokensText),
        h('div', { class: 'row' }, h('button', { class: 'run', onclick: () => act.confirm() }, '実行する'), h('button', { onclick: () => act.cancelConfirm() }, 'やめる')),
      )
    : null;

  const last = ui.last;
  const lastBox = last && !running
    ? h(
        'div',
        { class: 'batchlast' },
        h('p', { class: 'note' }, batchSummary(last)),
        ...last.filter((r) => !r.ok).map((r) => h('p', { class: 'err' }, `${r.title}: ${r.note}`)),
      )
    : null;

  return h(
    'section',
    { class: 'batch' },
    h('button', { class: 'small linkish', onclick: () => act.toggle() }, ui.open ? 'まとめて実行 ▾' : 'まとめて実行 ▸'),
    ui.open
      ? h(
          'div',
          {},
          lastBox,
          list,
          ctx.lintNote && ui.picked.has('lint') ? h('p', { class: 'note' }, ctx.lintNote) : null,
          est.count > 0 ? h('p', { class: 'note' }, `${est.count} 観点を選択中。${est.tokensText}`) : h('p', { class: 'muted' }, '観点を選ぶと、送る量と目安の時間が出ます。'),
          est.timeText ? h('p', { class: 'note est' }, est.timeText) : null,
          confirmBox,
          confirmBox || running ? null : h('button', { class: 'run', disabled: est.count === 0 || !!ctx.reason, onclick: () => act.start() }, est.count > 0 ? `${est.count} 観点を、順に実行` : '順に実行'),
          !running && ctx.reason && est.count > 0 ? h('p', { class: 'note' }, ctx.reason) : null,
          progress,
        )
      : progress,
  );
}
