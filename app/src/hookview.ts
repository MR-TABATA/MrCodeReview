// 設定画面の「Git フック」タブ。コミットのあとに、自動でレビューを走らせる（post-commit）。ON/OFF と観点、設計書を、見て、変える。
//
// - 状態の読み取りと、ON/OFF の反映は、Rust（`hook_status` / `hook_set`）。ここは、見せるだけ。
// - フックの置き場（.git/hooks）に書けない・mrcr（CLI）が無い、は、押せない理由として、そのまま見せる。
// - 観点と設計書は、保存するまで、画面で選んだ値（draft）と、保存済みの値（status）を分けて持つ。
// - 設計書の入力欄は、打つたびに画面を作り直さない（フォーカスが外れる）。打った値は状態に入れておき、ボタンを押したときに渡す。
//   設計書が要る観点なのに設計書が空、などの確認は、Rust 側（`hooks::validate_for_enable`）が、理由つきで断る。

import { h } from './dom';
import { formatWhen } from './historyview';
import { getAppLang, msg, type Lang } from './i18n';
import { CODE_IDS, DOC_IDS, TERRAFORM_IDS } from './recipescope';
import type { HookStatus, Recipe } from './types';

export const INSTALL_CMD = 'brew install mr-tabata/tap/mrcr';
export const INSTALL_CMD_NPM = 'npm i -g @mr-tabata/mrcr';

export interface HookUiState {
  status: HookStatus | null;
  /** 画面で選んでいる観点。保存するまで、`status.recipes` とは別。null なら、`status.recipes` のまま */
  draft: string[] | null;
  /** 画面で入れている設計書のパス。null なら、`status.spec` のまま（空文字は「指定なし」） */
  specDraft: string | null;
  busy: boolean;
  error: string;
}

export const newHookUi = (): HookUiState => ({ status: null, draft: null, specDraft: null, busy: false, error: '' });

export interface HookActions {
  /** ON/OFF にする。観点と設計書は、いま画面にあるもの（設計書は、空なら null） */
  toggle(enabled: boolean, recipes: string[], spec: string | null): void;
  /** ON のまま、観点と設計書を保存する */
  save(recipes: string[], spec: string | null): void;
  /** 観点の選択が変わった（まだ保存しない） */
  pick(recipes: string[]): void;
}

/** フックで選べる観点。LLM の観点（linter は、フックでは走らせない）。設計書が要るものも含む（設計書を指定すれば使える）。 */
export const selectableRecipes = (recipes: Recipe[]): Recipe[] => recipes.filter((r) => r.kind === 'llm');

/** いま画面で選んでいる観点。 */
export const currentRecipes = (st: HookUiState): string[] => st.draft ?? st.status?.recipes ?? [];

/** いま画面にある設計書のパス（空なら null）。 */
export const currentSpec = (st: HookUiState): string | null => {
  const v = (st.specDraft ?? st.status?.spec ?? '').trim();
  return v === '' ? null : v;
};

export type RecipeGroupKey = 'code' | 'tf' | 'doc' | 'other';

/** 観点が、どの種類か。分け方は、観点のボタン（recipescope）と同じ。設計書と照合する `spec` は、コードの観点。自分で足した観点は、その他。 */
export function groupOf(r: Recipe): RecipeGroupKey {
  if (TERRAFORM_IDS.includes(r.id)) return 'tf';
  if (DOC_IDS.includes(r.id)) return 'doc';
  if (CODE_IDS.includes(r.id) || r.id === 'spec') return 'code';
  return r.source === 'builtin' ? 'code' : 'other';
}

/** 種類ごとにまとめる（コード → Terraform → 文書 → その他の順。空の種類は出さない。種類の中は、渡された順）。 */
export function groupedRecipes(recipes: Recipe[]): { key: RecipeGroupKey; items: Recipe[] }[] {
  const order: RecipeGroupKey[] = ['code', 'tf', 'doc', 'other'];
  return order.map((key) => ({ key, items: recipes.filter((r) => groupOf(r) === key) })).filter((g) => g.items.length > 0);
}

/** 「ON にする」を押せない理由（押せるなら null）。表示用の文は、呼び出し側が出す。 */
export function enableBlock(st: HookUiState): 'busy' | 'cli' | 'blocked' | 'no-recipes' | null {
  const s = st.status;
  if (!s) return 'busy';
  if (st.busy) return 'busy';
  if (s.blocked) return 'blocked';
  if (!s.cli) return 'cli';
  if (currentRecipes(st).length === 0) return 'no-recipes';
  return null;
}

