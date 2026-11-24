import { h } from './dom';
import { icon } from './icons';
import { getAppLang, msg, type Lang } from './i18n';
import { repoDisplayName, type RegisteredRepo } from './reporegistry';

export interface RepoViewActions {
  onSelect(repo: RegisteredRepo): void;
  onAdd(): void;
  onAddFile?(): void;
  onRemove(repo: RegisteredRepo): void;
  onRename(repo: RegisteredRepo): void;
}

const BG_PALETTE = [
  '#4f46e5', '#2563eb', '#0891b2', '#059669',
  '#d97706', '#dc2626', '#7c3aed', '#db2777',
];

/** 「未」の指摘がこの件数以上のリポジトリは、件数のバッジを強く（塗りつぶして）見せる */
export const OPEN_STRONG_FROM = 20;

/** 文書グループの折りたたみ状態（キー: グループキー）。画面再描画時も状態を保持する */
export const collapsedDocGroups = new Set<string>();

export function appInitials(name: string): string {
  const clean = name.replace(/^[^a-zA-Z0-9\u3040-\u30ff\u4e00-\u9faf]+/, '');
  if (clean.length <= 2) return clean.toUpperCase();
  const uppers = clean.match(/[A-Z]/g);
  if (uppers && uppers.length >= 2) return (uppers[0] + uppers[1]).toUpperCase();
  return clean.slice(0, 2).toUpperCase();
}

function nameToColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return BG_PALETTE[hash % BG_PALETTE.length];
}

export function isDocPath(path: string): boolean {
  return /\.(md|markdown|txt)$/i.test(path);
}

export interface DocGroupInfo {
  key: string;
  name: string;
  subPath: string;
  relPath(path: string): string;
}

/**
 * 文書ファイルが所属する大枠グループ（リポジトリ単位、例: 'Spec'）を判定する。
 * 1. 登録済みリポジトリ（通常リポジトリ）の配下にあれば、そのリポジトリ名（例: 'Spec'）
 * 2. パスに /Git/<Name>/ 等があればそのプロジェクト名（例: 'Spec'）
 * 3. それ以外は親ディレクトリ名
 */
export function resolveDocGroup(docPath: string, allRepos: RegisteredRepo[] = []): DocGroupInfo {
  const norm = docPath.replace(/\\/g, '/');

  // 1. 登録済みリポジトリの配下にあるか？（より深いリポジトリを優先）
  const gitRepos = allRepos.filter((r) => !isDocPath(r.path));
  const matched = gitRepos
    .filter((r) => norm.startsWith(r.path.replace(/\\/g, '/') + '/'))
    .sort((a, b) => b.path.length - a.path.length)[0];

  if (matched) {
    const root = matched.path.replace(/\\/g, '/');
    const name = repoDisplayName(matched);
    const sub = root.replace(/^\/Users\/[^/]+/, '~');
    return {
      key: root,
      name,
      subPath: sub,
      relPath: (p: string) => {
        const pNorm = p.replace(/\\/g, '/');
        return pNorm.startsWith(root + '/') ? pNorm.slice(root.length + 1) : pNorm;
      },
    };
  }

  // 2. パス中の /Git/<Name>/ や /workspace/<Name>/ などのリポジトリ名を探す
  const gitMatch = norm.match(/(?:^|\/)(?:Git|GitHub|workspace|projects|repos)\/([^/]+)/i);
  if (gitMatch) {
    const name = gitMatch[1];
    const prefixEnd = gitMatch.index! + gitMatch[0].length;
    const root = norm.slice(0, prefixEnd);
    const sub = root.replace(/^\/Users\/[^/]+/, '~');
    return {
      key: root,
      name,
      subPath: sub,
      relPath: (p: string) => {
        const pNorm = p.replace(/\\/g, '/');
        return pNorm.startsWith(root + '/') ? pNorm.slice(root.length + 1) : pNorm;
      },
    };
  }

  // 3. フォールバック: 親ディレクトリ
  const lastSlash = norm.lastIndexOf('/');
  const parent = lastSlash > 0 ? norm.slice(0, lastSlash) : norm;
  const parts = parent.split('/').filter(Boolean);
  const name = parts[parts.length - 1] || parent;
  const sub = parent.replace(/^\/Users\/[^/]+/, '~');
  return {
    key: parent,
    name,
    subPath: sub,
    relPath: (p: string) => {
      const pNorm = p.replace(/\\/g, '/');
      return pNorm.startsWith(parent + '/') ? pNorm.slice(parent.length + 1) : pNorm;
    },
  };
}

