// 識別子。エージェントの報告（`mrcr:<レビュー>#<番号> 対応した: …`）と、画面の指摘を、照合するための印。
//
// - 番号は、レビューの中での、指摘の元の位置（1 始まり）。並べ替えや仕分けで、変わらない。
// - 書式は、Rust（`triage.rs` の `review_tag` / `finding_tag`）と同じ。食い違うと、報告を取り込めない（テストで、書式を照合している）。

import { h } from './dom';
import { getAppLang, msg } from './i18n';

export const reviewTag = (reviewId: string): string => `mrcr:${reviewId}`;

/** 指摘の識別子。`index` は 0 始まり（画面での、指摘の元の位置）。 */
export const findingTag = (reviewId: string, index: number): string => `${reviewTag(reviewId)}#${index + 1}`;

/** カードの見出しに出す番号の印（`#3`）。ホバーで、識別子の全文が出る。 */
export function findingNo(index: number, tag: string): HTMLElement {
  const m = msg(getAppLang()).mainUi;
  return h('span', { class: 'fc-no', title: `${m.idLabel}: ${tag}` }, `#${index + 1}`);
}

/** 開いたカードの先頭の、識別子の全文と「コピー」。報告と照合したり、貼り付けたりできる。 */
export function tagLine(tag: string, copy: (text: string) => Promise<boolean>): HTMLElement {
  const m = msg(getAppLang()).mainUi;
  const btn = h('button', { class: 'small fc-tag-copy', type: 'button' }, m.copyBtn);
  btn.addEventListener('click', () => {
    void copy(tag).then((ok) => {
      const mNow = msg(getAppLang()).mainUi;
      btn.textContent = ok ? mNow.copiedBtn : mNow.copyFailedBtn;
      setTimeout(() => {
        btn.textContent = msg(getAppLang()).mainUi.copyBtn;
      }, 1500);
    });
  });
  return h('div', { class: 'fc-tag' }, h('span', { class: 'f-ev-label' }, m.idLabel), h('code', {}, tag), btn);
}
