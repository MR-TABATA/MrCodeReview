// 「はい／キャンセル」の確認。**`window.confirm` は使わない。**
//
// `tauri-plugin-dialog` が `window.confirm` を `async function`（Promise を返す関数）に差し替えている。
// Promise は常に真なので、`if (!window.confirm(...))` は、答えを待たずに、いつも「はい」で進んでしまう。
// 確認は、ここの `confirmDialog`（答えを待つ）を使う。

import { confirm } from '@tauri-apps/plugin-dialog';

// ダイアログの既定の見出し。アプリの名前（殻で違う）は、起動時に Rust から読んで入れる（読めるまでは、Free 殻の名前）。
let appName = 'MrCodeReview';
export function setAppName(name: string): void {
  if (name) appName = name;
}

/** 「はい」のときだけ true。キャンセル・閉じる・ダイアログを出せなかったときは false（確認が取れなければ、進まない）。 */
export async function confirmDialog(message: string, opts: { title?: string; okLabel?: string } = {}): Promise<boolean> {
  try {
    return (await confirm(message, { title: opts.title ?? appName, kind: 'warning', okLabel: opts.okLabel ?? 'OK', cancelLabel: 'キャンセル' })) === true;
  } catch {
    return false;
  }
}
