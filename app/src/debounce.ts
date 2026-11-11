/**
 * 続けて呼ばれても、最後の呼び出しの `ms` ミリ秒あとに、1 回だけ実行する。
 * 仕分けを続けて押したとき、履歴の読み直し（件数の再集計）を、押すたびに走らせないために使う。
 */
export interface Debounced {
  /** 実行を予約する（予約済みなら、待ち時間を数え直す） */
  call(): void;
  /** 予約を取り消す */
  cancel(): void;
}

export function debounced(fn: () => void, ms: number): Debounced {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    call() {
      if (timer !== undefined) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = undefined;
        fn();
      }, ms);
    },
    cancel() {
      if (timer !== undefined) clearTimeout(timer);
      timer = undefined;
    },
  };
}
