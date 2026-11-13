// 実行の計画。「いま押したら、何が走るか」を決める純関数。
//
// 原則: **実行できない観点だけを今回の実行から外し、できるものは走らせる。** 外したものと理由は、画面が見せる。
// チェックは勝手に外さない（見えない変更をしない）。全体が実行できないとき（リポジトリ未選択・差分なし）と、
// 外したら 1 つも残らないときだけ、従来どおり、全体を止める。

import type { Recipe } from './types';

/** 画面の状態から取り出す、判断に要る値。 */
export interface RunInputs {
  hasRepo: boolean;
  /** 送る差分のファイル数。差分をまだ読めていないときは null（このときは、何も止めない） */
  filesCount: number | null;
  /** 比較元が「未コミットの変更」か（差分なしの理由の言い分けに使う） */
  baseIsHead: boolean;
  hasProvider: boolean;
  /** 設計書を指定したか */
  hasSpec: boolean;
  /** 指定した設計書が有効か（文字数の上限内など） */
  specValid: boolean;
}

export type SkipWhy = 'spec' | 'llm';

export interface SkippedRecipe {
  recipe: Recipe;
  why: SkipWhy;
  /** 外した理由（画面に出す） */
  reason: string;
}

export interface RunPlan {
  /** 全体を止める理由（無ければ空） */
  block: string;
  /** 今回実行する観点（観点の並びの順） */
  run: Recipe[];
  /** 今回は実行しない観点と、その理由 */
  skipped: SkippedRecipe[];
}

export const REASON_NO_REPO = 'リポジトリを選んでください（左サイドバー）';
export const REASON_NO_LLM = 'LLM を選んでください（右上の設定から）';
export const REASON_NO_SPEC = '設計書を指定してください（ファイル選択または貼り付け）';
export const REASON_BAD_SPEC = '設計書の内容を確認してください（文字数上限を超えているか無効です）';

/** 設計書（基準文書）が要る観点か。 */
export const needsSpecOf = (r: Recipe): boolean => r.id === 'spec' || !!r.needs_spec;

export function planRun(chosen: Recipe[], i: RunInputs): RunPlan {
  if (!i.hasRepo) return { block: REASON_NO_REPO, run: chosen, skipped: [] };
  // 差分をまだ読めていないあいだは、止めない（読めたら判断する）
  if (i.filesCount === null) return { block: '', run: chosen, skipped: [] };
  if (i.filesCount === 0) {
    const block = i.baseIsHead ? '送る差分がありません（未コミットの変更が無い）' : '送る差分がありません（この範囲に、変更が無い）';
    return { block, run: chosen, skipped: [] };
  }

  const run: Recipe[] = [];
  const skipped: SkippedRecipe[] = [];
  const specReason = !i.hasSpec ? REASON_NO_SPEC : REASON_BAD_SPEC;
  for (const recipe of chosen) {
    const specMissing = needsSpecOf(recipe) && (!i.hasSpec || !i.specValid);
    const llmMissing = recipe.kind !== 'tool' && !i.hasProvider;
    if (llmMissing) {
      // 設計書も足りないなら、両方を出す（設計書だけを直しても、LLM が無ければ、まだ走らない）
      skipped.push({ recipe, why: 'llm', reason: specMissing ? `${REASON_NO_LLM} ／ ${specReason}` : REASON_NO_LLM });
    } else if (specMissing) {
      skipped.push({ recipe, why: 'spec', reason: specReason });
    } else {
      run.push(recipe);
    }
  }

  // 1 つも残らないときだけ、全体を止める。理由は、従来の順（LLM、設計書）で最初のもの
  let block = '';
  if (chosen.length > 0 && run.length === 0) {
    block = (skipped.find((s) => s.why === 'llm') ?? skipped[0]).reason;
  }
  return { block, run, skipped };
}

/** 外した理由ごとに、観点をまとめる（同じ理由の観点は、1 つの注記にする）。`ids` は、カードに印を付けるため。 */
export function groupSkipped(skipped: SkippedRecipe[], titleOf: (r: Recipe) => string): { reason: string; why: SkipWhy; titles: string[]; ids: string[] }[] {
  const out: { reason: string; why: SkipWhy; titles: string[]; ids: string[] }[] = [];
  for (const s of skipped) {
    const hit = out.find((g) => g.reason === s.reason);
    if (hit) {
      hit.titles.push(titleOf(s.recipe));
      hit.ids.push(s.recipe.id);
    } else {
      out.push({ reason: s.reason, why: s.why, titles: [titleOf(s.recipe)], ids: [s.recipe.id] });
    }
  }
  return out;
}

/** 押したときに何を始めるか。入口（実行ボタン）は、この結果だけに従う（外した観点は、ここに入らない）。 */
export type Launch = { kind: 'none' } | { kind: 'single'; id: string } | { kind: 'batch'; ids: string[] };

export function launchOf(plan: RunPlan): Launch {
  if (plan.block || plan.run.length === 0) return { kind: 'none' };
  const ids = plan.run.map((r) => r.id);
  return ids.length === 1 ? { kind: 'single', id: ids[0] } : { kind: 'batch', ids };
}

/**
 * 「前回のレビューの続き」を選んでいるとき、前回の実行に含まれていない観点（前回より前の変更を見ていない観点）。
 * `covered` が無い（続きを選んでいない）ときは、空。
 */
export function notCovered(run: Recipe[], covered: string[] | undefined): Recipe[] {
  if (!covered) return [];
  return run.filter((r) => !covered.includes(r.id));
}
