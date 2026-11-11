import type { LintPlan } from './types';

/**
 * 失敗した「リンター」の結果に、「許可して再実行」のボタンを出すか。
 * 4 つがそろったときだけ: 観点が「リンター」全体（`lint`。`lint-eslint` のような個別のものは出さない）・実行に失敗している・
 * このリポジトリを、まだ許可していない・コードを動かす linter（ESLint・Clippy）がある。
 * 許可済みなのに出る、許可が要る失敗なのに出ない、のどちらも、画面の行き止まりになる。
 */
export function canTrustRerun(recipeId: string, error: string | null | undefined, plan: LintPlan | null): boolean {
  return recipeId === 'lint' && !!error && !plan?.trusted && !!plan?.items.some((i) => i.runs_repo_code);
}
