// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { createHistoryView, filterHooks, filterReviews, hookHaystack, matchesAll, normalize, parseTerms, reviewHaystack, stats } from './historyfilter';
import { displayRecipeTitle } from './historyview';
import type { HookRun, ReviewSummary } from './types';

const NOW = new Date(2026, 10, 28, 12, 0, 0); // 2026-11-28 12:00（ローカル）
const at = (m: number, d: number, h = 10) => new Date(2026, m - 1, d, h, 0, 0).getTime();

const S = (id: string, over: Partial<ReviewSummary> = {}): ReviewSummary => ({
  id,
  created_at: at(11, 28),
  repo: '/Users/me/Git/demo-repo',
  recipe_title: '潜在バグ',
  provider_label: 'Claude',
  findings: 2,
  verified: 2,
  total_tokens: null,
  failed: false,
  seq: 3,
  counts: { open: 2, done: 0, wait: 0, dismiss: 0 },
  head: 'abcdef0123456789',
  ...over,
});
const H = (run: string, over: Partial<HookRun> = {}): HookRun => ({
  run,
  started_at_ms: at(11, 27),
  commit: 'a3f9c21',
  subject: '上部タブの骨格',
  recipes: ['bugs', 'errors'],
  steps: [],
  state: 'done',
  code: 0,
  message: '完了',
  findings: 1,
  ms: 1000,
  ...over,
});
const ids = (xs: { id: string }[]) => xs.map((x) => x.id);

describe('語に分ける・照合する', () => {
  it('半角・全角のスペースで区切る。大文字・小文字、全角・半角は区別しない。空なら、語は無い', () => {
    expect(parseTerms('  潜在バグ　昨日   未対応 ')).toEqual(['潜在バグ', '昨日', '未対応']);
    expect(parseTerms('')).toEqual([]);
    expect(parseTerms('   　 ')).toEqual([]);
    expect(normalize('ＡＢｃ１２')).toBe('abc12');
  });

  it('すべての語が、含まれていれば、当てはまる（合わせ技）。語が無ければ、当てはまる', () => {
    expect(matchesAll('潜在バグ 昨日 未対応', parseTerms('潜在バグ 未対応'))).toBe(true);
    expect(matchesAll('潜在バグ 昨日', parseTerms('潜在バグ 未対応'))).toBe(false);
    expect(matchesAll('ABC', parseTerms('abc'))).toBe(true);
    expect(matchesAll('anything', [])).toBe(true);
  });
});

describe('レビューの行', () => {
  it('観点名・番号・コミット（全文・一部）・リポジトリ名・日付・プロバイダで、絞れる', () => {
    const items = [S('a'), S('b', { recipe_title: 'セキュリティ', head: '1111111999', seq: 1 })];
    const f = (q: string) => ids(filterReviews(items, parseTerms(q), 'ja', NOW));
    expect(f('潜在バグ')).toEqual(['a']);
    expect(f('セキュリティ')).toEqual(['b']);
    expect(f('abcdef0')).toEqual(['a']);
    expect(f('111')).toEqual(['b']);
    expect(f('demo-repo')).toEqual(['a', 'b']);
    expect(f('2026-11-28')).toEqual(['a', 'b']);
    expect(f('claude')).toEqual(['a', 'b']);
    expect(f('nomatch')).toEqual([]);
  });

  it('ブランチ名でも、絞れる（ブランチ名と、観点の合わせ技も）', () => {
    const items = [S('a', { branch: 'feature/x-y' }), S('b', { branch: 'main', recipe_title: 'セキュリティ' }), S('c', { branch: null })];
    const f = (q: string) => ids(filterReviews(items, parseTerms(q), 'ja', NOW));
    expect(f('feature')).toEqual(['a']);
    expect(f('main')).toEqual(['b']);
    expect(f('main セキュリティ')).toEqual(['b']);
    expect(f('main 潜在バグ')).toEqual([]);
  });

  it('合わせ技: 語を増やすと、減る。順番は、関係ない', () => {
    const items = [S('a'), S('b', { recipe_title: 'セキュリティ' }), S('c', { created_at: at(11, 26) })];
    const f = (q: string) => ids(filterReviews(items, parseTerms(q), 'ja', NOW));
    expect(f('潜在バグ')).toEqual(['a', 'c']);
    expect(f('潜在バグ 今日')).toEqual(['a']);
    expect(f('今日 潜在バグ')).toEqual(['a']);
    expect(f('潜在バグ 一昨日')).toEqual([]);
  });

  it('今日・昨日（英語でも）。日付の一部（11/26）も', () => {
    const items = [S('today'), S('yest', { created_at: at(11, 27) }), S('old', { created_at: at(11, 26) })];
    const f = (q: string) => ids(filterReviews(items, parseTerms(q), 'ja', NOW));
    expect(f('今日')).toEqual(['today']);
    expect(f('today')).toEqual(['today']);
    expect(f('昨日')).toEqual(['yest']);
    expect(f('yesterday')).toEqual(['yest']);
    expect(f('11/26')).toEqual(['old']);
  });

  it('状態の語: 未対応・済み・指摘なし・失敗（日本語・英語）', () => {
    const items = [
      S('open'),
      S('handled', { counts: { open: 0, done: 2, wait: 0, dismiss: 0 } }),
      S('clean', { findings: 0, counts: { open: 0, done: 0, wait: 0, dismiss: 0 } }),
      S('fail', { failed: true, findings: 0, counts: null }),
    ];
    const f = (q: string) => ids(filterReviews(items, parseTerms(q), 'ja', NOW));
    expect(f('未対応')).toEqual(['open']);
    expect(f('open')).toEqual(['open']);
    expect(f('済み')).toEqual(['handled']);
    expect(f('指摘なし')).toEqual(['clean']);
    expect(f('失敗')).toEqual(['fail']);
    expect(f('failed')).toEqual(['fail']);
    expect(f('レビュー')).toEqual(['open', 'handled', 'clean', 'fail']);
    // 「完了」は、レビューにも当てはまる（実行が最後まで行った行。失敗は除く）。フックの「完了」と、同じ意味
    expect(f('完了')).toEqual(['open', 'handled', 'clean']);
    expect(f('done')).toEqual(['open', 'handled', 'clean']);
    expect(f('フック')).toEqual([]);
  });

  it('コミットが無い行も、そのまま絞れる（head が null でも落ちない）', () => {
    expect(reviewHaystack(S('x', { head: null }), 'ja', NOW)).toContain('潜在バグ');
  });
});

