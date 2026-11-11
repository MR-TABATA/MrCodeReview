import { describe, it, expect } from 'vitest';
import { elapsedText, estimateText, fmtSeconds, type DurationStats } from './duration';

const st = (n: number, min: number, med: number, max: number): DurationStats => ({ n, min_ms: min, median_ms: med, max_ms: max });

describe('fmtSeconds / elapsedText', () => {
  it('秒・分に整える。目安は丸め、経過時間は切り捨てる', () => {
    expect(fmtSeconds(0)).toBe('0 秒');
    expect(fmtSeconds(24_600)).toBe('25 秒');
    expect(fmtSeconds(59_400)).toBe('59 秒');
    expect(fmtSeconds(60_000)).toBe('1 分');
    expect(fmtSeconds(65_000)).toBe('1 分 5 秒');
    expect(fmtSeconds(-5)).toBe('0 秒');
    expect(elapsedText(24_999)).toBe('24 秒');
    expect(elapsedText(59_999)).toBe('59 秒');
    expect(elapsedText(125_000)).toBe('2 分 5 秒');
    expect(elapsedText(60_000)).toBe('1 分 0 秒');
  });
});

describe('estimateText', () => {
  it('モックは、時間を測らないと言う', () => {
    expect(estimateText(st(5, 1, 1, 1), true)).toContain('モック');
    expect(estimateText(null, true)).toContain('モック');
  });

  it('読み込み前は空。実績が 0 なら、「実績なし」と言い、数字は出さない', () => {
    expect(estimateText(null, false)).toBe('');
    const t = estimateText(st(0, 0, 0, 0), false);
    expect(t).toContain('実績なし');
    expect(t).not.toMatch(/\d+ 秒/);
  });

  it('1〜2 回は、幅だけを見せ、参考程度と断る', () => {
    expect(estimateText(st(1, 25_000, 25_000, 25_000), false)).toBe('目安: 過去 1 回で 25 秒（実績が少ないので、参考程度）');
    expect(estimateText(st(2, 25_000, 35_000, 45_000), false)).toBe('目安: 過去 2 回で 25 秒〜45 秒（実績が少ないので、参考程度）');
    expect(estimateText(st(2, 30_000, 30_000, 30_000), false)).toBe('目安: 過去 2 回で 30 秒（実績が少ないので、参考程度）');
  });

  it('3 回以上は、中央値と幅。全部同じなら、幅は出さない', () => {
    expect(estimateText(st(5, 25_000, 30_000, 45_000), false)).toBe('目安: 過去 5 回の中央値 30 秒（25 秒〜45 秒）');
    expect(estimateText(st(10, 2_000, 2_000, 2_000), false)).toBe('目安: 過去 10 回とも 2 秒');
    expect(estimateText(st(4, 50_000, 65_000, 130_000), false)).toBe('目安: 過去 4 回の中央値 1 分 5 秒（50 秒〜2 分 10 秒）');
  });
});