const DOC_GROUP_COLORS = [
  '#059669', // emerald
  '#2563eb', // blue
  '#7c3aed', // violet
  '#d97706', // amber
  '#0891b2', // cyan
  '#dc2626', // rose/red
  '#4f46e5', // indigo
  '#0d9488', // teal
];

export function dirGroupColor(dirPath: string): string {
  let hash = 0;
  for (let i = 0; i < dirPath.length; i++) hash = (hash * 31 + dirPath.charCodeAt(i)) >>> 0;
  return DOC_GROUP_COLORS[hash % DOC_GROUP_COLORS.length];
}

export function renderAppIcon(title: string, iconUrl?: string | null, isDoc = false): HTMLElement {
  if (isDoc) {
    const wrap = h('span', { class: 'repo-app-icon-doc', title: '文書ファイル' });
    wrap.append(icon('report', 16));
    return wrap;
  }
  if (iconUrl) {
    return h('img', {
      class: 'repo-app-icon-img',
      src: iconUrl,
      alt: title,
    });
  }
  const initials = appInitials(title);
  const bg = nameToColor(title);
  return h('span', { class: 'repo-app-icon-badge', style: `background-color: ${bg};` }, initials);
}

export function repoListPanel(
  repos: RegisteredRepo[],
  selectedPath: string,
  acts: RepoViewActions,
  iconMap: Record<string, string | null> = {},
  /** 「+ 追加」を押せない理由（枠がいっぱい、など）。押せるなら null */
  addBlocked: string | null = null,
  lang: Lang = getAppLang(),
  /** リポジトリのパス → 「未」の指摘の数。0 や未取得は、出さない */
  openCounts: Record<string, number> = {},
): HTMLElement {
  const m = msg(lang).repoView;
  const listEl = h('div', { class: 'repo-list' });

  if (repos.length === 0) {
    listEl.append(
      h(
        'div',
        { class: 'empty-notice' },
        h('p', {}, m.emptyTitle),
        h('p', { class: 'muted' }, m.emptyHint),
      ),
    );
  } else {
    const renderItem = (r: RegisteredRepo, displayPath?: string) => {
      const isSelected = r.path === selectedPath;
      const title = repoDisplayName(r);
      const isCustomName = !!r.name?.trim();
      const iconUrl = iconMap[r.path];
      const shownPath = displayPath ?? r.path;

      return h(
        'div',
        {
          class: `repo-item ${isSelected ? 'active' : ''}`,
          onclick: () => acts.onSelect(r),
        },
        h(
          'div',
          { class: 'repo-item-main' },
          h('span', { class: 'repo-item-icon', title: m.appIconTitle(title) }, renderAppIcon(title, iconUrl, isDocPath(r.path))),
          h(
            'div',
            { class: 'repo-item-text' },
            h('div', { class: 'repo-item-name' }, title),
            h('div', { class: 'repo-item-path', title: r.path }, shownPath),
          ),
        ),
        h(
          'div',
          { class: 'repo-item-actions', onclick: (e) => e.stopPropagation() },
          h(
            'button',
            {
              class: 'repo-action-btn edit',
              title: isCustomName ? m.renameTitle : m.setNameTitle,
              onclick: () => acts.onRename(r),
            },
            '✎',
          ),
          h(
            'button',
            {
              class: 'repo-action-btn delete',
              title: m.unlinkTitle,
              onclick: () => acts.onRemove(r),
            },
            '✕',
          ),
        ),
        (openCounts[r.path] ?? 0) > 0
          ? h('span', { class: `repo-open-badge ${openCounts[r.path] >= OPEN_STRONG_FROM ? 'strong' : ''}`, title: m.openCountTitle(openCounts[r.path]) }, String(openCounts[r.path]))
          : null,
      );
    };

    // 文書ファイル（isDocPath）は、リポジトリ単位（Spec等）でまとめる
    type GroupEntry =
      | { kind: 'repo'; repo: RegisteredRepo }
      | { kind: 'doc-group'; info: DocGroupInfo; repos: RegisteredRepo[] };

    const entries: GroupEntry[] = [];
    const docGroups = new Map<string, { kind: 'doc-group'; info: DocGroupInfo; repos: RegisteredRepo[] }>();

    for (const r of repos) {
      if (!isDocPath(r.path)) {
        entries.push({ kind: 'repo', repo: r });
      } else {
        const info = resolveDocGroup(r.path, repos);
        let group = docGroups.get(info.key);
        if (!group) {
          group = { kind: 'doc-group', info, repos: [] };
          docGroups.set(info.key, group);
          entries.push(group);
        }
        group.repos.push(r);
      }
    }

    for (const entry of entries) {
      if (entry.kind === 'repo') {
        listEl.append(renderItem(entry.repo));
      } else {
        const color = dirGroupColor(entry.info.key);
        const hasSelected = entry.repos.some((r) => r.path === selectedPath);
        // 選択中のファイルが含まれている場合は折りたたみを解除して見せる
        if (hasSelected) {
          collapsedDocGroups.delete(entry.info.key);
        }
        const isCollapsed = collapsedDocGroups.has(entry.info.key);

        const chevronEl = h('span', { class: 'repo-doc-group-chevron' }, isCollapsed ? '▸' : '▾');

        const groupEl = h(
          'div',
          {
            class: `repo-doc-group ${isCollapsed ? 'collapsed' : ''}`,
            style: `background: color-mix(in srgb, ${color} 7%, var(--panel)); border-color: color-mix(in srgb, ${color} 28%, var(--line));`,
          },
          h(
            'div',
            {
              class: 'repo-doc-group-header',
              title: `${entry.info.name}（クリックで折りたたみ）`,
              style: `color: color-mix(in srgb, ${color} 80%, var(--text));`,
              onclick: () => {
                if (groupEl.classList.contains('collapsed')) {
                  groupEl.classList.remove('collapsed');
                  collapsedDocGroups.delete(entry.info.key);
                  chevronEl.textContent = '▾';
                } else {
                  groupEl.classList.add('collapsed');
                  collapsedDocGroups.add(entry.info.key);
                  chevronEl.textContent = '▸';
                }
              },
            },
            chevronEl,
            h('span', { class: 'repo-doc-group-icon' }, '📁'),
            h('span', { class: 'repo-doc-group-name' }, entry.info.name),
            entry.info.subPath !== entry.info.name ? h('span', { class: 'repo-doc-group-sub' }, entry.info.subPath) : null,
            h('span', { class: 'repo-doc-group-count', style: `background: color-mix(in srgb, ${color} 18%, transparent); color: ${color};` }, String(entry.repos.length)),
          ),
          h(
            'div',
            { class: 'repo-doc-group-items' },
            ...entry.repos.map((r) => renderItem(r, entry.info.relPath(r.path))),
          ),
        );
        listEl.append(groupEl);
      }
    }
  }

  let addMenuOpen = false;
  let addMenuCleanup: (() => void) | null = null;

  const menuDropdown = h(
    'div',
    { class: 'add-repo-dropdown', style: 'display: none;' },
    h(
      'button',
      {
        class: 'add-repo-dropdown-item',
        onclick: (e: Event) => {
          e.stopPropagation();
          closeAddMenu();
          acts.onAdd();
        },
      },
      h('span', { class: 'add-dropdown-icon' }, '📁'),
      h('span', { class: 'add-dropdown-text' }, lang === 'ja' ? 'フォルダ（Gitリポジトリ）…' : 'Folder (Git Repository)…'),
    ),
    acts.onAddFile
      ? h(
          'button',
          {
            class: 'add-repo-dropdown-item',
            onclick: (e: Event) => {
              e.stopPropagation();
              closeAddMenu();
              acts.onAddFile!();
            },
          },
          h('span', { class: 'add-dropdown-icon' }, '📄'),
          h('span', { class: 'add-dropdown-text' }, lang === 'ja' ? '文書ファイル（.md / .txt）…' : 'Document (.md / .txt)…'),
        )
      : null,
  );

  const closeAddMenu = () => {
    addMenuOpen = false;
    menuDropdown.style.display = 'none';
    if (addMenuCleanup) {
      addMenuCleanup();
      addMenuCleanup = null;
    }
  };

  const toggleAddMenu = (e: Event) => {
    e.stopPropagation();
    if (addBlocked !== null) return;
    if (addMenuOpen) {
      closeAddMenu();
    } else {
      addMenuOpen = true;
      menuDropdown.style.display = 'flex';
      const onDocClick = (evt: MouseEvent) => {
        if (!menuDropdown.contains(evt.target as Node)) {
          closeAddMenu();
        }
      };
      setTimeout(() => window.addEventListener('click', onDocClick, { once: true }), 0);
      addMenuCleanup = () => window.removeEventListener('click', onDocClick);
    }
  };

  return h(
    'section',
    { class: 'repo-sidebar-section' },
    h(
      'div',
      { class: 'repo-sidebar-header' },
      h('h2', {}, m.heading),
      h(
        'div',
        { class: 'repo-add-container', style: 'position: relative;' },
        h(
          'button',
          {
            class: 'add-repo-btn',
            disabled: addBlocked !== null,
            title: addBlocked ?? m.addTitle,
            onclick: acts.onAddFile ? toggleAddMenu : () => acts.onAdd(),
          },
          m.addBtn,
        ),
        acts.onAddFile ? menuDropdown : null,
      ),
    ),
    listEl,
  );
}
