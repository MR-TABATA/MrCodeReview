// 設定画面の「Git フック」タブ。コミットのあとに、自動でレビューを走らせる（post-commit）。ON/OFF と観点を、見て、変える。
//
// - 状態の読み取りと、ON/OFF の反映は、Rust（`hook_status` / `hook_set`）。ここは、見せるだけ。
// - フックの置き場（.git/hooks）に書けない・mrcr（CLI）が無い、は、押せない理由として、そのまま見せる。
// - 観点は、保存するまで、画面で選んだ値（draft）と、保存済みの値（status.recipes）を分けて持つ。

import { h } from './dom';
import { formatWhen } from './historyview';
import { getAppLang, msg, type Lang } from './i18n';
import type { HookStatus, Recipe } from './types';

export const INSTALL_CMD = 'brew install mr-tabata/tap/mrcr';
export const INSTALL_CMD_NPM = 'npm i -g @mr-tabata/mrcr';

export interface HookUiState {
  status: HookStatus | null;
  /** 画面で選んでいる観点。保存するまで、`status.recipes` とは別。null なら、`status.recipes` のまま */
  draft: string[] | null;
  busy: boolean;
  error: string;
}

export const newHookUi = (): HookUiState => ({ status: null, draft: null, busy: false, error: '' });

export interface HookActions {
  /** ON/OFF にする。観点は、いま画面で選んでいるもの */
  toggle(enabled: boolean, recipes: string[]): void;
  /** ON のまま、観点だけを保存する */
  saveRecipes(recipes: string[]): void;
  /** 観点の選択が変わった（まだ保存しない） */
  pick(recipes: string[]): void;
}

/** フックで選べる観点。LLM の観点で、設計書を要らないもの（フックから、設計書は渡せない）。 */
export const selectableRecipes = (recipes: Recipe[]): Recipe[] => recipes.filter((r) => r.kind === 'llm' && !r.needs_spec);

/** いま画面で選んでいる観点。 */
export const currentRecipes = (st: HookUiState): string[] => st.draft ?? st.status?.recipes ?? [];

const sameSet = (a: string[], b: string[]): boolean => a.length === b.length && [...a].sort().join('\0') === [...b].sort().join('\0');

/** 観点を、保存済みの値から変えたか。 */
export const recipesChanged = (st: HookUiState): boolean => !!st.status && !sameSet(currentRecipes(st), st.status.recipes);

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
  const boxes = options.length
    ? options.map((r) =>
        h(
          'label',
          { class: 'hookrecipe' },
          h('input', {
            type: 'checkbox',
            disabled: st.busy,
            ...(chosen.includes(r.id) ? { checked: true } : {}),
            onchange: (e: Event) => act.pick(pickOf(r.id, (e.target as HTMLInputElement).checked)),
          }),
          ' ',
          r.title,
          ' ',
          h('code', {}, r.id),
        ),
      )
    : [h('p', { class: 'muted' }, m.noRecipes)];

  const block = enableBlock(st);
  const buttons = s.enabled
    ? [
        h('button', { disabled: st.busy, onclick: () => act.toggle(false, chosen) }, m.disable),
        recipesChanged(st) ? h('button', { disabled: st.busy || chosen.length === 0, onclick: () => act.saveRecipes(chosen) }, m.saveRecipes) : null,
      ]
    : [h('button', { class: 'primary', disabled: block !== null, onclick: () => act.toggle(true, chosen) }, m.enable)];

  const last = s.last_run;
  return h(
    'section',
    { class: 'sgroup hookview' },
    ...head,
    h('p', {}, h('strong', {}, s.enabled ? m.stateOn : m.stateOff), ' ', st.busy ? h('span', { class: 'muted' }, m.busy) : null),
    h('p', { class: 'muted' }, s.installed ? m.installedYes : m.installedNo),
    !s.cli ? h('p', { class: 'err' }, m.cliMissing(INSTALL_CMD, INSTALL_CMD_NPM)) : h('p', { class: 'muted' }, m.cliPath(s.cli)),
    s.blocked ? h('p', { class: 'err' }, m.blocked(s.blocked)) : null,
    s.target_missing ? h('p', { class: 'err' }, m.targetMissing) : null,
    h('div', { class: 'hookrecipes' }, h('h4', {}, m.recipesLabel), ...boxes, h('p', { class: 'muted' }, m.recipesNote)),
    h('div', { class: 'row actions' }, ...buttons),
    st.error ? h('p', { class: 'err' }, m.failed(st.error)) : null,
    h('p', { class: 'muted' }, m.costNote),
    h('p', { class: 'muted' }, m.keyNote),
    h('h4', {}, m.lastRunTitle),
    last ? h('p', { class: last.code === 2 ? 'err' : '' }, m.lastRun(last.code, formatWhen(last.at_ms, lang), last.message)) : h('p', { class: 'muted' }, m.lastRunNone),
  );
}
