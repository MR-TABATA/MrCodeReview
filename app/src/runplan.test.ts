import { describe, expect, it } from 'vitest';
import { groupSkipped, launchOf, notCovered, planRun, REASON_BAD_SPEC, REASON_NO_LLM, REASON_NO_REPO, REASON_NO_SPEC, type RunInputs } from './runplan';
import type { Recipe } from './types';

const r = (id: string, over: Partial<Recipe> = {}): Recipe => ({ id, title: id, kind: 'llm', source: 'builtin', prompt: '', ...over });
const eight: Recipe[] = [
  r('bugs'), r('errors'), r('security'), r('performance'), r('tests'), r('consistency'),
  r('spec', { needs_spec: true }),
  r('lint', { kind: 'tool' }),
];
const base: RunInputs = { hasRepo: true, filesCount: 10, baseIsHead: false, hasProvider: true, hasSpec: false, specValid: false };

describe('planRun: 実行できない観点だけを外して、できるものは走らせる', () => {
  it('設計書が未指定でも、設計書が要らない観点は走る（今日の画面: 8 件 → 7 件）', () => {
    const p = planRun(eight, base);
    expect(p.block).toBe('');
    expect(p.run.map((x) => x.id)).toEqual(['bugs', 'errors', 'security', 'performance', 'tests', 'consistency', 'lint']);
    expect(p.skipped.map((s) => [s.recipe.id, s.why, s.reason])).toEqual([['spec', 'spec', REASON_NO_SPEC]]);
  });

  it('設計書を指定すれば、8 件とも走る', () => {
    const p = planRun(eight, { ...base, hasSpec: true, specValid: true });
    expect(p.run).toHaveLength(8);
    expect(p.skipped).toEqual([]);
    expect(p.block).toBe('');
  });

  it('設計書が無効（文字数の上限超えなど）なら、その理由で外す', () => {
    const p = planRun(eight, { ...base, hasSpec: true, specValid: false });
    expect(p.skipped.map((s) => s.reason)).toEqual([REASON_BAD_SPEC]);
    expect(p.run).toHaveLength(7);
  });

  it('設計書の観点だけを選んでいて未指定なら、全体を止める（従来どおり）', () => {
    const p = planRun([eight[6]], base);
    expect(p.block).toBe(REASON_NO_SPEC);
    expect(p.run).toEqual([]);
  });

  it('独自の観点でも、needs_spec なら同じ扱い', () => {
    const custom = r('my-spec', { needs_spec: true, source: 'repo' });
    const p = planRun([r('bugs'), custom], base);
    expect(p.run.map((x) => x.id)).toEqual(['bugs']);
    expect(p.skipped.map((s) => s.recipe.id)).toEqual(['my-spec']);
  });

  it('LLM が未選択なら、LLM が要らない観点（リンター）だけ走る', () => {
    const p = planRun(eight, { ...base, hasProvider: false, hasSpec: true, specValid: true });
    expect(p.run.map((x) => x.id)).toEqual(['lint']);
    expect(p.skipped.every((s) => s.why === 'llm' && s.reason === REASON_NO_LLM)).toBe(true);
    expect(p.skipped).toHaveLength(7);
  });

  it('LLM 未選択で LLM の観点だけなら、全体を止める。理由は LLM（従来の順）', () => {
    const p = planRun([r('bugs'), eight[6]], { ...base, hasProvider: false });
    expect(p.block).toBe(REASON_NO_LLM);
    expect(p.run).toEqual([]);
  });

  it('設計書も LLM も足りないとき、設計書の観点は、両方の理由を出す（設計書だけを直しても、まだ走らないため）', () => {
    const p = planRun(eight, { ...base, hasProvider: false });
    const spec = p.skipped.find((s) => s.recipe.id === 'spec')!;
    expect(spec.why).toBe('llm');
    expect(spec.reason).toBe(`${REASON_NO_LLM} ／ ${REASON_NO_SPEC}`);
    // 設計書を指定したあとは、LLM の理由だけになる
    const after = planRun(eight, { ...base, hasProvider: false, hasSpec: true, specValid: true });
    expect(after.skipped.find((s) => s.recipe.id === 'spec')?.reason).toBe(REASON_NO_LLM);
    expect(p.run.map((x) => x.id)).toEqual(['lint']);
  });

  it('設計書が無効で LLM も無いときは、無効の理由と LLM の理由を並べる', () => {
    const p = planRun([eight[6]], { ...base, hasProvider: false, hasSpec: true, specValid: false });
    expect(p.block).toBe(`${REASON_NO_LLM} ／ ${REASON_BAD_SPEC}`);
  });

  it('リポジトリ未選択・差分なしは、全体を止める（外しても実行できない）', () => {
    expect(planRun(eight, { ...base, hasRepo: false }).block).toBe(REASON_NO_REPO);
    expect(planRun(eight, { ...base, filesCount: 0, baseIsHead: true }).block).toBe('送る差分がありません（未コミットの変更が無い）');
    expect(planRun(eight, { ...base, filesCount: 0 }).block).toBe('送る差分がありません（この範囲に、変更が無い）');
  });

  it('差分をまだ読めていないあいだは、止めも外しもしない', () => {
    const p = planRun(eight, { ...base, filesCount: null });
    expect(p.block).toBe('');
    expect(p.run).toHaveLength(8);
    expect(p.skipped).toEqual([]);
  });

  it('何も選んでいないときは、止めない（ボタンは、選択数 0 で無効になる）', () => {
    expect(planRun([], base)).toEqual({ block: '', run: [], skipped: [] });
  });
});

