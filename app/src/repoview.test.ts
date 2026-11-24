// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { appInitials, OPEN_STRONG_FROM, renderAppIcon, repoListPanel, resolveDocGroup } from './repoview';
import type { RegisteredRepo } from './reporegistry';

describe('appInitials & renderAppIcon', () => {
  it('アプリ名から適切なイニシャルを抽出する', () => {
    expect(appInitials('MRDown')).toBe('MR');
    expect(appInitials('MrCodeReview')).toBe('MC');
    expect(appInitials('SyncVey')).toBe('SV');
    expect(appInitials('app')).toBe('AP');
  });

  it('iconUrl があれば img 要素を、無ければバッジ要素を返す', () => {
    const imgEl = renderAppIcon('MRDown', 'data:image/png;base64,abc');
    expect(imgEl.tagName).toBe('IMG');
    expect((imgEl as HTMLImageElement).src).toBe('data:image/png;base64,abc');

    const badgeEl = renderAppIcon('MRDown', null);
    expect(badgeEl.tagName).toBe('SPAN');
    expect(badgeEl.textContent).toBe('MR');
  });

  it('isDoc が true のときはファイルアイコン（svg）を返す', () => {
    const docEl = renderAppIcon('daemon.md', null, true);
    expect(docEl.tagName).toBe('SPAN');
    expect(docEl.classList.contains('repo-app-icon-doc')).toBe(true);
    expect(docEl.querySelector('svg')).not.toBeNull();
  });
});

