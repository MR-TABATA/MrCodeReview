// 設定画面。項目の定義（`settings.rs` の `meta()`）から作るので、項目が増えても、画面は作り直さない。
//
// 入力のたびに画面全体を作り直すと、入力欄のフォーカスが外れる。そのため、この画面は一度だけ作り、
// 入力に応じて、変更の印・エラー・保存ボタンだけを更新する。

import { h } from './dom';
import { getAppLang, msg, resolveLang, SUPPORTED_LANGS } from './i18n';
import {
  changedKeys,
  dirtyKeys,
  formatValue,
  groupsOf,
  listText,
  parseList,
  parseNumber,
  rangeText,
  same,
  tabIdOf,
  tabsFor,
  validate,
  type Values,
} from './settingsmodel';
import type { SettingMeta, SettingsView } from './types';

export interface SettingsPanelState {
  view: SettingsView;
  /** 入力中の値。保存するまで、`view.values` とは別に持つ。 */
  draft: Values;
  saving: boolean;
  message: string;
  messageKind: 'ok' | 'err' | '';
  /** いま開いているタブ（画面を作り直しても、同じタブを開く）。無ければ、先頭 */
  tab?: string;
  /** 未保存の変更があるまま閉じようとして、「保存するか」を聞いている最中 */
  confirmClose?: boolean;
}

/** 設定の項目ではない欄（タブごとに置く）。 */
export interface SettingsExtras {
  /** 「LLM・API キー」タブ */
  llm?: HTMLElement | null;
  /** 「リポジトリ」タブ（登録したリポジトリの編集） */
  repos?: HTMLElement | null;
  /** 「ライセンス」タブ（Free / Pro の状態と、キーの登録） */
  /** 「連携」タブ（会話の記録を読む許可など） */
  link?: HTMLElement | null;
}

export interface SettingsActions {
  save(): void;
  /** 保存して、保存できたら閉じる（保存できなければ、開いたまま） */
  saveAndClose(): void;
  /** 保存せずに閉じる（未保存の変更は、捨てる） */
  close(): void;
  /** 表示言語の選択が切り替わったときの通知（即時プレビュー用） */
  onLangChange?: (lang: string) => void;
}

const clone = <T,>(v: T): T => JSON.parse(JSON.stringify(v)) as T;

