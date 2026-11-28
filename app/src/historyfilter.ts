import { formatWhen, reviewName } from './historyview';
import type { Lang } from './i18n';
import type { HookRun, ReviewSummary } from './types';

/**
 * 「履歴・ログ」の絞り込み。入力欄は 1 つで、スペース区切りの複数の語を、すべて満たす行だけを残す（合わせ技）。
 * 語は、行ごとの「検索用の文字列」に、含まれていれば、当てはまる。大文字・小文字、全角・半角は区別しない。
 */

/** 照合のために、そろえる（NFKC・小文字）。 */
export const normalize = (s: string): string => s.normalize('NFKC').toLowerCase();

/** 入力を語に分ける。半角・全角のスペースで区切る。空なら、絞り込まない。 */
export const parseTerms = (q: string): string[] => normalize(q).split(/[\s　]+/).filter(Boolean);

/** すべての語が、検索用の文字列に含まれているか（語が無ければ、当てはまる）。 */
export function matchesAll(haystack: string, terms: string[]): boolean {
  if (!terms.length) return true;
  const hay = normalize(haystack);
  return terms.every((t) => hay.includes(t));
}

const pad = (n: number): string => String(n).padStart(2, '0');
const ymd = (d: Date): string => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;

/** 今日・昨日の日付（`YYYY-MM-DD`）。1 回の絞り込みで、1 度だけ求めて、全行で使い回す。 */
function dayKeys(now: Date): { today: string; yesterday: string } {
  const y = new Date(now);
  y.setDate(y.getDate() - 1);
  return { today: ymd(now), yesterday: ymd(y) };
}

/** 今日・昨日の語（日本語と英語の、どちらでも当てはまる）。 */
function dayWords(day: string, keys: { today: string; yesterday: string }): string {
  return day === keys.today ? '今日 today' : day === keys.yesterday ? '昨日 yesterday' : '';
}

const baseName = (path: string): string => path.replace(/\/+$/, '').split('/').pop() ?? path;

/** 日の語を除いた、レビューの行の検索用の文字列（照合できるよう、そろえる前）。 */
function reviewText(s: ReviewSummary, lang: Lang): string {
  const words: string[] = ['レビュー review'];
  if (s.failed) words.push('失敗 failed');
  else if (s.findings === 0) words.push('完了 done', '指摘なし clean');
  else if (s.counts) words.push('完了 done', s.counts.open > 0 ? '未対応 open' : '済み handled');
  return [
    formatWhen(s.created_at, lang),
    ymd(new Date(s.created_at)),
    reviewName(s.recipe_title, s.seq, lang),
    s.recipe_title,
    s.repo,
    baseName(s.repo),
    s.head ?? '',
    s.branch ?? '',
    s.provider_label,
    ...words,
  ].join(' ');
}

const HOOK_STATE_WORDS: Record<string, string> = { done: '完了 done', failed: '失敗 failed', aborted: '中断 aborted', running: '実行中 running' };

/** 日の語を除いた、フックの実行の行の検索用の文字列。 */
function hookText(r: HookRun, lang: Lang): string {
  return [
    'フック hook',
    formatWhen(r.started_at_ms, lang),
    ymd(new Date(r.started_at_ms)),
    r.commit ?? '',
    r.subject ?? '',
    ...r.recipes,
    ...r.steps.map((st) => st.recipe),
    // 終了メッセージ（message）は、検索の対象に入れない: 状態の語（完了・失敗・中断・実行中）と、別の語が混ざり、
    // 「失敗」で絞ったとき、状態が完了の実行まで、メッセージの言葉のせいで残ってしまうため
    HOOK_STATE_WORDS[r.state] ?? r.state,
  ].join(' ');
}

/** レビューの行の、検索用の文字列（日の語を含む）。 */
export function reviewHaystack(s: ReviewSummary, lang: Lang, now: Date = new Date()): string {
  return `${reviewText(s, lang)} ${dayWords(ymd(new Date(s.created_at)), dayKeys(now))}`;
}

/** フックの実行の行の、検索用の文字列（日の語を含む）。 */
export function hookHaystack(r: HookRun, lang: Lang, now: Date = new Date()): string {
  return `${hookText(r, lang)} ${dayWords(ymd(new Date(r.started_at_ms)), dayKeys(now))}`;
}

/** 行ごとに、1 度だけ作って、使い回す（絞り込みの 1 文字ごとに、全行を作り直さない）。履歴は、読み直すたびに、新しい行の配列に入れ替わるので、古い値が残らない。 */
interface Entry {
  lang: Lang;
  /** そろえた（NFKC・小文字）、日の語を除く検索用の文字列 */
  norm: string;
  /** 日付（`YYYY-MM-DD`） */
  day: string;
}
const reviewCache = new WeakMap<ReviewSummary, Entry>();
const hookCache = new WeakMap<HookRun, Entry>();
/** 試験用: 検索用の文字列を、作った回数。 */
export const stats = { builds: 0 };

function entryOf<T extends object>(cache: WeakMap<T, Entry>, row: T, lang: Lang, text: () => string, day: () => string): Entry {
  const hit = cache.get(row);
  if (hit && hit.lang === lang) return hit;
  stats.builds++;
  const e: Entry = { lang, norm: normalize(text()), day: day() };
  cache.set(row, e);
  return e;
}

const matchEntry = (e: Entry, terms: string[], keys: { today: string; yesterday: string }): boolean => {
  const words = dayWords(e.day, keys);
  const hay = words ? `${e.norm} ${words}` : e.norm;
  return terms.every((t) => hay.includes(t));
};

export function filterReviews(items: ReviewSummary[], terms: string[], lang: Lang, now: Date = new Date()): ReviewSummary[] {
  if (!terms.length) return items;
  const keys = dayKeys(now);
  return items.filter((s) => matchEntry(entryOf(reviewCache, s, lang, () => reviewText(s, lang), () => ymd(new Date(s.created_at))), terms, keys));
}

export function filterHooks(runs: HookRun[], terms: string[], lang: Lang, now: Date = new Date()): HookRun[] {
  if (!terms.length) return runs;
  const keys = dayKeys(now);
  return runs.filter((r) => matchEntry(entryOf(hookCache, r, lang, () => hookText(r, lang), () => ymd(new Date(r.started_at_ms))), terms, keys));
}

/** 絞り込みの結果（1 回の描き直しで、何度も求めない）。 */
export interface HistoryView {
  terms: string[];
  reviews: ReviewSummary[];
  hooks: HookRun[];
}

/**
 * 絞り込みの結果を、1 つの入力（履歴・フックの実行・入力・言語・今日）につき 1 回だけ求めて、同じものを返す入れ物。
 * 件数・一覧・「すべて選ぶ」・選択の整理が、それぞれ絞り込み直さないように。
 */
export function createHistoryView(): (history: ReviewSummary[], runs: HookRun[], query: string, lang: Lang, now?: Date) => HistoryView {
  let last: { history: ReviewSummary[]; runs: HookRun[]; query: string; lang: Lang; today: string; view: HistoryView } | null = null;
  return (history, runs, query, lang, now = new Date()) => {
    const today = ymd(now);
    if (last && last.history === history && last.runs === runs && last.query === query && last.lang === lang && last.today === today) return last.view;
    const terms = parseTerms(query);
    const view: HistoryView = { terms, reviews: filterReviews(history, terms, lang, now), hooks: filterHooks(runs, terms, lang, now) };
    last = { history, runs, query, lang, today, view };
    return view;
  };
}
