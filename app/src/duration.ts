// 実行時間の表示。経過時間は実測そのもの。目安は、過去の実績（同じ LLM・同じ観点の、新しい順に 10 件まで）から。
// 実績が少ないうちは、幅だけを見せ、回数を添える。実績が無ければ「実績なし」と言い、推定で埋めない。

export interface DurationStats {
  /** 実績の件数（0 なら、ほかは 0） */
  n: number;
  min_ms: number;
  median_ms: number;
  max_ms: number;
}

/** 「25 秒」「1 分 5 秒」「2 分」。秒に丸める。 */
export function fmtSeconds(ms: number): string {
  const s = Math.max(0, Math.round(ms / 1000));
  if (s < 60) return `${s} 秒`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  return r === 0 ? `${m} 分` : `${m} 分 ${r} 秒`;
}

/** 実行中の経過時間（切り捨て。数えている途中の値なので、丸めて先走らない）。 */
export function elapsedText(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s} 秒`;
  return `${Math.floor(s / 60)} 分 ${s % 60} 秒`;
}

/** 実行ボタンの下に出す、目安の 1 行。`null` は、まだ読み込んでいない。 */
export function estimateText(s: DurationStats | null, isMock: boolean): string {
  if (isMock) return 'モックは本物の LLM ではないので、時間を測りません。';
  if (!s) return '';
  if (s.n === 0) return '目安: 実績なし（この LLM・この観点では、まだ測っていません。使うと、ここに出ます）';
  if (s.n < 3) {
    const range = s.n === 1 || s.min_ms === s.max_ms ? fmtSeconds(s.median_ms) : `${fmtSeconds(s.min_ms)}〜${fmtSeconds(s.max_ms)}`;
    return `目安: 過去 ${s.n} 回で ${range}（実績が少ないので、参考程度）`;
  }
  if (s.min_ms === s.max_ms) return `目安: 過去 ${s.n} 回とも ${fmtSeconds(s.median_ms)}`;
  return `目安: 過去 ${s.n} 回の中央値 ${fmtSeconds(s.median_ms)}（${fmtSeconds(s.min_ms)}〜${fmtSeconds(s.max_ms)}）`;
}
