// 「比較元」（差分の元になるコミット）の表示。差分は、比較元からいまの作業ツリーまで。

import type { ComboItem } from './combo';
import { getAppLang, msg, type Lang } from './i18n';
import type { Base, BaseOption } from './types';

export const CUSTOM = '__custom__';
export const HEAD = 'HEAD';
/** 「最初から（リポジトリ全体）」。何も無い状態から、いままで。バックエンドの `ROOT_SPEC` と同じ値。 */
export const ROOT = ':root';

/** 前回のレビューの時刻の表示（「11/13 09:42」）。言語に依らない。 */
export function whenText(ms: number): string {
  const d = new Date(ms);
  const p = (n: number): string => String(n).padStart(2, '0');
  return `${d.getMonth() + 1}/${d.getDate()} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

/** 「未コミットの変更」「b869afd 画面を組み替え… から」。古い記録（比較元が無い）は空。 */
export function baseLabel(b: Base | null | undefined, lang: Lang = getAppLang()): string {
  if (!b || !b.hash) return '';
  const m = msg(lang).baseInfo;
  if (b.spec === HEAD || b.spec === '') return m.uncommitted;
  if (b.spec === ROOT) return m.entireRepo;
  const subject = b.subject.length > 40 ? `${b.subject.slice(0, 40)}…` : b.subject;
  return m.fromCommit(b.hash.slice(0, 7), subject);
}

/**
 * 「送る内容」に出す、見る範囲の 1 行。
 * 内部では、選んだコミットの親と比べるが（`ハッシュ^`）、見る範囲には、選んだコミットが含まれる。表示は、選んだコミットの名前で言う（親の名前は出さない）。
 */
export function rangeText(b: Base | null | undefined, bases: BaseOption[], lang: Lang = getAppLang()): string {
  const m = msg(lang).baseInfo;
  const scope = m.scopeSuffix;
  if (!b || !b.hash || b.spec === HEAD || b.spec === '') return m.onlyUncommitted;
  if (b.spec === ROOT) return m.entireRepo;
  if (b.spec === 'HEAD~1') return m.lastCommitAndUncommitted;
  const opt = bases.find((o) => o.value === b.spec);
  if (opt?.last) return m.allChangesSinceLastReview(whenText(opt.last.created_at), scope);
  if (b.spec.endsWith('^')) {
    const name = opt?.label ?? b.spec.slice(0, 7);
    return m.allChangesFromForward(name, scope);
  }
  const forkEn = opt?.label.match(/^Fork point with (.*)$/);
  if (forkEn) return m.allChangesAfterBranch(forkEn[1], scope);
  const forkJa = opt?.label.match(/^(.*) との分岐点から$/);
  if (forkJa) return m.allChangesAfterBranch(forkJa[1], scope);
  return m.allChangesFromBase(baseLabel(b, lang), scope);
}

/**
 * コンボに出す項目。いま選んでいる値が一覧に無いとき（自分で入力した場所）は、先頭に足す。
 * 最後に、自分で入力する項目を付ける。
 */
export function baseItems(bases: BaseOption[], current: string, lang: Lang = getAppLang(), recipeTitle: (id: string) => string = (id) => id): ComboItem[] {
  const m = msg(lang).baseInfo;
  const items: ComboItem[] = bases.map((b) => {
    let title = b.label;
    let desc = m.translateBaseHint(b.hint);
    const norm = m.normalizeBaseTitle(b.value, b.label);
    if (norm) {
      title = norm.title;
      if (norm.desc !== undefined) desc = norm.desc;
    }
    // 「前回のレビューの続き」: いつ・どの観点を走らせたかを出す（言語は、画面の言語で組み立てる）
    if (b.last) {
      title = m.lastReviewTitle(whenText(b.last.created_at));
      desc = m.lastReviewDesc(b.last.head_short, b.last.covered.map(recipeTitle).join(' / ') || '—');
    }
    return {
      value: b.value,
      title,
      desc,
      // `ハッシュ^`（最近のコミット）だけ、別のグループにする
      group: b.value.endsWith('^') ? m.groupRecent : m.groupRange,
    };
  });
  if (current && current !== HEAD && !items.some((i) => i.value === current)) {
    items.unshift({
      value: current,
      title: m.fromCustomTitle(current),
      desc: m.customDesc,
      group: m.groupRange,
    });
  }
  items.push({
    value: CUSTOM,
    title: m.customItemTitle,
    desc: m.customItemDesc,
    group: m.groupOther,
  });
  return items;
}

/** 直近のコミットを比較元にできるか（コミットが 2 つ以上ある）。 */
export const hasLastCommit = (bases: BaseOption[]): boolean => bases.some((b) => b.value === 'HEAD~1');