/** `extra` は、項目ではない欄（LLM・API キー、会話の記録の読み取りの許可）。それぞれのタブに置く。 */
export function settingsPanel(st: SettingsPanelState, act: SettingsActions, extra?: SettingsExtras): HTMLElement {
  const meta = st.view.meta;
  const lang = resolveLang(st.draft.language ?? st.view.values.language ?? getAppLang());
  const dict = msg(lang);
  const ui = dict.settingsUi;
  const tabs = tabsFor(meta, lang);
  if (!tabs.some((t) => t.id === st.tab)) st.tab = tabs[0].id;
  /** タブの見出しの印（別のタブにある、入力の誤りと未保存の変更を、見落とさないため） */
  const marks = new Map<string, HTMLElement>();
  const rows = new Map<string, { badge: HTMLElement; err: HTMLElement; reset: HTMLButtonElement; card: HTMLElement; setInput: () => void }>();

  const tally = h('span', { class: 'tally' });
  const saveBtn = h('button', { class: 'run save', onclick: () => act.save() }, ui.save);
  const undoBtn = h('button', {}, ui.undo);
  const resetAllBtn = h('button', {}, ui.resetAll);
  const msgEl = h('span', { class: `smsg ${st.messageKind}` }, st.message);

  // 未保存の変更があるまま「閉じる」を押したとき、確認なしで捨てない。保存して閉じる・捨てて閉じる・戻る、を選ばせる
  const confirmText = h('p', {});
  const confirmSave = h('button', { class: 'run save', onclick: () => act.saveAndClose() }, ui.saveAndClose);
  const confirmDiscard = h('button', { onclick: () => act.close() }, ui.discardAndClose);
  const confirmBack = h('button', {
    onclick: () => {
      st.confirmClose = false;
      refresh();
    },
  }, ui.keepEditing);
  const closeConfirm = h('div', { class: 'confirm close-confirm', role: 'alert' }, confirmText, h('div', { class: 'row' }, confirmSave, confirmDiscard, confirmBack));
  closeConfirm.hidden = true;
  const requestClose = (): void => {
    if (dirtyKeys(meta, st.view.values, st.draft).length > 0) {
      st.confirmClose = true;
      refresh();
    } else {
      act.close();
    }
  };

  const refresh = (): void => {
    const errors = validate(meta, st.draft);
    const dirty = dirtyKeys(meta, st.view.values, st.draft);
    const changed = changedKeys(meta, st.draft);
    for (const m of meta) {
      const r = rows.get(m.key)!;
      const err = errors[m.key];
      r.err.textContent = err ?? '';
      r.err.hidden = !err;
      r.badge.hidden = !changed.includes(m.key);
      r.reset.disabled = same(st.draft[m.key], m.recommended);
      r.card.classList.toggle('invalid', !!err);
      r.card.classList.toggle('dirty', dirty.includes(m.key));
    }
    for (const t of tabs) {
      const keys = meta.filter((m) => tabIdOf(m.group) === t.id).map((m) => m.key);
      const e = keys.filter((k) => errors[k]).length;
      const d = keys.filter((k) => dirty.includes(k)).length;
      const mark = marks.get(t.id)!;
      mark.textContent = e ? `⚠ ${e}` : d ? '●' : '';
      mark.className = `tmark ${e ? 'bad' : d ? 'dirty' : ''}`;
      mark.title = e
        ? ui.validationErrorsCount(e)
        : d
        ? ui.unsavedChangesCount(d)
        : '';
    }
    const nErr = Object.keys(errors).length;
    tally.textContent = ui.tallyText(changed.length, dirty.length, nErr);
    saveBtn.disabled = st.saving || dirty.length === 0 || nErr > 0;
    saveBtn.textContent = st.saving ? ui.saving : ui.save;
    undoBtn.disabled = st.saving || dirty.length === 0;
    resetAllBtn.disabled = st.saving || changed.length === 0;
    // 変更を取り消した・保存できたなどで、未保存が無くなったら、確認は、引っ込める
    const asking = !!st.confirmClose && dirty.length > 0;
    if (!asking) st.confirmClose = false;
    closeConfirm.hidden = !asking;
    confirmText.textContent = ui.confirmUnsaved(dirty.length);
    confirmSave.disabled = st.saving || nErr > 0;
    confirmSave.title = nErr > 0 ? ui.cannotSaveWithErrors : '';
  };

  const assign = (values: Values): void => {
    const prevLang = String(st.draft.language ?? 'auto');
    for (const m of meta) st.draft[m.key] = clone(values[m.key]);
    for (const r of rows.values()) r.setInput();
    refresh();
    const nextLang = String(st.draft.language ?? 'auto');
    if (prevLang !== nextLang) act.onLangChange?.(nextLang);
  };
  undoBtn.addEventListener('click', () => assign(st.view.values));
  resetAllBtn.addEventListener('click', () => assign(Object.fromEntries(meta.map((m) => [m.key, m.recommended]))));

  const control = (m: SettingMeta): { el: HTMLElement; setInput: () => void } => {
    if (m.kind === 'select') {
      const sel = h(
        'select',
        { class: 'sinput select' },
        h('option', { value: 'auto' }, dict.autoLanguageOptionLabel),
        ...SUPPORTED_LANGS.map((l) => h('option', { value: l.code }, dict.languageNames[l.code] ?? l.nativeName)),
      ) as HTMLSelectElement;
      const setInput = () => void (sel.value = String(st.draft[m.key] ?? m.recommended));
      setInput();
      sel.addEventListener('change', () => {
        st.draft[m.key] = sel.value;
        if (m.key === 'language') {
          act.onLangChange?.(sel.value);
        }
        refresh();
      });
      return { el: h('div', { class: 'sctl' }, sel), setInput };
    }
    if (m.kind === 'list') {
      const ta = h('textarea', { class: 'sinput list', rows: '4', spellcheck: 'false' });
      const setInput = () => void (ta.value = listText(st.draft[m.key]));
      setInput();
      ta.addEventListener('input', () => {
        st.draft[m.key] = parseList(ta.value);
        refresh();
      });
      return { el: h('div', { class: 'sctl' }, ta, h('span', { class: 'muted' }, ui.onePerLine)), setInput };
    }
    const input = h('input', {
      class: 'sinput',
      type: 'number',
      step: m.kind === 'integer' ? '1' : 'any',
      ...(m.min !== null ? { min: String(m.min) } : {}),
      ...(m.max !== null ? { max: String(m.max) } : {}),
    });
    const setInput = () => void (input.value = Number.isFinite(st.draft[m.key] as number) ? String(st.draft[m.key]) : '');
    setInput();
    input.addEventListener('input', () => {
      st.draft[m.key] = parseNumber(input.value);
      refresh();
    });
    return { el: h('div', { class: 'sctl' }, input, m.unit ? h('span', { class: 'muted' }, m.unit) : null), setInput };
  };

  const card = (m: SettingMeta): HTMLElement => {
    const { el, setInput } = control(m);
    const metaLocalized = dict.settingMeta[m.key];
    const labelText = metaLocalized?.label ?? m.label;
    const descText = metaLocalized?.description ?? m.description;
    const whyText = metaLocalized?.why ?? m.why_recommended;
    const whenText = metaLocalized?.when ?? m.when_to_change;

    const badge = h('span', { class: 'badge warnb' }, ui.customizedBadge);
    const err = h('div', { class: 'serr' });
    const reset = h('button', { class: 'small', onclick: () => {
      const prevLang = String(st.draft.language ?? 'auto');
      st.draft[m.key] = clone(m.recommended);
      setInput();
      refresh();
      if (m.key === 'language' && prevLang !== String(m.recommended)) {
        act.onLangChange?.(String(m.recommended));
      }
    } }, ui.resetToDefault);
    const rText = rangeText(m, lang);
    const c = h(
      'article',
      { class: 'scard' },
      h(
        'div',
        { class: 'shead' },
        h('strong', {}, labelText),
        h('code', {}, m.key),
        badge,
        st.view.repo.keys.includes(m.key) ? h('span', { class: 'badge repo' }, ui.overriddenByRepo) : null,
        h('span', { class: 'scope muted' }, m.repo_scope ? ui.canOverrideByRepo : ui.personalSettingOnly),
      ),
      h('p', { class: 'sdesc' }, descText),
      el,
      err,
      h(
        'div',
        { class: 'srec' },
        h('span', {}, ui.defaultLabel(formatValue(m, m.recommended, lang))),
        rText ? h('span', { class: 'muted' }, ui.rangeLabel(rText)) : null,
        reset,
      ),
      h(
        'details',
        { class: 'swhy' },
        h('summary', {}, ui.whySummary),
        h('p', {}, h('b', {}, ui.whyPrefix), whyText),
        h('p', {}, h('b', {}, ui.whenPrefix), whenText),
      ),
    );
    rows.set(m.key, { badge, err, reset, card: c, setInput });
    return c;
  };

  // タブ: 項目の入るグループごとに、まとめる。選んだタブ以外は、見えなくするだけ（入力の検証・保存は、全部のタブをまとめて行う）
  const groups = groupsOf(meta);
  const panes = new Map<string, HTMLElement>();
  for (const t of tabs) {
    panes.set(
      t.id,
      h(
        'div',
        { class: 'stab' },
        h('p', { class: 'muted stab-desc' }, t.desc),
        t.id === 'llm' ? (extra?.llm ?? null) : null,
        t.id === 'repos' ? (extra?.repos ?? null) : null,
        t.id === 'link' ? (extra?.link ?? null) : null,
        ...groups.filter((g) => tabIdOf(g) === t.id).map((g) => {
          const groupTitle = dict.groupNames[g] ?? g;
          return h('section', { class: 'sgroup' }, h('h3', {}, groupTitle), ...meta.filter((m) => m.group === g).map(card));
        }),
      ),
    );
  }
  const tabBtns = new Map<string, HTMLButtonElement>();
  const select = (id: string): void => {
    st.tab = id;
    for (const [tid, pane] of panes) pane.hidden = tid !== id;
    for (const [tid, b] of tabBtns) b.classList.toggle('on', tid === id);
  };
  const nav = h(
    'nav',
    { class: 'tabs stabs' },
    ...tabs.map((t) => {
      const mark = h('span', { class: 'tmark' });
      marks.set(t.id, mark);
      const b = h('button', { onclick: () => select(t.id) }, t.label, mark);
      tabBtns.set(t.id, b);
      return b;
    }),
  );

  const v = st.view;
  const repoKeysText = v.repo.keys.length ? v.repo.keys.join(dict.listSeparator) : dict.noneParen;
  const panel = h(
    'div',
    { class: 'settings' },
    h(
      'div',
      { class: 'd-head' },
      h('h2', {}, ui.title),
      h('p', { class: 'muted' }, ui.intro1),
      h('p', { class: 'muted' }, ui.introFile(v.path, v.exists)),
    ),
    v.repo.path
      ? v.repo.error
        ? h('p', { class: 'err' }, ui.repoInvalid(v.repo.path, v.repo.error))
        : h('p', { class: 'note-line' }, ui.repoOverrides(v.repo.path, repoKeysText))
      : null,
    v.error ? h('p', { class: 'err' }, ui.fileInvalid(v.error)) : null,
    h('div', { class: 'sbar' }, h('div', { class: 'row' }, tally, h('span', { class: 'spacer' }), msgEl), closeConfirm, h('div', { class: 'row actions' }, saveBtn, undoBtn, resetAllBtn, h('span', { class: 'spacer' }), h('button', { onclick: () => requestClose() }, ui.close)), nav),
    ...panes.values(),
  );
  select(st.tab!);
  refresh();
  return panel;
}
