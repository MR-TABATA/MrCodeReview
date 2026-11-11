// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { fixAllBySeverity, fixAllSeverityButtons } from './fixall';
import type { Handling, TriageState } from './types';

const handling = (triage: TriageState[]): Handling => ({ triage, notes: [] });

describe('fixAllBySeverity', () => {
  it('2件目で失敗したら、適用済みを残して停止し、エラー表示と更新を行う', async () => {
    const review = {
      findings: [{ severity: 'high' }, { severity: 'high' }, { severity: 'high' }],
      triage: ['new', 'new', 'new'] as TriageState[],
    };
    const setTriage = vi.fn()
      .mockResolvedValueOnce(handling(['fix', 'new', 'new']))
      .mockRejectedValueOnce(new Error('保存に失敗'));
    const onError = vi.fn();
    const refreshHistory = vi.fn();
    const refreshHandoff = vi.fn().mockResolvedValue(undefined);
    const render = vi.fn();

    await fixAllBySeverity(review, 'high', {
      setTriage,
      applyHandling: (result) => { review.triage = result.triage; },
      onError,
      refreshHistory,
      refreshHandoff,
      render,
    });

    expect(review.triage).toEqual(['fix', 'new', 'new']);
    expect(onError).toHaveBeenCalledExactlyOnceWith('Error: 保存に失敗');
    expect(setTriage.mock.calls).toEqual([[0, 'fix'], [1, 'fix']]);
    expect(refreshHistory).toHaveBeenCalledOnce();
    expect(refreshHandoff).toHaveBeenCalledOnce();
    expect(render).toHaveBeenCalledOnce();
  });

  it('重大度が一致し、未設定は new とみなし、fix 済み以外だけを保存する', async () => {
    const review = {
      findings: [
        { severity: 'high' },
        { severity: 'high' },
        { severity: 'medium' },
        { severity: 'low' },
      ],
      triage: [undefined, 'fix', 'new', 'new'] as (TriageState | undefined)[],
    };
    const setTriage = vi.fn().mockResolvedValue(handling(['fix']));

    await fixAllBySeverity(review, 'high', {
      setTriage,
      applyHandling: vi.fn(),
      onError: vi.fn(),
      refreshHistory: vi.fn(),
      refreshHandoff: vi.fn().mockResolvedValue(undefined),
      render: vi.fn(),
    });

    expect(setTriage).toHaveBeenCalledExactlyOnceWith(0, 'fix');
  });
});

describe('fixAllSeverityButtons', () => {
  const findings = [
    { severity: 'high' },
    { severity: 'high' },
    { severity: 'medium' },
    { severity: 'low' },
  ];
  const triage: TriageState[] = ['new', 'fix', 'fix', 'new'];
  const label = (severity: string, count: number, status: string) => `${status}('${severity}', ${count})`;

  it('high/medium は「対応」に、low は「無視」に。未仕分けの数がある重大度だけを、数えて出す', () => {
    const buttons = fixAllSeverityButtons(findings, triage, true, label, vi.fn());
    expect(buttons.map((button) => button.textContent)).toEqual(["fix('high', 1)", "dismiss('low', 1)"]);
  });

  it('low のボタンは、「対応」「待ち」「無視」にした low を数えない', () => {
    const lows = [{ severity: 'low' }, { severity: 'low' }, { severity: 'low' }, { severity: 'low' }];
    const buttons = fixAllSeverityButtons(lows, ['new', 'fix', 'wait', 'dismiss'], true, label, vi.fn());
    expect(buttons.map((button) => button.textContent)).toEqual(["dismiss('low', 1)"]);
  });

  it('押すと、重大度と、変える先の状態を渡す', () => {
    const onFix = vi.fn();
    const buttons = fixAllSeverityButtons(findings, triage, true, label, onFix);
    buttons[1].click();
    expect(onFix).toHaveBeenCalledExactlyOnceWith('low', 'dismiss');
  });

  it('withExpandAll が false のときは出さない', () => {
    expect(fixAllSeverityButtons(findings, triage, false, label, vi.fn())).toEqual([]);
  });
});

describe('fixAllBySeverity（無視へ）', () => {
  it('low を「無視」にするとき、まだ仕分けていない low だけを保存する（自分で「対応」「待ち」にした指摘は触らない）', async () => {
    const review = {
      findings: [{ severity: 'low' }, { severity: 'low' }, { severity: 'low' }, { severity: 'low' }, { severity: 'high' }],
      triage: ['new', 'fix', 'wait', 'dismiss', 'new'] as TriageState[],
    };
    const setTriage = vi.fn().mockResolvedValue(handling(['dismiss']));

    await fixAllBySeverity(review, 'low', {
      setTriage,
      applyHandling: vi.fn(),
      onError: vi.fn(),
      refreshHistory: vi.fn(),
      refreshHandoff: vi.fn().mockResolvedValue(undefined),
      render: vi.fn(),
    }, 'dismiss');

    expect(setTriage).toHaveBeenCalledExactlyOnceWith(0, 'dismiss');
  });
});
