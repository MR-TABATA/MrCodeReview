/**
 * 登録リポジトリの管理。
 * - 名前（省略可）とパスを保持する。
 * - 名前が省略された場合は、パス末尾のディレクトリ名（例: `/Users/.../MRDown` → `MRDown`）を表示名とする。
 */

export interface RegisteredRepo {
  id: string;
  path: string;
  name?: string;
  /** 解除した（論理削除）。一覧には出ないが、行は残り、枠は使い続ける（構想書 §8）。 */
  hidden?: boolean;
}

/** 一覧に出すもの（解除していないもの）。 */
export const visibleRepos = (repos: RegisteredRepo[]): RegisteredRepo[] => repos.filter((r) => !r.hidden);

/** 解除済み（非表示）のもの。 */
export const hiddenRepos = (repos: RegisteredRepo[]): RegisteredRepo[] => repos.filter((r) => r.hidden);

/** 枠を使っている数。**解除（非表示）した行も数える。** */
export const slotCount = (repos: RegisteredRepo[]): number => repos.length;

/**
 * 「追加」を押せない理由。押せるなら null。
 * 枠がいっぱいのときは押せない。ただし、登録のフォルダが「見つかりません」のものがあるあいだは押せる
 * （フォルダを移動したときの付け替えは、移動先を「追加」して行うため。枠がいっぱいでも、付け替えは枠を増やさない）。
 */
export function addBlockedReason(repos: RegisteredRepo[], limit: number | null, missing: (path: string) => boolean): string | null {
  if (limit === null || slotCount(repos) < limit) return null;
  if (visibleRepos(repos).some((r) => missing(r.path))) return null;
  return `枠がいっぱいです（${limit} / ${limit}）。追加するには、設定の「リポジトリ」で「データを削除…」して枠を空けてください`;
}

/** 以前の版が、登録を画面に持っていたキー。いまは Rust（SQLite）が持つ。初回の取り込みにだけ読む。 */
export const REPOS_KEY = 'mrcr.registered_repos';

/**
 * リポジトリの表示名を返す。
 * 名前が指定されていればそれを使い、省略時はパス末尾のディレクトリ名を返す。
 */
export function repoDisplayName(repo: { path: string; name?: string }): string {
  const trimmed = repo.name?.trim();
  if (trimmed) return trimmed;
  const clean = repo.path.replace(/\/+$/, '');
  const last = clean.split('/').filter(Boolean).pop();
  return last || repo.path;
}

export function loadRegisteredRepos(raw?: string | null): RegisteredRepo[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((r): r is RegisteredRepo => typeof r === 'object' && r !== null && typeof r.path === 'string');
  } catch {
    return [];
  }
}

const trimSlash = (p: string): string => (p.length > 1 ? p.replace(/\/+$/, '') : p);

/**
 * パスから、登録のリポジトリを探す。登録のパスと同じか、その中のサブディレクトリなら、いちばん深く一致する登録を返す。
 * 区切り（`/`）で比べるので、`/a/repo-other` は `/a/repo` に一致しない。解除済みの行も対象（その扱いは、呼び出し側）。
 */
export function resolveRegisteredRepo(repos: RegisteredRepo[], path: string): RegisteredRepo | undefined {
  const target = trimSlash(path.trim());
  let best: RegisteredRepo | undefined;
  for (const r of repos) {
    const root = trimSlash(r.path);
    if (target === root || target.startsWith(root === '/' ? '/' : root + '/')) {
      if (!best || root.length > trimSlash(best.path).length) best = r;
    }
  }
  return best;
}
