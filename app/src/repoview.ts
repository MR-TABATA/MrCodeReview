import { h } from './dom';
import { getAppLang, msg, type Lang } from './i18n';
import { repoDisplayName, type RegisteredRepo } from './reporegistry';

export interface RepoViewActions {
  onSelect(repo: RegisteredRepo): void;
  onAdd(): void;
  onRemove(repo: RegisteredRepo): void;
  onRename(repo: RegisteredRepo): void;
}

const BG_PALETTE = [
  '#4f46e5', '#2563eb', '#0891b2', '#059669',
  '#d97706', '#dc2626', '#7c3aed', '#db2777',
];

/** 「未」の指摘がこの件数以上のリポジトリは、件数のバッジを強く（塗りつぶして）見せる */
export const OPEN_STRONG_FROM = 20;

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

export function renderAppIcon(title: string, iconUrl?: string | null): HTMLElement {
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
    for (const r of repos) {
      const isSelected = r.path === selectedPath;
      const title = repoDisplayName(r);
      const isCustomName = !!r.name?.trim();
      const iconUrl = iconMap[r.path];

      const item = h(
        'div',
        {
          class: `repo-item ${isSelected ? 'active' : ''}`,
          onclick: () => acts.onSelect(r),
        },
        h(
          'div',
          { class: 'repo-item-main' },
          h('span', { class: 'repo-item-icon', title: m.appIconTitle(title) }, renderAppIcon(title, iconUrl)),
          h(
            'div',
            { class: 'repo-item-text' },
            h('div', { class: 'repo-item-name' }, title),
            h('div', { class: 'repo-item-path', title: r.path }, r.path),
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
        // 件数は、行の右端（ホバーで出る編集・解除ボタンの、さらに右）
        (openCounts[r.path] ?? 0) > 0
          ? h('span', { class: `repo-open-badge ${openCounts[r.path] >= OPEN_STRONG_FROM ? 'strong' : ''}`, title: m.openCountTitle(openCounts[r.path]) }, String(openCounts[r.path]))
          : null,
      );
      listEl.append(item);
    }
  }

  return h(
    'section',
    { class: 'repo-sidebar-section' },
    h(
      'div',
      { class: 'repo-sidebar-header' },
      h('h2', {}, m.heading),
      h('button', { class: 'add-repo-btn', disabled: addBlocked !== null, title: addBlocked ?? m.addTitle, onclick: () => acts.onAdd() }, m.addBtn),
    ),
    listEl,
  );
}
