// @vitest-environment jsdom
// 設定画面を開いて「Git フック」タブを出すまでの流れ全体（openSettings → hook_status → 描画）を、呼び出しを差し替えて確かめる。
import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: vi.fn(), message: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { openSettings, state } from './main';
import { newHookUi } from './hookview';
import type { HookStatus } from './types';

const status = (over: Partial<HookStatus> = {}): HookStatus => ({
  repo: '/r',
  enabled: false,
  recipes: ['bugs'],
  spec: null,
  installed: false,
  cli: '/opt/homebrew/bin/mrcr',
  blocked: null,
  target_missing: false,
  last_run: null,
  ...over,
});

const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
  await new Promise((r) => setTimeout(r, 0));
};

describe('設定を開いて Git フックのタブを出すまで', () => {
  beforeEach(() => {
    document.body.innerHTML = '<div id="app"></div>';
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return { values: { language: 'ja' }, meta: [], path: '/d/settings.json', exists: false, error: null, repo: { path: '/r', keys: [], error: null } };
      if (cmd === 'hook_status') return status();
      if (cmd === 'agent_log_consent') return false;
      throw new Error(`未対応のコマンド: ${cmd}`);
    });
    state.repo = '/r';
    state.settings = null;
    state.hook = newHookUi();
  });

  it('openSettings のあと、hook_status が呼ばれ、状態が入り、タブに出る', async () => {
    await openSettings();
    await flush();
    expect(invokeMock).toHaveBeenCalledWith('hook_status', { repo: '/r' });
    expect(state.hook.status?.enabled).toBe(false);
    expect(state.hook.error).toBe('');
    const tab = [...document.querySelectorAll<HTMLButtonElement>('.stabs button')].find((b) => b.textContent?.startsWith('Git フック'))!;
    tab.click();
    const pane = document.querySelector('.hookview')!;
    expect(pane.textContent).not.toContain('読み取っています');
    expect(pane.textContent).toContain('OFF');
  });
});
