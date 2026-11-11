// レポート（成果物にできるレビューの記録）の画面。期間・リポジトリ・含める項目を選び、プレビューを見て、
// Markdown / Excel で保存するか、PDF（印刷）で出す。内容は、バックエンド（`report.rs`）が作る。

import { h } from './dom';
import { DEFAULT_LANG, getAppLang, msg, type Lang } from './i18n';
import type { ReportOptions, ReportPreview } from './types';

export type RPeriod = 'today' | 'week' | 'month' | 'all' | 'custom';

export function getRPeriods(lang: Lang): [RPeriod, string][] {
  const p = msg(lang).reportView.periods;
  return [
    ['today', p.today],
    ['week', p.week],
    ['month', p.month],
    ['all', p.all],
    ['custom', p.custom],
  ];
}

export const RPERIODS: [RPeriod, string][] = getRPeriods(DEFAULT_LANG);

export interface ReportState {
  period: RPeriod;
  /** 期間を指定するときの日付（yyyy-mm-dd） */
  from: string;
  to: string;
  /** すべてのリポジトリ（false なら、選んでいるリポジトリだけ） */
  allRepos: boolean;
  evidence: boolean;
  prompt: boolean;
  tokens: boolean;
  log: boolean;
  preview: ReportPreview | null;
  loading: boolean;
  error: string;
  message: string;
  /** 出力中の形式（空なら、していない） */
  busy: '' | 'md' | 'xlsx' | 'pdf';
  /** 履歴から選んだレビューの識別子。null なら、期間とリポジトリで絞る */
  reviewIds: string[] | null;
}

export function newReportState(): ReportState {
  return { period: 'week', from: '', to: '', allRepos: false, evidence: true, prompt: false, tokens: true, log: true, preview: null, loading: false, error: '', message: '', busy: '', reviewIds: null };
}

const pad = (n: number): string => String(n).padStart(2, '0');
export const ymd = (d: Date): string => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** yyyy-mm-dd を、その日の 0 時（この Mac の時刻）にする。読めなければ null。 */
function parseDay(s: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  if (!m) return null;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return Number.isNaN(d.getTime()) || d.getMonth() !== Number(m[2]) - 1 ? null : d;
}

/** 期間の始まりと終わり（UNIX ミリ秒）。null は、その側に限りが無い。期間の指定が正しくなければ、理由を返す。 */
export function reportRange(
  st: Pick<ReportState, 'period' | 'from' | 'to'>,
  now: Date = new Date(),
  lang: Lang = getAppLang(),
): { since: number | null; until: number | null; error: string } {
  const m = msg(lang).reportView;
  const startOfToday = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  if (st.period === 'all') return { since: null, until: null, error: '' };
  if (st.period === 'today') return { since: startOfToday.getTime(), until: null, error: '' };
  if (st.period === 'week' || st.period === 'month') {
    const d = new Date(startOfToday);
    d.setDate(d.getDate() - (st.period === 'week' ? 6 : 29));
    return { since: d.getTime(), until: null, error: '' };
  }
  const from = st.from ? parseDay(st.from) : null;
  const to = st.to ? parseDay(st.to) : null;
  if (st.from && !from) return { since: null, until: null, error: m.invalidStartDate };
  if (st.to && !to) return { since: null, until: null, error: m.invalidEndDate };
  // 終了の日は、その日の終わりまで含める
  const until = to ? new Date(to.getFullYear(), to.getMonth(), to.getDate() + 1).getTime() - 1 : null;
  if (from && until !== null && from.getTime() > until) {
    return { since: null, until: null, error: m.startAfterEnd };
  }
  return { since: from ? from.getTime() : null, until, error: '' };
}

/** バックエンドに渡す指定。`repo` は、選んでいるリポジトリ（すべてのリポジトリなら null）。 */
export function reportOptions(st: ReportState, repo: string, now: Date = new Date(), lang: Lang = getAppLang()): ReportOptions {
  const picked = st.reviewIds !== null;
  const r = reportRange(st, now, lang);
  return {
    // 選んだレビューのときは、リポジトリと期間を使わない
    repo: picked || st.allRepos || !repo ? null : repo,
    since_ms: picked ? null : r.since,
    until_ms: picked ? null : r.until,
    review_ids: st.reviewIds,
    include_evidence: st.evidence,
    include_prompt: st.prompt,
    include_tokens: st.tokens,
    include_log: st.log,
    tz_offset_min: -now.getTimezoneOffset(),
    lang,
  };
}

