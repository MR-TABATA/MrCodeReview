// メニューの「アップデートを確認…」の流れ。通信は Rust（`update_check` / `update_install`）が行う。
//
// - 押したときだけ確かめる（起動時には、問い合わせない）。
// - 確認できなかったとき（ネットワーク・署名・サーバ）は、黙らず、理由を出す。「最新です」と区別する。
// - 更新を入れる前に、必ず、聞く（入れると、アプリが再起動する）。

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string | null;
}

export interface UpdateDeps {
  check(): Promise<UpdateInfo | null>;
  /** 「更新する」なら true */
  confirm(info: UpdateInfo): Promise<boolean>;
  /** 入れて、再起動する（成功すると、この関数の先には戻らない） */
  install(): Promise<void>;
  notify(text: string, kind: 'ok' | 'err'): Promise<void> | void;
}

export type UpdateResult = 'latest' | 'declined' | 'installed' | 'error';

export function confirmText(info: UpdateInfo): string {
  const notes = info.notes?.trim();
  return [
    `新しい版 ${info.version} があります（いまは ${info.current}）。`,
    notes ? `\n${notes.length > 300 ? `${notes.slice(0, 300)}…` : notes}` : '',
    '\nダウンロードして入れ替え、アプリを再起動します。実行中のレビューは、止まります。',
  ].join('');
}

export async function runUpdateCheck(d: UpdateDeps): Promise<UpdateResult> {
  let info: UpdateInfo | null;
  try {
    info = await d.check();
  } catch (e) {
    await d.notify(String(e), 'err');
    return 'error';
  }
  if (!info) {
    await d.notify('いまの版は、最新です。', 'ok');
    return 'latest';
  }
  if (!(await d.confirm(info))) return 'declined';
  try {
    await d.install();
  } catch (e) {
    await d.notify(String(e), 'err');
    return 'error';
  }
  return 'installed';
}
