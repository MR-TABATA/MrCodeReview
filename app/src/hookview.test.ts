// @vitest-environment jsdom
import { describe, expect, it } from 'vitest';
import { runLogView, currentRecipes, currentSpecs, parseSpecLines, specsToText, enableBlock, groupedRecipes, groupOf, hookSection, newHookUi, selectableRecipes, type HookActions, type HookUiState } from './hookview';
import type { HookRun, HookStatus, Recipe } from './types';

const recipe = (id: string, over: Partial<Recipe> = {}): Recipe => ({ id, title: `観点 ${id}`, source: 'builtin', prompt: '', kind: 'llm', ...over });
const status = (over: Partial<HookStatus> = {}): HookStatus => ({
  repo: '/r',
  enabled: false,
  recipes: ['bugs'],
  spec: null,
  specs: [],
  installed: false,
  cli: '/opt/homebrew/bin/mrcr',
  hook_cli: null,
  blocked: null,
  target_missing: false,
  last_run: null,
  recent: [],
  log_unreadable: 0,
  ...over,
});
const ui = (over: Partial<HookUiState> = {}): HookUiState => ({ ...newHookUi(), status: status(), ...over });
const noop: HookActions = { toggle: () => {}, save: () => {}, pick: () => {}, refresh: () => {} };
const text = (el: HTMLElement): string => el.textContent ?? '';
const buttons = (el: HTMLElement): HTMLButtonElement[] => [...el.querySelectorAll('button')];
const btn = (el: HTMLElement, label: string): HTMLButtonElement => buttons(el).find((b) => b.textContent === label)!;
const sw = (el: HTMLElement): HTMLInputElement => el.querySelector<HTMLInputElement>('input[role=switch]')!;
/** スイッチを切り替える（jsdom では、画面に繋がっていない要素の click は change を起こさないので、直接起こす） */
const flip = (el: HTMLElement): void => {
  const i = sw(el);
  i.checked = !i.checked;
  i.dispatchEvent(new Event('change'));
};
const specBox = (el: HTMLElement): HTMLTextAreaElement => el.querySelector<HTMLTextAreaElement>('textarea.hookspec')!;
const typeSpec = (el: HTMLElement, v: string): void => {
  const i = specBox(el);
  i.value = v;
  i.dispatchEvent(new Event('input'));
};

describe('selectableRecipes', () => {
  it('LLM の観点は、設計書が要るものも含めて選べる。linter は選べない', () => {
    const list = [recipe('bugs'), recipe('spec', { needs_spec: true }), recipe('eslint', { kind: 'tool' })];
    expect(selectableRecipes(list).map((r) => r.id)).toEqual(['bugs', 'spec']);
  });
});

describe('画面で選んでいる値（draft）', () => {
  it('draft が無ければ保存済みの値のまま。設計書は、空白だけなら「指定なし」', () => {
    const st = ui({ status: status({ recipes: ['bugs', 'errors'], spec: 'docs/spec.md', specs: ['docs/spec.md'] }) });
    expect(currentRecipes(st)).toEqual(['bugs', 'errors']);
    expect(currentSpecs(st)).toEqual(['docs/spec.md']);
    expect(currentSpecs({ ...st, specDraft: '  ' })).toEqual([]);
    expect(currentSpecs({ ...st, specDraft: ' a.md ' })).toEqual(['a.md']);
    expect(currentRecipes({ ...st, draft: ['bugs'] })).toEqual(['bugs']);
  });

  it('設計書は 1 行に 1 つ。複数も読める。空行と前後の空白は捨てる。古い状態（spec だけ）も読める', () => {
    expect(parseSpecLines(' a.md \r\n\n b.md\n  \nc.md ')).toEqual(['a.md', 'b.md', 'c.md']);
    expect(parseSpecLines('')).toEqual([]);
    const two = ui({ status: status({ spec: 'docs/req.md', specs: ['docs/req.md', 'docs/design.md'] }) });
    expect(currentSpecs(two)).toEqual(['docs/req.md', 'docs/design.md']);
    expect(specsToText({ spec: 'old.md' })).toBe('old.md'); // specs が無い古い状態
    expect(specsToText({ spec: null, specs: [] })).toBe('');
  });
});

