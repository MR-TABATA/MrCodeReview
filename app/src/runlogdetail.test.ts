// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { runLogDetail } from './runlogdetail';
import type { ReviewView } from './types';

const review = (over: Partial<ReviewView> = {}): ReviewView =>
  ({
    id: 'r1',
    repo: '/r',
    recipe: { id: 'bugs', title: '潜在バグ', source: 'builtin', prompt: '', kind: 'llm' },
    provider: { id: 'claude', name: 'claude', label: 'Claude', note: '', models: [] },
    model: 'sonnet',
    created_at: 1000,
    duration_ms: 12_500,
    usage: { input_tokens: 1200, output_tokens: 300, cache_read_tokens: null, cache_write_tokens: null },
    cost_usd: null,
    snapshot: { head: 'abc', diff_hash: 'x', diff_bytes: 4096, files: [{ path: 'a.ts', bytes: 10 }, { path: 'b.ts', bytes: 20 }], excluded: [{ path: 'lock', reason: 'x' }] },
    related: null,
    error: null,
    prompt: 'あいうえお',
    findings: [{ severity: 'high', title: '例外が握りつぶされる', file: 'src/a.ts', line: 12, detail: '本文です', evidence: 'catch {}', verified: true, rule: null }],
    triage: ['fix'],
    finding_notes: [{ memo: '直しました' }],
    ...over,
  }) as unknown as ReviewView;

const text = (el: HTMLElement) => [...el.querySelectorAll('.rld-row')].map((r) => r.textContent ?? '');
const has = (el: HTMLElement, label: string, value: string) => text(el).some((t) => t.includes(label) && t.includes(value));

