import { describe, it, expect } from 'vitest';
import { addBlockedReason, repoDisplayName, loadRegisteredRepos, resolveRegisteredRepo, visibleRepos, hiddenRepos, slotCount } from './reporegistry';

describe('reporegistry', () => {
  describe('repoDisplayName', () => {
    it('名前が指定されている場合は、その名前を返す', () => {
      expect(repoDisplayName({ path: '/Users/me/Git/MRDown', name: 'マイエディタ' })).toBe('マイエディタ');
    });

    it('名前が空文字や空白のみの場合は、末尾ディレクトリ名を返す', () => {
      expect(repoDisplayName({ path: '/Users/me/Git/MRDown', name: '   ' })).toBe('MRDown');
      expect(repoDisplayName({ path: '/Users/me/Git/MRDown', name: '' })).toBe('MRDown');
    });

    it('名前が省略された場合は、末尾ディレクトリ名を返す', () => {
      expect(repoDisplayName({ path: '/Users/me/Git/MRDown' })).toBe('MRDown');
      expect(repoDisplayName({ path: '~/Git/MRDown/' })).toBe('MRDown');
      expect(repoDisplayName({ path: '/repo' })).toBe('repo');
    });

    it('末尾にスラッシュが連続しても、最後のディレクトリ名を正しく抽出する', () => {
      expect(repoDisplayName({ path: '/Users/me/Git/SyncLane///' })).toBe('SyncLane');
    });
  });


  describe('loadRegisteredRepos（以前の版の登録を、初回に取り込むための読み取り）', () => {
    it('空文字や壊れたJSONの場合は空配列を返す', () => {
      expect(loadRegisteredRepos(null)).toEqual([]);
      expect(loadRegisteredRepos('')).toEqual([]);
      expect(loadRegisteredRepos('invalid json')).toEqual([]);
      expect(loadRegisteredRepos('{"not":"array"}')).toEqual([]);
    });

    it('有効なJSONを、そのまま復元する。パスの無い要素は捨てる', () => {
      const repos = [
        { id: '1', path: '/a/b', name: 'カスタム名' },
        { id: '2', path: '/c/d', hidden: true },
      ];
      expect(loadRegisteredRepos(JSON.stringify([...repos, { id: '3' }, null]))).toEqual(repos);
    });
  });

  describe('一覧の見え方と枠の数え方（登録そのものは Rust が持つ。repos.rs のテスト）', () => {
    const r = (p: string, hidden = false) => ({ id: p, path: p, ...(hidden ? { hidden: true } : {}) });

    it('解除済みは、一覧（visibleRepos）に出ないが、枠（slotCount）には数える', () => {
      const repos = [r('/a'), r('/b', true), r('/c')];
      expect(slotCount(repos)).toBe(3);
      expect(visibleRepos(repos).map((x) => x.path)).toEqual(['/a', '/c']);
      expect(hiddenRepos(repos).map((x) => x.path)).toEqual(['/b']);
    });
  });

  describe('addBlockedReason（「追加」を押せない理由）', () => {
    const r = (p: string, hidden = false) => ({ id: p, path: p, ...(hidden ? { hidden: true } : {}) });
    const none = () => false;

    it('枠に空きがあるとき・無制限のときは、押せる', () => {
      expect(addBlockedReason([r('/a'), r('/b')], 3, none)).toBeNull();
      expect(addBlockedReason([r('/a'), r('/b'), r('/c'), r('/d')], null, none)).toBeNull();
      expect(addBlockedReason([], 3, none)).toBeNull();
    });

    it('枠がいっぱい（解除済みも数える）なら、押せない。理由を返す', () => {
      expect(addBlockedReason([r('/a'), r('/b'), r('/c')], 3, none)).toContain('枠がいっぱいです');
      expect(addBlockedReason([r('/a'), r('/b'), r('/c', true)], 3, none), '解除済みも枠を使う').not.toBeNull();
    });

    it('超過しているとき（4 件で上限 3）も、押せない', () => {
      expect(addBlockedReason([r('/a'), r('/b'), r('/c'), r('/d')], 3, none)).not.toBeNull();
    });

    it('見つからない登録があるあいだは、満杯でも押せる（フォルダを移動したときの付け替えのため）', () => {
      const repos = [r('/a'), r('/b'), r('/c')];
      expect(addBlockedReason(repos, 3, (p) => p === '/b')).toBeNull();
    });

    it('見つからないのが、解除済みの行だけなら、押せない（付け替えは、一覧に出ているものだけが対象）', () => {
      const repos = [r('/a'), r('/b'), r('/c', true)];
      expect(addBlockedReason(repos, 3, (p) => p === '/c')).not.toBeNull();
    });
  });

  describe('resolveRegisteredRepo（起動時に、選択中のパスを、登録のパスへ揃える）', () => {
    const r = (path: string) => ({ id: path, path });

    it('同じパスは、そのまま。末尾の / があっても同じ', () => {
      expect(resolveRegisteredRepo([r('/a/repo')], '/a/repo')?.path).toBe('/a/repo');
      expect(resolveRegisteredRepo([r('/a/repo/')], '/a/repo')?.path).toBe('/a/repo/');
      expect(resolveRegisteredRepo([r('/a/repo')], '/a/repo///')?.path).toBe('/a/repo');
    });

    it('サブディレクトリは、登録の根に解決される', () => {
      expect(resolveRegisteredRepo([r('/a/repo')], '/a/repo/src/deep')?.path).toBe('/a/repo');
    });

    it('名前が似ている兄弟には、一致しない（区切りで比べる）', () => {
      expect(resolveRegisteredRepo([r('/a/repo')], '/a/repo-other')).toBeUndefined();
      expect(resolveRegisteredRepo([r('/a/repo')], '/a/repository/x')).toBeUndefined();
      expect(resolveRegisteredRepo([r('/a/repo-other')], '/a/repo')).toBeUndefined();
    });

    it('候補が複数あるときは、いちばん深く一致するものを選ぶ（登録の並びに依らない）', () => {
      const shallow = r('/a/repo');
      const deep = r('/a/repo/sub');
      expect(resolveRegisteredRepo([shallow, deep], '/a/repo/sub/x')).toBe(deep);
      expect(resolveRegisteredRepo([deep, shallow], '/a/repo/sub/x')).toBe(deep);
      expect(resolveRegisteredRepo([deep, shallow], '/a/repo/other/x')).toBe(shallow);
      expect(resolveRegisteredRepo([deep, shallow], '/a/repo/sub')).toBe(deep);
    });

    it('登録が無い・どれにも一致しないときは undefined。ルートの登録も壊れない', () => {
      expect(resolveRegisteredRepo([], '/a/repo')).toBeUndefined();
      expect(resolveRegisteredRepo([r('/b')], '/a/repo')).toBeUndefined();
      expect(resolveRegisteredRepo([r('/')], '/a/repo')?.path).toBe('/');
    });
  });
});
