// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { findingNo, findingTag, reviewTag, tagLine } from './findingtag';

describe('識別子の書式', () => {
  it('レビューの識別子は mrcr:<id>。指摘は、元の位置（0 始まり）に 1 を足した番号', () => {
    expect(reviewTag('1793280553817-5754b156')).toBe('mrcr:1793280553817-5754b156');
    expect(findingTag('1793280553817-5754b156', 0)).toBe('mrcr:1793280553817-5754b156#1');
    expect(findingTag('1793280553817-5754b156', 9)).toBe('mrcr:1793280553817-5754b156#10');
  });

  it('Rust（mrcr-core の triage.rs）の書式と同じ。食い違うと、エージェントの報告を取り込めなくなる', () => {
    // triage.rs は、同じリポジトリの crates/mrcr-core にある（src-tauri/Cargo.toml が、同じ相対パスで使っている）
    const files = import.meta.glob('../../crates/mrcr-core/src/triage.rs', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
    const src = Object.values(files)[0];
    if (!src) throw new Error('triage.rs が見つからない（../../crates/mrcr-core/src/triage.rs）');
    expect(src).toContain('format!("mrcr:{review_id}")');
    expect(src).toContain('format!("mrcr:{review_id}#{}", index0 + 1)');
  });
});

describe('番号の印と、識別子の行', () => {
  it('見出しには、#番号。ホバーで、識別子の全文', () => {
    const el = findingNo(2, 'mrcr:r#3');
    expect(el.textContent).toBe('#3');
    expect(el.title).toBe('識別子: mrcr:r#3');
  });

  it('識別子の行は、全文を出し、「コピー」で、その文字列を渡す。結果を、ボタンに出す', async () => {
    vi.useFakeTimers();
    const copy = vi.fn().mockResolvedValue(true);
    const el = tagLine('mrcr:r#3', copy);
    expect(el.querySelector('code')!.textContent).toBe('mrcr:r#3');
    const btn = el.querySelector('button') as HTMLButtonElement;
    btn.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(copy).toHaveBeenCalledWith('mrcr:r#3');
    expect(btn.textContent).toBe('コピーしました');
    await vi.advanceTimersByTimeAsync(1500);
    expect(btn.textContent).toBe('コピー');
    vi.useRealTimers();
  });

  it('コピーできなかったときは、「コピーしました」と言わない', async () => {
    vi.useFakeTimers();
    const el = tagLine('mrcr:r#3', vi.fn().mockResolvedValue(false));
    const btn = el.querySelector('button') as HTMLButtonElement;
    btn.click();
    await vi.advanceTimersByTimeAsync(0);
    expect(btn.textContent).toBe('コピーできませんでした');
    vi.useRealTimers();
  });
});
