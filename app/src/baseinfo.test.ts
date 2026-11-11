import { describe, it, expect } from 'vitest';
import { baseItems, baseLabel, hasLastCommit, rangeText, CUSTOM } from './baseinfo';
import type { Base, BaseOption } from './types';

const B = (over: Partial<Base>): Base => ({ spec: 'HEAD~1', hash: 'b869afd0123456789012345678901234567890ab', subject: '画面を組み替え', ...over });
const OPTS: BaseOption[] = [
  { value: 'HEAD', label: '未コミットの変更', hint: 'h0' },
  { value: 'HEAD~1', label: '直近のコミット', hint: 'h1' },
  { value: 'abc123', label: 'main との分岐点から', hint: 'h2' },
  { value: ':root', label: '最初から（リポジトリ全体）', hint: 'h3' },
  { value: 'def456^', label: '16ae9ab 実行中の…', hint: 'このコミットの変更から' },
];

describe('baseLabel', () => {
  it('HEAD は「未コミットの変更」。それ以外は、短いハッシュと説明', () => {
    expect(baseLabel(B({ spec: 'HEAD' }))).toBe('未コミットの変更');
    expect(baseLabel(B({}))).toBe('b869afd 画面を組み替え から');
  });
  it('「最初から」は、コミットの名前ではなく、リポジトリ全体と言う', () => {
    expect(baseLabel(B({ spec: ':root', hash: '4b825dc642cb6eb9a060e54bf8d69288fbee4904', subject: '（何も無い状態）' }))).toBe('最初から（リポジトリ全体）');
  });
  it('長い説明は切る。古い記録（比較元が無い）・ハッシュが空は、何も言わない', () => {
    expect(baseLabel(B({ subject: 'あ'.repeat(60) }))).toBe(`b869afd ${'あ'.repeat(40)}… から`);
    expect(baseLabel(undefined)).toBe('');
    expect(baseLabel(null)).toBe('');
    expect(baseLabel(B({ hash: '' }))).toBe('');
  });
});

describe('baseItems', () => {
  it('一覧を、グループ分けして、最後に「任意の場所…」を付ける', () => {
    const items = baseItems(OPTS, 'HEAD');
    expect(items.map((i) => [i.value, i.group])).toEqual([
      ['HEAD', '範囲を選ぶ'],
      ['HEAD~1', '範囲を選ぶ'],
      ['abc123', '範囲を選ぶ'],
      [':root', '範囲を選ぶ'],
      ['def456^', '最近のコミットから'],
      [CUSTOM, 'ほか'],
    ]);
    expect(items[4]?.desc).toBe('このコミットの変更から');
  });
  it('自分で入力した場所が、一覧に無ければ、先頭に足す。あれば足さない', () => {
    const typed = baseItems(OPTS, 'v1.2.0');
    expect(typed[0]).toMatchObject({ value: 'v1.2.0', title: 'v1.2.0 から', desc: '入力した場所' });
    expect(baseItems(OPTS, 'HEAD~1').filter((i) => i.value === 'HEAD~1')).toHaveLength(1);
    expect(baseItems(OPTS, 'HEAD')).toHaveLength(OPTS.length + 1);
  });
  it('選択肢が空でも、任意の場所は選べる', () => {
    expect(baseItems([], 'HEAD').map((i) => i.value)).toEqual([CUSTOM]);
  });
});

describe('hasLastCommit', () => {
  it('HEAD~1 が一覧にあるときだけ（コミットが 1 つしか無ければ、無い）', () => {
    expect(hasLastCommit(OPTS)).toBe(true);
    expect(hasLastCommit([OPTS[0]!])).toBe(false);
  });
});

