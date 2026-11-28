import type { ReviewSummary } from './types';

/** 観点 1 つぶんの集計。 */
export interface AspectRow {
  /** 観点名（記録の `recipe_title`。連番は付かない） */
  title: string;
  /** レビューの回数（失敗した実行も、走ったので数える） */
  runs: number;
  /** うち、失敗した実行 */
  failed: number;
  /** 指摘の合計 */
  findings: number;
  /** 仕分けの内訳（読めた記録だけ） */
  open: number;
  /** 判断待ち（エージェントが「確認して対応しなかった」と申告し、あなたの判断が残っているもの） */
  pending: number;
  done: number;
  wait: number;
  dismiss: number;
  /** 無視 ÷（済み＋待ち＋無視）の百分率（四捨五入）。判断が 0 件なら null */
  ignorePct: number | null;
}

export interface AspectStats {
  rows: AspectRow[];
  totals: { aspects: number; runs: number; findings: number; ignorePct: number | null };
  /** 仕分けの内訳を読めなかった記録（`counts` が null）の数 */
  unreadable: number;
}

/** 無視の割合。未は、まだ判断していないので、分母に入れない。判断が 0 件なら null。 */
export function ignorePct(done: number, wait: number, dismiss: number): number | null {
  const decided = done + wait + dismiss;
  return decided === 0 ? null : Math.round((dismiss / decided) * 100);
}

/**
 * 履歴（レビューの一覧）を、観点ごとに数える。フックの実行は、レビューの記録ではないので、入らない。
 * 内訳を読めなかった記録（counts が null）は、指摘の件数には入れるが、内訳と無視の割合には入れない。
 * 並びは、実行の多い順、同数なら観点名の順。
 */
export function aggregateAspects(history: readonly ReviewSummary[]): AspectStats {
  const by = new Map<string, AspectRow>();
  let unreadable = 0;
  for (const s of history) {
    let r = by.get(s.recipe_title);
    if (!r) {
      r = { title: s.recipe_title, runs: 0, failed: 0, findings: 0, open: 0, pending: 0, done: 0, wait: 0, dismiss: 0, ignorePct: null };
      by.set(s.recipe_title, r);
    }
    r.runs += 1;
    if (s.failed) r.failed += 1;
    r.findings += s.findings;
    if (s.counts) {
      r.open += s.counts.open;
      r.pending += s.counts.pending ?? 0;
      r.done += s.counts.done;
      r.wait += s.counts.wait;
      r.dismiss += s.counts.dismiss;
    } else if (s.findings > 0) {
      // 指摘が無い記録は、内訳が空で当然。読めなかったのは、指摘があるのに、内訳が無いものだけ
      unreadable += 1;
    }
  }
  const rows = [...by.values()];
  let done = 0;
  let wait = 0;
  let dismiss = 0;
  let runs = 0;
  let findings = 0;
  for (const r of rows) {
    r.ignorePct = ignorePct(r.done, r.wait, r.dismiss);
    done += r.done;
    wait += r.wait;
    dismiss += r.dismiss;
    runs += r.runs;
    findings += r.findings;
  }
  rows.sort((a, b) => b.runs - a.runs || (a.title < b.title ? -1 : a.title > b.title ? 1 : 0));
  return { rows, totals: { aspects: rows.length, runs, findings, ignorePct: ignorePct(done, wait, dismiss) }, unreadable };
}
