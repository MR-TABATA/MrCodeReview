// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { appInitials, OPEN_STRONG_FROM, renderAppIcon, repoListPanel } from './repoview';

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
});
