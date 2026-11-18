// 結果画面の「前回 → 今回」の内訳と、絞り込み。画面の状態には触れない（テストできるように、main.ts から切り出した）。

import type { Fate, ReviewSummary, ReviewView } from './types';

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

/** 流れの行の 1 つ。 */
export interface FlowItem {
  id: string;
  /** 連番（番号の無い記録は、流れに入れない） */
  seq: number;
  failed: boolean;
  findings: number;
  created_at: number;
  /** いま開いているレビュー */
  now: boolean;
}

export interface Flow {
  items: FlowItem[];
  /** 表示した分より前（古いほう）が、まだある */
  hasBefore: boolean;
  /** 表示した分より後（新しいほう）が、まだある */
  hasAfter: boolean;
}

/**
 * 同じリポジトリ・同じ観点の流れ（「1 → 2 → [3]」）。履歴にある分だけで、番号の無い記録は入れない（「0」などと出さない）。
 * 長いときは、いまのレビューの近くの `max` 件だけ。2 件に満たなければ null（出さない）。
 */
export function flowOf(history: readonly ReviewSummary[], r: { id: string; repo: string; recipe: { title: string } }, max = 8): Flow | null {
  const all = history
    .filter((s): s is ReviewSummary & { seq: number } => s.repo === r.repo && s.recipe_title === r.recipe.title && typeof s.seq === 'number' && s.seq > 0)
    .sort((a, b) => a.seq - b.seq);
  if (all.length < 2) return null;
  const at = Math.max(0, all.findIndex((s) => s.id === r.id));
  const from = Math.max(0, Math.min(at - Math.floor(max / 2), all.length - max));
  const items = all.slice(from, from + max).map((s) => ({ id: s.id, seq: s.seq, failed: s.failed, findings: s.findings, created_at: s.created_at, now: s.id === r.id }));
  return { items, hasBefore: from > 0, hasAfter: from + max < all.length };
}
