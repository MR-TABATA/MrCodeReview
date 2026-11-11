// トークン使用量の画面。記録した「測った値」だけを集計して見せる（推定で埋めない）。

import { h } from './dom';
import { displayRecipeTitle } from './historyview';
import { DEFAULT_LANG, getAppLang, msg, type Lang } from './i18n';
import type { UsageRow, UsageSummary } from './types';

export type Period = 'today' | 'week' | 'all';

export function getPeriods(lang: Lang): [Period, string][] {
  const p = msg(lang).usageView.periods;
  return [
    ['today', p.today],
    ['week', p.week],
    ['all', p.all],
  ];
}

export const PERIODS: [Period, string][] = getPeriods(DEFAULT_LANG);

/** 期間の始まり（UNIX ミリ秒）。「すべて」は null。「今日」は、今日の 0 時（この Mac の時刻）から。 */
export function periodSince(p: Period, now: Date = new Date()): number | null {
  if (p === 'all') return null;
  const d = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  if (p === 'week') d.setDate(d.getDate() - 6);
  return d.getTime();
}

/** 棒の長さ（0〜100）。最大の行を 100 とする。 */
export function barPct(row: UsageRow, rows: UsageRow[]): number {
  const max = Math.max(0, ...rows.map((r) => r.input + r.output));
  return max === 0 ? 0 : Math.round(((row.input + row.output) / max) * 100);
}

export interface UsageState {
  period: Period;
  data: UsageSummary | null;
  loading: boolean;
  error: string;
}

export interface UsageActions {
  setPeriod(p: Period): void;
  close(): void;
}

const nf = new Intl.NumberFormat('ja-JP');
const shortRepo = (p: string): string => p.split('/').filter(Boolean).slice(-2).join('/');

function table(title: string, rows: UsageRow[], label: (k: string) => string, lang: Lang = getAppLang()): HTMLElement {
  const m = msg(lang).usageView;
  return h(
    'section',
    { class: 'utable' },
    h('h3', {}, title),
    rows.length
      ? h(
          'table',
          { class: 'kvt wide' },
          h('thead', {}, h('tr', {}, h('th', {}, ''), h('th', {}, m.colReviews), h('th', {}, m.colInput), h('th', {}, m.colOutput), h('th', {}, ''))),
          h(
            'tbody',
            {},
            ...rows.map((r) =>
              h(
                'tr',
                {},
                h('td', { title: r.key }, label(r.key)),
                h('td', {}, nf.format(r.reviews), r.failed ? h('span', { class: 'muted' }, m.failedSuffix(r.failed)) : null),
                h('td', {}, nf.format(r.input)),
                h('td', {}, nf.format(r.output), r.unreported ? h('span', { class: 'muted', title: m.unreportedTitle }, m.unreportedSuffix(r.unreported)) : null),
                h('td', { class: 'ubar-cell' }, h('div', { class: 'ubar', style: `width:${barPct(r, rows)}%` })),
              ),
            ),
          ),
        )
      : h('p', { class: 'muted' }, m.noRecords),
  );
}

export function usagePanel(st: UsageState, act: UsageActions, lang: Lang = getAppLang()): HTMLElement {
  const m = msg(lang).usageView;
  const d = st.data;
  const card = (label: string, value: number) => h('div', { class: 'ucard' }, h('div', { class: 'ulabel' }, label), h('div', { class: 'unum' }, nf.format(value)));
  const periods = getPeriods(lang);
  return h(
    'div',
    { class: 'usage' },
    h(
      'div',
      { class: 'd-head' },
      h('h2', {}, m.heading),
      h('p', { class: 'muted' }, m.subtitle),
    ),
    h(
      'div',
      { class: 'row ubar-row' },
      h(
        'div',
        { class: 'dmode' },
        ...periods.map(([k, label]) => h('button', { class: st.period === k ? 'on' : '', onclick: () => act.setPeriod(k) }, label)),
      ),
      h('span', { class: 'spacer' }),
      h('button', { onclick: () => act.close() }, m.closeBtn),
    ),
    st.error ? h('p', { class: 'err' }, st.error) : null,
    st.loading && !d ? h('p', { class: 'muted' }, m.calculating) : null,
    d
      ? h(
          'div',
          {},
          h(
            'div',
            { class: 'ucards' },
            card(m.cardInput, d.total.input),
            card(m.cardOutput, d.total.output),
            card(m.cardCacheRead, d.total.cache_read),
            card(m.cardCacheWrite, d.total.cache_write),
          ),
          h(
            'p',
            { class: 'usum' },
            m.totalReviews(nf.format(d.total.reviews)),
            d.total.failed ? m.totalFailedExcluded(d.total.failed) : '',
            d.total.unreported ? m.totalUnreportedExcluded(d.total.unreported) : '',
          ),
          d.unreadable > 0 ? h('p', { class: 'err' }, m.unreadableError(d.unreadable)) : null,
          d.cost_usd !== null ? h('p', { class: 'muted' }, m.reportedCost(d.cost_usd.toFixed(4), d.cost_reviews)) : null,
          table(m.byAngle, d.by_recipe, (k) => displayRecipeTitle(k, lang), lang),
          table(m.byRepo, d.by_repo, shortRepo, lang),
          table(m.byModel, d.by_model, (k) => k, lang),
          h('p', { class: 'note' }, m.footnote),
        )
      : null,
  );
}
