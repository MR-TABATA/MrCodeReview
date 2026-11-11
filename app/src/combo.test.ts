// @vitest-environment jsdom
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { combo, closeCombo, comboIsOpen, type ComboItem } from './combo';

const ITEMS: ComboItem[] = [
  { value: 'bugs', title: '潜在バグ', desc: '壊れる書き方を探す', group: 'LLM でレビュー' },
  { value: 'sec', title: 'セキュリティ', desc: '穴を探す', group: 'LLM でレビュー', badge: 'このリポジトリ' },
  { value: 'lint', title: 'リンター', desc: '全部まとめて実行', group: 'LLM を使わない' },
];

/** 画面全体を作り直す本番と同じく、rerender のたびに部品を作り直して、同じ場所に差し替える。 */
function mount(value = 'bugs') {
  const host = document.createElement('div');
  document.body.append(host);
  const onChange = vi.fn();
  const draw = () => host.replaceChildren(combo({ items: ITEMS, value, onChange, rerender: draw }));
  draw();
  return { host, onChange, draw };
}

beforeEach(() => {
  closeCombo();
  document.body.replaceChildren();
});

describe('combo', () => {
  it('閉じているときは、いまの項目の名前と説明だけを出す', () => {
    const { host } = mount('sec');
    expect(host.querySelector('.combo-title')?.textContent).toContain('セキュリティ');
    expect(host.querySelector('.combo-desc')?.textContent).toBe('穴を探す');
    expect(host.querySelector('.combo-pop')).toBeNull();
    expect(host.querySelector('.combo-btn')?.getAttribute('aria-expanded')).toBe('false');
  });

  it('押すと開き、グループ名と、項目ごとの説明が出る。いまの項目に印が付く', () => {
    const { host } = mount('sec');
    (host.querySelector('.combo-btn') as HTMLElement).click();
    expect(comboIsOpen()).toBe(true);
    expect([...host.querySelectorAll('.combo-group')].map((g) => g.textContent)).toEqual(['LLM でレビュー', 'LLM を使わない']);
    const opts = [...host.querySelectorAll('.combo-opt')];
    expect(opts).toHaveLength(3);
    expect(opts[1]?.getAttribute('aria-selected')).toBe('true');
    expect(opts[1]?.textContent).toContain('このリポジトリ');
    expect(opts[2]?.querySelector('.combo-desc')?.textContent).toBe('全部まとめて実行');
    expect(host.querySelector('.combo-btn')?.getAttribute('aria-expanded')).toBe('true');
  });

  it('項目を押すと、その値で onChange を呼び、閉じる', () => {
    const { host, onChange } = mount();
    (host.querySelector('.combo-btn') as HTMLElement).click();
    (host.querySelectorAll('.combo-opt')[2] as HTMLElement).click();
    expect(onChange).toHaveBeenCalledExactlyOnceWith('lint');
    expect(comboIsOpen()).toBe(false);
  });

  it('キー操作: ↓ で動かして Enter で決める。Esc で、選ばずに閉じる', () => {
    const { host, onChange } = mount('bugs');
    (host.querySelector('.combo-btn') as HTMLElement).click();
    const key = (k: string) => host.querySelector('.combo-pop')!.dispatchEvent(new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true }));
    key('ArrowDown');
    key('ArrowDown');
    expect(host.querySelector('.combo-opt.active')?.getAttribute('data-value')).toBe('lint');
    key('ArrowDown'); // 最後の次は、先頭に戻る
    expect(host.querySelector('.combo-opt.active')?.getAttribute('data-value')).toBe('bugs');
    key('ArrowUp'); // 先頭の前は、最後に戻る
    key('Enter');
    expect(onChange).toHaveBeenCalledExactlyOnceWith('lint');

    (host.querySelector('.combo-btn') as HTMLElement).click();
    key('Escape');
    expect(comboIsOpen()).toBe(false);
    expect(onChange).toHaveBeenCalledOnce();
  });

  it('外を押すと閉じる。中を押しても閉じない', () => {
    const { host } = mount();
    (host.querySelector('.combo-btn') as HTMLElement).click();
    host.querySelector('.combo-pop')!.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(comboIsOpen()).toBe(true);
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(comboIsOpen()).toBe(false);
    expect(host.querySelector('.combo-pop')).toBeNull();
  });

  it('2 つ並べられる。1 つを開くと、もう 1 つは閉じ、それぞれの状態を別に持つ', () => {
    const host = document.createElement('div');
    document.body.append(host);
    const a = vi.fn();
    const b = vi.fn();
    const draw = () =>
      host.replaceChildren(
        combo({ id: 'a', items: ITEMS, value: 'bugs', onChange: a, rerender: draw }),
        combo({ id: 'b', items: ITEMS, value: 'lint', onChange: b, rerender: draw, compact: true, prefix: '比較元' }),
      );
    draw();
    const btns = () => [...host.querySelectorAll('.combo-btn')] as HTMLElement[];
    expect(host.querySelectorAll('.combo')[1]?.classList.contains('compact')).toBe(true);
    expect(btns()[1]?.querySelector('.combo-prefix')?.textContent).toBe('比較元');
    expect(btns()[1]?.querySelector('.combo-desc'), '小さい見た目では、説明をボタンに出さない').toBeNull();
    btns()[0]!.click();
    expect([comboIsOpen('a'), comboIsOpen('b')]).toEqual([true, false]);
    btns()[1]!.click();
    expect([comboIsOpen('a'), comboIsOpen('b')]).toEqual([false, true]);
    expect(host.querySelectorAll('.combo-pop')).toHaveLength(1);
    (host.querySelectorAll('.combo-opt')[0] as HTMLElement).click();
    expect(b).toHaveBeenCalledExactlyOnceWith('bugs');
    expect(a).not.toHaveBeenCalled();
    expect([comboIsOpen('a'), comboIsOpen('b')]).toEqual([false, false]);
  });

  it('無効のときは、開けない', () => {
    const host = document.createElement('div');
    document.body.append(host);
    host.append(combo({ items: ITEMS, value: 'bugs', onChange: vi.fn(), rerender: vi.fn(), disabled: true }));
    (host.querySelector('.combo-btn') as HTMLElement).click();
    expect(comboIsOpen()).toBe(false);
  });
});