export function hookSection(st: HookUiState, repoSelected: boolean, recipes: Recipe[], act: HookActions, lang: Lang = getAppLang()): HTMLElement {
  const m = msg(lang).hookUi;
  const head = [h('h3', {}, m.title), h('p', { class: 'muted' }, m.desc)];
  if (!repoSelected) return h('section', { class: 'sgroup hookview' }, ...head, h('p', { class: 'muted' }, m.noRepo));
  if (st.error && !st.status) return h('section', { class: 'sgroup hookview' }, ...head, h('p', { class: 'err' }, m.failed(st.error)));
  const s = st.status;
  if (!s) return h('section', { class: 'sgroup hookview' }, ...head, h('p', { class: 'muted' }, m.loading));

  const chosen = currentRecipes(st);
  const options = selectableRecipes(recipes);
  const pickOf = (id: string, on: boolean): string[] => (on ? [...chosen.filter((x) => x !== id), id] : chosen.filter((x) => x !== id));

  // 種類ごと（コード・Terraform・文書・その他）に、見出しと「すべて選ぶ／すべて外す」を付ける。
  // 中は 1 行に 1 つ（チェック・名前・id）。幅に応じて、2〜3 列の格子に並ぶ
  const recipeRow = (r: Recipe): HTMLElement =>
    h(
      'label',
      { class: 'hookrecipe' },
      h('input', {
        type: 'checkbox',
        disabled: st.busy,
        ...(chosen.includes(r.id) ? { checked: true } : {}),
        onchange: (e: Event) => act.pick(pickOf(r.id, (e.target as HTMLInputElement).checked)),
      }),
      h('span', { class: 'hookrecipe-title' }, r.title, r.needs_spec ? h('span', { class: 'hookrecipe-badge' }, m.needsSpecBadge) : null),
      h('code', { class: 'hookrecipe-id' }, r.id),
    );
  const groupTexts: Record<RecipeGroupKey, { label: string; desc: string }> = {
    code: { label: m.groupCode, desc: m.groupCodeDesc },
    tf: { label: m.groupTf, desc: m.groupTfDesc },
    doc: { label: m.groupDoc, desc: m.groupDocDesc },
    other: { label: m.groupOther, desc: m.groupOtherDesc },
  };
  const boxes = options.length
    ? groupedRecipes(options).map((g) => {
        const ids = g.items.map((r) => r.id);
        const picked = ids.filter((id) => chosen.includes(id)).length;
        const t = groupTexts[g.key];
        return h(
          'section',
          { class: `hookgroup hookgroup-${g.key}` },
          h(
            'div',
            { class: 'hookgroup-head' },
            h('strong', {}, t.label),
            h('span', { class: 'muted' }, m.groupCount(picked, ids.length)),
            h('span', { class: 'spacer' }),
            h('button', { class: 'small', disabled: st.busy || picked === ids.length, onclick: () => act.pick([...chosen, ...ids.filter((id) => !chosen.includes(id))]) }, m.selectAll),
            h('button', { class: 'small', disabled: st.busy || picked === 0, onclick: () => act.pick(chosen.filter((id) => !ids.includes(id))) }, m.clearAll),
          ),
          h('p', { class: 'muted hookgroup-desc' }, t.desc),
          h('div', { class: 'hookgrid' }, ...g.items.map(recipeRow)),
        );
      })
    : [h('p', { class: 'muted' }, m.noRecipes)];

  // 設計書の入力欄。打つたびに画面を作り直さない。打った値は状態に入れておき、ボタンを押したときに読む
  const specInput = h('input', { class: 'sinput hookspec', type: 'text', placeholder: m.specPlaceholder, disabled: st.busy, spellcheck: 'false' });
  specInput.value = st.specDraft ?? s.spec ?? '';
  specInput.addEventListener('input', () => {
    st.specDraft = specInput.value;
  });
  const specNow = (): string | null => {
    const v = specInput.value.trim();
    return v === '' ? null : v;
  };

  // ON/OFF は、状態の行のスイッチ。切り替えると、その場で反映する（ON にするときは、いまの観点と設計書も、一緒に保存する）。
  // ON にできない理由があるとき（mrcr が無い・フックを置けない、など）は、押せない。OFF にするのは、いつでもできる
  const block = enableBlock(st);
  const sw = h('input', {
    type: 'checkbox',
    role: 'switch',
    'aria-label': m.switchLabel,
    title: s.enabled ? m.disable : m.enable,
    disabled: s.enabled ? st.busy : block !== null,
    ...(s.enabled ? { checked: true } : {}),
    onchange: () => act.toggle(!s.enabled, chosen, specNow()),
  });
  const toggle = h('label', { class: 'switch' }, sw, h('span', { class: 'slider' }));
  // 「保存する」は、状態の行（ON の隣）に置く。ON のときだけ。OFF のときは、ON にするときに保存される
  const saveBtn = s.enabled
    ? h('button', { class: 'primary hooksave', disabled: st.busy || chosen.length === 0, onclick: () => act.save(chosen, specNow()) }, m.saveRecipes)
    : null;

  const last = s.last_run;
  return h(
    'section',
    { class: 'sgroup hookview' },
    ...head,
    h('div', { class: 'hookstate' }, toggle, h('strong', {}, s.enabled ? m.stateOn : m.stateOff), st.busy ? h('span', { class: 'muted' }, m.busy) : null, saveBtn),
    h('p', { class: 'muted' }, s.installed ? m.installedYes : m.installedNo),
    !s.cli ? h('p', { class: 'err' }, m.cliMissing(INSTALL_CMD, INSTALL_CMD_NPM)) : h('p', { class: 'muted' }, m.cliPath(s.cli)),
    s.blocked ? h('p', { class: 'err' }, m.blocked(s.blocked)) : null,
    s.target_missing ? h('p', { class: 'err' }, m.targetMissing) : null,
    h('div', { class: 'hookrecipes' }, h('h4', {}, m.recipesLabel), ...boxes, h('p', { class: 'muted' }, m.recipesNote)),
    h('div', { class: 'hookspecbox' }, h('h4', {}, m.specLabel), specInput, h('p', { class: 'muted' }, m.specHint)),
    st.error ? h('p', { class: 'err' }, m.failed(st.error)) : null,
    h('p', { class: 'muted' }, m.costNote),
    h('p', { class: 'muted' }, m.keyNote),
    h('h4', {}, m.lastRunTitle),
    last ? h('p', { class: last.code === 2 ? 'err' : '' }, m.lastRun(last.code, formatWhen(last.at_ms, lang), last.message)) : h('p', { class: 'muted' }, m.lastRunNone),
  );
}
