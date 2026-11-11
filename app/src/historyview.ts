// 右のパネル「履歴」。いつも出すのは、新しい 5 件と、「未」が残っているもの。それより古いものは、年月日ごとに、閉じてまとめる。
// 目次に絞る（観点・日時・指摘の件数と、仕分けの件数）。細かい数字（所要時間・トークンなど）は、「詳細」に集める。
// 各行にチェックの欄があり、選んだレビューだけを、レポートに出せる。

import { h } from './dom';
import { getAppLang, msg, type Lang } from './i18n';
import type { ReviewSummary, TriageCounts } from './types';

export interface HistoryActions {
  /** 行を押した（結果を開く） */
  open(id: string): void;
  /** 「詳細」を押した（結果を開いて、「詳細」タブを出す） */
  detail(id: string): void;
  /** チェックを付ける／外す */
  toggle(id: string): void;
  /** 日付のグループを、開く／閉じる（`dayKey` の値） */
  toggleDay(key: string): void;
  /** すべて選ぶ／すべて外す */
  toggleAll(): void;
  /** 選んだものを、レポートに出す */
  report(): void;
  /** 選んだものを、履歴から削除する（確認は、呼ぶ側で取る） */
  remove(): void;
}

export function displayRecipeTitle(title: string, lang: Lang = getAppLang()): string {
  return msg(lang).builtinRecipeTitles[title] ?? title;
}

const when = (ms: number, lang: Lang): string => new Date(ms).toLocaleString(msg(lang).bcp47);

/** 履歴に、いつも出す件数（新しいほうから）。これより古いものは、「未」が残っていなければ、日付ごとにまとめる */
export const RECENT_KEEP = 5;

/** まだ、終わっていない作業がある（未が 1 件以上）。件数が読めなかったとき（null）も、隠さず出す。指摘が 0 件のものは、対象外 */
export const needsAttention = (s: ReviewSummary): boolean => s.findings > 0 && (s.counts === null || s.counts.open > 0);

const pad = (n: number): string => String(n).padStart(2, '0');
/** 日付のグループの識別（手元の時刻の、年-月-日） */
export const dayKey = (ms: number): string => {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
};
export const dayLabel = (ms: number, lang: Lang = getAppLang()): string => msg(lang).historyView.formatDay(new Date(ms));

export interface DayGroup {
  key: string;
  label: string;
  items: ReviewSummary[];
}

/**
 * 履歴を、「いつも出すもの」と、「日付ごとにまとめるもの」に分ける（`items` は、新しい順）。
 * いつも出す: 新しいほうから `RECENT_KEEP` 件、または、「未」が残っているもの（古くても）。
 * 残りは、年月日ごと（新しい日から）。並びは、いつも、新しい順のまま。
 */
export function splitHistory(items: ReviewSummary[], lang: Lang = getAppLang()): { shown: ReviewSummary[]; groups: DayGroup[] } {
  const shown: ReviewSummary[] = [];
  const groups: DayGroup[] = [];
  items.forEach((s, i) => {
    if (i < RECENT_KEEP || needsAttention(s)) {
      shown.push(s);
      return;
    }
    const key = dayKey(s.created_at);
    const last = groups[groups.length - 1];
    if (last && last.key === key) last.items.push(s);
    else groups.push({ key, label: dayLabel(s.created_at, lang), items: [s] });
  });
  return { shown, groups };
}

/** 履歴にある識別子だけを残す（切り替えや削除で、無くなったものを、選んだままにしない）。 */
export function prunePicks(picked: Set<string>, items: ReviewSummary[]): Set<string> {
  const ids = new Set(items.map((s) => s.id));
  return new Set([...picked].filter((id) => ids.has(id)));
}

/** 仕分けの件数の 1 行: 未・済み・待ち・無視（4 つを足すと、指摘の数）。0 件のものは、薄くする。 */
export function countsLine(c: TriageCounts, lang: Lang = getAppLang()): HTMLElement {
  const m = msg(lang).historyView;
  const chip = (cls: string, label: string, n: number) => h('span', { class: `hc hc-${cls}${n === 0 ? ' zero' : ''}` }, m.countUnit(label, n));
  return h(
    'div',
    { class: 'h2 hcounts', title: m.countsTitle },
    chip('open', m.countOpen, c.open),
    chip('done', m.countDone, c.done),
    chip('wait', m.countWait, c.wait),
    chip('dismiss', m.countDismiss, c.dismiss),
  );
}

