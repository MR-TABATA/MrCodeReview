import { describe, expect, it } from 'vitest';
import { canTrustRerun } from './lintrerun';
import type { LintItem, LintPlan } from './types';

const item = (id: string, runs_repo_code: boolean): LintItem => ({
  id,
  title: id,
  lang: 'x',
  exts: [],
  command: '',
  runs_repo_code,
  state: 'needs_trust',
  hint: null,
});
const plan = (trusted: boolean, ...items: LintItem[]): LintPlan => ({ items, trusted });
const eslint = item('lint-eslint', true);
const ruff = item('lint-ruff', false);

describe('「許可して再実行」を出す条件', () => {
  it('「リンター」が失敗し、未許可で、コードを動かす linter があるときだけ出る', () => {
    expect(canTrustRerun('lint', '許可が無く、実行していません', plan(false, eslint, ruff))).toBe(true);
  });

  it('許可済みなら、出さない', () => {
    expect(canTrustRerun('lint', 'エラー', plan(true, eslint))).toBe(false);
  });

  it('失敗していない（エラーが無い・空）なら、出さない', () => {
    expect(canTrustRerun('lint', null, plan(false, eslint))).toBe(false);
    expect(canTrustRerun('lint', undefined, plan(false, eslint))).toBe(false);
    expect(canTrustRerun('lint', '', plan(false, eslint))).toBe(false);
  });

  it('個別の linter（lint-eslint）や、ほかの観点では出さない', () => {
    expect(canTrustRerun('lint-eslint', 'エラー', plan(false, eslint))).toBe(false);
    expect(canTrustRerun('bugs', 'エラー', plan(false, eslint))).toBe(false);
  });

  it('コードを動かす linter が無ければ（許可しても変わらない）、出さない', () => {
    expect(canTrustRerun('lint', 'エラー', plan(false, ruff))).toBe(false);
    expect(canTrustRerun('lint', 'エラー', plan(false))).toBe(false);
  });

  it('一覧がまだ無い（リポジトリを選ぶ前）なら、出さない', () => {
    expect(canTrustRerun('lint', 'エラー', null)).toBe(false);
  });
});
