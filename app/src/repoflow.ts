/**
 * 登録リポジトリまわりの、画面の判断（Rust への呼び出しは、引数で受け取る。テストできるように）。
 * - 起動時: 登録を読み、選択中のリポジトリを登録に揃える（読めなかったときは、選択に触れない）。
 * - 追加のあと: 上限で断られたときは、選ばない。
 * - 窓に戻ったとき: 比較元の一覧を取り直す（失敗しても、いまの一覧を残す）。
 */

import { resolveRegisteredRepo, type RegisteredRepo } from './reporegistry';
import type { BaseOption } from './types';

export interface RepoList {
  repos: RegisteredRepo[];
  limit: number | null;
  /** 取り込みなどで、止めずに進めた失敗（最初のコミットを調べられなかった、など） */
  warnings?: string[];
}

export interface RepoAddResult extends RepoList {
  path: string | null;
  limit_reached: boolean;
  /** 選んだフォルダとは別の場所に、同じリポジトリが登録済みだった（クローン・ワークツリー）。その登録のパス */
  already_at: string | null;
  /** 登録のフォルダが見つからず、選んだフォルダが「移動した先」かもしれない。その、見つからない登録のパス（何も変えていない） */
  move_from: string | null;
}

/**
 * 選択中のパスを、登録に揃える。
 * - 登録を読めていない（`loaded` が偽）ときは、登録が空に見えるだけなので、何も変えない。
 * - 登録のパスと違う書き方（サブディレクトリなど）なら、登録のパスにする。どの登録にも当たらなければ、空。
 * - 解除（非表示）した登録なら、見えている先頭の登録にする（無ければ空）。
 */
export function settleSelection(repos: RegisteredRepo[], selected: string, loaded: boolean): string {
  if (!loaded || !selected) return selected;
  let sel = selected;
  if (!repos.some((r) => r.path === sel)) sel = resolveRegisteredRepo(repos, sel)?.path ?? '';
  if (sel && repos.some((r) => r.hidden && r.path === sel)) sel = repos.find((r) => !r.hidden)?.path ?? '';
  return sel;
}

/**
 * 「送る内容」の欄に出すエラー。送る内容を読めなかった理由が先。それが無くて、見る範囲の候補だけ読めなかったときは、その理由
 * （黙って候補を空にしない）。どちらも無ければ、空。
 */
export function previewErrorText(previewError: string, basesError: string): string {
  if (previewError) return previewError;
  return basesError ? `見る範囲の候補を読めませんでした: ${basesError}` : '';
}

export interface BootReposResult {
  /** 読めた登録（読めなかったときは null） */
  list: RepoList | null;
  /** 読めなかったときの理由（読めたときは空） */
  error: string;
  /** 読めたが、止めずに進めた失敗（取り込みで、最初のコミットを調べられなかった登録など） */
  warnings: string[];
  /** 起動後に選択しておくパス（読めなかったときは、渡された値のまま） */
  selected: string;
}

/** 起動時: 登録を読み（以前の版の登録を、初回だけ取り込む）、選択中のリポジトリを揃える。 */
export async function bootRepos(d: {
  legacy: RegisteredRepo[];
  selected: string;
  load(legacy: RegisteredRepo[]): Promise<RepoList>;
}): Promise<BootReposResult> {
  const legacy = [...d.legacy];
  if (d.selected && !legacy.some((r) => r.path === d.selected)) legacy.push({ id: '', path: d.selected });
  try {
    const list = await d.load(legacy);
    return { list, error: '', warnings: list.warnings ?? [], selected: settleSelection(list.repos, d.selected, true) };
  } catch (e) {
    return { list: null, error: `登録リポジトリを読めません: ${String(e)}`, warnings: [], selected: d.selected };
  }
}

export interface AddPlan {
  /** 追加（または復元）したあとに選ぶパス。上限で断られたときは null */
  select: string | null;
  limitReached: boolean;
  /** 別の場所に登録済みだったときの、その場所 */
  alreadyAt: string | null;
  /** 移動したかもしれないときの、見つからない登録のパス。移動したと確認できたら、確認つきでもう一度追加する */
  moveFrom: string | null;
}

/** 「移動した」ものとして付け替える確認の文面。何を・どこからどこへ変えるかを先に言う。 */
export function moveConfirmText(from: string, to: string): string {
  return [
    '登録を、新しいフォルダに付け替えます。',
    '',
    `元: ${from}`,
    `新: ${to}`,
    '',
    '履歴（レビュー・仕分け・メモ）は、そのまま引き継ぎます。',
    '',
    '元のフォルダは、いま見つかりません。外付けドライブが接続されていないだけなら、「キャンセル」にしてください（何も変えません）。',
  ].join('\n');
}

