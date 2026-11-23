// @vitest-environment jsdom
// 設定画面を開いて「Git フック」タブを出すまでの流れ全体（openSettings → hook_status → 描画）を、呼び出しを差し替えて確かめる。
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invokeMock(...args) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));
vi.mock('@tauri-apps/plugin-dialog', () => ({ confirm: vi.fn(), message: vi.fn(), open: vi.fn(), save: vi.fn() }));

import { changeRepo, openSettings, state } from './main';
import { newHookUi } from './hookview';
import type { HookRun, HookStatus } from './types';

const status = (over: Partial<HookStatus> = {}): HookStatus => ({
  repo: '/r',
  enabled: false,
  recipes: ['bugs'],
  spec: null,
  installed: false,
  cli: '/opt/homebrew/bin/mrcr',
  hook_cli: null,
  blocked: null,
  target_missing: false,
  last_run: null,
  recent: [],
  log_unreadable: 0,
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

const runOf = (state: HookRun['state']): HookRun => ({
  run: 'r1',
  started_at_ms: 1_700_000_000_000,
  commit: 'abc1234',
  subject: '件名',
  recipes: ['bugs', 'spec'],
  steps: state === 'running' ? [{ recipe: 'bugs', index: 1, total: 2, ok: true, findings: 1, ms: 1000, error: null }] : [{ recipe: 'bugs', index: 1, total: 2, ok: true, findings: 1, ms: 1000, error: null }, { recipe: 'spec', index: 2, total: 2, ok: true, findings: 0, ms: 500, error: null }],
  state,
  code: state === 'running' ? null : 1,
  message: state === 'running' ? null : '完了',
  findings: state === 'running' ? null : 1,
  ms: state === 'running' ? null : 1500,
});

describe('実行ログの定期更新', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = '<div id="app"></div>';
    invokeMock.mockReset();
    state.repo = '/r';
    state.settings = null;
    state.hook = newHookUi();
  });
  afterEach(() => {
    state.settings = null; // 次の定期更新で、止まる
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  const hookCalls = () => invokeMock.mock.calls.filter((c) => c[0] === 'hook_status').length;
  const open = async (responses: HookStatus[]) => {
    let i = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return { values: { language: 'ja' }, meta: [], path: '/d/settings.json', exists: false, error: null, repo: { path: '/r', keys: [], error: null } };
      if (cmd === 'hook_status') return responses[Math.min(i++, responses.length - 1)];
      if (cmd === 'agent_log_consent') return false;
      throw new Error(`未対応のコマンド: ${cmd}`);
    });
    await openSettings();
    await vi.advanceTimersByTimeAsync(0);
  };

  it('実行中は 2 秒ごとに読み直し、終わったら表示が変わる。終わったあとは、間隔が 8 秒になる', async () => {
    await open([status({ recent: [runOf('running')] }), status({ recent: [runOf('done')] })]);
    expect(hookCalls()).toBe(1);
    expect(state.hook.status?.recent[0].state).toBe('running');
    await vi.advanceTimersByTimeAsync(1900);
    expect(hookCalls()).toBe(1);
    await vi.advanceTimersByTimeAsync(200);
    expect(hookCalls()).toBe(2);
    expect(state.hook.status?.recent[0].state).toBe('done');
    await vi.advanceTimersByTimeAsync(2100);
    expect(hookCalls()).toBe(2); // もう実行中ではないので、2 秒では読まない
    await vi.advanceTimersByTimeAsync(6000);
    expect(hookCalls()).toBe(3);
  });

  it('内容が変わらないときは、画面を作り直さない（入力中の欄を、邪魔しない）', async () => {
    await open([status()]);
    const tab = [...document.querySelectorAll<HTMLButtonElement>('.stabs button')].find((b) => b.textContent?.startsWith('Git フック'))!;
    tab.click();
    const before = document.querySelector('.hookview');
    await vi.advanceTimersByTimeAsync(8100);
    expect(hookCalls()).toBe(2);
    expect(document.querySelector('.hookview')).toBe(before);
  });

  it('設定を閉じると、定期更新は止まる', async () => {
    await open([status()]);
    state.settings = null;
    await vi.advanceTimersByTimeAsync(20000);
    expect(hookCalls()).toBe(1);
  });
});

