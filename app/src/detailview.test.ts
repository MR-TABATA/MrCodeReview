// @vitest-environment jsdom
import { describe, it, expect } from 'vitest';
import { detailPanel, GROUNDING_NOTE, resultText } from './detailview';
import type { ReviewView } from './types';

const nf = new Intl.NumberFormat('ja-JP');
const FMT = { when: (ms: number) => `T${ms}`, dur: (ms: number) => `${ms} ms`, num: (n: number | null) => (n === null ? '（報告なし）' : nf.format(n)), base: '' };

const F = (verified: boolean) => ({ file: 'a.rs', line: 1, severity: 'high', title: 't', detail: 'd', evidence: 'e', verified, rule: null });
function review(over: Record<string, unknown> = {}): ReviewView {
  return {
    id: '1793-abc',
    created_at: 1_793_000_000_000,
    repo: '/r',
    recipe: { id: 'perf', title: 'パフォーマンス', source: 'builtin', prompt: 'p', kind: 'llm', needs_related: false },
    provider: { id: 'claude-cli', label: 'claude -p', note: 'n' },
    snapshot: { head: 'abc1234def', diff_hash: 'd0d0d0d0', diff_bytes: 12_345, files: [{ path: 'a.rs', bytes: 10 }, { path: 'b.rs', bytes: 20 }], excluded: [{ path: 'c.png', reason: 'バイナリ' }] },
    prompt: 'x'.repeat(2000),
    model: 'claude-sonnet-5-5',
    response_text: '',
    raw: '',
    usage: { input_tokens: 1500, output_tokens: 320, cache_read_tokens: 50, cache_write_tokens: null },
    cost_usd: null,
    duration_ms: 14_500,
    findings: [F(true), F(true), F(false)],
    carries: [],
    fates: [],
    notes: [],
    related: null,
    parse_error: null,
    error: null,
    triage: ['fix', 'dismiss', 'new'],
    finding_notes: [
      { memo: '', memo_at: null, done_at: 5, done_head: 'h', triage_at: null, triage_head: null, triage_source: null },
      { memo: '', memo_at: null, done_at: null, done_head: null, triage_at: null, triage_head: null, triage_source: null },
      { memo: '', memo_at: null, done_at: null, done_head: null, triage_at: null, triage_head: null, triage_source: null },
    ],
    agent_reports: [{ verdict: 'did', text: 'x', reported_at: 1, source: 's' }, null, null],
    ...over,
  } as unknown as ReviewView;
}
const cells = (el: HTMLElement) => Object.fromEntries([...el.querySelectorAll('tr')].map((tr) => [tr.querySelector('th')!.textContent!, tr.querySelector('td')!.textContent!]));

describe('resultText', () => {
  it('失敗は理由つき。指摘を読めなかった実行は、実行の成功と分けて言う', () => {
    expect(resultText(review())).toBe('成功');
    expect(resultText(review({ error: 'タイムアウト' }))).toBe('失敗: タイムアウト');
    expect(resultText(review({ parse_error: 'JSON ではない' }))).toBe('実行は成功。指摘を読めなかった: JSON ではない');
  });
});

