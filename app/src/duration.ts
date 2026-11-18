// 実行時間の表示。経過時間は実測そのもの。目安は、過去の実績（同じ LLM・同じ観点の、新しい順に 10 件まで）から。
// 実績が少ないうちは、幅だけを見せ、回数を添える。実績が無ければ「実績なし」と言い、推定で埋めない。

import { getAppLang, type Lang } from './i18n';
import { ut } from './uitext';

export interface DurationStats {
  /** 実績の件数（0 なら、ほかは 0） */
  n: number;
  min_ms: number;
  median_ms: number;
  max_ms: number;
}

/** 「25 秒」「1 分 5 秒」「2 分」。秒に丸める。 */
export function fmtSeconds(ms: number, lang: Lang = getAppLang()): string {
  const t = ut(lang).duration;
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return t.sec(s);
  const m = Math.floor(s / 60);
  const r = s % 60;
  return r === 0 ? t.min(m) : t.minSec(m, r);
}

/** 実行中の経過時間（切り捨て。数えている途中の値なので、丸めて先走らない）。 */
export function elapsedText(ms: number, lang: Lang = getAppLang()): string {
  const t = ut(lang).duration;
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return t.sec(s);
  return t.minSec(Math.floor(s / 60), s % 60);
}

/** 実行ボタンの下に出す、目安の 1 行。`null` は、まだ読み込んでいない。 */
export function estimateText(s: DurationStats | null, isMock: boolean, lang: Lang = getAppLang()): string {
  const t = ut(lang).duration;
  const f = (ms: number) => fmtSeconds(ms, lang);
  if (isMock) return t.mockNoTime;
  if (!s) return '';
  if (s.n === 0) return t.noHistory;
  if (s.n < 3) {
    const range = s.n === 1 || s.min_ms === s.max_ms ? f(s.median_ms) : t.range(f(s.min_ms), f(s.max_ms));
    return t.few(s.n, range);
  }
  if (s.min_ms === s.max_ms) return t.same(s.n, f(s.median_ms));
  return t.median(s.n, f(s.median_ms), f(s.min_ms), f(s.max_ms));
}