describe('enableBlock（ON にできない理由）', () => {
  it('押せる状態なら null', () => expect(enableBlock(ui())).toBeNull());
  it('読み込み前・処理中は busy', () => {
    expect(enableBlock({ ...newHookUi() })).toBe('busy');
    expect(enableBlock(ui({ busy: true }))).toBe('busy');
  });
  it('CLI が無い・置き場に書けない・観点が空は、それぞれ別の理由', () => {
    expect(enableBlock(ui({ status: status({ cli: null }) }))).toBe('cli');
    expect(enableBlock(ui({ status: status({ blocked: 'husky' }) }))).toBe('blocked');
    expect(enableBlock(ui({ draft: [] }))).toBe('no-recipes');
  });
});

describe('hookSection', () => {
  it('リポジトリが選ばれていないときは、設定を出さない', () => {
    const el = hookSection(ui(), false, [recipe('bugs')], noop, 'ja');
    expect(text(el)).toContain('リポジトリを選ぶと');
    expect(buttons(el)).toHaveLength(0);
  });

  it('読み込み前は読み取り中、失敗したら理由を出す', () => {
    expect(text(hookSection(newHookUi(), true, [], noop, 'ja'))).toContain('読み取っています');
    expect(text(hookSection({ ...newHookUi(), error: 'boom' }, true, [], noop, 'ja'))).toContain('boom');
  });

  it('OFF: 「ON にする」を押すと、いま選んでいる観点で toggle(true)。設計書は空なら null', () => {
    const calls: unknown[] = [];
    const el = hookSection(ui(), true, [recipe('bugs'), recipe('errors')], { ...noop, toggle: (e, r, s) => calls.push([e, r, s]) }, 'ja');
    expect(sw(el).disabled).toBe(false);
    expect(sw(el).checked).toBe(false);
    flip(el);
    expect(calls).toEqual([[true, ['bugs'], []]]);
  });

  it('設計書の入力欄: 保存済みの値が入り、打った値が、ボタンを押したときに渡る（打つ間は画面を作り直さない）', () => {
    const calls: unknown[] = [];
    const st = ui({ status: status({ spec: 'docs/old.md', specs: ['docs/old.md'] }) });
    const el = hookSection(st, true, [recipe('bugs'), recipe('spec', { needs_spec: true })], { ...noop, toggle: (e, r, s) => calls.push([e, r, s]) }, 'ja');
    expect(specBox(el).value).toBe('docs/old.md');
    const same = specBox(el);
    typeSpec(el, ' docs/new.md ');
    expect(specBox(el)).toBe(same);
    expect(st.specDraft).toBe(' docs/new.md ');
    flip(el);
    expect(calls).toEqual([[true, ['bugs'], ['docs/new.md']]]);
    typeSpec(el, '');
    sw(el).checked = false;
    flip(el);
    expect(calls[1]).toEqual([true, ['bugs'], []]);
  });

  it('設計書の入力欄: 複数（1 行に 1 つ）が、保存済みの値から表示され、順番のまま渡る', () => {
    const calls: unknown[] = [];
    const st = ui({ status: status({ spec: 'docs/req.md', specs: ['docs/req.md', 'docs/design.md'] }) });
    const el = hookSection(st, true, [recipe('bugs'), recipe('spec', { needs_spec: true })], { ...noop, toggle: (e, r, s) => calls.push([e, r, s]) }, 'ja');
    expect(specBox(el).value).toBe('docs/req.md\ndocs/design.md');
    typeSpec(el, 'docs/design.md\n\n docs/req.md \ndocs/tasks.md');
    flip(el);
    expect(calls).toEqual([[true, ['bugs'], ['docs/design.md', 'docs/req.md', 'docs/tasks.md']]]);
  });

  it('設計書が要る観点には、印が付く', () => {
    const el = hookSection(ui(), true, [recipe('bugs'), recipe('spec', { needs_spec: true })], noop, 'ja');
    const labels = [...el.querySelectorAll('.hookrecipe')].map((l) => l.textContent ?? '');
    expect(labels[0]).not.toContain('設計書が要る');
    expect(labels[1]).toContain('設計書が要る');
  });

  it('CLI が無いと「ON にする」は押せず、案内（brew・npm・npx の注意）を出す', () => {
    const el = hookSection(ui({ status: status({ cli: null }) }), true, [recipe('bugs')], noop, 'ja');
    expect(sw(el).disabled).toBe(true);
    expect(text(el)).toContain('brew install mr-tabata/tap/mrcr');
    expect(text(el)).toContain('npm i -g @mr-tabata/mrcr');
    expect(text(el)).toContain('npx');
  });

  it('フックが呼ぶ mrcr が消えているときは、入れ直しを案内する', () => {
    const el = hookSection(ui({ status: status({ enabled: true, installed: true, cli: null, hook_cli: '/gone/mrcr', target_missing: true }) }), true, [recipe('bugs')], noop, 'ja');
    expect(text(el)).toContain('フックが呼ぶ mrcr が見つかりません');
  });

  describe('フックが呼び出す mrcr の表示', () => {
    const lines = (s: Partial<HookStatus>) => text(hookSection(ui({ status: status(s) }), true, [recipe('bugs')], noop, 'ja'));

    it('まだ入っていないときは、ON にしたときに使うもの（いま見つかるもの）を出す', () => {
      const t = lines({ installed: false, hook_cli: null, cli: '/opt/homebrew/bin/mrcr' });
      expect(t).toContain('ON にすると、フックが呼び出す mrcr: /opt/homebrew/bin/mrcr');
      expect(t).not.toContain('いま見つかる mrcr は');
    });

    it('入っているときは、フックに実際に書かれているパスを出す。いま見つかるものと同じなら、注記は無い', () => {
      const t = lines({ enabled: true, installed: true, hook_cli: '/opt/homebrew/bin/mrcr', cli: '/opt/homebrew/bin/mrcr' });
      expect(t).toContain('フックが呼び出す mrcr: /opt/homebrew/bin/mrcr');
      expect(t).not.toContain('いま見つかる mrcr は');
      expect(t).not.toContain('ON にすると、フックが');
    });

    it('書かれているパスと、いま見つかるものが違えば、両方を出して、入れ直しを案内する', () => {
      const t = lines({ enabled: true, installed: true, hook_cli: '/old/bin/mrcr', cli: '/new/bin/mrcr' });
      expect(t).toContain('フックが呼び出す mrcr: /old/bin/mrcr');
      expect(t).toContain('いま見つかる mrcr は /new/bin/mrcr です');
      expect(t).toContain('OFF にしてから ON にし直す');
    });

    it('入っていて、いま mrcr が見つからなくても、書かれているパスがあれば、「見つかりません」とは言わない（消えていれば、別の警告が出る）', () => {
      const t = lines({ enabled: true, installed: true, hook_cli: '/opt/x/mrcr', cli: null, target_missing: false });
      expect(t).toContain('フックが呼び出す mrcr: /opt/x/mrcr');
      expect(t).not.toContain('mrcr（CLI）が見つかりません');
    });

    it('どちらも無ければ、入れ方を案内する。英語でも出る', () => {
      expect(lines({ installed: false, hook_cli: null, cli: null })).toContain('mrcr（CLI）が見つかりません');
      const en = text(hookSection(ui({ status: status({ installed: false, hook_cli: null, cli: '/x/mrcr' }) }), true, [recipe('bugs')], noop, 'en'));
      expect(en).toContain('When turned ON, the hook will call mrcr: /x/mrcr');
    });
  });

  it('フックを置けないリポジトリは、理由を出して押せない', () => {
    const el = hookSection(ui({ status: status({ blocked: 'core.hooksPath' }) }), true, [recipe('bugs')], noop, 'ja');
    expect(text(el)).toContain('core.hooksPath');
    expect(sw(el).disabled).toBe(true);
  });

  it('ON: スイッチは ON で、切り替えると toggle(false)。OFF にするのは、処理中でなければ、いつでもできる', () => {
    const calls: unknown[] = [];
    const on = ui({ status: status({ enabled: true, installed: true, cli: null }), draft: ['bugs'] });
    const el = hookSection(on, true, [recipe('bugs')], { ...noop, toggle: (e, r, s) => calls.push([e, r, s]) }, 'ja');
    expect(sw(el).checked).toBe(true);
    expect(sw(el).disabled).toBe(false);
    flip(el);
    expect(calls).toEqual([[false, ['bugs'], []]]);
    expect(sw(hookSection({ ...on, busy: true }, true, [recipe('bugs')], noop, 'ja')).disabled).toBe(true);
  });

  it('ON: 「保存する」は状態の行（スイッチの隣）にだけあり、いまの観点と設計書を渡す。「OFF にする」というボタンは無い', () => {
    const calls: unknown[] = [];
    const on = ui({ status: status({ enabled: true, installed: true, spec: 'docs/spec.md', specs: ['docs/spec.md'] }), draft: ['bugs', 'errors'] });
    const el = hookSection(on, true, [recipe('bugs'), recipe('errors')], { ...noop, save: (r, s) => calls.push([r, s]) }, 'ja');
    const state = el.querySelector('.hookstate')!;
    expect(state.textContent).toContain('ON');
    expect(state.querySelector('input[role=switch]')).not.toBeNull();
    expect([...state.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['保存する']);
    expect(buttons(el).map((b) => b.textContent).filter((t) => t === 'OFF にする' || t === 'ON にする')).toEqual([]);
    btn(el, '保存する').click();
    expect(calls).toEqual([[['bugs', 'errors'], ['docs/spec.md']]]);
  });

  it('OFF のときは「保存する」を出さない（スイッチを ON にするとき、設定をまとめて保存する）', () => {
    const el = hookSection(ui(), true, [recipe('bugs')], noop, 'ja');
    expect(el.querySelector('.hookstate button')).toBeNull();
    expect(buttons(el).map((b) => b.textContent)).not.toContain('保存する');
  });

  it('観点のチェックを変えると pick に、新しい選択が渡る', () => {
    const picked: string[][] = [];
    const el = hookSection(ui(), true, [recipe('bugs'), recipe('errors')], { ...noop, pick: (r) => picked.push(r) }, 'ja');
    const boxes = [...el.querySelectorAll<HTMLInputElement>('.hookrecipe input[type=checkbox]')];
    expect(boxes.map((b) => b.checked)).toEqual([true, false]);
    boxes[1].checked = true;
    boxes[1].dispatchEvent(new Event('change'));
    expect(picked).toEqual([['bugs', 'errors']]);
  });

  it('runLogView: コミット・状態・観点ごとの結果が出る', () => {
    const r = run({ commit: 'bbb2222', subject: '新しい' });
    const t = text(runLogView(r, 'ja'));
    expect(t).toContain('コミット bbb2222「新しい」');
    expect(t).toContain('[1/2]');
    expect(t).toContain('指摘 3 件');
    expect(t).toContain('7.5 秒');
  });

  it('操作の失敗と、読み取りの失敗は、別々に出る（状態があるときも）', () => {
    const both = text(hookSection(ui({ error: '設計書が要ります', loadError: '接続できません' }), true, [], noop, 'ja'));
    expect(both).toContain('操作に失敗しました: 設計書が要ります');
    expect(both).toContain('最新の状態を読み取れませんでした: 接続できません');
    expect(text(hookSection(ui({ loadError: '接続できません' }), true, [], noop, 'ja'))).not.toContain('操作に失敗しました');
    expect(text(hookSection({ ...newHookUi(), loadError: 'x' }, true, [], noop, 'ja'))).toContain('x');
  });

  it('英語でも出る', () => {
    const t = text(hookSection(ui(), true, [recipe('bugs')], noop, 'en'));
    expect(sw(hookSection(ui(), true, [recipe('bugs')], noop, 'en')).title).toBe('Turn ON');
    expect(t).toContain('Spec document');
  });
});

const ALL = [
  recipe('bugs'), recipe('errors'), recipe('security'), recipe('performance'), recipe('tests'), recipe('consistency'),
  recipe('tf-state'), recipe('tf-replace'), recipe('tf-repro'), recipe('tf-security'), recipe('tf-refs'),
  recipe('doc-core'), recipe('doc-claims'), recipe('doc-reality'), recipe('doc-steps'),
  recipe('spec', { needs_spec: true }),
  recipe('mine', { source: 'repo' }),
];

describe('観点のグルーピング', () => {
  it('groupOf: コード・Terraform・文書・その他。spec はコード。自分で足した観点は、その他', () => {
    expect(['bugs', 'security', 'spec'].map((id) => groupOf(recipe(id)))).toEqual(['code', 'code', 'code']);
    expect(['tf-state', 'tf-refs'].map((id) => groupOf(recipe(id)))).toEqual(['tf', 'tf']);
    expect(['doc-core', 'doc-claims', 'doc-steps'].map((id) => groupOf(recipe(id)))).toEqual(['doc', 'doc', 'doc']);
    expect(groupOf(recipe('mine', { source: 'repo' }))).toBe('other');
    expect(groupOf(recipe('bugs', { source: 'repo' }))).toBe('code');
  });

  it('groupedRecipes: コード → Terraform → 文書 → その他の順。空の種類は出さない', () => {
    const g = groupedRecipes(ALL);
    expect(g.map((x) => x.key)).toEqual(['code', 'tf', 'doc', 'other']);
    expect(g[0].items.map((r) => r.id)).toEqual(['bugs', 'errors', 'security', 'performance', 'tests', 'consistency', 'spec']);
    expect(g[1].items).toHaveLength(5);
    expect(g[2].items).toHaveLength(4);
    expect(groupedRecipes([recipe('bugs')]).map((x) => x.key)).toEqual(['code']);
  });

  it('種類ごとに、見出し・選択数・説明が出る', () => {
    const el = hookSection(ui({ draft: ['bugs', 'tf-state', 'tf-refs'] }), true, ALL, noop, 'ja');
    const heads = [...el.querySelectorAll('.hookgroup-head')].map((h) => h.textContent ?? '');
    expect(heads).toHaveLength(4);
    expect(heads[0]).toContain('コード');
    expect(heads[0]).toContain('1 / 7 選択中');
    expect(heads[1]).toContain('Terraform（tfstate）');
    expect(heads[1]).toContain('2 / 5 選択中');
    expect(heads[2]).toContain('文書');
    expect(heads[2]).toContain('0 / 4 選択中');
    expect(heads[3]).toContain('自分で足した観点');
  });

  const groupBtn = (el: HTMLElement, group: string, label: string): HTMLButtonElement =>
    [...el.querySelectorAll<HTMLButtonElement>(`.hookgroup-${group} .hookgroup-head button`)].find((b) => b.textContent === label)!;

  it('「すべて選ぶ」は、その種類だけを足す（ほかの種類の選択は、そのまま）', () => {
    const picked: string[][] = [];
    const el = hookSection(ui({ draft: ['bugs'] }), true, ALL, { ...noop, pick: (r) => picked.push(r) }, 'ja');
    groupBtn(el, 'tf', 'すべて選ぶ').click();
    expect(picked[0]).toEqual(['bugs', 'tf-state', 'tf-replace', 'tf-repro', 'tf-security', 'tf-refs']);
    groupBtn(el, 'doc', 'すべて選ぶ').click();
    expect(picked[1]).toEqual(['bugs', 'doc-core', 'doc-claims', 'doc-reality', 'doc-steps']);
  });

  it('「すべて外す」は、その種類だけを外す', () => {
    const picked: string[][] = [];
    const draft = ['bugs', 'errors', 'tf-state', 'doc-claims'];
    const el = hookSection(ui({ draft }), true, ALL, { ...noop, pick: (r) => picked.push(r) }, 'ja');
    groupBtn(el, 'code', 'すべて外す').click();
    expect(picked[0]).toEqual(['tf-state', 'doc-claims']);
  });

  it('全部選んでいれば「すべて選ぶ」、何も選んでいなければ「すべて外す」は、押せない。処理中は、どちらも押せない', () => {
    const allDoc = ['doc-core', 'doc-claims', 'doc-reality', 'doc-steps'];
    const el = hookSection(ui({ draft: allDoc }), true, ALL, noop, 'ja');
    expect(groupBtn(el, 'doc', 'すべて選ぶ').disabled).toBe(true);
    expect(groupBtn(el, 'doc', 'すべて外す').disabled).toBe(false);
    expect(groupBtn(el, 'tf', 'すべて外す').disabled).toBe(true);
    const busy = hookSection(ui({ draft: allDoc, busy: true }), true, ALL, noop, 'ja');
    expect(groupBtn(busy, 'doc', 'すべて外す').disabled).toBe(true);
  });

  it('英語でも見出しが出る', () => {
    const el = hookSection(ui(), true, ALL, noop, 'en');
    expect(el.querySelector('.hookgroup-head')!.textContent).toContain('Code');
    expect(text(el)).toContain('Select all');
  });
});

const run = (over: Partial<HookRun> = {}): HookRun => ({
  run: 'r1',
  started_at_ms: 1_700_000_000_000,
  commit: 'abc1234',
  subject: '件名',
  recipes: ['bugs', 'spec'],
  steps: [
    { recipe: 'bugs', index: 1, total: 2, ok: true, findings: 3, ms: 7500, error: null },
    { recipe: 'spec', index: 2, total: 2, ok: true, findings: 0, ms: 4000, error: null },
  ],
  state: 'done',
  code: 1,
  message: 'レビュー完了（指摘あり 3 件）',
  findings: 3,
  ms: 11600,
  ...over,
});

describe('runLogView', () => {
  it('完了: コミット・状態・件数・全体の時間・各観点・終わりのメッセージ', () => {
    const t = text(runLogView(run(), 'ja'));
    expect(t).toContain('コミット abc1234「件名」');
    expect(t).toContain('完了');
    expect(t).toContain('指摘 3 件・11.6 秒');
    expect(t).toContain('[1/2]');
    expect(t).toContain('bugs');
    expect(t).toContain('→ レビュー完了（指摘あり 3 件）');
  });

  it('失敗した観点は、理由つきで赤く出る。失敗した実行は、失敗の印が付く', () => {
    const failed = run({
      state: 'failed',
      code: 2,
      message: '一部の観点が失敗しました',
      steps: [
        { recipe: 'bugs', index: 1, total: 2, ok: true, findings: 1, ms: 1000, error: null },
        { recipe: 'spec', index: 2, total: 2, ok: false, findings: 0, ms: 200, error: 'API キーがありません' },
      ],
    });
    const el = runLogView(failed, 'ja');
    expect(el.className).toContain('hookrun-failed');
    expect(el.querySelector('.hookstep-failed')!.textContent).toContain('失敗: API キーがありません');
    expect(text(el)).toContain('失敗');
  });

  it('実行中: まだ終わっていない次の観点が「実行中…」と出る。件数や時間は、まだ出さない', () => {
    const running = run({ state: 'running', code: null, message: null, findings: null, ms: null, steps: [{ recipe: 'bugs', index: 1, total: 2, ok: true, findings: 3, ms: 7500, error: null }] });
    const el = runLogView(running, 'ja');
    expect(text(el)).toContain('実行中');
    expect(el.querySelector('.hookstep-running')!.textContent).toContain('spec');
    expect(el.querySelector('.hookstep-running')!.textContent).toContain('実行中…');
    expect(el.querySelector('.hookrun-head')!.textContent).not.toContain('秒');
  });

  it('コミットの記録が無い従来の 1 行も、出せる。中断も、そう出る。英語でも出る', () => {
    expect(text(runLogView(run({ commit: null, subject: null, steps: [] }), 'ja'))).toContain('コミットの記録なし');
    expect(text(runLogView(run({ state: 'aborted' }), 'ja'))).toContain('中断');
    expect(text(runLogView(run(), 'en'))).toContain('Commit abc1234 "件名"');
  });

  it('設定画面（hookSection）: 最近の実行ログがあっても実行ログ欄は表示されない', () => {
    const s = ui({
      status: status({
        recent: [run()],
      }),
    });
    const el = hookSection(s, true, [recipe('bugs')], noop, 'ja');
    expect(text(el)).not.toContain('最近の実行');
    expect(text(el)).not.toContain('まだ実行されていません');
    expect(text(el)).not.toContain('abc1234');
    expect(el.querySelector('.hookruns')).toBeNull();
  });
});
