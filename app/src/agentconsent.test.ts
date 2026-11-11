// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { agentConsentSection, newAgentConsent } from './agentconsent';

const btn = (el: HTMLElement, label: string) => [...el.querySelectorAll('button')].find((b) => b.textContent === label) as HTMLButtonElement | undefined;

describe('agentConsentSection（設定画面の「会話の記録の読み取り」）', () => {
  it('何を読み、何を保存しないかを、先に言う', () => {
    const el = agentConsentSection({ consented: false, error: '' }, vi.fn());
    expect(el.textContent).toContain('Claude Code・agy・GitHub Copilot CLI の会話の記録');
    expect(el.textContent).toContain('会話の本文は、保存しません');
  });

  it('許可済みのときは、「許可済み」と、取り消しボタンを出す。押すと、取り消す。取り消しても報告が残ることを言う', () => {
    const revoke = vi.fn();
    const el = agentConsentSection({ consented: true, error: '' }, revoke);
    expect(el.querySelector('strong')?.textContent).toBe('許可済み');
    btn(el, '許可を取り消す')!.click();
    expect(revoke).toHaveBeenCalledOnce();
    expect(el.textContent).toContain('取り込み済みの報告は、残ります');
    expect(el.textContent).toContain('もう一度、読んでよいかを聞きます');
  });

  it('未許可のときは、取り消しボタンを出さない（取り消すものが無い）。いつ聞くかを言う', () => {
    const el = agentConsentSection({ consented: false, error: '' }, vi.fn());
    expect(el.querySelector('strong')?.textContent).toBe('未許可');
    expect(btn(el, '許可を取り消す')).toBeUndefined();
    expect(el.textContent).toContain('初めて押したときに、読んでよいかを聞きます');
  });

  it('読み取り中は、そう言う。読み取りに失敗したときは、理由を出し、状態を決めつけない', () => {
    const loading = agentConsentSection(newAgentConsent(), vi.fn());
    expect(loading.textContent).toContain('許可の状態を読み取っています');
    expect(btn(loading, '許可を取り消す')).toBeUndefined();
    const failed = agentConsentSection({ consented: null, error: '許可の印を読み取れません: x' }, vi.fn());
    expect(failed.querySelector('.err')?.textContent).toBe('許可の印を読み取れません: x');
    expect(failed.textContent).not.toContain('許可済み');
    expect(failed.textContent).not.toContain('未許可');
  });
});
