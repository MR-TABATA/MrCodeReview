import { isDoneFinding } from './findingorder';
import type { AgentReport, Finding, Note, TriageCounts, TriageState } from './types';

/** 結果の集計の帯に出す数。重大度ごとの件数と、判断の内訳（履歴の行と同じ数え方）。 */
export interface FindingStats {
  total: number;
  severity: { high: number; medium: number; low: number; other: number };
  counts: Required<TriageCounts>;
}

interface ReviewLike {
  findings: readonly Pick<Finding, 'severity'>[];
  triage: readonly TriageState[];
  finding_notes?: readonly (Note | null | undefined)[];
  agent_reports?: readonly (AgentReport | null | undefined)[];
}

/**
 * 1 つのレビューの指摘を、数える。判断の内訳は、履歴の行（`TriageCounts`）と同じ数え方:
 * 待ち・無視はそのまま／「対応」で、対応済みの印か、エージェントの「対応した」→ 済み／「対応」で、エージェントの「確認して対応しなかった」（済みでない）→ 判断待ち／それ以外（未仕分け・対応中）→ 未。
 * 5 つを足すと、指摘の数になる。
 */
export function findingStats(r: ReviewLike): FindingStats {
  const severity = { high: 0, medium: 0, low: 0, other: 0 };
  const counts = { open: 0, pending: 0, done: 0, wait: 0, dismiss: 0 };
  r.findings.forEach((f, i) => {
    if (f.severity === 'high' || f.severity === 'medium' || f.severity === 'low') severity[f.severity] += 1;
    else severity.other += 1;
    const status = r.triage[i] ?? 'new';
    if (status === 'wait') counts.wait += 1;
    else if (status === 'dismiss') counts.dismiss += 1;
    else if (status === 'fix' && isDoneFinding(status, r.finding_notes?.[i] ?? undefined, r.agent_reports?.[i])) counts.done += 1;
    else if (status === 'fix' && r.agent_reports?.[i]?.verdict === 'skipped') counts.pending += 1;
    else counts.open += 1;
  });
  return { total: r.findings.length, severity, counts };
}
