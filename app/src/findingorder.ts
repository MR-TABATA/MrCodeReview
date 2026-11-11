// 指摘のカードの並び。重要度の高い順（high → medium → low）。対応済みのものは、下に置く（押した直後に、下へ移る）。
//
// 「対応済み」は、履歴の「済み」と同じ定義（`store.rs` の `triage_counts`）:
//   「対応」の指摘で、対応済みの印がある、または、エージェントが「対応した」と申告したもの。
// 並べ替えは、見た目だけ。指摘の番号（識別子・仕分けの対象）は、変わらない。

import type { AgentReport, Note } from './types';

export function isDoneFinding(status: string, note: Note | undefined, agent: AgentReport | null | undefined): boolean {
  return status === 'fix' && (agent?.verdict === 'did' || (note?.done_at ?? null) !== null);
}

/** 重要度の順位（小さいほど先）。知らない値は、いちばん後ろ。 */
const SEVERITY_RANK: Record<string, number> = { high: 0, medium: 1, low: 2 };
const rankOf = (severity: string): number => SEVERITY_RANK[severity] ?? 3;

/**
 * 表示する順の、指摘の番号（0 始まり）。
 * 対応済みは、あと。同じ組の中は、重要度の高い順（high → medium → low → それ以外）。重要度が同じなら、元の順を保つ。
 * `severity` を渡さなければ、重要度では並べず、元の順。
 */
export function findingOrder(n: number, done: (i: number) => boolean, severity?: (i: number) => string): number[] {
  return [...Array(n).keys()].sort(
    (a, b) => Number(done(a)) - Number(done(b)) || (severity ? rankOf(severity(a)) - rankOf(severity(b)) : 0) || a - b,
  );
}
