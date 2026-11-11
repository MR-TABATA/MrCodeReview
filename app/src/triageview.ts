// 「対応」にした指摘の、状態の印（未対応／対応済み）。カードの、説明文の下の行に出す。閉じたままでも、見える。
//
// - 保存の形は、仕分け ＝ 対応 ＋ 対応済みの印（またはエージェントの「対応した」の申告）。履歴の「済み」の数え方と、同じ。
// - 「対応」以外（未仕分け・待ち・無視）には、出さない。

import { DEFAULT_LANG, msg, type Lang } from './i18n';
import type { TriageState } from './types';

export interface FixStateChip {
  text: string;
  cls: 'open' | 'did';
  title: string;
}

/** `resolved` は、直し済み（対応済みの印、または、エージェントの「対応した」の申告）。 */
export function fixStateChip(status: TriageState, resolved: boolean, lang: Lang = DEFAULT_LANG): FixStateChip | null {
  if (status !== 'fix') return null;
  const m = msg(lang).triageChip;
  return resolved
    ? { text: m.doneText, cls: 'did', title: m.doneTitle }
    : { text: m.openText, cls: 'open', title: m.openTitle };
}
