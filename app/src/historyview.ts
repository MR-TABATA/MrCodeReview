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

/** 連番を丸数字（①〜⑳、㉑〜㉟、㊱〜㊵）に変換する。50を超える場合は (n) */
export function circleNumber(n: number): string {
  if (n >= 1 && n <= 20) return String.fromCharCode(0x2460 + n - 1);
  if (n >= 21 && n <= 35) return String.fromCharCode(0x3251 + n - 21);
  if (n >= 36 && n <= 50) return String.fromCharCode(0x32b1 + n - 36);
  return `(${n})`;
}

/** レビューの名前。「潜在バグ ①」（観点の名前 + スペース + 丸数字連番）。番号を振る前の記録は、観点の名前だけ。 */
export function reviewName(title: string, seq: number | null | undefined, lang: Lang = getAppLang()): string {
  const t = displayRecipeTitle(title, lang);
  return seq ? `${t} ${circleNumber(seq)}` : t;
}

/** 日時の表示。言語（`bcp47`）に合わせる。 */
export const formatWhen = (ms: number, lang: Lang = getAppLang()): string => new Date(ms).toLocaleString(msg(lang).bcp47);
const when = formatWhen;

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

/** コミットの短い表示（7 文字）。無ければ空。 */
export const shortCommit = (commit: string | null | undefined): string => (commit ?? '').slice(0, 7);

/**
 * レビュー 1 回ぶんの 1 行。履歴と、ログで、同じ書式にする。
 * 左から: 先頭の欄（チェック。フックの行は空。幅は同じ）・日時・観点（と失敗）・コミット・指摘の件数・仕分けの件数。
 * フックの実行の行（hookview.ts の hookRunRow）も、同じ列にそろえる。
 */
export function reviewRow(
  s: ReviewSummary,
  lang: Lang,
  o: { cls?: string; lead?: Node | null; onOpen: () => void; extra?: Node | null },
): HTMLElement {
  const m = msg(lang).historyView;
  return h(
    'li',
    { class: (o.cls ?? '').trim(), onclick: o.onOpen },
    h('span', { class: 'hlead' }, o.lead ?? null),
    // 1 行: 日時 ・ 観点（と失敗） ・ 指摘の件数 ・ 仕分けの件数。行を押すと、開く
    h('span', { class: 'hwhen' }, when(s.created_at, lang)),
    h(
      'span',
      { class: 'h1' },
      h('span', { class: 'ht' }, reviewName(s.recipe_title, s.seq, lang)),
      s.failed ? h('span', { class: 'badge bad' }, m.failedBadge) : null,
    ),
    h('span', { class: 'hcommit', title: s.head ?? '' }, shortCommit(s.head)),
    h('span', { class: 'hfind' }, m.findingsCount(s.findings)),
    // 指摘のあるレビューだけ（0 件・失敗には、出さない）。読み取りに失敗したとき（null）も、出さない（0 件に見せない）
    s.counts && s.findings > 0 ? countsLine(s.counts, lang) : h('span', { class: 'hcounts-empty' }),
    o.extra ?? null,
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
    reviewRow(s, lang, {
      cls: `${openId === s.id ? 'on' : ''} ${picked.has(s.id) ? 'picked' : ''}`,
      onOpen: () => act.open(s.id),
      lead: h('input', {
        class: 'pick',
        type: 'checkbox',
        'aria-label': m.selectForReportAria,
        ...(picked.has(s.id) ? { checked: true } : {}),
        // 行を押したことには、しない（結果が開いてしまう）
        onclick: (e) => e.stopPropagation(),
        onchange: () => act.toggle(s.id),
      }),
    });
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
