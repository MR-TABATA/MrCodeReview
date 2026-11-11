import { describe, it, expect, vi, afterEach } from 'vitest';
import { debounced } from './debounce';

describe('debounced', () => {
  afterEach(() => vi.useRealTimers());

  it('続けて呼んでも、最後の呼び出しの待ち時間のあとに、1 回だけ実行する', () => {
    vi.useFakeTimers();
    const fn = vi.fn();
    const d = debounced(fn, 400);
    d.call();
    vi.advanceTimersByTime(300);
    d.call(); // 待ち時間を数え直す
    vi.advanceTimersByTime(300);
    expect(fn).not.toHaveBeenCalled();
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(1);
    vi.advanceTimersByTime(5000);
    expect(fn).toHaveBeenCalledTimes(1);
  });

  it('実行したあとの呼び出しは、また 1 回実行する', () => {
    vi.useFakeTimers();
    const fn = vi.fn();
    const d = debounced(fn, 100);
    d.call();
    vi.advanceTimersByTime(100);
    d.call();
    d.call();
    vi.advanceTimersByTime(100);
    expect(fn).toHaveBeenCalledTimes(2);
  });

  it('取り消すと、実行しない。呼んでいないときの取り消しも壊れない', () => {
    vi.useFakeTimers();
    const fn = vi.fn();
    const d = debounced(fn, 100);
    d.cancel();
    d.call();
    d.cancel();
    vi.advanceTimersByTime(1000);
    expect(fn).not.toHaveBeenCalled();
  });
});