export function historyPanel(
  items: ReviewSummary[],
  openId: string | null,
  picked: Set<string>,
  scope: string,
  scopeTitle: string,
  emptyText: string,
  act: HistoryActions,
  openDays: Set<string> = new Set(),
  isCollapsed: boolean = false,
  onToggleCollapse?: () => void,
  lang: Lang = getAppLang(),
): HTMLElement {
  const m = msg(lang).historyView;
  const all = items.length > 0 && items.every((s) => picked.has(s.id));
  const row = (s: ReviewSummary): HTMLElement =>
    h(
      'li',
      { class: `${openId === s.id ? 'on' : ''} ${picked.has(s.id) ? 'picked' : ''}`, onclick: () => act.open(s.id) },
      h('input', {
        class: 'pick',
        type: 'checkbox',
        'aria-label': m.selectForReportAria,
        ...(picked.has(s.id) ? { checked: true } : {}),
        // 行を押したことには、しない（結果が開いてしまう）
        onclick: (e) => e.stopPropagation(),
        onchange: () => act.toggle(s.id),
      }),
      h(
        'div',
        { class: 'hbody' },
        h(
          'div',
          { class: 'h1' },
          h('span', { class: 'ht' }, displayRecipeTitle(s.recipe_title, lang)),
          s.failed ? h('span', { class: 'badge bad' }, m.failedBadge) : null,
          h(
            'button',
            {
              class: 'small linkish hdetail',
              title: m.detailTitle,
              // 行を押したことには、しない（「詳細」タブを出すため、別に開く）
              onclick: (e) => {
                e.stopPropagation();
                act.detail(s.id);
              },
            },
            m.detailBtn,
          ),
        ),
        h('div', { class: 'h2' }, `${when(s.created_at, lang)} · ${m.findingsCount(s.findings)}`),
        // 指摘のあるレビューだけ（0 件・失敗には、出さない）。読み取りに失敗したとき（null）も、出さない（0 件に見せない）
        s.counts && s.findings > 0 ? countsLine(s.counts, lang) : null,
      ),
    );
  const { shown, groups } = splitHistory(items, lang);
  // 日付のグループ: 閉じておく。結果を開いているレビューが入っているグループは、開く（行が、見えなくならないように）
  const groupEls = groups.map((g) => {
    const open = openDays.has(g.key) || g.items.some((s) => s.id === openId);
    const countLabel = m.groupCount(g.items.length);
    return h(
      'div',
      { class: 'hday' },
      h('button', { class: 'hday-h', 'aria-expanded': String(open), onclick: () => act.toggleDay(g.key) }, `${open ? '▾' : '▸'} ${g.label}${countLabel}`),
      open ? h('ul', { class: 'history' }, ...g.items.map(row)) : null,
    );
  });
  const n = items.filter((s) => picked.has(s.id)).length;

  const headTitle = h(
    'h2',
    {
      class: 'hhead-title clickable',
      title: isCollapsed ? m.expandHistory : m.collapseHistory,
      onclick: onToggleCollapse ?? (() => {}),
    },
    h('span', { class: 'toggle-arrow' }, isCollapsed ? '▸ ' : '▾ '),
    m.heading,
    items.length ? h('span', { class: 'count-badge' }, ` (${items.length})`) : null,
  );

  const head = h(
    'div',
    { class: 'hhead' },
    headTitle,
    !isCollapsed && items.length
      ? h('button', { class: 'small linkish', onclick: () => act.toggleAll() }, all ? m.deselectAll : m.selectAll)
      : null,
  );

  if (isCollapsed) {
    return h('section', { class: 'history-section collapsed' }, head);
  }

  return h(
    'section',
    { class: 'history-section' },
    head,
    h('p', { class: 'scope-line', title: scopeTitle }, scope),
    n > 0
      ? h(
          'div',
          { class: 'pickbar' },
          h('span', {}, m.selectedCount(n)),
          h('button', { class: 'run small', onclick: () => act.report() }, m.exportReportBtn),
          h('button', { class: 'small danger', onclick: () => act.remove() }, m.deleteBtn),
        )
      : null,
    items.length ? h('ul', { class: 'history' }, ...shown.map(row)) : h('p', { class: 'muted' }, emptyText),
    ...groupEls,
  );
}
