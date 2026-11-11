import { describe, it, expect, vi } from 'vitest';
import { settleSelection, bootRepos, planAfterAdd, planBatchAdd, previewErrorText, moveConfirmText, refreshBasesOnFocus, BASES_MIN_GAP_MS, type RepoAddResult, type BasesFocusDeps } from './repoflow';
import type { BaseOption } from './types';

const r = (path: string, hidden = false) => ({ id: path, path, ...(hidden ? { hidden: true } : {}) });
const opt = (value: string): BaseOption => ({ value, label: value, hint: '' });

describe('settleSelection', () => {
  it('登録を読めていないときは、選択に触れない（登録が空に見えるだけ）', () => {
    expect(settleSelection([], '/a/repo', false)).toBe('/a/repo');
    expect(settleSelection([r('/b')], '/a/repo', false)).toBe('/a/repo');
  });

  it('登録のパスと同じなら、そのまま。選択が空でも、そのまま', () => {
    expect(settleSelection([r('/a/repo')], '/a/repo', true)).toBe('/a/repo');
    expect(settleSelection([r('/a/repo')], '', true)).toBe('');
  });

  it('サブディレクトリは、登録のパスに揃える。どの登録にも当たらなければ、空', () => {
    expect(settleSelection([r('/a/repo')], '/a/repo/src', true)).toBe('/a/repo');
    expect(settleSelection([r('/a/repo')], '/a/repo-other', true)).toBe('');
    expect(settleSelection([], '/a/repo', true)).toBe('');
  });

  it('解除（非表示）した登録は、選択に残さない。見えている先頭にする（無ければ空）', () => {
    expect(settleSelection([r('/b'), r('/a/repo', true)], '/a/repo', true)).toBe('/b');
    expect(settleSelection([r('/a/repo', true)], '/a/repo', true)).toBe('');
  });
});

describe('bootRepos', () => {
  it('読めたとき: 以前の登録と選択中のパスを渡し、選択を揃える', async () => {
    const load = vi.fn(async () => ({ repos: [r('/a/repo')], limit: 3 }));
    const out = await bootRepos({ legacy: [r('/x')], selected: '/a/repo/src', load });
    expect(load).toHaveBeenCalledWith([r('/x'), { id: '', path: '/a/repo/src' }]);
    expect(out).toEqual({ list: { repos: [r('/a/repo')], limit: 3 }, error: '', warnings: [], selected: '/a/repo' });
  });

  it('読めたが、止めずに進めた失敗（警告）があれば、そのまま返す', async () => {
    const load = vi.fn(async () => ({ repos: [r('/a')], limit: 3, warnings: ['/a: 最初のコミットを調べられません'] }));
    const out = await bootRepos({ legacy: [], selected: '/a', load });
    expect(out.error).toBe('');
    expect(out.warnings).toEqual(['/a: 最初のコミットを調べられません']);
  });

  it('選択中のパスがすでに以前の登録にあれば、重ねて足さない', async () => {
    const load = vi.fn(async () => ({ repos: [], limit: 3 }));
    await bootRepos({ legacy: [r('/a')], selected: '/a', load });
    expect(load).toHaveBeenCalledWith([r('/a')]);
  });

  it('読めなかったとき: 理由を返し、選択はそのまま（保存済みの選択を消さない）', async () => {
    const load = vi.fn(async () => {
      throw new Error('database is locked');
    });
    const out = await bootRepos({ legacy: [], selected: '/a/repo', load });
    expect(out.list).toBeNull();
    expect(out.selected).toBe('/a/repo');
    expect(out.error).toContain('登録リポジトリを読めません');
    expect(out.error).toContain('database is locked');
  });
});