describe('フックの行', () => {
  it('「フック」・コミット・件名・観点・状態で、絞れる。レビューの行には、当てはまらない語（フック）', () => {
    const runs = [H('h1'), H('h2', { commit: 'ffff000', subject: '直した', state: 'failed', recipes: ['security'] }), H('h3', { state: 'aborted', commit: null, subject: null })];
    const f = (q: string) => ids(filterHooks(runs.map((r) => ({ ...r, id: r.run })) as never, parseTerms(q), 'ja', NOW).map((r) => ({ id: r.run })));
    expect(f('フック')).toEqual(['h1', 'h2', 'h3']);
    expect(f('a3f9c21')).toEqual(['h1']); // h3 は、コミットの記録が無い
    expect(f('骨格')).toEqual(['h1']);
    expect(f('失敗')).toEqual(['h2']);
    expect(f('中断')).toEqual(['h3']);
    expect(f('security')).toEqual(['h2']);
    expect(f('フック 失敗 直した')).toEqual(['h2']);
    expect(f('レビュー')).toEqual([]);
  });

  it('日付と今日・昨日', () => {
    expect(hookHaystack(H('h'), 'ja', NOW)).toContain('昨日');
    expect(hookHaystack(H('h', { started_at_ms: at(11, 28) }), 'ja', NOW)).toContain('今日');
  });
});

describe('フックの行: 途中経過・終わりのメッセージ・状態の語', () => {
  const steps = (recipe: string) => [{ recipe, index: 1, total: 1, ok: true, findings: 0, ms: 100, error: null }];
  const runs = [
    H('p', { recipes: ['bugs'], steps: steps('perf'), message: '完了' }),
    H('t', { recipes: ['bugs'], steps: steps('bugs'), message: '時間切れ' }),
    H('done', { state: 'done', message: null }),
    H('run', { state: 'running', code: null, message: null, findings: null, ms: null }),
  ];
  const f = (q: string) => filterHooks(runs, parseTerms(q), 'ja', NOW).map((r) => r.run);

  it('実行の途中経過（steps の観点）が、検索の対象に入る。終わりのメッセージは、入れない（状態の語と、混ざらないように）', () => {
    expect(f('perf')).toEqual(['p']);
    expect(f('時間切れ'), 'メッセージの言葉では、当てはまらない').toEqual([]);
  });

  it('メッセージに「失敗」を含む、状態が完了の実行は、「失敗」で絞っても、残らない', () => {
    const odd = [H('ok', { state: 'done', message: '失敗した観点は、ありません' }), H('ng', { state: 'failed', message: '一部の観点が失敗しました' })];
    expect(filterHooks(odd, parseTerms('失敗'), 'ja', NOW).map((r) => r.run)).toEqual(['ng']);
  });

  it('「完了」「実行中」と、英語の done・running も、それぞれ該当する行だけに当てはまる', () => {
    expect(f('実行中')).toEqual(['run']);
    expect(f('running')).toEqual(['run']);
    expect(f('完了')).toEqual(['p', 't', 'done']); // 実行中の行は、含まれない
    expect(f('done')).toEqual(['p', 't', 'done']);
  });
});

