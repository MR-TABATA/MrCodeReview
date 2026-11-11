import { describe, expect, it, vi } from 'vitest';
import { getAppLang, resolveLang, runLangChange, setAppLang, t, tWithLang } from './i18n';

describe('i18n', () => {
  it('resolves explicit setting value', () => {
    expect(resolveLang('en')).toBe('en');
    expect(resolveLang('ja')).toBe('ja');
  });

  it('translates status and severity in ja and en', () => {
    setAppLang('ja');
    expect(t('status_new')).toBe('未対応');
    expect(t('severity_high')).toBe('高');
    expect(t('evidence_verified')).toBe('根拠あり');

    setAppLang('en');
    expect(t('status_new')).toBe('New');
    expect(t('severity_high')).toBe('High');
    expect(t('evidence_verified')).toBe('Evidence found');
  });

  it('tWithLang translates for specified language without modifying global state', () => {
    setAppLang('ja');
    expect(tWithLang('en', 'run_review')).toBe('Run Review');
    expect(getAppLang()).toBe('ja');
  });

  it('runLangChange updates app lang, calls list_recipes with lang:"en", and proceeds to render even if set_menu_language rejects', async () => {
    setAppLang('ja');
    const invoke = vi.fn(async (cmd: string, _args?: Record<string, unknown>) => {
      if (cmd === 'set_menu_language') throw new Error('menu fail');
      return { recipes: [], warnings: [], lint: null };
    });
    const loadRecipes = vi.fn(async (repo: string | null) => {
      await invoke('list_recipes', { repo, lang: getAppLang() });
    });
    const loadBases = vi.fn(async () => {
      await invoke('list_bases', { repo: '/r', lang: getAppLang() });
    });
    const render = vi.fn();
    const onError = vi.fn();
    const errSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    await runLangChange('en', { invoke, repo: '/r', loadRecipes, loadBases, render, onError });

    expect(getAppLang()).toBe('en');
    expect(invoke).toHaveBeenCalledWith('set_menu_language', { lang: 'en' });
    expect(invoke).toHaveBeenCalledWith('list_recipes', { repo: '/r', lang: 'en' });
    expect(invoke).toHaveBeenCalledWith('list_bases', { repo: '/r', lang: 'en' });
    expect(onError).toHaveBeenCalledWith(expect.stringContaining('menu fail'));
    expect(render).toHaveBeenCalledTimes(1);

    // 同じ言語への再実行は省く
    invoke.mockClear();
    loadRecipes.mockClear();
    loadBases.mockClear();
    render.mockClear();
    await runLangChange('en', { invoke, repo: '/r', loadRecipes, loadBases, render });
    expect(invoke).not.toHaveBeenCalled();
    expect(loadRecipes).not.toHaveBeenCalled();
    expect(loadBases).not.toHaveBeenCalled();
    expect(render).not.toHaveBeenCalled();
    errSpy.mockRestore();
  });
});
