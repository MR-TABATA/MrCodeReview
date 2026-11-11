// レビューの種類を選ぶ欄（コンボボックス）に出す、1 行の説明と、グループ分け。

import type { Recipe } from './types';
import type { ComboItem } from './combo';
import { DEFAULT_LANG, getAppLang, msg } from './i18n';

const MAX = 44;

/** 説明が無いもの（リポジトリ側のレシピ）は、本文の最初の 1 行（見出しは除く）を切って使う。 */
export function recipeSummary(r: Pick<Recipe, 'id' | 'prompt' | 'kind'>, lang?: string): string {
  const m = msg(lang ?? getAppLang());
  const known = m.recipeSummaryMap[r.id] ?? msg(DEFAULT_LANG).recipeSummaryMap[r.id];
  if (known) return known;
  const line = r.prompt
    .split('\n')
    .map((l) => l.trim())
    .find((l) => l && !l.startsWith('#') && !l.startsWith('<!--'));
  if (!line) return '';
  return line.length > MAX ? `${line.slice(0, MAX)}…` : line;
}

export const GROUP_LLM = msg(DEFAULT_LANG).recipeGroupLlm;
export const GROUP_TOOL = msg(DEFAULT_LANG).recipeGroupTool;

/** 一覧の順は、LLM の観点（組み込み → リポジトリ側）、そのあとにリンター。 */
export function comboItems(recipes: Recipe[], lang?: string): ComboItem[] {
  const currentLang = lang ?? getAppLang();
  const m = msg(currentLang);
  const llm = recipes.filter((r) => r.kind !== 'tool');
  const tool = recipes.filter((r) => r.kind === 'tool');
  const item = (r: Recipe, group: string): ComboItem => ({
    value: r.id,
    title: r.title,
    desc: recipeSummary(r, currentLang),
    group,
    badge: r.source === 'repo' ? m.recipeBadgeRepo : undefined,
  });
  return [...llm.map((r) => item(r, m.recipeGroupLlm)), ...tool.map((r) => item(r, m.recipeGroupTool))];
}
