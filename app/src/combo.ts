// 「かっこいい」コンボボックス。項目ごとに、1 行の説明を添える。ネイティブの <select> は、説明を出せないので使わない。
//
// 画面全体を作り直す作りなので、開いているか・どの項目にカーソルがあるかは、この部品の外（モジュールの状態）に、`id` ごとに持つ。
// 画面に複数並べられる（1 つを開くと、ほかは閉じる）。
// キー操作: ↑↓ で動かす・Enter で決める・Esc で閉じる。外を押しても閉じる。

import { h } from './dom';
import { icon } from './icons';

export interface ComboItem {
  value: string;
  title: string;
  desc: string;
  group?: string;
  badge?: string;
}

export interface ComboOptions {
  items: ComboItem[];
  value: string;
  onChange: (value: string) => void;
  /** 開閉・カーソルの移動のあとに、画面を作り直す関数 */
  rerender: () => void;
  disabled?: boolean;
  placeholder?: string;
  /** 画面に複数並べるときの名前。省略すると `default`。 */
  id?: string;
  /** ヘッダなどに置く、小さい見た目（説明はボタンに出さない）。 */
  compact?: boolean;
  /** ボタンの先頭に、薄く出す言葉（「比較元」など）。 */
  prefix?: string;
}

interface State {
  open: boolean;
  active: number;
}
const states = new Map<string, State>();
function stateOf(id: string): State {
  let s = states.get(id);
  if (!s) {
    s = { open: false, active: 0 };
    states.set(id, s);
  }
  return s;
}
let latestRerender: (() => void) | null = null;
let listening = false;

export function comboIsOpen(id = 'default'): boolean {
  return stateOf(id).open;
}

/** `id` を渡すとそれを、省略するとすべてを閉じる。 */
export function closeCombo(id?: string): void {
  if (id) stateOf(id).open = false;
  else states.forEach((s) => (s.open = false));
}

/** 外を押したら閉じる（一度だけ登録する）。 */
function listenOutside(): void {
  if (listening) return;
  listening = true;
  document.addEventListener('mousedown', (e) => {
    if ([...states.values()].some((s) => s.open) && !(e.target as Element | null)?.closest?.('.combo')) {
      closeCombo();
      latestRerender?.();
    }
  });
}

export function combo(o: ComboOptions): HTMLElement {
  latestRerender = o.rerender;
  listenOutside();
  const id = o.id ?? 'default';
  const st = stateOf(id);
  const idx = Math.max(0, o.items.findIndex((i) => i.value === o.value));
  const cur = o.items.find((i) => i.value === o.value);
  if (st.active >= o.items.length) st.active = idx;

  const open = () => {
    closeCombo(); // 1 つを開くと、ほかは閉じる
    st.open = true;
    st.active = idx;
    o.rerender();
  };
  const close = () => {
    st.open = false;
    o.rerender();
  };
  const choose = (i: number) => {
    st.open = false;
    const it = o.items[i];
    if (it) o.onChange(it.value);
    else o.rerender();
  };

  const btn = h(
    'button',
    {
      class: 'combo-btn',
      type: 'button',
      'aria-haspopup': 'listbox',
      'aria-expanded': st.open ? 'true' : 'false',
      ...(o.disabled ? { disabled: true } : {}),
      onclick: () => (st.open ? close() : open()),
      onkeydown: (e) => {
        const k = (e as KeyboardEvent).key;
        if (k === 'ArrowDown' || k === 'ArrowUp') {
          e.preventDefault();
          if (!st.open) open();
        }
      },
    },
    o.prefix ? h('span', { class: 'combo-prefix' }, o.prefix) : null,
    h(
      'span',
      { class: 'combo-cur' },
      h('span', { class: 'combo-title' }, h('span', { class: 'combo-name' }, cur?.title ?? o.placeholder ?? '選んでください')),
      cur?.desc && !o.compact ? h('span', { class: 'combo-desc' }, cur.desc) : null,
    ),
    icon('chevron', 16),
  );

  let pop: HTMLElement | null = null;
  if (st.open) {
    let lastGroup: string | undefined;
    const kids: HTMLElement[] = [];
    o.items.forEach((it, i) => {
      if (it.group && it.group !== lastGroup) {
        kids.push(h('div', { class: 'combo-group', role: 'presentation' }, it.group));
        lastGroup = it.group;
      }
      kids.push(
        h(
          'div',
          {
            class: `combo-opt ${i === st.active ? 'active' : ''} ${it.value === o.value ? 'on' : ''}`,
            role: 'option',
            'aria-selected': it.value === o.value ? 'true' : 'false',
            'data-value': it.value,
            onclick: () => choose(i),
            onmousemove: () => {
              if (st.active !== i) {
                st.active = i;
                // 画面を作り直さず、見た目だけ更新する（作り直すと、押した瞬間の click を取り逃がす）
                pop?.querySelectorAll('.combo-opt').forEach((el, n) => el.classList.toggle('active', n === i));
              }
            },
          },
          h(
            'span',
            { class: 'combo-opt-main' },
            h('span', { class: 'combo-title' }, h('span', { class: 'combo-name' }, it.title), it.badge ? h('span', { class: 'badge repo combo-badge' }, it.badge) : null),
            it.desc ? h('span', { class: 'combo-desc' }, it.desc) : null,
          ),
          it.value === o.value ? icon('check', 16) : null,
        ),
      );
    });
    pop = h(
      'div',
      {
        class: 'combo-pop',
        role: 'listbox',
        tabindex: '-1',
        onkeydown: (e) => {
          const k = (e as KeyboardEvent).key;
          const n = o.items.length;
          if (k === 'ArrowDown' || k === 'ArrowUp') {
            e.preventDefault();
            st.active = (st.active + (k === 'ArrowDown' ? 1 : -1) + n) % n;
            o.rerender();
          } else if (k === 'Enter') {
            e.preventDefault();
            choose(st.active);
          } else if (k === 'Escape') {
            e.preventDefault();
            close();
          }
        },
      },
      ...kids,
    );
    const p = pop;
    // 作り直した直後に、キー操作を受けられるよう、フォーカスを移す
    queueMicrotask(() => p.focus({ preventScroll: true }));
  }
  return h('div', { class: `combo ${o.compact ? 'compact' : ''}` }, btn, pop);
}
