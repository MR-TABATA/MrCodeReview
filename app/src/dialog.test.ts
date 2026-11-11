import { describe, it, expect, vi, beforeEach } from 'vitest';

const confirmMock = vi.fn();
vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: (...a: unknown[]) => confirmMock(...a) }));

import { confirmDialog, setAppName } from './dialog';

describe('confirmDialog（答えを待つ確認）', () => {
  beforeEach(() => {
    confirmMock.mockReset();
  });

  it('「OK」のときだけ true。メッセージと、日本語のボタン名を渡す', async () => {
    confirmMock.mockResolvedValue(true);
    expect(await confirmDialog('消しますか？')).toBe(true);
    expect(confirmMock).toHaveBeenCalledWith('消しますか？', expect.objectContaining({ okLabel: 'OK', cancelLabel: 'キャンセル', kind: 'warning' }));
  });

  it('見出しと「はい」のボタン名を、変えられる（キャンセルは固定）', async () => {
    confirmMock.mockResolvedValue(true);
    await confirmDialog('x', { title: '登録の付け替え', okLabel: '付け替える' });
    expect(confirmMock).toHaveBeenCalledWith('x', expect.objectContaining({ title: '登録の付け替え', okLabel: '付け替える', cancelLabel: 'キャンセル' }));
  });

  it('見出しの既定は、アプリの名前（殻で違う）。空の名前では、変えない', async () => {
    confirmMock.mockResolvedValue(true);
    setAppName('MrkCodeReview');
    await confirmDialog('x');
    expect(confirmMock).toHaveBeenLastCalledWith('x', expect.objectContaining({ title: 'MrkCodeReview' }));
    setAppName('');
    await confirmDialog('x');
    expect(confirmMock).toHaveBeenLastCalledWith('x', expect.objectContaining({ title: 'MrkCodeReview' }));
    setAppName('MrCodeReview');
  });

  it('キャンセルは false', async () => {
    confirmMock.mockResolvedValue(false);
    expect(await confirmDialog('x')).toBe(false);
  });

  it('真に見えるだけの値（Promise・文字列など）は、「はい」と見なさない', async () => {
    confirmMock.mockResolvedValue('yes');
    expect(await confirmDialog('x')).toBe(false);
  });

  it('ダイアログを出せなかったとき（例外）は、進まない（false）', async () => {
    confirmMock.mockImplementation(() => Promise.reject(new Error('no dialog')));
    expect(await confirmDialog('x')).toBe(false);
  });
});

describe('window.confirm を、画面のコードで使わない（上の理由。使うと、確認が素通りする）', () => {
  it('src の中に、window.confirm( が無い', () => {
    const files = import.meta.glob('./*.ts', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
    const bad = Object.entries(files)
      .filter(([name]) => !name.endsWith('.test.ts') && !name.endsWith('dialog.ts'))
      .filter(([, src]) => /(window|globalThis)\.confirm\s*\(|(?<![.\w])confirm\(\s*[`'"]/.test(src.replace(/\/\/.*$/gm, '')))
      .map(([name]) => name);
    expect(bad).toEqual([]);
  });
});