describe('ログの、実行の詳細', () => {
  it('実行（観点・プロバイダ・モデル・所要時間・トークン）と、入力（比較元・差分・設計書・プロンプト）を出す', () => {
    const el = runLogDetail(review(), 'ja', vi.fn());
    expect(has(el, '観点', '潜在バグ')).toBe(true);
    expect(has(el, 'プロバイダ', 'Claude · sonnet')).toBe(true);
    expect(has(el, '所要時間', '12')).toBe(true);
    expect(has(el, 'トークン', '入力 1,200 / 出力 300')).toBe(true);
    expect(has(el, '比較元', '未コミットの変更')).toBe(true);
    expect(has(el, '差分', '2 ファイル・4,096 バイト')).toBe(true);
    expect(has(el, '設計書', 'なし')).toBe(true);
    expect(has(el, 'プロンプト', '5 文字')).toBe(true);
  });

  it('比較元がコミットなら、そのコミットと件名。設計書があれば、名前を並べる。関連コードがあれば、件数', () => {
    const el = runLogDetail(
      review({
        snapshot: { head: 'abc', base: { spec: 'HEAD~2', hash: 'h', subject: '直した' }, diff_hash: 'x', diff_bytes: 1, files: [], excluded: [] } as never,
        specs: [{ display_name: '設計書.md', content: '', content_hash: '', char_count: 1 }, { display_name: '仕様.md', content: '', content_hash: '', char_count: 1 }],
        related: { names: [], snippets: [{}, {}, {}], notes: [] } as never,
      }),
      'ja',
      vi.fn(),
    );
    expect(has(el, '比較元', 'HEAD~2（直した）')).toBe(true);
    expect(has(el, '設計書', '設計書.md、仕様.md')).toBe(true);
    expect(has(el, '関連コード', '3 件')).toBe(true);
  });

  it('トークンが報告されないときは「なし」。失敗したときは、その理由と、費用があれば費用を出す', () => {
    const el = runLogDetail(
      review({ usage: { input_tokens: null, output_tokens: null, cache_read_tokens: null, cache_write_tokens: null }, error: '応答が途中で切れました', cost_usd: 0.0123 }),
      'ja',
      vi.fn(),
    );
    expect(has(el, 'トークン', 'なし')).toBe(true);
    expect(has(el, '失敗の理由', '応答が途中で切れました')).toBe(true);
    expect(has(el, '費用', '$0.0123')).toBe(true);
  });

  it('結果の内容: 対象ファイル・本文・引用・対応内容・仕分けの状態を、読むだけで出す（ボタンは、結果を開く 1 つだけ）', () => {
    const el = runLogDetail(review(), 'ja', vi.fn());
    const f = el.querySelector('.rld-finding') as HTMLElement;
    expect(f.textContent).toContain('例外が握りつぶされる');
    expect(f.textContent).toContain('src/a.ts:12');
    expect(f.textContent).toContain('本文です');
    expect(f.textContent).toContain('catch {}');
    expect(f.textContent).toContain('直しました');
    expect(f.querySelector('button')).toBeNull();
    expect(el.querySelectorAll('button').length).toBe(1);
  });

  it('対応内容が空なら「記入なし」。指摘が無ければ、その旨。失敗なら、理由', () => {
    expect(runLogDetail(review({ finding_notes: [{ memo: '  ' }] as never }), 'ja', vi.fn()).textContent).toContain('（記入なし）');
    expect(runLogDetail(review({ findings: [], triage: [], finding_notes: [] }), 'ja', vi.fn()).textContent).toContain('指摘はありませんでした。');
    expect(runLogDetail(review({ findings: [], triage: [], finding_notes: [], error: '失敗しました' }), 'ja', vi.fn()).querySelector('.rld-nofindings')?.textContent).toBe('失敗しました');
  });

  it('差分: 閉じているあいだは、取り出さない（中身を作らない）。開くと、プロンプトの差分ブロックを取り出して出す', () => {
    const withDiff = review({ prompt: '前置き\n\n## 差分\n\n```diff\n+追加した行\n-消した行\n```\n' });
    const toggled: boolean[] = [];
    const el = runLogDetail(withDiff, 'ja', vi.fn(), { onToggle: (o) => toggled.push(o) });
    expect(el.querySelector('.rld-diff-pre'), '閉じているときは、作らない').toBeNull();
    expect(el.querySelector('.rld-diff-body')).toBeNull();
    const det = el.querySelector('details') as HTMLDetailsElement;
    det.open = true;
    det.dispatchEvent(new Event('toggle'));
    expect(el.querySelector('.rld-diff-pre')?.textContent).toContain('+追加した行');
    expect(toggled).toEqual([true]);
    // 閉じて、開き直しても、作り直さない
    det.open = false;
    det.dispatchEvent(new Event('toggle'));
    det.open = true;
    det.dispatchEvent(new Event('toggle'));
    expect(el.querySelectorAll('.rld-diff-body').length).toBe(1);
    expect(toggled).toEqual([true, false, true]);
  });

  it('差分: 開いたままの行（open: true）は、描き直しても、すぐ出る。取り出せなければ、その旨', () => {
    const withDiff = review({ prompt: '前置き\n\n## 差分\n\n```diff\n+x\n```\n' });
    const el = runLogDetail(withDiff, 'ja', vi.fn(), { open: true });
    expect((el.querySelector('details') as HTMLDetailsElement).open).toBe(true);
    expect(el.querySelector('.rld-diff-pre')?.textContent).toContain('+x');
    const none = runLogDetail(review(), 'ja', vi.fn(), { open: true });
    expect(none.querySelector('.rld-diff')?.textContent).toContain('取り出せませんでした');
  });

  it('「結果を開く」で、コールバックが呼ばれる', () => {
    const open = vi.fn();
    const el = runLogDetail(review(), 'ja', open);
    el.querySelector<HTMLElement>('.rld-foot button')!.click();
    expect(open).toHaveBeenCalledTimes(1);
  });

  it('英語でも出る', () => {
    const el = runLogDetail(review(), 'en', vi.fn());
    expect(has(el, 'Diff', '2 files')).toBe(true);
    expect(has(el, 'Compared from', 'Uncommitted')).toBe(true);
  });
});
