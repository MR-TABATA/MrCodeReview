import { h } from './dom';
import { displayRecipeTitle } from './historyview';
import { DEFAULT_LANG, msg, type Lang } from './i18n';
import { DOC_IDS, TERRAFORM_IDS, TERRAFORM_LABEL } from './recipescope';
import type { Recipe } from './types';

export interface RecipeBarState {
  recipes: Recipe[];
  checkedIds: Set<string>;
  running: string; // 実行中のレシピタイトルまたはID（空なら待機中）
  runIndex?: number; // 複数実行中のインデックス（0始まり）
  totalRuns?: number;
  disabledReason?: string;
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

  const canRun = !isBusy && checkedCount > 0 && !st.disabledReason;

  // チェックボックス一覧
  const chipOf = (r: Recipe): HTMLElement => {
    const isChecked = st.checkedIds.has(r.id);
    const isTool = r.kind === 'tool';
    const isCurrentRunning = isBusy && (st.running === r.title || st.running === r.id);
    const isDone = isBusy && !!st.doneIds?.has(r.id);
    const isWaiting = isBusy && isChecked && !isCurrentRunning && !isDone;

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
        class: `recipe-chip ${isChecked ? 'checked' : ''} ${isCurrentRunning ? 'running' : ''} ${isDone ? 'done' : ''} ${isWaiting ? 'waiting' : ''}`,
      },
      cb,
      h('span', { class: 'recipe-title' }, displayRecipeTitle(r.title, lang)),
      isCurrentRunning ? h('span', { class: 'chip-spin', role: 'status', 'aria-label': m.runningLabel('') }) : null,
      isDone ? h('span', { class: 'chip-done', 'aria-hidden': 'true' }, '✓') : null,
      isTool ? h('span', { class: 'badge tool' }, 'Tool') : null,
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

  const runBtn = h(
    'button',
    {
      class: 'run main-run-btn',
      disabled: !canRun,
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
    h(
      'div',
      { class: 'recipe-bar-meta' },
      h('span', { class: 'recipe-chips-tools' }, selectAllBtn, clearAllBtn, showAllBtn),
      st.disabledReason ? h('span', { class: 'disabled-reason' }, st.disabledReason) : null,
      hints.length ? h('span', { class: 'muted hint' }, hints.join(' ・ ')) : null,
      st.runError ? h('span', { class: 'err' }, st.runError) : null,
    ),
  );
}
