import { h } from './dom';
import type { Handling, TriageState } from './types';

interface FixAllReview {
  findings: { severity: string }[];
  triage: (TriageState | undefined)[];
}

interface FixAllActions {
  setTriage: (index: number, status: TriageState) => Promise<Handling>;
  applyHandling: (handling: Handling) => void;
  onError: (message: string) => void;
  refreshHistory: () => void;
  refreshHandoff: () => Promise<void>;
  render: () => void;
}

/** まとめて変える対象か。「対応」へは、すでに「対応」のもの以外すべて。「無視」へは、まだ仕分けていないものだけ（自分で「対応」「待ち」にした指摘を、上書きしない） */
const eligible = (status: TriageState, current: TriageState): boolean => (status === 'dismiss' ? current === 'new' : current !== status);

/** 重大度ごとの一括操作（high・medium は「対応」へ、low は「無視」へ） */
const BULK: { severity: string; status: TriageState }[] = [
  { severity: 'high', status: 'fix' },
  { severity: 'medium', status: 'fix' },
  { severity: 'low', status: 'dismiss' },
];

export async function fixAllBySeverity(r: FixAllReview, severity: string, actions: FixAllActions, status: TriageState = 'fix'): Promise<void> {
  let changed = false;
  try {
    for (let i = 0; i < r.findings.length; i++) {
      if (r.findings[i].severity !== severity || !eligible(status, r.triage[i] ?? 'new')) continue;
      const handling = await actions.setTriage(i, status);
      changed = true;
      actions.applyHandling(handling);
    }
  } catch (e) {
    actions.onError(String(e));
  } finally {
    if (changed) actions.refreshHistory();
  }
  await actions.refreshHandoff();
  actions.render();
}

export function fixAllSeverityButtons(
  findings: { severity: string }[],
  triage: (TriageState | undefined)[],
  withExpandAll: boolean,
  label: (severity: string, count: number, status: TriageState) => string,
  onFix: (severity: string, status: TriageState) => void,
): HTMLElement[] {
  if (!withExpandAll) return [];
  return BULK.flatMap(({ severity, status }) => {
    const count = findings.filter((finding, i) => finding.severity === severity && eligible(status, triage[i] ?? 'new')).length;
    return count > 0
      ? [h('button', { class: 'small linkish', onclick: () => onFix(severity, status) }, label(severity, count, status))]
      : [];
  });
}