describe('rangeText（「送る内容」の、見る範囲の 1 行）', () => {
  const SCOPE = '（コミット済み・未コミット・新規ファイル）';

  it('未コミットの変更・最初から・直近のコミットは、決まった言い方', () => {
    expect(rangeText(B({ spec: 'HEAD' }), OPTS)).toBe('コミットしていない変更だけ');
    expect(rangeText(B({ spec: '' }), OPTS)).toBe('コミットしていない変更だけ');
    expect(rangeText(null, OPTS)).toBe('コミットしていない変更だけ');
    expect(rangeText(B({ spec: ':root' }), OPTS)).toBe('最初から（リポジトリ全体）');
    expect(rangeText(B({ spec: 'HEAD~1' }), OPTS)).toBe('最後のコミットと、コミットしていない変更');
  });

  it('「ハッシュ^」は、親ではなく、選んだコミットの名前で言う（そのコミットを含む）', () => {
    const o = OPTS.find((x) => x.value === 'def456^')!;
    // 内部の比較先（base）は、親のコミット。画面には、親の名前（subject）を出さない
    const b = B({ spec: 'def456^', hash: '2ff1b41aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', subject: '親のコミット' });
    const text = rangeText(b, OPTS);
    expect(text).toBe(`${o.label} 以降すべて${SCOPE}`);
    expect(text).not.toContain('親のコミット');
    expect(text).not.toContain('2ff1b41');
  });

  it('一覧に無い「ハッシュ^」（一覧が古い）は、ハッシュの先頭 7 文字で言う', () => {
    expect(rangeText(B({ spec: '1234567890abcdef^' }), OPTS)).toBe(`1234567 以降すべて${SCOPE}`);
  });

  it('分岐点は、ブランチの名前で言う', () => {
    expect(rangeText(B({ spec: 'abc123' }), OPTS)).toBe(`main から分かれたあとの変更すべて${SCOPE}`);
  });

  it('自分で入力した場所（タグ・ブランチなど）は、その名前から、いままで', () => {
    const b = B({ spec: 'v1.0', hash: 'b869afd0123456789012345678901234567890ab', subject: 'リリース' });
    expect(rangeText(b, OPTS)).toBe(`b869afd リリース から、いままでの変更すべて${SCOPE}`);
  });

  it('英語表示時に from From と重複せず、自然な英語になる', () => {
    const b = B({ spec: 'v1.0', hash: 'b869afd0123456789012345678901234567890ab', subject: 'Release' });
    expect(rangeText(b, OPTS, 'en')).toBe('All changes from b869afd Release (committed, uncommitted, untracked)');
    expect(rangeText(b, OPTS, 'en')).not.toContain('from From');
  });

  it('英語表示で baseLabel・rangeText・baseItems の各分岐が英語で返る', () => {
    const SCOPE_EN = ' (committed, uncommitted, untracked)';
    expect(baseLabel(B({ spec: 'HEAD' }), 'en')).toBe('Uncommitted changes');
    expect(baseLabel(B({ spec: ':root' }), 'en')).toBe('Entire repository (from start)');
    expect(baseLabel(B({ spec: 'HEAD~1', subject: 'init' }), 'en')).toBe('From b869afd init');

    expect(rangeText(B({ spec: 'HEAD' }), OPTS, 'en')).toBe('Only uncommitted changes');
    expect(rangeText(B({ spec: ':root' }), OPTS, 'en')).toBe('Entire repository (from start)');
    expect(rangeText(B({ spec: 'HEAD~1' }), OPTS, 'en')).toBe('Last commit and uncommitted changes');
    expect(rangeText(B({ spec: 'abc1234^', hash: '1111111', subject: 'parent' }), [], 'en')).toBe(
      `All changes from abc1234 forward${SCOPE_EN}`,
    );
    const enOpts: BaseOption[] = [
      { value: 'HEAD', label: 'Uncommitted changes', hint: 'Only uncommitted changes' },
      { value: 'HEAD~1', label: '直近のコミット', hint: 'b869afd init（最後のコミットと、コミットしていない変更）' },
      { value: 'abc123', label: 'Fork point with main', hint: 'b869afd init (All changes since branching from main)' },
      { value: ':root', label: '最初から（リポジトリ全体）', hint: 'h3' },
      { value: 'def456^', label: '16ae9ab feat', hint: 'このコミット以降すべて（このコミットを含む）' },
    ];
    expect(rangeText(B({ spec: 'abc123' }), enOpts, 'en')).toBe(
      `All changes after branching from main${SCOPE_EN}`,
    );

    const items = baseItems(enOpts, 'v2.0', 'en');
    expect(items[0]).toMatchObject({ value: 'v2.0', title: 'From v2.0', desc: 'Custom location', group: 'Select Range' });
    expect(items[1]).toMatchObject({ value: 'HEAD', title: 'Uncommitted changes', desc: 'Only uncommitted changes', group: 'Select Range' });
    expect(items[2]).toMatchObject({ value: 'HEAD~1', title: 'Latest commit', desc: 'b869afd init (Last commit and uncommitted changes)', group: 'Select Range' });
    expect(items[4]).toMatchObject({ value: ':root', title: 'Entire repository (from start)', group: 'Select Range' });
    expect(items[5]).toMatchObject({
      value: 'def456^',
      title: '16ae9ab feat',
      desc: 'All changes from this commit forward (including this commit)',
      group: 'From Recent Commits',
    });
    expect(items[items.length - 1]).toMatchObject({ value: CUSTOM, title: 'Custom location…', group: 'Other' });
  });
});