describe('planBatchAdd（複数のフォルダを、まとめて追加したあと）', () => {
  const ok = (path: string, over: Partial<RepoAddResult> = {}) => ({ path, out: { repos: [], limit: 3, path, limit_reached: false, already_at: null, move_from: null, ...over } as RepoAddResult });

  it('すべて追加できたら、最初のものを選び、問題なしと言う', () => {
    const p = planBatchAdd([ok('/a/x'), ok('/a/y'), ok('/a/z')]);
    expect(p.select).toBe('/a/x');
    expect(p.problems).toBe(false);
    expect(p.message).toBe('3 件のうち、3 件を追加しました。');
  });

  it('枠がいっぱいで断られたものは、名前を挙げて伝える。追加できたものは、そのまま選べる', () => {
    const p = planBatchAdd([ok('/a/x'), { path: '/a/y', out: { repos: [], limit: 3, path: null, limit_reached: true, already_at: null, move_from: null } }, { path: '/a/z/', out: { repos: [], limit: 3, path: null, limit_reached: true, already_at: null, move_from: null } }]);
    expect(p.select).toBe('/a/x');
    expect(p.problems).toBe(true);
    expect(p.message).toContain('3 件のうち、1 件を追加しました。');
    expect(p.message).toContain('枠がいっぱいで追加できなかったもの: y、z。');
  });

  it('移動したかもしれない登録は、何も変えず、1 件ずつ確認するよう伝える（確認なしに付け替えない）', () => {
    const p = planBatchAdd([{ path: '/new/x', out: { repos: [], limit: 3, path: null, limit_reached: false, already_at: null, move_from: '/old/x' } }]);
    expect(p.select).toBeNull();
    expect(p.problems).toBe(true);
    expect(p.message).toContain('何も変えていません');
    expect(p.message).toContain('x');
  });

  it('呼び出しが失敗したものは、理由を添える。ほかの追加は止めない', () => {
    const p = planBatchAdd([{ path: '/a/bad', error: 'git のリポジトリではありません' }, ok('/a/good')]);
    expect(p.select).toBe('/a/good');
    expect(p.problems).toBe(true);
    expect(p.message).toContain('追加できなかったもの: bad（git のリポジトリではありません）。');
  });

  it('別の場所に登録済みだったものは、そちらを使うと伝える（問題ではない）', () => {
    const p = planBatchAdd([ok('/a/x', { already_at: '/b/x' })]);
    expect(p.problems).toBe(false);
    expect(p.select).toBe('/a/x');
    expect(p.message).toContain('すでに別の場所に登録されていた');
  });

  it('ひとつも追加できなかったときは、選ばない', () => {
    const p = planBatchAdd([{ path: '/a/bad', error: 'x' }]);
    expect(p.select).toBeNull();
    expect(p.message).toContain('0 件を追加しました');
  });
});

describe('planAfterAdd', () => {
  const base: RepoAddResult = { repos: [], limit: 3, path: '/a', limit_reached: false, already_at: null, move_from: null };

  it('追加できたら、そのパスを選ぶ', () => {
    expect(planAfterAdd(base)).toEqual({ select: '/a', limitReached: false, alreadyAt: null, moveFrom: null });
  });

  it('上限で断られたら、選ばない（パスが入っていても）', () => {
    expect(planAfterAdd({ ...base, limit_reached: true, path: null })).toEqual({ select: null, limitReached: true, alreadyAt: null, moveFrom: null });
    expect(planAfterAdd({ ...base, limit_reached: true }).select).toBeNull();
    expect(planAfterAdd({ ...base, path: null }).select).toBeNull();
  });

  it('登録のフォルダが見つからないときは、選ばず、確認のために元のパスを返す（何も変えていない）', () => {
    expect(planAfterAdd({ ...base, path: null, move_from: '/old/place' })).toEqual({ select: null, limitReached: false, alreadyAt: null, moveFrom: '/old/place' });
  });

  it('別の場所に登録済みだったときは、その場所を伝える', () => {
    expect(planAfterAdd({ ...base, path: '/orig', already_at: '/orig' })).toEqual({ select: '/orig', limitReached: false, alreadyAt: '/orig', moveFrom: null });
  });
});

