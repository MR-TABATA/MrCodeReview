import { h } from './dom';

/**
 * 上部タブで切り替える、1 ページ。
 * 設定・レポート・使用量は、ページではなく、これまでどおり、ヘッダーのボタンで開く「モード」（ページ領域を丸ごと使う）。
 *  */
/** 上部タブに並ぶページ。 */
export type TabPage = 'repos' | 'aspects' | 'history' | 'newreview';
/**
 * ページ。タブで切り替える 4 つ（リポジトリ・観点・履歴・ログ・新規レビュー）と、「結果」。
 * 「結果」は、タブではない。履歴・ログの行の下に開いた結果から、「ページで開く」で移る（タブは、履歴・ログのまま。戻るボタンで、履歴・ログへ戻る）。
 */
export type Page = TabPage | 'result';

/** タブの並び。「結果」は、タブではなく、履歴・ログ・実行直後から掘り下げて開くページ（戻るボタンで、来たページへ戻る）。 */
export const PAGES: readonly TabPage[] = ['repos', 'aspects', 'history', 'newreview'];

/** 起動時に開くページ。新しいレビューを実行する場所。 */
export const DEFAULT_PAGE: Page = 'newreview';

export interface PageTabLabels {
  /** タブ全体の名前（読み上げ用） */
  nav: string;
  repos: string;
  aspects: string;
  history: string;
  newreview: string;
}

/** 上部タブ。押したページを `pick` で受け取る。いまのページは `on` と `aria-selected`。`current` が null のときは、どれも「いま」にしない。 */
export function pageTabs(current: Page | null, labels: PageTabLabels, pick: (page: TabPage) => void): HTMLElement {
  return h(
    'nav',
    { class: 'pagetabs', role: 'tablist', 'aria-label': labels.nav },
    ...PAGES.map((p) =>
      h(
        'button',
        {
          class: `pagetab ${p === current ? 'on' : ''}`,
          role: 'tab',
          'data-page': p,
          'aria-selected': p === current ? 'true' : 'false',
          onclick: () => pick(p),
        },
        labels[p],
      ),
    ),
  );
}