describe('repoListPanel', () => {
  const acts = () => ({
    onSelect: vi.fn(),
    onAdd: vi.fn(),
    onRemove: vi.fn(),
    onRename: vi.fn(),
  });

  it('「+ 追加」: 押せる理由が無ければ押せる。枠がいっぱいなどで押せないときは、無効にして、理由を見せる', () => {
    const a = acts();
    const open = repoListPanel([], '', a).querySelector('.add-repo-btn') as HTMLButtonElement;
    expect(open.disabled).toBe(false);
    open.click();
    expect(a.onAdd).toHaveBeenCalledOnce();

    const b = acts();
    const blocked = repoListPanel([], '', b, {}, '枠がいっぱいです（3 / 3）').querySelector('.add-repo-btn') as HTMLButtonElement;
    expect(blocked.disabled).toBe(true);
    expect(blocked.title).toBe('枠がいっぱいです（3 / 3）');
  });

  it('「未」の指摘がある登録リポジトリにだけ、件数のバッジを出す（0 件・未取得は出さない）', () => {
    const a = acts();
    const repos = [
      { id: '1', path: '/r/a' },
      { id: '2', path: '/r/b' },
      { id: '3', path: '/r/c' },
    ];
    const el = repoListPanel(repos, '', a, {}, null, undefined, { '/r/a': 7, '/r/b': 0 });
    const badges = [...el.querySelectorAll('.repo-open-badge')];
    expect(badges.map((b) => b.textContent)).toEqual(['7']);
    expect(badges[0].getAttribute('title')).toContain('7');
    expect(el.querySelectorAll('.repo-item')[0].contains(badges[0])).toBe(true);
    // 件数を渡さなければ、これまでどおり、バッジは無い
    expect(repoListPanel(repos, '', a).querySelectorAll('.repo-open-badge')).toHaveLength(0);
  });

  it('件数のバッジは、20 件以上のときだけ strong（塗りつぶし）。19 件までは、数字だけ', () => {
    expect(OPEN_STRONG_FROM).toBe(20);
    const repos = [{ id: '1', path: '/r/a' }, { id: '2', path: '/r/b' }, { id: '3', path: '/r/c' }];
    const el = repoListPanel(repos, '', acts(), {}, null, undefined, { '/r/a': 19, '/r/b': 20, '/r/c': 135 });
    const badges = [...el.querySelectorAll('.repo-open-badge')];
    expect(badges.map((b) => [b.textContent, b.classList.contains('strong')])).toEqual([['19', false], ['20', true], ['135', true]]);
  });

  it('リポジトリが空のときは案内を表示する', () => {
    const a = acts();
    const el = repoListPanel([], '', a);
    expect(el.textContent).toContain('登録されたリポジトリがありません');
  });

  it('登録リポジトリの一覧を表示し、選択中のリポジトリに active クラスが付く', () => {
    const a = acts();
    const repos = [
      { id: '1', path: '/Users/me/Git/MRDown' },
      { id: '2', path: '/Users/me/Git/MrCodeReview', name: 'レビュア' },
    ];
    const el = repoListPanel(repos, '/Users/me/Git/MRDown', a);
    const items = el.querySelectorAll('.repo-item');
    expect(items).toHaveLength(2);
    expect(items[0].classList.contains('active')).toBe(true);
    expect(items[0].textContent).toContain('MRDown');
    expect(items[1].classList.contains('active')).toBe(false);
    expect(items[1].textContent).toContain('レビュア');
  });

  it('クリックで onSelect が呼ばれる', () => {
    const a = acts();
    const repos = [{ id: '1', path: '/repo/one' }];
    const el = repoListPanel(repos, '', a);
    const item = el.querySelector('.repo-item') as HTMLElement;
    item.click();
    expect(a.onSelect).toHaveBeenCalledWith(repos[0]);
  });

  it('追加ボタンで onAdd が呼ばれる', () => {
    const a = acts();
    const el = repoListPanel([], '', a);
    const addBtn = el.querySelector('.add-repo-btn') as HTMLElement;
    addBtn.click();
    expect(a.onAdd).toHaveBeenCalled();
  });

  it('削除ボタンで onRemove が呼ばれ、onSelect は伝播しない', () => {
    const a = acts();
    const repos = [{ id: '1', path: '/repo/one' }];
    const el = repoListPanel(repos, '', a);
    const delBtn = el.querySelector('.repo-action-btn.delete') as HTMLElement;
    delBtn.click();
    expect(a.onRemove).toHaveBeenCalledWith(repos[0]);
    expect(a.onSelect).not.toHaveBeenCalled();
  });

  it('Specなどのリポジトリ単位で文書ファイルがグルーピングされ、囲み枠に色が付く', () => {
    const a = acts();
    const repos = [
      { id: '1', path: '/Users/me/Git/Spec/SyncLane/daemon.md' },
      { id: '2', path: '/Users/me/Git/Spec/SyncLane/auth.md' },
      { id: '3', path: '/Users/me/Git/Spec/MCR/doc-file-review.md' },
      { id: '4', path: '/Users/me/Git/MrCodeReview' }, // 別リポジトリ
    ];
    const el = repoListPanel(repos, '/Users/me/Git/Spec/SyncLane/daemon.md', a);

    // .repo-item は全 4 件存在
    const items = el.querySelectorAll('.repo-item');
    expect(items).toHaveLength(4);

    // グループ枠は Spec の 1 つにまとまる（SyncLane と MCR の両方の配下ファイルが含まれる）
    const groups = el.querySelectorAll('.repo-doc-group');
    expect(groups).toHaveLength(1);

    // グループ名は Spec、3 件の文書が所属
    expect(groups[0].querySelector('.repo-doc-group-name')?.textContent).toBe('Spec');
    expect(groups[0].querySelectorAll('.repo-item')).toHaveLength(3);
    expect((groups[0] as HTMLElement).style.background).toContain('color-mix');

    // グループ内の表示パスは Spec からの相対パス（SyncLane/daemon.md 等）になる
    const docPaths = [...groups[0].querySelectorAll('.repo-item-path')].map((p) => p.textContent);
    expect(docPaths).toEqual(['SyncLane/daemon.md', 'SyncLane/auth.md', 'MCR/doc-file-review.md']);

    // 通常リポジトリ（MrCodeReview）はグループの外に直接配置される
    const directItems = el.querySelectorAll(':scope > .repo-list > .repo-item');
    expect(directItems).toHaveLength(1);
    expect(directItems[0].textContent).toContain('MrCodeReview');

    // ヘッダークリックで折りたたみ／展開がトグルできる
    const header = groups[0].querySelector('.repo-doc-group-header') as HTMLElement;
    const chevron = groups[0].querySelector('.repo-doc-group-chevron') as HTMLElement;
    expect(groups[0].classList.contains('collapsed')).toBe(false);
    expect(chevron.textContent).toBe('▾');

    // 1 回目クリック: 折りたたみ
    header.click();
    expect(groups[0].classList.contains('collapsed')).toBe(true);
    expect(chevron.textContent).toBe('▸');

    // 2 回目クリック: 再展開
    header.click();
    expect(groups[0].classList.contains('collapsed')).toBe(false);
    expect(chevron.textContent).toBe('▾');
  });

  it('resolveDocGroup は 3 つの規則（登録リポジトリ配下、パスパターン、親ディレクトリ）と入れ子の優先度を正しく判定する', () => {
    // 規則 1: 登録済みリポジトリの配下にある文書
    const regRepos: RegisteredRepo[] = [
      { id: '1', path: '/a/proj', name: 'MyProject' },
      { id: '2', path: '/a/proj/sub', name: 'SubProject' },
    ];
    // 深いほう（/a/proj/sub）が優先される
    const gSub = resolveDocGroup('/a/proj/sub/docs/x.md', regRepos);
    expect(gSub.name).toBe('SubProject');
    expect(gSub.relPath('/a/proj/sub/docs/x.md')).toBe('docs/x.md');

    // 親の /a/proj に該当
    const gProj = resolveDocGroup('/a/proj/docs/y.md', regRepos);
    expect(gProj.name).toBe('MyProject');
    expect(gProj.relPath('/a/proj/docs/y.md')).toBe('docs/y.md');

    // 規則 2: パスパターン（Git/<Name>）
    const gGit = resolveDocGroup('/Users/test/workspace/SyncVey/notes/todo.md', []);
    expect(gGit.name).toBe('SyncVey');
    expect(gGit.relPath('/Users/test/workspace/SyncVey/notes/todo.md')).toBe('notes/todo.md');

    // 規則 3: どれにも当たらない一般パス（親ディレクトリ名）
    const gFallback = resolveDocGroup('/var/log/notes/today.txt', []);
    expect(gFallback.name).toBe('notes');
    expect(gFallback.relPath('/var/log/notes/today.txt')).toBe('today.txt');
  });
});