/** 追加の結果から、次の動きを決める。上限で断られたときは、選ばない。 */
export function planAfterAdd(out: RepoAddResult): AddPlan {
  if (out.move_from) return { select: null, limitReached: false, alreadyAt: null, moveFrom: out.move_from };
  if (out.limit_reached || !out.path) return { select: null, limitReached: true, alreadyAt: null, moveFrom: null };
  return { select: out.path, limitReached: false, alreadyAt: out.already_at, moveFrom: null };
}

export interface BatchAddItem {
  path: string;
  /** `repo_add` の結果。呼び出しに失敗したときは、無い */
  out?: RepoAddResult;
  /** 呼び出しに失敗した理由 */
  error?: string;
}

export interface BatchAddPlan {
  /** まとめて追加したあとに選ぶパス（追加できたうちの、最初のもの）。ひとつも追加できなかったときは null */
  select: string | null;
  /** 画面に出す結果（成功も、失敗も、黙らず、1 件ずつの理由を添える） */
  message: string;
  /** 追加できなかったものがある（警告として出す） */
  problems: boolean;
}

const folderName = (path: string): string => path.replace(/\/+$/, '').split('/').filter(Boolean).pop() || path;

/**
 * 複数のフォルダを、まとめて追加した結果をまとめる。1 件ずつの判断は、`planAfterAdd` と同じ（一人で追加するときと食い違わない）。
 * - 追加できた・登録済みで選べた: 数える。最初のものを、追加後の選択にする。
 * - 枠がいっぱいで断られた・移動したかもしれない（確認が要る）・呼び出しに失敗した: 何も変えず、名前を挙げて伝える。
 */
export function planBatchAdd(items: BatchAddItem[]): BatchAddPlan {
  const added: string[] = [];
  const elsewhere: string[] = [];
  const refused: string[] = [];
  const maybeMoved: string[] = [];
  const failed: string[] = [];
  for (const it of items) {
    if (!it.out) {
      failed.push(`${folderName(it.path)}（${it.error ?? '理由が分かりません'}）`);
      continue;
    }
    const p = planAfterAdd(it.out);
    if (p.moveFrom) maybeMoved.push(folderName(it.path));
    else if (p.select === null) refused.push(folderName(it.path));
    else {
      added.push(p.select);
      if (p.alreadyAt) elsewhere.push(folderName(it.path));
    }
  }
  const parts = [`${items.length} 件のうち、${added.length} 件を追加しました。`];
  if (elsewhere.length) parts.push(`${elsewhere.length} 件は、同じリポジトリがすでに別の場所に登録されていたので、そちらを使います（${elsewhere.join('、')}）。`);
  if (refused.length) parts.push(`枠がいっぱいで追加できなかったもの: ${refused.join('、')}。設定の「リポジトリ」で「データを削除…」して枠を空けてください。`);
  if (maybeMoved.length) parts.push(`登録のフォルダが移動したかもしれないので、何も変えていません（1 件ずつ追加して確認してください）: ${maybeMoved.join('、')}。`);
  if (failed.length) parts.push(`追加できなかったもの: ${failed.join('、')}。`);
  return { select: added[0] ?? null, message: parts.join('\n'), problems: refused.length + maybeMoved.length + failed.length > 0 };
}

/**
 * 窓に戻ったとき、続けて取り直さない間隔（ミリ秒）。取り直しのたびに、Rust 側で git を数回起動するので、窓の行き来が多くても、頻繁には走らせない。
 * （コミットしてから窓に戻るまでは、普通は数秒以上かかる。取りこぼしても、次に戻ったときに取り直す）
 */
export const BASES_MIN_GAP_MS = 5000;

export interface BasesFocusDeps {
  repo(): string;
  running(): string;
  bases(): BaseOption[];
  now(): number;
  fetch(repo: string): Promise<BaseOption[]>;
  apply(next: BaseOption[]): void;
  /** 取得に失敗した（未登録・解除済みなど）。理由を出す。一覧は、そのまま残す */
  fail(message: string): void;
  /** 取得に成功した（前に出した失敗の表示を、消すため） */
  ok(): void;
  /** 前回取り直した時刻の置き場 */
  gate: { at: number };
}

/**
 * 窓に戻ったとき、比較元の一覧を取り直す。
 * 取り直さない: リポジトリが無い・実行中・前回から間が無い。
 * 一覧を替えない: 取得に失敗した（理由は `fail` で出す）・取り直しの間にリポジトリが変わった・中身が同じ。
 */
export async function refreshBasesOnFocus(d: BasesFocusDeps): Promise<void> {
  const repo = d.repo();
  if (!repo || d.running() || d.now() - d.gate.at < BASES_MIN_GAP_MS) return;
  d.gate.at = d.now();
  let next: BaseOption[];
  try {
    next = await d.fetch(repo);
  } catch (e) {
    if (d.repo() === repo) d.fail(String(e));
    return;
  }
  if (d.repo() !== repo) return;
  d.ok();
  if (JSON.stringify(next) === JSON.stringify(d.bases())) return;
  d.apply(next);
}