/** 保存のときの、初めのファイル名。「レビュー記録_リポジトリ名_2026-10-27.md」。ファイル名に使えない文字は、`_` にする。 */
export function reportFileName(repo: string, allRepos: boolean, ext: string, now: Date = new Date(), selectedCount = 0, lang: Lang = getAppLang()): string {
  const m = msg(lang).reportView;
  const dateStr = ymd(now);
  if (selectedCount > 0) {
    return m.fileNameSelected(selectedCount, dateStr, ext);
  }
  const name = allRepos || !repo ? m.fileNameAll : (repo.split('/').filter(Boolean).pop() ?? 'repo');
  const safe = name.replace(/[\\/:*?"<>|\s]+/g, '_').replace(/^_+|_+$/g, '').slice(0, 60) || 'repo';
  return m.fileNameRepo(safe, dateStr, ext);
}

const REMOVE = ['script', 'style', 'iframe', 'object', 'embed', 'link', 'meta', 'base', 'form', 'svg', 'math', 'template', 'noscript', 'audio', 'video'];

/**
 * バックエンドが作った HTML を、画面に出せる要素にする。バックエンドは、記録の文字を全部エスケープしているが、
 * 念のため、スクリプトや外部の読み込みになりうる要素を落とし、属性も `class` 以外はすべて除去する（二重の防御）。
 */
export function htmlNodes(html: string): Node[] {
  const doc = new DOMParser().parseFromString(html, 'text/html');
  doc.querySelectorAll(REMOVE.join(',')).forEach((el) => el.remove());
  doc.querySelectorAll('*').forEach((el) => {
    for (const a of [...el.attributes]) {
      if (a.name.toLowerCase() !== 'class') el.removeAttribute(a.name);
    }
  });
  return [...doc.body.childNodes].map((n) => document.importNode(n, true));
}

export interface ReportActions {
  /** 条件を変えた（プレビューを取り直す） */
  changed(): void;
  save(format: 'md' | 'xlsx'): void;
  print(): void;
  /** 選んだレビューをやめて、期間とリポジトリで絞る形に戻す */
  clearSelection(): void;
  close(): void;
}

const nf = new Intl.NumberFormat('ja-JP');

export function reportPanel(st: ReportState, act: ReportActions, hasRepo: boolean, lang: Lang = getAppLang()): HTMLElement {
  const m = msg(lang).reportView;
  const range = st.reviewIds !== null ? { since: null, until: null, error: '' } : reportRange(st, new Date(), lang);
  const picked = st.reviewIds !== null;
  const check = (label: string, on: boolean, set: (v: boolean) => void, hint?: string): HTMLElement =>
    h(
      'label',
      { class: 'rcheck' },
      h('input', { type: 'checkbox', ...(on ? { checked: true } : {}), onchange: (e) => ((set((e.target as HTMLInputElement).checked), act.changed())) }),
      h('span', {}, label),
      hint ? h('span', { class: 'muted' }, ` ${hint}`) : null,
    );
  const s = st.preview?.summary;
  const ready = !!st.preview && !st.loading && !range.error && st.busy === '' && (s?.reviews ?? 0) > 0;
  const dateInput = (value: string, set: (v: string) => void): HTMLElement =>
    h('input', { type: 'date', class: 'rdate', value, onchange: (e) => ((set((e.target as HTMLInputElement).value), act.changed())) });
  const periods = getRPeriods(lang);

  return h(
    'div',
    { class: 'reportview' },
    h(
      'div',
      { class: 'rcontrols' },
      h(
        'div',
        { class: 'd-head' },
        h('h2', {}, m.heading),
        h('p', { class: 'muted' }, m.subtitle),
      ),
      picked
        ? h(
            'div',
            { class: 'row rrow pickedinfo' },
            h('span', {}, m.pickedTargeting(st.reviewIds?.length ?? 0)),
            h('button', { class: 'small', onclick: () => act.clearSelection() }, m.switchToDateRange),
            h('span', { class: 'spacer' }),
            h('button', { onclick: () => act.close() }, m.closeBtn),
          )
        : null,
      picked
        ? null
        : h(
        'div',
        { class: 'row rrow' },
        h('div', { class: 'dmode' }, ...periods.map(([k, label]) => h('button', { class: st.period === k ? 'on' : '', onclick: () => ((st.period = k), act.changed()) }, label))),
        st.period === 'custom' ? h('span', { class: 'rdates' }, dateInput(st.from, (v) => (st.from = v)), m.dateRangeSep, dateInput(st.to, (v) => (st.to = v))) : null,
        h('span', { class: 'spacer' }),
        h('button', { onclick: () => act.close() }, m.closeBtn),
      ),
      picked
        ? null
        : h(
        'div',
        { class: 'row rrow' },
        h('div', { class: 'dmode' },
          h('button', { class: !st.allRepos ? 'on' : '', disabled: !hasRepo, onclick: () => ((st.allRepos = false), act.changed()) }, m.selectedRepoBtn),
          h('button', { class: st.allRepos || !hasRepo ? 'on' : '', onclick: () => ((st.allRepos = true), act.changed()) }, m.allReposBtn),
        ),
      ),
      h(
        'div',
        { class: 'rchecks' },
        check(m.includeEvidence, st.evidence, (v) => (st.evidence = v)),
        check(m.includeTokens, st.tokens, (v) => (st.tokens = v)),
        check(m.includeLog, st.log, (v) => (st.log = v), m.includeLogHint),
        check(m.includePrompt, st.prompt, (v) => (st.prompt = v), m.includePromptHint),
      ),
      h('p', { class: 'note-line' }, m.secretNotice),
      range.error ? h('p', { class: 'err' }, range.error) : null,
      st.error ? h('p', { class: 'err' }, st.error) : null,
      st.message ? h('p', { class: 'ok-line' }, st.message) : null,
      h('p', { class: 'rcount muted' }, st.loading ? m.loading : s ? m.summaryLine(nf.format(s.reviews), nf.format(s.findings), s.fix, s.wait, s.dismiss, s.new) : ''),
      h(
        'div',
        { class: 'row rout' },
        h('button', { class: 'run', disabled: !ready, onclick: () => act.save('md') }, st.busy === 'md' ? m.savingBtn : m.saveMdBtn),
        h('button', { class: 'run', disabled: !ready, onclick: () => act.save('xlsx') }, st.busy === 'xlsx' ? m.savingBtn : m.saveXlsxBtn),
        h('button', { class: 'run', disabled: !ready, title: m.printTitle, onclick: () => act.print() }, st.busy === 'pdf' ? m.openingPrintBtn : m.exportPdfBtn),
      ),
    ),
    h('div', { class: 'rpreview' }, ...(st.preview ? htmlNodes(st.preview.html) : [h('p', { class: 'muted' }, st.loading ? '' : m.generatingPreview)])),
  );
}