describe('groupSkipped', () => {
  it('同じ理由の観点は、1 つにまとめる', () => {
    const p = planRun([r('spec', { needs_spec: true }), r('my-spec', { needs_spec: true }), r('bugs')], { ...base, hasProvider: true });
    const g = groupSkipped(p.skipped, (x) => x.id);
    expect(g).toEqual([{ reason: REASON_NO_SPEC, why: 'spec', titles: ['spec', 'my-spec'], ids: ['spec', 'my-spec'] }]);
  });
});

describe('launchOf: 入口が始めるもの（外した観点は、入らない）', () => {
  it('spec を含む 8 件・設計書なし → まとめて実行する 7 件で、spec を含まない', () => {
    const l = launchOf(planRun(eight, base));
    expect(l.kind).toBe('batch');
    expect(l.kind === 'batch' && l.ids).toEqual(['bugs', 'errors', 'security', 'performance', 'tests', 'consistency', 'lint']);
    expect(l.kind === 'batch' && l.ids.includes('spec')).toBe(false);
  });

  it('外して 1 件だけ残るときは、単発で実行する', () => {
    const l = launchOf(planRun([r('bugs'), eight[6]], base));
    expect(l).toEqual({ kind: 'single', id: 'bugs' });
  });

  it('設計書を指定すれば、8 件とも入る', () => {
    const l = launchOf(planRun(eight, { ...base, hasSpec: true, specValid: true }));
    expect(l.kind === 'batch' && l.ids).toHaveLength(8);
  });

  it('全体を止めるとき・何も無いときは、何も始めない', () => {
    expect(launchOf(planRun([eight[6]], base))).toEqual({ kind: 'none' });
    expect(launchOf(planRun(eight, { ...base, hasRepo: false }))).toEqual({ kind: 'none' });
    expect(launchOf(planRun(eight, { ...base, filesCount: 0 }))).toEqual({ kind: 'none' });
    expect(launchOf(planRun([], base))).toEqual({ kind: 'none' });
  });
});

describe('notCovered: 前回の実行に含まれない観点', () => {
  const run = [r('bugs'), r('security')];
  it('前回の続きを選んでいない（covered が無い）ときは、空', () => {
    expect(notCovered(run, undefined)).toEqual([]);
  });
  it('全部が前回に含まれるときは、空', () => {
    expect(notCovered(run, ['security', 'bugs', 'tests'])).toEqual([]);
  });
  it('一部だけ含まれるときは、含まれないものだけ（前回が bugs だけ → security）', () => {
    expect(notCovered(run, ['bugs']).map((x) => x.id)).toEqual(['security']);
  });
  it('前回の観点が空（どれも含まれない）なら、全部', () => {
    expect(notCovered(run, []).map((x) => x.id)).toEqual(['bugs', 'security']);
  });
});
