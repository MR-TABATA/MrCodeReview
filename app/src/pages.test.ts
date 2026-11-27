// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { DEFAULT_PAGE, PAGES, pageTabs, type PageTabLabels } from './pages';

const labels: PageTabLabels = { nav: 'ページ', repos: 'リポジトリ', history: '履歴', log: 'ログ', newreview: '新規レビュー' };

describe('上部タブ', () => {
  it('並びは、リポジトリ・履歴・ログ・新規レビュー。観点のタブは、まだ出さない。結果は、タブではない', () => {
    expect(PAGES).toEqual(['repos', 'history', 'log', 'newreview']);
    const el = pageTabs('newreview', labels, vi.fn());
    expect([...el.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['リポジトリ', '履歴', 'ログ', '新規レビュー']);
  });

  it('起動時のページは、実行する場所の「新規レビュー」', () => {
    expect(DEFAULT_PAGE).toBe('newreview');
  });

  it('いまのページだけが、選ばれた状態になる', () => {
    const el = pageTabs('log', labels, vi.fn());
    const on = [...el.querySelectorAll('button')].filter((b) => b.getAttribute('aria-selected') === 'true');
    expect(on.map((b) => b.dataset.page)).toEqual(['log']);
    expect(on[0].classList.contains('on')).toBe(true);
  });

  it('current が「結果」のときも、どのタブも選ばれない（結果は、タブではない）', () => {
    const el = pageTabs('result', labels, vi.fn());
    expect(el.querySelectorAll('[aria-selected="true"]').length).toBe(0);
  });

  it('current が null（設定・レポート・使用量を開いているとき）なら、どれも選ばれない', () => {
    const el = pageTabs(null, labels, vi.fn());
    expect(el.querySelectorAll('[aria-selected="true"]').length).toBe(0);
    expect(el.querySelectorAll('.on').length).toBe(0);
  });

  it('押したタブの、ページが、渡される（いまのページを押しても渡る）', () => {
    const pick = vi.fn();
    const el = pageTabs('newreview', labels, pick);
    (el.querySelector('[data-page="history"]') as HTMLElement).click();
    (el.querySelector('[data-page="newreview"]') as HTMLElement).click();
    expect(pick.mock.calls.map((c) => c[0])).toEqual(['history', 'newreview']);
  });
});