describe('操作と自動更新のぶつかり', () => {
  beforeEach(() => {
    vi.useFakeTimers();
    document.body.innerHTML = '<div id="app"></div>';
    invokeMock.mockReset();
    state.repo = '/r';
    state.settings = null;
    state.hook = newHookUi();
  });
  afterEach(() => {
    state.settings = null;
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  const flip = (): void => {
    const sw = document.querySelector<HTMLInputElement>('input[role=switch]')!;
    sw.checked = !sw.checked;
    sw.dispatchEvent(new Event('change'));
  };
  const settingsView = { values: { language: 'ja' }, meta: [], path: '/d/settings.json', exists: false, error: null, repo: { path: '/r', keys: [], error: null } };

  it('読み取りの途中で ON にしても、遅れて届いた古い結果（OFF）で、画面が OFF に戻らない', async () => {
    let release: (s: HookStatus) => void = () => {};
    let statusCalls = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return settingsView;
      if (cmd === 'agent_log_consent') return false;
      if (cmd === 'hook_status') {
        statusCalls++;
        if (statusCalls === 1) return status(); // 最初の読み取り: OFF
        return new Promise<HookStatus>((resolve) => (release = resolve)); // 2 回目: 応答を遅らせる
      }
      if (cmd === 'hook_set') return status({ enabled: true, installed: true, hook_cli: '/opt/homebrew/bin/mrcr' });
      throw new Error(`未対応のコマンド: ${cmd}`);
    });
    await openSettings();
    await vi.advanceTimersByTimeAsync(0);
    expect(state.hook.status?.enabled).toBe(false);

    await vi.advanceTimersByTimeAsync(8100); // 定期の読み取りが始まり、応答待ちになる
    expect(statusCalls).toBe(2);
    flip(); // その間に、ON にする
    await vi.advanceTimersByTimeAsync(0);
    expect(state.hook.status?.enabled).toBe(true);

    release(status()); // 遅れて、古い結果（OFF）が届く
    await vi.advanceTimersByTimeAsync(0);
    expect(state.hook.status?.enabled).toBe(true);
    expect(document.querySelector<HTMLInputElement>('input[role=switch]')!.checked).toBe(true);
  });

  it('操作の失敗メッセージは、自動更新が成功しても消えない。次の操作を始めると、消える', async () => {
    let failSet = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return settingsView;
      if (cmd === 'agent_log_consent') return false;
      if (cmd === 'hook_status') return status();
      if (cmd === 'hook_set') {
        if (failSet) throw new Error('設計書が要ります');
        return status({ enabled: true, installed: true, hook_cli: '/opt/homebrew/bin/mrcr' });
      }
      throw new Error(`未対応のコマンド: ${cmd}`);
    });
    await openSettings();
    await vi.advanceTimersByTimeAsync(0);

    flip(); // ON にしようとして、失敗する
    await vi.advanceTimersByTimeAsync(0);
    expect(state.hook.error).toContain('設計書が要ります');
    expect(document.querySelector('.hookview')!.textContent).toContain('操作に失敗しました');
    expect(document.querySelector('.hookview')!.textContent).toContain('設計書が要ります');

    await vi.advanceTimersByTimeAsync(8100); // 自動更新が、成功する
    await vi.advanceTimersByTimeAsync(8100);
    expect(state.hook.loadError).toBe('');
    expect(state.hook.error).toContain('設計書が要ります');
    expect(document.querySelector('.hookview')!.textContent).toContain('操作に失敗しました');
    expect(document.querySelector('.hookview')!.textContent).toContain('設計書が要ります');

    failSet = false;
    flip(); // 次の操作を始めると、前の失敗は消える
    await vi.advanceTimersByTimeAsync(0);
    expect(state.hook.error).toBe('');
    expect(state.hook.status?.enabled).toBe(true);
  });

  it('読み取りが失敗したら読み取りのエラーに出て、次に成功したら消える（操作のエラーとは別）', async () => {
    let n = 0;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === 'get_settings') return settingsView;
      if (cmd === 'agent_log_consent') return false;
      if (cmd === 'hook_status') {
        n++;
        if (n === 2) throw new Error('一時的な失敗');
        return status();
      }
      throw new Error(`未対応のコマンド: ${cmd}`);
    });
    await openSettings();
    await vi.advanceTimersByTimeAsync(0);
    await vi.advanceTimersByTimeAsync(8100);
    expect(state.hook.loadError).toContain('一時的な失敗');
    expect(state.hook.error).toBe('');
    expect(document.querySelector('.hookview')!.textContent).toContain('最新の状態を読み取れませんでした');
    await vi.advanceTimersByTimeAsync(8100);
    expect(state.hook.loadError).toBe('');
  });
});

