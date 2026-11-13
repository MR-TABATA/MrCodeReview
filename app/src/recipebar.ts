import { h } from './dom';
import { displayRecipeTitle } from './historyview';
import { DEFAULT_LANG, msg, type Lang } from './i18n';
import { DOC_IDS, TERRAFORM_IDS, TERRAFORM_LABEL } from './recipescope';
import type { SkipWhy } from './runplan';
import type { Recipe } from './types';

/** 今回の実行から外した観点（理由ごとにまとめたもの）。 */
export interface SkippedGroup {
  reason: string;
  why: SkipWhy;
  titles: string[];
  /** カードに「今回は実行しない」の印を付ける観点の id */
  ids: string[];
}

export interface RecipeBarState {
  recipes: Recipe[];
  checkedIds: Set<string>;
  running: string; // 実行中のレシピタイトルまたはID（空なら待機中）
  runIndex?: number; // 複数実行中のインデックス（0始まり）
  totalRuns?: number;
  disabledReason?: string;
  /** 今回走らせる件数。チェックした件数より少ないときは、外した観点がある（無ければ、チェックした件数） */
  runCount?: number;
  /** 今回の実行から外した観点 */
  skipped?: SkippedGroup[];
  /** 「前回のレビューの続き」を選んでいて、前回の実行に含まれない観点があるときの注記（事実だけ。無ければ空） */
  baseNote?: string;
  collapsed?: boolean;
  /** まとめて実行中に、すでに終わった観点の id */
  doneIds?: Set<string>;
  /** この差分に向かないので、いま隠している観点の数 */
  hiddenCount?: number;
  /** 隠している観点も、すべて出しているか */
  showAll?: boolean;
  estimatedTokens?: number | null;
  estimatedTimeText?: string;
  runError?: string;
  specSlot?: HTMLElement | null;
}

export interface RecipeBarActions {
  onToggle(recipeId: string, checked: boolean): void;
  onSelectAll(): void;
  onClearAll(): void;
  onRun(): void;
  onCancel(): void;
  onExpand(): void;
  onToggleShowAll(): void;
}

