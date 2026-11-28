import { h } from './dom';
import { displayRecipeTitle } from './historyview';
import { msg, type Lang } from './i18n';
import type { AspectRow, AspectStats } from './aspectstats';

const pct = (p: number | null): string => (p === null ? '−' : `${p}%`);

function card(label: string, value: string): HTMLElement {
  return h('div', { class: 'aspect-card' }, h('div', { class: 'aspect-card-t' }, label), h('div', { class: 'aspect-card-n' }, value));
}

/** 判断の内訳の横棒。幅は、その観点の仕分けの合計に対する割合。合計が 0 なら、空の棒。 */
function breakdownBar(r: AspectRow, lang: Lang): HTMLElement {
  const m = msg(lang).mainUi.aspects;
  const total = r.open + r.done + r.wait + r.dismiss;
  const seg = (cls: string, n: number) => (n > 0 ? h('i', { class: `ab-${cls}`, style: `width:${(n / total) * 100}%` }) : null);
  return h(
    'div',
    { class: 'aspect-bar', title: m.breakdownTitle(r.open, r.done, r.wait, r.dismiss), role: 'img', 'aria-label': m.breakdownTitle(r.open, r.done, r.wait, r.dismiss) },
    ...(total === 0 ? [] : [seg('done', r.done), seg('wait', r.wait), seg('dismiss', r.dismiss), seg('open', r.open)]),
  );
}

function aspectRow(r: AspectRow, lang: Lang, tools: ReadonlySet<string>, onHistory: (title: string) => void): HTMLElement {
  const m = msg(lang).mainUi.aspects;
  const name = displayRecipeTitle(r.title, lang);
  return h(
    'div',
    { class: 'aspect-row', 'data-aspect': r.title },
    h(
      'div',
      { class: 'aspect-name' },
      h('b', {}, name),
      tools.has(r.title) ? h('span', { class: 'badge' }, m.toolBadge) : null,
      r.failed > 0 ? h('span', { class: 'aspect-failed' }, m.failedNote(r.failed)) : null,
    ),
    h('div', { class: 'aspect-num' }, String(r.runs)),
    h('div', { class: 'aspect-num' }, String(r.findings)),
    breakdownBar(r, lang),
    h('div', { class: 'aspect-num', title: m.ignoreTitle }, pct(r.ignorePct)),
    h('button', { class: 'btn aspect-hist', type: 'button', title: m.historyBtnTitle(name), onclick: () => onHistory(name) }, m.historyBtn),
  );
}

/**
 * 観点のページ。カード 4 枚と、観点ごとの表。履歴・ログへは、観点名で絞り込んで移る。
 * `tools`: tool 型の観点のタイトル（印を付ける）。
 */
export function aspectsPanel(stats: AspectStats, total: number, lang: Lang, tools: ReadonlySet<string>, onHistory: (title: string) => void): HTMLElement {
  const m = msg(lang).mainUi.aspects;
  if (stats.rows.length === 0) return h('div', { class: 'aspects' }, h('p', { class: 'hint aspects-empty' }, m.empty));
  return h(
    'div',
    { class: 'aspects' },
    h(
      'div',
      { class: 'aspect-cards' },
      card(m.cardAspects, String(stats.totals.aspects)),
      card(m.cardRuns, String(stats.totals.runs)),
      card(m.cardFindings, String(stats.totals.findings)),
      card(m.cardIgnore, pct(stats.totals.ignorePct)),
    ),
    h(
      'div',
      { class: 'aspect-table', role: 'table' },
      h(
        'div',
        { class: 'aspect-row aspect-head', role: 'row' },
        h('div', {}, m.colAspect),
        h('div', { class: 'aspect-num' }, m.colRuns),
        h('div', { class: 'aspect-num' }, m.colFindings),
        h('div', {}, m.colBreakdown),
        h('div', { class: 'aspect-num', title: m.ignoreTitle }, m.colIgnore),
        h('div', {}),
      ),
      ...stats.rows.map((r) => aspectRow(r, lang, tools, onHistory)),
    ),
    h('p', { class: 'hint aspects-scope' }, m.scope(total)),
    stats.unreadable > 0 ? h('p', { class: 'err aspects-unreadable' }, m.unreadable(stats.unreadable)) : null,
  );
}