describe('全角・大文字の入力（絞り込み経由）', () => {
  it('コミットが abcdef0 の行に、全角で ＡＢＣＤＥＦ０ と入れても、その行だけ残る。全角のスペースで区切った語も、合わせ技になる', () => {
    const items = [S('a', { head: 'abcdef0123456789' }), S('b', { head: '9999999zzz', recipe_title: 'セキュリティ' })];
    const f = (q: string) => ids(filterReviews(items, parseTerms(q), 'ja', NOW));
    expect(f('ＡＢＣＤＥＦ０')).toEqual(['a']);
    expect(f('ABCDEF0')).toEqual(['a']);
    expect(f('ｓｅｃｕｒｉｔｙ')).toEqual([]); // 英字ではなく、カタカナの観点名。全角英字は、一致しない
    expect(f('セキュリティ　９９９９９９９')).toEqual(['b']); // 全角スペース・全角の数字
  });
});

describe('使い回し（入力のたびに、全行を作り直さない）', () => {
  it('検索用の文字列は、行ごとに 1 度だけ作る。語を変えて、何度絞り込んでも、増えない', () => {
    const items = Array.from({ length: 100 }, (_, i) => S(`x${i}`, { recipe_title: `観点${i}` }));
    stats.builds = 0;
    filterReviews(items, parseTerms('観点'), 'ja', NOW);
    expect(stats.builds).toBe(100);
    filterReviews(items, parseTerms('観点 1'), 'ja', NOW);
    filterReviews(items, parseTerms('今日'), 'ja', NOW);
    expect(stats.builds, '2 回目以降は、作り直さない').toBe(100);
    // 言語が変われば、作り直す（日時の書式が違う）
    filterReviews(items, parseTerms('観点'), 'en', NOW);
    expect(stats.builds).toBe(200);
  });

  it('日付が変わっても、今日・昨日の語は、正しい（日の語は、行の文字列に、焼き込まない）', () => {
    const item = S('a', { created_at: at(11, 28) });
    const f = (now: Date, q: string) => ids(filterReviews([item], parseTerms(q), 'ja', now));
    expect(f(new Date(2026, 10, 28, 9), '今日')).toEqual(['a']);
    expect(f(new Date(2026, 10, 29, 9), '今日')).toEqual([]);
    expect(f(new Date(2026, 10, 29, 9), '昨日')).toEqual(['a']);
  });

  it('絞り込みの結果は、同じ入力なら、同じものを返す（1 回の描き直しで、何度求めても、1 回）。入力が変われば、求め直す', () => {
    const view = createHistoryView();
    const history = [S('a'), S('b', { recipe_title: 'セキュリティ' })];
    const runs = [H('h1')];
    const v1 = view(history, runs, '潜在バグ', 'ja', NOW);
    expect(view(history, runs, '潜在バグ', 'ja', NOW)).toBe(v1);
    expect(v1.reviews.map((x) => x.id)).toEqual(['a']);
    expect(view(history, runs, 'セキュリティ', 'ja', NOW)).not.toBe(v1);
    expect(view([...history], runs, '潜在バグ', 'ja', NOW), '履歴が読み直された（別の配列）').not.toBe(v1);
    expect(view(history, runs, '', 'ja', NOW).reviews).toBe(history); // 語が無ければ、そのまま
  });

  it('観点のページの「履歴」ボタンが渡す、画面に出ている観点名（連番なし）で絞ると、その観点の行が残る（英語表示でも）', () => {
    const rows = [S('a', { recipe_title: 'Latent Bugs', seq: 2 }), S('b', { recipe_title: '潜在バグ', seq: 1 }), S('c', { recipe_title: 'セキュリティ' })];
    for (const lang of ['ja', 'en'] as const) {
      for (const r of rows) {
        const shown = displayRecipeTitle(r.recipe_title, lang);
        expect(filterReviews(rows, parseTerms(shown), lang, NOW).map((x) => x.id)).toContain(r.id);
      }
    }
  });

  it('状態の語「判断待ち」「未対応」: 判断待ちのある行は「判断待ち」、未のある行は「未対応」に当てはまる。両方なければ「済み」', () => {
    const rows = [
      S('a', { counts: { open: 1, pending: 0, done: 1, wait: 0, dismiss: 0 } }),
      S('b', { counts: { open: 0, pending: 2, done: 0, wait: 0, dismiss: 0 } }),
      S('c', { counts: { open: 0, pending: 0, done: 2, wait: 0, dismiss: 0 } }),
    ];
    const ids = (q: string) => filterReviews(rows, parseTerms(q), 'ja', NOW).map((x) => x.id);
    expect(ids('判断待ち')).toEqual(['b']);
    expect(ids('未対応')).toEqual(['a']);
    expect(ids('済み')).toEqual(['c']);
  });
});
