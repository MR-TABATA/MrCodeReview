import { describe, it, expect } from 'vitest';
import { fixStateChip } from './triageview';

describe('fixStateChip（未対応／対応済み）', () => {
  it('「対応」にした指摘だけに出る。直していなければ「未対応」、直し済みなら「対応済み」', () => {
    expect(fixStateChip('fix', false)).toMatchObject({ text: '未対応', cls: 'open' });
    expect(fixStateChip('fix', true)).toMatchObject({ text: '対応済み', cls: 'did' });
  });

  it('未仕分け・待ち・無視には、出さない（済みの印があっても）', () => {
    for (const s of ['new', 'wait', 'dismiss'] as const) {
      expect(fixStateChip(s, false)).toBeNull();
      expect(fixStateChip(s, true)).toBeNull();
    }
  });

  it('「未対応」の説明は、直したあとにすることを言う', () => {
    expect(fixStateChip('fix', false)!.title).toContain('対応済みにする');
  });
});
