import { describe, it, expect, vi } from 'vitest';
import { confirmText, runUpdateCheck, type UpdateDeps, type UpdateInfo } from './updateflow';

const info: UpdateInfo = { version: '1.1.0', current: '1.0.0', notes: null };
const deps = (over: Partial<UpdateDeps> = {}): UpdateDeps => ({
  check: vi.fn().mockResolvedValue(info),
  confirm: vi.fn().mockResolvedValue(true),
  install: vi.fn().mockResolvedValue(undefined),
  notify: vi.fn(),
  ...over,
});

describe('runUpdateCheck（メニューの「アップデートを確認…」）', () => {
  it('新しい版があり、「更新する」なら、入れる', async () => {
    const d = deps();
    expect(await runUpdateCheck(d)).toBe('installed');
    expect(d.install).toHaveBeenCalledOnce();
  });

  it('聞かれて断ったら、入れない。何も出さない', async () => {
    const d = deps({ confirm: vi.fn().mockResolvedValue(false) });
    expect(await runUpdateCheck(d)).toBe('declined');
    expect(d.install).not.toHaveBeenCalled();
    expect(d.notify).not.toHaveBeenCalled();
  });

  it('新しい版が無ければ、「最新です」と伝える。聞かず、入れない', async () => {
    const d = deps({ check: vi.fn().mockResolvedValue(null) });
    expect(await runUpdateCheck(d)).toBe('latest');
    expect(d.notify).toHaveBeenCalledWith('いまの版は、最新です。', 'ok');
    expect(d.confirm).not.toHaveBeenCalled();
    expect(d.install).not.toHaveBeenCalled();
  });

  it('確認できなかったときは、「最新です」とは言わず、理由を出す（黙って成功にしない）', async () => {
    const d = deps({ check: vi.fn().mockRejectedValue('更新を確認できませんでした: 404') });
    expect(await runUpdateCheck(d)).toBe('error');
    expect(d.notify).toHaveBeenCalledWith('更新を確認できませんでした: 404', 'err');
    expect(d.notify).not.toHaveBeenCalledWith(expect.stringContaining('最新'), 'ok');
    expect(d.install).not.toHaveBeenCalled();
  });

  it('入れるのに失敗したら、理由を出す', async () => {
    const d = deps({ install: vi.fn().mockRejectedValue('更新を入れられませんでした: 署名') });
    expect(await runUpdateCheck(d)).toBe('error');
    expect(d.notify).toHaveBeenCalledWith('更新を入れられませんでした: 署名', 'err');
  });
});

describe('confirmText', () => {
  it('新しい版と、いまの版と、再起動することを言う', () => {
    const t = confirmText(info);
    expect(t).toContain('1.1.0');
    expect(t).toContain('1.0.0');
    expect(t).toContain('再起動します');
  });

  it('更新のメモがあれば出す。長すぎるときは切る', () => {
    expect(confirmText({ ...info, notes: '直したこと' })).toContain('直したこと');
    const t = confirmText({ ...info, notes: 'あ'.repeat(500) });
    expect(t).toContain('あ'.repeat(300) + '…');
    expect(t).not.toContain('あ'.repeat(301));
  });
});
