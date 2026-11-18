// 結果画面の「前回 → 今回」の内訳と、絞り込み。画面の状態には触れない（テストできるように、main.ts から切り出した）。

import type { Fate, ReviewView } from './types';

export type ContinuityKind = 'persist' | 'gone' | 'new' | 'other';

export interface Continuity {
  /** 継続: 前回の指摘と同じと判定した今回の指摘（添字） */
  persist: number[];
  /** 新規: 前回の指摘と結びつかなかった今回の指摘（添字） */
  isNew: number[];
  /** 解消: 前回の指摘のうち、引用の行がコードから消えたもの */
  gone: Fate[];
  /** 再指摘なし・不明: 前回の指摘のうち、今回の指摘と結びつかず、解消とも言えないもの */
  other: Fate[];
  persistSet: ReadonlySet<number>;
  newSet: ReadonlySet<number>;
}

// 記録の `carries` / `previous` / `fates` は、あとから変わらない。同じ記録に対しては、1 回だけ計算する（指摘 1 枚ごとに、全件を数え直さない）
const memo = new WeakMap<ReviewView, Continuity | null>();

/** 前回が無い（初回・古い記録）ときは null。 */
export function continuityOf(r: ReviewView): Continuity | null {
  if (memo.has(r)) return memo.get(r) ?? null;
  const out = compute(r);
  memo.set(r, out);
  return out;
}

function compute(r: ReviewView): Continuity | null {
  const prev = r.previous;
  if (!prev) return null;
  const persist: number[] = [];
  const isNew: number[] = [];
  r.findings.forEach((_, i) => (r.carries?.[i]?.from_review === prev.id ? persist : isNew).push(i));
  const rest = (r.fates ?? []).filter((x) => x.from_review === prev.id && x.fate !== 'persists');
  return {
    persist,
    isNew,
    gone: rest.filter((x) => x.fate === 'gone'),
    other: rest.filter((x) => x.fate !== 'gone'),
    persistSet: new Set(persist),
    newSet: new Set(isNew),
  };
}

/** 絞り込みに合う今回の指摘か。絞り込みが無い・前回が無いときは、全部。「解消」「再指摘なし」を選んだときは、今回の指摘は 1 件も出ない。 */
export function shownByContinuity(c: Continuity | null, kind: ContinuityKind | null, i: number): boolean {
  if (!kind || !c) return true;
  return kind === 'persist' ? c.persistSet.has(i) : kind === 'new' ? c.newSet.has(i) : false;
}

/** 前回の指摘の行方の一覧。「継続」「新規」を選んだときは出さない。「解消」「再指摘なし」を選んだときは、その分だけ。 */
export function filteredFates(r: ReviewView, c: Continuity | null, kind: ContinuityKind | null): Fate[] {
  if (!kind || !c) return r.fates ?? [];
  return kind === 'gone' ? c.gone : kind === 'other' ? c.other : [];
}