describe('設定を開いているあいだに、リポジトリが変わったとき', () => {
  beforeEach(() => {
    document.body.innerHTML = '<div id="app"></div>';
    invokeMock.mockReset();
    state.settings = null;
    state.hook = newHookUi();
    state.registeredRepos = [];
  });
  afterEach(() => {
    state.settings = null;
  });

  const settingsView = { values: { language: 'ja' }, meta: [], path: '/d/settings.json', exists: false, error: null, repo: { path: null, keys: [], error: null } };
  const flush = async () => {
    for (let i = 0; i < 20; i++) await Promise.resolve();
    await new Promise((r) => setTimeout(r, 0));
  };
  const mock = (byRepo: Record<string, HookStatus>) =>
    invokeMock.mockImplementation(async (cmd: string, args?: { repo?: string }) => {
      if (cmd === 'get_settings') return settingsView;
      if (cmd === 'agent_log_consent') return false;
      if (cmd === 'list_reviews') return [];
      if (cmd === 'hook_status') return byRepo[args?.repo ?? ''];
      throw new Error(`未対応のコマンド: ${cmd}`); // 履歴・プレビューまわりの読み取りは、ここでは使わない（呼び出し側が、握る）
    });
  const hookText = (): string => document.querySelector('.hookview')?.textContent ?? '';

  it('リポジトリが無い状態で設定を開き、あとからリポジトリを選ぶと、そのリポジトリの状態を読みに行く（「読み取っています」のままにならない）', async () => {
    mock({ '/r': status({ repo: '/r', enabled: true, installed: true, hook_cli: '/opt/homebrew/bin/mrcr' }) });
    state.repo = '';
    await openSettings();
    await flush();
    expect(hookText()).toContain('リポジトリを選ぶと');
    expect(invokeMock.mock.calls.some((c) => c[0] === 'hook_status')).toBe(false);

    await changeRepo('/r').catch(() => {});
    await flush();
    expect(invokeMock).toHaveBeenCalledWith('hook_status', { repo: '/r' });
    expect(state.hook.status?.enabled).toBe(true);
    expect(hookText()).not.toContain('読み取っています');
    expect(hookText()).toContain('ON');
  });

  it('別のリポジトリに切り替えたら、前のリポジトリの状態は消えて、新しいリポジトリの状態が出る', async () => {
    mock({
      '/a': status({ repo: '/a', enabled: true, installed: true, recipes: ['bugs'], hook_cli: '/a/mrcr' }),
      '/b': status({ repo: '/b', enabled: false, installed: false, recipes: ['errors'] }),
    });
    state.repo = '/a';
    await openSettings();
    await flush();
    expect(state.hook.status?.repo).toBe('/a');
    expect(document.querySelector<HTMLInputElement>('input[role=switch]')!.checked).toBe(true);

    await changeRepo('/b').catch(() => {});
    await flush();
    expect(state.hook.status?.repo).toBe('/b');
    expect(document.querySelector<HTMLInputElement>('input[role=switch]')!.checked).toBe(false);
    expect(hookText()).not.toContain('/a/mrcr');
  });

  it('切り替えの前に始めた読み取りの結果が、あとから届いても、新しいリポジトリの画面を上書きしない', async () => {
    let releaseA: (s: HookStatus) => void = () => {};
    invokeMock.mockImplementation(async (cmd: string, args?: { repo?: string }) => {
      if (cmd === 'get_settings') return settingsView;
      if (cmd === 'agent_log_consent') return false;
      if (cmd === 'list_reviews') return [];
      if (cmd === 'hook_status') {
        if (args?.repo === '/a') return new Promise<HookStatus>((resolve) => (releaseA = resolve)); // A の応答を遅らせる
        return status({ repo: '/b', enabled: false });
      }
      throw new Error(`未対応のコマンド: ${cmd}`);
    });
    state.repo = '/a';
    await openSettings();
    await flush(); // A の読み取りは、まだ返っていない
    await changeRepo('/b').catch(() => {});
    await flush();
    expect(state.hook.status?.repo).toBe('/b');

    releaseA(status({ repo: '/a', enabled: true }));
    await flush();
    expect(state.hook.status?.repo, '遅れて届いた A の結果は、捨てる').toBe('/b');
  });

  it('操作の最中にリポジトリを切り替えても、前のリポジトリの操作の結果が、新しい画面に入らない', async () => {
    let releaseSet: (s: HookStatus) => void = () => {};
    invokeMock.mockImplementation(async (cmd: string, args?: { repo?: string }) => {
      if (cmd === 'get_settings') return settingsView;
      if (cmd === 'agent_log_consent') return false;
      if (cmd === 'list_reviews') return [];
      if (cmd === 'hook_status') return status({ repo: args?.repo ?? '', enabled: false });
      if (cmd === 'hook_set') return new Promise<HookStatus>((resolve) => (releaseSet = resolve)); // A への操作を遅らせる
      throw new Error(`未対応のコマンド: ${cmd}`);
    });
    state.repo = '/a';
    await openSettings();
    await flush();
    const sw = document.querySelector<HTMLInputElement>('input[role=switch]')!;
    sw.checked = true;
    sw.dispatchEvent(new Event('change')); // A を ON にする（応答待ち）
    await flush();
    await changeRepo('/b').catch(() => {});
    await flush();
    expect(state.hook.status?.repo).toBe('/b');

    releaseSet(status({ repo: '/a', enabled: true, installed: true }));
    await flush();
    expect(state.hook.status?.repo, 'A への操作の結果は、B の画面に入らない').toBe('/b');
    expect(state.hook.busy).toBe(false);
  });
});