describe('refreshBasesOnFocus', () => {
  const mk = (over: Partial<BasesFocusDeps> = {}) => {
    const state = { repo: '/a', running: '', bases: [opt('HEAD')], t: 10_000 };
    const applied: BaseOption[][] = [];
    const calls = { fail: [] as string[], ok: 0 };
    const deps: BasesFocusDeps = {
      repo: () => state.repo,
      running: () => state.running,
      bases: () => state.bases,
      now: () => state.t,
      fetch: async () => [opt('HEAD'), opt('HEAD~1')],
      apply: (n) => {
        state.bases = n;
        applied.push(n);
      },
      fail: (m) => calls.fail.push(m),
      ok: () => void (calls.ok += 1),
      gate: { at: 0 },
      ...over,
    };
    return { state, applied, deps, calls };
  };

  it('一覧が変わっていたら、取り直して入れ替える', async () => {
    const { applied, deps } = mk();
    await refreshBasesOnFocus(deps);
    expect(applied).toEqual([[opt('HEAD'), opt('HEAD~1')]]);
  });

  it('取得に失敗したら、いまの一覧を残し（入れ替えない）、理由を出す（黙って捨てない）', async () => {
    const { state, applied, deps, calls } = mk({
      fetch: async () => {
        throw new Error('このリポジトリは、登録されていません');
      },
    });
    await refreshBasesOnFocus(deps);
    expect(applied).toEqual([]);
    expect(state.bases).toEqual([opt('HEAD')]);
    expect(calls.fail).toHaveLength(1);
    expect(calls.fail[0]).toContain('登録されていません');
    expect(calls.ok).toBe(0);
  });

  it('成功したら、前の失敗の表示を消すために、ok を呼ぶ（中身が同じでも）', async () => {
    const { applied, deps, calls } = mk({ fetch: async () => [opt('HEAD')] });
    await refreshBasesOnFocus(deps);
    expect(calls.ok).toBe(1);
    expect(calls.fail).toEqual([]);
    expect(applied).toEqual([]);
  });

  it('中身が同じなら、入れ替えない（描き直さない）', async () => {
    const { applied, deps } = mk({ fetch: async () => [opt('HEAD')] });
    await refreshBasesOnFocus(deps);
    expect(applied).toEqual([]);
  });

  it('取り直している間にリポジトリが変わったら、結果も失敗も捨てる（別のリポジトリに出さない）', async () => {
    const { state, applied, deps, calls } = mk();
    deps.fetch = async () => {
      state.repo = '/other';
      return [opt('HEAD'), opt('HEAD~1')];
    };
    await refreshBasesOnFocus(deps);
    const m2 = mk();
    m2.deps.fetch = async () => {
      m2.state.repo = '/other';
      throw new Error('x');
    };
    await refreshBasesOnFocus(m2.deps);
    expect(applied).toEqual([]);
    expect(calls).toEqual({ fail: [], ok: 0 });
    expect(m2.calls).toEqual({ fail: [], ok: 0 });
  });

  it('リポジトリが無い・実行中は、取り直さない', async () => {
    const fetch = vi.fn(async () => [opt('X')]);
    await refreshBasesOnFocus(mk({ fetch, repo: () => '' }).deps);
    await refreshBasesOnFocus(mk({ fetch, running: () => 'bugs' }).deps);
    expect(fetch).not.toHaveBeenCalled();
  });

  it('続けて窓に戻っても、間隔（1.5 秒）が空くまでは、取り直さない', async () => {
    const fetch = vi.fn(async () => [opt('HEAD'), opt('HEAD~1')]);
    const { state, deps } = mk({ fetch });
    await refreshBasesOnFocus(deps);
    state.t += BASES_MIN_GAP_MS - 1;
    await refreshBasesOnFocus(deps);
    expect(fetch).toHaveBeenCalledTimes(1);
    state.t += 1;
    await refreshBasesOnFocus(deps);
    expect(fetch).toHaveBeenCalledTimes(2);
  });

  it('間引かれた分も、失敗した分も、時刻は記録する（失敗が続いても、連打しない）', async () => {
    const fetch = vi.fn(async () => {
      throw new Error('x');
    });
    const { state, deps } = mk({ fetch });
    await refreshBasesOnFocus(deps);
    await refreshBasesOnFocus(deps);
    expect(fetch).toHaveBeenCalledTimes(1);
    state.t += BASES_MIN_GAP_MS;
    await refreshBasesOnFocus(deps);
    expect(fetch).toHaveBeenCalledTimes(2);
  });
});

describe('previewErrorText', () => {
  it('送る内容を読めなかった理由が、先（候補の理由は、重ねない）', () => {
    expect(previewErrorText('登録されていません', '登録されていません')).toBe('登録されていません');
    expect(previewErrorText('差分がありません', '')).toBe('差分がありません');
  });

  it('送る内容は読めて、候補だけ読めなかったときは、その理由を出す（黙って空にしない）', () => {
    expect(previewErrorText('', 'git の失敗')).toBe('見る範囲の候補を読めませんでした: git の失敗');
  });

  it('どちらも無ければ、空', () => {
    expect(previewErrorText('', '')).toBe('');
  });
});

describe('moveConfirmText（付け替えの確認の文面）', () => {
  it('何をするかを先に言い、元と新しいパスを並べ、履歴が引き継がれることと、キャンセルの意味を言う', () => {
    const t = moveConfirmText('/old/repo', '/new/repo');
    expect(t.split('\n')[0]).toBe('登録を、新しいフォルダに付け替えます。');
    expect(t).toContain('元: /old/repo');
    expect(t).toContain('新: /new/repo');
    expect(t).toContain('履歴（レビュー・仕分け・メモ）は、そのまま引き継ぎます');
    expect(t).toContain('「キャンセル」にしてください（何も変えません）');
  });

  it('「見つかりません」で始めない（付け替えようとしている人には、意味が通らない）', () => {
    expect(moveConfirmText('/a', '/b').startsWith('登録しているフォルダが見つかりません')).toBe(false);
  });
});