describe('detailPanel（詳細タブ）', () => {
  it('記録している値を、項目ごとに出す: 実行・見る範囲と送った内容・トークン・指摘・記録', () => {
    const el = detailPanel(review(), FMT);
    expect([...el.querySelectorAll('h3')].map((h) => h.textContent)).toEqual(['実行', '見る範囲と、送った内容', 'トークン', '指摘', '記録']);
    const c = cells(el);
    expect(c['日時']).toBe('T1793000000000');
    expect(c['所要時間']).toBe('14500 ms');
    expect([c['LLM'], c['モデル'], c['結果']]).toEqual(['claude -p', 'claude-sonnet-5-5', '成功']);
    expect([c['HEAD'], c['差分の識別子'], c['差分の大きさ']]).toEqual(['abc1234def', 'd0d0d0d0', '12,345 B']);
    expect([c['送ったファイル'], c['送らなかったもの'], c['プロンプト']]).toEqual(['2 件', '1 件', '2,000 文字']);
    expect([c['入力'], c['出力'], c['キャッシュ読み取り'], c['入力＋出力']]).toEqual(['1,500', '320', '50', '1,820']);
    expect(c['識別子']).toBe('mrcr:1793-abc');
    expect(c['実行メモ']).toBe('なし');
  });

  it('報告の無いトークン・金額は、推定で埋めず、「（報告なし）」と言う', () => {
    const c = cells(detailPanel(review(), FMT));
    expect(c['キャッシュ書き込み']).toBe('（報告なし）');
    expect(c['提供元の報告額（参考）']).toBe('（報告なし）');
    const none = cells(detailPanel(review({ usage: { input_tokens: null, output_tokens: null, cache_read_tokens: null, cache_write_tokens: null }, cost_usd: 0.1234 }), FMT));
    expect([none['入力'], none['入力＋出力']]).toEqual(['（報告なし）', '（報告なし）']);
    expect(none['提供元の報告額（参考）']).toBe('$0.1234');
  });

  it('見る範囲は、渡された表示を使う。無ければ「未コミットの変更（HEAD から）」', () => {
    expect(cells(detailPanel(review(), FMT))['見る範囲']).toBe('未コミットの変更（HEAD から）');
    expect(cells(detailPanel(review(), { ...FMT, base: '直近のコミット' }))['見る範囲']).toBe('直近のコミット');
  });

  it('指摘: 件数・根拠・仕分けの内訳・対応済み・エージェントの申告', () => {
    const c = cells(detailPanel(review(), FMT));
    expect(c['件数']).toBe('3 件');
    expect(c['根拠あり／根拠不一致']).toBe('2 件／1 件');
    expect(c['仕分け']).toBe('対応 1 ・待ち 0 ・無視 1 ・未仕分け 1');
    expect(c['対応済み（あなたの操作）']).toBe('1 件');
    expect(c['エージェントの申告']).toContain('対応した 1 件 ・対応しなかった 0 件');
    expect(c['エージェントの申告']).toContain('どれも、エージェントの申告・判断で、MCR が確かめたものではありません');
    expect(c['エージェントの申告'], '保存元が「s」（ログの名前）なので、会話の記録から取り込み').toContain('会話の記録から取り込み 1 件');
    expect(cells(detailPanel(review({ agent_reports: [null, null, null] }), FMT))['エージェントの申告']).toBe('（申告はありません）');
  });

  it('エージェントの申告: 保存元（`mrcr handoff report`・MCP・会話の記録）を、件数で分けて言う。取り込みの経路に、決めつけない', () => {
    const rep = (source: string, verdict: 'did' | 'skipped' = 'did') => ({ verdict, text: 'x', reported_at: 1, source });
    const c = cells(
      detailPanel(review({ agent_reports: [rep('cli:claude-code'), rep('cli:claude-code', 'skipped'), rep('mcp:x'), rep('sess1')] }), FMT),
    )['エージェントの申告'];
    expect(c).toContain('対応した 3 件 ・対応しなかった 1 件');
    expect(c).toContain('mrcr handoff report で保存 2 件');
    expect(c).toContain('MCP で保存 1 件');
    expect(c).toContain('会話の記録から取り込み 1 件');
    // CLI だけのときは、「会話の記録から取り込み」とは、言わない
    const only = cells(detailPanel(review({ agent_reports: [rep('cli:claude-code'), null, null] }), FMT))['エージェントの申告'];
    expect(only).toContain('mrcr handoff report で保存 1 件');
    expect(only).not.toContain('会話の記録');
  });

  it('「根拠」の意味（引用が差分に実在するかだけ。正しさの判断ではない）を、指摘の表の下に添える。linter には、必ず根拠ありになる理由を足す', () => {
    const el = detailPanel(review(), FMT);
    expect(el.textContent).toContain(GROUNDING_NOTE);
    expect(GROUNDING_NOTE).toContain('指摘が正しいかどうかの判断ではありません');
    expect(el.textContent).not.toContain('必ず根拠ありになります');
    expect(detailPanel(review({ provider: { id: 'tool', label: 'リンター', note: '' } }), FMT).textContent).toContain('必ず根拠ありになります');
  });

  it('失敗した実行は、結果に理由を出す。実行メモは、つなげて出す', () => {
    const c = cells(detailPanel(review({ error: 'タイムアウト', notes: ['ESLint: 1 件を除外', '別のメモ'] }), FMT));
    expect(c['結果']).toBe('失敗: タイムアウト');
    expect(c['実行メモ']).toBe('ESLint: 1 件を除外 ／ 別のメモ');
  });

  it('LLM を使わない実行（リンター）には、「測っての 0」の注を添える。LLM の実行には出さない', () => {
    const tool = detailPanel(review({ provider: { id: 'tool', label: 'リンター', note: '' } }), FMT);
    expect(tool.textContent).toContain('LLM を呼んでいないので、トークンは 0 です');
    expect(detailPanel(review(), FMT).textContent).not.toContain('LLM を呼んでいないので');
  });

  it('記録の文字は、文字として出す（HTML として解釈しない）', () => {
    const el = detailPanel(review({ notes: ['<img src=x onerror=alert(1)>'], model: '<b>m</b>' }), FMT);
    expect(el.querySelector('img')).toBeNull();
    expect(el.querySelector('b')).toBeNull();
    expect(cells(el)['実行メモ']).toBe('<img src=x onerror=alert(1)>');
  });
});