export function recipeBar(st: RecipeBarState, acts: RecipeBarActions, lang: Lang = DEFAULT_LANG): HTMLElement {
  const m = msg(lang).recipeBar;
  const isBusy = !!st.running;
  const checkedCount = st.checkedIds.size;
  const progress = st.totalRuns && st.totalRuns > 1 ? ` (${(st.runIndex ?? 0) + 1}/${st.totalRuns})` : '';

  // 実行ボタンのラベル
  let runLabel: string;
  if (isBusy) {
    runLabel = m.runningLabel(progress);
  } else if (checkedCount === 0) {
    runLabel = m.selectAngles;
  } else if (checkedCount === 1) {
    const firstId = [...st.checkedIds][0];
    const r = st.recipes.find((x) => x.id === firstId);
    const dispTitle = r ? displayRecipeTitle(r.title, lang) : '';
    runLabel = r ? m.runSingle(dispTitle) : m.runFallback;
  } else {
    runLabel = m.runMultiple(checkedCount);
  }
  // 実行できない観点を外すときは、走る件数と、外すものを、ボタンに出す（チェックの数と食い違うので、黙らない）
  // 全体を止めているときは、止める理由だけを出す（外した観点の注記は、同じ理由の重複になる）
  const skippedGroups = st.disabledReason ? [] : (st.skipped ?? []);
  const runCount = st.runCount ?? checkedCount;
  if (!isBusy && skippedGroups.length > 0 && runCount > 0) {
    const kinds = new Set(skippedGroups.map((g) => g.why));
    const what = kinds.size > 1 ? m.skipWhatMixed : kinds.has('spec') ? m.skipWhatSpec : m.skipWhatLlm;
    runLabel = m.runSkipping(runCount, what);
  }

  const canRun = !isBusy && checkedCount > 0 && !st.disabledReason;

  // チェックボックス一覧
  const chipOf = (r: Recipe): HTMLElement => {
    const isChecked = st.checkedIds.has(r.id);
    const isTool = r.kind === 'tool';
    const isCurrentRunning = isBusy && (st.running === r.title || st.running === r.id);
    const isDone = isBusy && !!st.doneIds?.has(r.id);
    const isWaiting = isBusy && isChecked && !isCurrentRunning && !isDone;
    // 実行できなくて、今回は外す観点（チェックは残る）。理由は、カードのツールチップと、ボタンの下の注記に出す
    const skipReason = isChecked && !isBusy ? skippedGroups.find((g) => g.ids.includes(r.id))?.reason : undefined;

    const cb = h('input', {
      type: 'checkbox',
      disabled: isBusy,
      ...(isChecked ? { checked: true } : {}),
      onchange: (e) => {
        acts.onToggle(r.id, (e.target as HTMLInputElement).checked);
      },
      onclick: (e) => {
        acts.onToggle(r.id, (e.target as HTMLInputElement).checked);
      },
    });

    const label = h(
      'label',
      {
        class: `recipe-chip ${isChecked ? 'checked' : ''} ${isCurrentRunning ? 'running' : ''} ${isDone ? 'done' : ''} ${isWaiting ? 'waiting' : ''} ${skipReason ? 'skipped' : ''}`,
        ...(skipReason ? { title: skipReason } : {}),
      },
      cb,
      h('span', { class: 'recipe-title' }, displayRecipeTitle(r.title, lang)),
      isCurrentRunning ? h('span', { class: 'chip-spin', role: 'status', 'aria-label': m.runningLabel('') }) : null,
      isDone ? h('span', { class: 'chip-done', 'aria-hidden': 'true' }, '✓') : null,
      isTool ? h('span', { class: 'badge tool' }, 'Tool') : null,
      skipReason ? h('span', { class: 'badge skip' }, m.skipBadge) : null,
    );

    return label;
  };
  const isTf = (r: Recipe): boolean => TERRAFORM_IDS.includes(r.id);
  const isDoc = (r: Recipe): boolean => DOC_IDS.includes(r.id);
  const plainChips = st.recipes.filter((r) => !isTf(r) && !isDoc(r)).map(chipOf);
  // 見出しつきの塊（Terraform・文書）。出す塊だけ、この順に並べる
  const groups = [
    { label: TERRAFORM_LABEL, chips: st.recipes.filter(isTf).map(chipOf) },
    { label: m.groupDocs, chips: st.recipes.filter(isDoc).map(chipOf) },
  ].filter((g) => g.chips.length);

  const selectAllBtn = h(
    'button',
    {
      class: 'link-btn',
      disabled: isBusy || checkedCount === st.recipes.length,
      onclick: acts.onSelectAll,
    },
    m.selectAll,
  );

  const clearAllBtn = h(
    'button',
    {
      class: 'link-btn',
      disabled: isBusy || checkedCount === 0,
      onclick: acts.onClearAll,
    },
    m.clearAll,
  );

  const showAllBtn =
    st.showAll || st.hiddenCount
      ? h(
          'button',
          { class: 'link-btn', type: 'button', disabled: isBusy, onclick: acts.onToggleShowAll },
          st.showAll ? m.showFitOnly : m.showAllRecipes(st.hiddenCount ?? 0),
        )
      : null;

  // ボタンのすぐ下に出す注記: 全体を止める理由／外した観点と理由／前回の続きの事実
  const notes: string[] = [];
  if (st.disabledReason) notes.push(st.disabledReason);
  for (const g of skippedGroups) notes.push(m.skipNotice(g.titles, g.reason));
  if (st.baseNote) notes.push(st.baseNote);

  const runBtn = h(
    'button',
    {
      class: 'run main-run-btn',
      disabled: !canRun,
      title: notes.join('\n'),
      onclick: acts.onRun,
    },
    runLabel,
  );

  const hints: string[] = [];
  if (st.estimatedTokens) {
    hints.push(m.estTokens(st.estimatedTokens.toLocaleString()));
  }
  if (st.estimatedTimeText) {
    hints.push(m.estTime(st.estimatedTimeText));
  }

  const cancelBtn = isBusy
    ? h('button', { class: 'cancel-btn', onclick: acts.onCancel }, m.cancel)
    : null;

  const runGroup = h('div', { class: 'run-button-group' }, runBtn, cancelBtn);

  if (st.collapsed) {
    return h(
      'section',
      { class: 'recipe-bar-top recipe-bar-folded' },
      h('button', { class: 'recipe-bar-expand', type: 'button', onclick: acts.onExpand }, m.expandNewReview),
    );
  }

  return h(
    'section',
    { class: 'recipe-bar-top' },
    // 観点のチップ一覧（一般観点およびTerraform・文書グループ）
    ...(plainChips.length || !groups.length
      ? [h('div', { class: 'recipe-chips' }, ...plainChips)]
      : []),
    ...groups.map((g) =>
      h(
        'div',
        { class: 'recipe-group' },
        h('div', { class: 'recipe-group-label' }, g.label),
        h('div', { class: 'recipe-chips' }, ...g.chips),
      ),
    ),
    // 参照する設計書・仕様書の指定フォーム（あれば）
    st.specSlot ?? null,
    // 実行ボタンは常に右寄せで配置（観点数やフォーム有無によらず右端に揃える）
    h('div', { class: 'recipe-run-row' }, runGroup),
    notes.length
      ? h(
          'div',
          { class: 'recipe-run-notes', role: 'status' },
          ...(st.disabledReason ? [h('div', { class: 'disabled-reason' }, st.disabledReason)] : []),
          ...skippedGroups.map((g) => h('div', { class: 'disabled-reason skip-note' }, m.skipNotice(g.titles, g.reason))),
          ...(st.baseNote ? [h('div', { class: 'muted hint' }, st.baseNote)] : []),
        )
      : null,
    h(
      'div',
      { class: 'recipe-bar-meta' },
      h('span', { class: 'recipe-chips-tools' }, selectAllBtn, clearAllBtn, showAllBtn),
      hints.length ? h('span', { class: 'muted hint' }, hints.join(' ・ ')) : null,
      st.runError ? h('span', { class: 'err' }, st.runError) : null,
    ),
  );
}
