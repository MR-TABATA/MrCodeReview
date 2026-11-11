// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { settingsPanel, type SettingsPanelState } from './settingsui';
import type { SettingMeta, SettingsView } from './types';

function meta(over: Partial<SettingMeta> & { key: string }): SettingMeta {
  return {
    group: 'g',
    label: `${over.key}の名前`,
    kind: 'integer',
    unit: '',
    description: `${over.key}の説明`,
    why_recommended: `${over.key}がこの値な理由`,
    when_to_change: `${over.key}を変えるとき`,
    recommended: 10,
    min: 1,
    max: 100,
    repo_scope: true,
    ...over,
  };
}

const META: SettingMeta[] = [
  meta({ key: 'max_file_bytes', recommended: 60_000, min: 1_000, max: 5_000_000, unit: 'バイト', group: '送る差分' }),
  meta({ key: 'max_total_bytes', recommended: 200_000, min: 1_000, max: 20_000_000, unit: 'バイト', group: '送る差分' }),
  meta({ key: 'fuzzy_min', kind: 'number', recommended: 0.9, min: 0.5, max: 1, group: '引き継ぎ' }),
  meta({ key: 'ignored_lockfiles', kind: 'list', recommended: ['a.lock'], min: null, max: null, group: '送る差分' }),
  meta({ key: 'history_limit', recommended: 500, min: 10, max: 10_000, group: '画面', repo_scope: false }),
];

const RECOMMENDED = { max_file_bytes: 60_000, max_total_bytes: 200_000, fuzzy_min: 0.9, ignored_lockfiles: ['a.lock'], history_limit: 500 };

function setup(values: Record<string, unknown> = RECOMMENDED, error: string | null = null) {
  const view: SettingsView = { values: { ...values }, meta: META, path: '/data/settings.json', exists: !!error, error, repo: { path: null, keys: [], error: null } };
  const st: SettingsPanelState = { view, draft: JSON.parse(JSON.stringify(values)), saving: false, message: '', messageKind: '' };
  const act = { save: vi.fn(), saveAndClose: vi.fn(), close: vi.fn() };
  const el = settingsPanel(st, act);
  document.body.replaceChildren(el);
  const card = (key: string) => [...el.querySelectorAll('.scard')].find((c) => c.querySelector('code')?.textContent === key) as HTMLElement;
  const input = (key: string) => card(key).querySelector('input, textarea') as HTMLInputElement;
  const type = (key: string, text: string) => {
    const i = input(key);
    i.value = text;
    i.dispatchEvent(new Event('input', { bubbles: true }));
  };
  const button = (label: string) => [...el.querySelectorAll('button')].find((b) => b.textContent === label) as HTMLButtonElement;
  return { st, act, el, card, input, type, button, tally: () => el.querySelector('.tally')!.textContent!, err: (key: string) => card(key).querySelector('.serr') as HTMLElement };
}

describe('設定画面: 開いたとき', () => {
  it('項目はグループごとに並び、説明・推奨値・「なぜ」が出る', () => {
    const { el, card } = setup();
    expect([...el.querySelectorAll('.sgroup h3')].map((h) => h.textContent)).toEqual(['送る差分', '画面', '引き継ぎ']); // タブの順（「引き継ぎ」は、どのタブにも無いので、「その他」で最後）
    const c = card('max_file_bytes');
    expect(c.textContent).toContain('max_file_bytesの名前');
    expect(c.textContent).toContain('max_file_bytesの説明');
    expect(c.textContent).toContain('推奨: 60,000バイト');
    expect(c.textContent).toContain('範囲: 1,000〜5,000,000バイト');
    expect(c.textContent).toContain('max_file_bytesがこの値な理由');
    expect(c.textContent).toContain('max_file_bytesを変えるとき');
    expect(el.textContent).toContain('/data/settings.json');
  });

  it('推奨値のままなら、変更なし・保存できない・取り消しも戻すもできない', () => {
    const { tally, button, card } = setup();
    expect(tally()).toBe('推奨と違う項目 0 件');
    expect(button('保存').disabled).toBe(true);
    expect(button('変更を取り消す').disabled).toBe(true);
    expect(button('すべて推奨値に戻す').disabled).toBe(true);
    expect((card('max_file_bytes').querySelector('.badge.warnb') as HTMLElement).hidden).toBe(true);
    expect(([...card('max_file_bytes').querySelectorAll('button')].find((b) => b.textContent === '推奨に戻す') as HTMLButtonElement).disabled).toBe(true);
  });

  it('保存済みの設定が推奨と違えば、印を出す（まだ未保存の変更ではない）', () => {
    const { tally, card, button } = setup({ ...RECOMMENDED, max_file_bytes: 120_000 });
    expect(tally()).toBe('推奨と違う項目 1 件');
    expect((card('max_file_bytes').querySelector('.badge.warnb') as HTMLElement).hidden).toBe(false);
    expect(button('保存').disabled).toBe(true);
    expect(button('すべて推奨値に戻す').disabled).toBe(false);
  });

  it('設定ファイルが正しくないときは、理由と、保存したときの扱いを出す', () => {
    const { el } = setup(RECOMMENDED, '設定 `fuzzy_min` が範囲外です');
    expect(el.querySelector('.err')!.textContent).toContain('設定 `fuzzy_min` が範囲外です');
    expect(el.querySelector('.err')!.textContent).toContain('settings.json.bak');
  });
});

describe('設定画面: 入力したとき', () => {
  it('正しい値に変えると、変更の印が付き、保存できる。入力欄は作り直されない', () => {
    const { type, tally, button, card, input } = setup();
    const before = input('max_file_bytes');
    type('max_file_bytes', '120000');
    expect(input('max_file_bytes')).toBe(before); // フォーカスを失わない
    expect(tally()).toBe('推奨と違う項目 1 件 ・ 未保存の変更 1 件');
    expect(button('保存').disabled).toBe(false);
    expect(card('max_file_bytes').classList.contains('dirty')).toBe(true);
    expect((card('max_file_bytes').querySelector('.badge.warnb') as HTMLElement).hidden).toBe(false);
  });

  it('範囲外・数値でない入力は、その場で理由を出し、保存できなくする', () => {
    const { type, err, button, tally, card } = setup();
    type('max_file_bytes', '5');
    expect(err('max_file_bytes').hidden).toBe(false);
    expect(err('max_file_bytes').textContent).toBe('1,000〜5,000,000バイト の範囲で入れてください');
    expect(button('保存').disabled).toBe(true);
    expect(tally()).toContain('入力に誤り 1 件');
    expect(card('max_file_bytes').classList.contains('invalid')).toBe(true);
    type('max_file_bytes', 'たくさん');
    expect(err('max_file_bytes').textContent).toBe('数値を入れてください');
    type('max_file_bytes', '');
    expect(err('max_file_bytes').textContent).toBe('数値を入れてください');
    type('max_file_bytes', '60000'); // 推奨値に戻る → 誤りは消え、変更もなくなる
    expect(err('max_file_bytes').hidden).toBe(true);
    expect(button('保存').disabled).toBe(true);
    expect(tally()).toBe('推奨と違う項目 0 件');
  });

  it('項目をまたぐ誤り（合計 < 1 ファイル）も、その場で出る', () => {
    const { type, err, button } = setup();
    type('max_file_bytes', '500000');
    expect(err('max_total_bytes').textContent).toBe('1 ファイルの上限（500,000）以上にしてください');
    expect(button('保存').disabled).toBe(true);
    type('max_total_bytes', '600000');
    expect(err('max_total_bytes').hidden).toBe(true);
    expect(button('保存').disabled).toBe(false);
  });

  it('リストは、1 行 1 件で読み、パスは断る', () => {
    const { type, st, err, button } = setup();
    type('ignored_lockfiles', 'a.lock\n  bun.lockb  \n\n');
    expect(st.draft.ignored_lockfiles).toEqual(['a.lock', 'bun.lockb']);
    expect(err('ignored_lockfiles').hidden).toBe(true);
    expect(button('保存').disabled).toBe(false);
    type('ignored_lockfiles', 'dir/x.lock');
    expect(err('ignored_lockfiles').textContent).toContain('ファイル名だけ');
    expect(button('保存').disabled).toBe(true);
  });

  it('小数の項目は、小数を受け付ける', () => {
    const { type, err, st } = setup();
    type('fuzzy_min', '0.85');
    expect(err('fuzzy_min').hidden).toBe(true);
    expect(st.draft.fuzzy_min).toBe(0.85);
  });
});

describe('設定画面: 戻す・取り消す・保存する', () => {
  it('「推奨に戻す」は、その項目だけを推奨値に戻し、入力欄にも反映する', () => {
    const { type, card, input, st, tally } = setup();
    type('max_file_bytes', '120000');
    type('fuzzy_min', '0.8');
    const reset = [...card('max_file_bytes').querySelectorAll('button')].find((b) => b.textContent === '推奨に戻す') as HTMLButtonElement;
    expect(reset.disabled).toBe(false);
    reset.click();
    expect(input('max_file_bytes').value).toBe('60000');
    expect(st.draft.max_file_bytes).toBe(60_000);
    expect(st.draft.fuzzy_min).toBe(0.8);
    expect(reset.disabled).toBe(true);
    expect(tally()).toBe('推奨と違う項目 1 件 ・ 未保存の変更 1 件');
  });

  it('「変更を取り消す」は、保存済みの値に戻す（推奨値ではない）', () => {
    const { type, button, input, st } = setup({ ...RECOMMENDED, max_file_bytes: 120_000 });
    type('max_file_bytes', '300000');
    type('ignored_lockfiles', 'x.lock');
    button('変更を取り消す').click();
    expect(input('max_file_bytes').value).toBe('120000');
    expect(input('ignored_lockfiles').value).toBe('a.lock');
    expect(st.draft).toEqual({ ...RECOMMENDED, max_file_bytes: 120_000 });
    expect(button('変更を取り消す').disabled).toBe(true);
  });

  it('「すべて推奨値に戻す」は、全項目を推奨値にして、未保存の変更にする', () => {
    const { button, input, tally, st } = setup({ max_file_bytes: 120_000, max_total_bytes: 900_000, fuzzy_min: 0.7, ignored_lockfiles: ['z.lock'], history_limit: 700 });
    expect(tally()).toBe('推奨と違う項目 5 件');
    button('すべて推奨値に戻す').click();
    expect(st.draft).toEqual(RECOMMENDED);
    expect(input('fuzzy_min').value).toBe('0.9');
    expect(input('ignored_lockfiles').value).toBe('a.lock');
    expect(tally()).toBe('推奨と違う項目 0 件 ・ 未保存の変更 5 件');
    expect(button('保存').disabled).toBe(false); // 保存するまで、ファイルは変わらない
  });
});

describe('設定画面: 保存と閉じる', () => {
  it('保存ボタンは、変更があって誤りが無いときだけ押せて、押すと一度だけ呼ぶ', () => {
    const { type, button, act, st } = setup();
    type('max_file_bytes', '120000');
    button('保存').click();
    expect(act.save).toHaveBeenCalledTimes(1);
    st.view.values.max_file_bytes = 120_000; // 保存できた（実際は、保存した値で、画面が差し替わる）
    button('閉じる').click();
    expect(act.close, '保存済みなら、確認なしで閉じる').toHaveBeenCalledTimes(1);
  });

  it('保存中は、保存・取り消し・すべて推奨値に戻す、がすべて押せない', () => {
    const view: SettingsView = { values: { ...RECOMMENDED, max_file_bytes: 120_000 }, meta: META, path: '/x', exists: true, error: null, repo: { path: null, keys: [], error: null } };
    const st: SettingsPanelState = { view, draft: { ...RECOMMENDED, max_file_bytes: 99_000 }, saving: true, message: '', messageKind: '' };
    const el = settingsPanel(st, { save: vi.fn(), saveAndClose: vi.fn(), close: vi.fn() });
    const b = (label: string) => [...el.querySelectorAll('button')].find((x) => x.textContent?.startsWith(label)) as HTMLButtonElement;
    expect(b('保存中').disabled).toBe(true);
    expect(b('変更を取り消す').disabled).toBe(true);
    expect(b('すべて推奨値に戻す').disabled).toBe(true);
  });

  it('保存の結果のメッセージを、成功と失敗で出し分ける', () => {
    const view: SettingsView = { values: { ...RECOMMENDED }, meta: META, path: '/x', exists: false, error: null, repo: { path: null, keys: [], error: null } };
    const mk = (message: string, kind: 'ok' | 'err') =>
      settingsPanel({ view, draft: { ...RECOMMENDED }, saving: false, message, messageKind: kind }, { save: vi.fn(), saveAndClose: vi.fn(), close: vi.fn() }).querySelector('.smsg')!;
    expect(mk('保存しました', 'ok').className).toBe('smsg ok');
    expect(mk('設定 `x` が範囲外', 'err').className).toBe('smsg err');
    expect(mk('設定 `x` が範囲外', 'err').textContent).toBe('設定 `x` が範囲外');
  });
});

describe('設定画面: リポジトリ側の設定', () => {
  function withRepo(repo: SettingsView['repo']) {
    const view: SettingsView = { values: { ...RECOMMENDED }, meta: META, path: '/x', exists: true, error: null, repo };
    const el = settingsPanel({ view, draft: { ...RECOMMENDED }, saving: false, message: '', messageKind: '' }, { save: vi.fn(), saveAndClose: vi.fn(), close: vi.fn() });
    return el;
  }

  it('各項目に、リポジトリ側でも変えられるかを出す', () => {
    const { card } = setup();
    expect(card('max_file_bytes').querySelector('.scope')!.textContent).toBe('リポジトリ側でも変えられる');
    expect(card('history_limit').querySelector('.scope')!.textContent).toBe('あなた個人の設定だけ');
  });

  it('リポジトリ側の設定が無ければ、何も出さない', () => {
    const el = withRepo({ path: null, keys: [], error: null });
    expect(el.querySelector('.note-line')).toBeNull();
    expect(el.querySelector('.badge.repo')).toBeNull();
  });

  it('リポジトリ側が上書きしている項目を、一覧と、項目の印で出す', () => {
    const el = withRepo({ path: '/repo/.mrcodereview/settings.json', keys: ['max_file_bytes', 'fuzzy_min'], error: null });
    const note = el.querySelector('.note-line')!.textContent!;
    expect(note).toContain('/repo/.mrcodereview/settings.json');
    expect(note).toContain('max_file_bytes、fuzzy_min');
    expect(note).toContain('リポジトリ側の値が使われます');
    const marked = [...el.querySelectorAll('.scard')].filter((c) => c.querySelector('.badge.repo')).map((c) => c.querySelector('code')!.textContent);
    expect(marked).toEqual(['max_file_bytes', 'fuzzy_min']);
  });

  it('リポジトリ側の設定が正しくなければ、レビューが止まる理由を出す', () => {
    const el = withRepo({ path: '/repo/.mrcodereview/settings.json', keys: [], error: '`llm_timeout_secs` は変えられません' });
    expect(el.querySelector('.err')!.textContent).toContain('`llm_timeout_secs` は変えられません');
    expect(el.querySelector('.err')!.textContent).toContain('直すまで実行できません');
  });
});

describe('設定画面: タブ', () => {
  const visiblePanes = (el: HTMLElement) => [...el.querySelectorAll<HTMLElement>('.stab')].filter((p) => !p.hidden);
  const tabBtn = (el: HTMLElement, label: string) => [...el.querySelectorAll<HTMLButtonElement>('.stabs button')].find((b) => b.textContent?.startsWith(label))!;

  it('先頭の「LLM・API キー」だけが見え、押したタブに切り替わる（項目は、見えなくなるだけで、DOM に残る）', () => {
    const { el, st, card } = setup();
    expect([...el.querySelectorAll('.stabs button')].map((b) => b.textContent)).toEqual(['LLM・API キー', 'リポジトリ', '送る内容', '実行', '仕分け', '画面', '連携', 'その他']);
    expect(visiblePanes(el)).toHaveLength(1);
    expect(visiblePanes(el)[0].contains(card('max_file_bytes'))).toBe(false);
    tabBtn(el, '送る内容').click();
    expect(st.tab).toBe('diff');
    expect(visiblePanes(el)).toHaveLength(1);
    expect(visiblePanes(el)[0].contains(card('max_file_bytes'))).toBe(true);
    expect(tabBtn(el, '送る内容').classList.contains('on')).toBe(true);
    expect(tabBtn(el, 'LLM').classList.contains('on')).toBe(false);
  });

  it('見えていないタブの入力の誤り・未保存の変更も、タブの印に出る。直せば消える', () => {
    const { el, type, button } = setup();
    expect(tabBtn(el, '送る内容').querySelector('.tmark')!.textContent).toBe('');
    type('max_file_bytes', '0'); // 範囲外
    expect(tabBtn(el, '送る内容').querySelector('.tmark')!.textContent).toBe('⚠ 1');
    type('history_limit', '600'); // 正しい変更（未保存）
    expect(tabBtn(el, '画面').querySelector('.tmark')!.textContent).toBe('●');
    expect(tabBtn(el, '仕分け').querySelector('.tmark')!.textContent).toBe(''); // 別のタブの変更は、印を付けない
    expect(button('保存').disabled).toBe(true); // 誤りがある間は、保存できない（全部のタブをまとめて）
    type('max_file_bytes', '60000');
    expect(tabBtn(el, '送る内容').querySelector('.tmark')!.textContent).toBe('');
  });

  it('LLM・連携の欄は、それぞれのタブに入り、開いていたタブは、作り直しても同じ', () => {
    const view: SettingsView = { values: { ...RECOMMENDED }, meta: META, path: '/data/settings.json', exists: true, error: null, repo: { path: null, keys: [], error: null } };
    const st: SettingsPanelState = { view, draft: { ...RECOMMENDED }, saving: false, message: '', messageKind: '', tab: 'link' };
    const llm = document.createElement('div');
    const link = document.createElement('div');
    const el = settingsPanel(st, { save: vi.fn(), saveAndClose: vi.fn(), close: vi.fn() }, { llm, link });
    const panes = [...el.querySelectorAll<HTMLElement>('.stab')];
    expect(panes.find((p) => p.contains(llm))!.hidden).toBe(true);
    expect(panes.find((p) => p.contains(link))!.hidden).toBe(false);
    expect(visiblePanes(el)).toHaveLength(1);
  });

});

describe('設定画面: 未保存のまま閉じようとしたとき', () => {
  const confirmBox = (el: HTMLElement) => el.querySelector('.close-confirm') as HTMLElement;

  it('変更が無ければ、確認なしで閉じる', () => {
    const { act, button, el } = setup();
    button('閉じる').click();
    expect(act.close).toHaveBeenCalledTimes(1);
    expect(confirmBox(el).hidden).toBe(true);
  });

  it('未保存の変更があれば、閉じずに、保存するかを聞く（件数つき）', () => {
    const { act, button, el, type } = setup();
    type('max_file_bytes', '80000');
    type('max_total_bytes', '260000');
    expect(confirmBox(el).hidden, '押すまでは、出ない').toBe(true);
    button('閉じる').click();
    expect(act.close).not.toHaveBeenCalled();
    expect(confirmBox(el).hidden).toBe(false);
    expect(confirmBox(el).textContent).toContain('未保存の変更が 2 件');
  });

  it('「保存して閉じる」は、保存して閉じる側を呼ぶ（閉じるだけを呼ばない）', () => {
    const { act, button, type } = setup();
    type('max_file_bytes', '80000');
    button('閉じる').click();
    button('保存して閉じる').click();
    expect(act.saveAndClose).toHaveBeenCalledTimes(1);
    expect(act.close).not.toHaveBeenCalled();
    expect(act.save).not.toHaveBeenCalled();
  });

  it('「破棄して閉じる」は、閉じる。「戻る」は、確認を引っ込めて、入力を残す', () => {
    const a = setup();
    a.type('max_file_bytes', '80000');
    a.button('閉じる').click();
    a.button('破棄して閉じる').click();
    expect(a.act.close).toHaveBeenCalledTimes(1);

    const b = setup();
    b.type('max_file_bytes', '80000');
    b.button('閉じる').click();
    b.button('戻る').click();
    expect(confirmBox(b.el).hidden).toBe(true);
    expect(b.act.close).not.toHaveBeenCalled();
    expect(b.input('max_file_bytes').value).toBe('80000');
    expect(b.st.confirmClose).toBe(false);
  });

  it('入力に誤りがあるときは、「保存して閉じる」を押せない（破棄・戻るは、押せる）', () => {
    const { button, type, el } = setup();
    type('max_file_bytes', '5');
    button('閉じる').click();
    expect(button('保存して閉じる').disabled).toBe(true);
    expect(button('保存して閉じる').title).toContain('入力に誤り');
    expect(button('破棄して閉じる').disabled).toBe(false);
    expect(button('戻る').disabled).toBe(false);
    expect(confirmBox(el).hidden).toBe(false);
  });

  it('確認の最中に、変更を取り消したら、確認は引っ込む（未保存が無いので）', () => {
    const { button, type, el, st } = setup();
    type('max_file_bytes', '80000');
    button('閉じる').click();
    expect(confirmBox(el).hidden).toBe(false);
    button('変更を取り消す').click();
    expect(confirmBox(el).hidden).toBe(true);
    expect(st.confirmClose).toBe(false);
  });

  it('ほかの画面へ移ろうとして、外から聞かれた（confirmClose を立てた）ときも、描き直すと確認が出る', () => {
    const { st, act } = setup();
    st.draft.max_file_bytes = 80000;
    st.confirmClose = true;
    const el = settingsPanel(st, act);
    expect((el.querySelector('.close-confirm') as HTMLElement).hidden).toBe(false);
    st.draft.max_file_bytes = 60000;
    expect((settingsPanel(st, act).querySelector('.close-confirm') as HTMLElement).hidden, '未保存が無ければ、立っていても出ない').toBe(true);
  });
});

describe('設定画面: 多言語（英語モード）', () => {
  it('draft.language が en のときはタブ、ボタン、ラベルが英語で表示される', () => {
    const metaList: SettingMeta[] = [
      meta({ key: 'language', kind: 'select', recommended: 'auto', group: '画面' }),
      meta({ key: 'max_file_bytes', recommended: 60_000, group: '送る差分' }),
    ];
    const view: SettingsView = {
      values: { language: 'en', max_file_bytes: 60_000 },
      meta: metaList,
      path: '/data/settings.json',
      exists: true,
      error: null,
      repo: { path: null, keys: [], error: null },
    };
    const st: SettingsPanelState = {
      view,
      draft: { language: 'en', max_file_bytes: 60_000 },
      saving: false,
      message: '',
      messageKind: '',
    };
    const onLangChange = vi.fn();
    const act = { save: vi.fn(), saveAndClose: vi.fn(), close: vi.fn(), onLangChange };
    const el = settingsPanel(st, act);

    expect(el.querySelector('.sbar .actions .run.save')?.textContent).toBe('Save');
    const buttons = [...el.querySelectorAll('button')].map((b) => b.textContent);
    expect(buttons).toContain('Save');
    expect(buttons).toContain('Undo changes');
    expect(buttons).toContain('Reset all to recommended');
    expect(buttons).toContain('Close');

    // 項目ラベルが英語化されているか
    expect(el.textContent).toContain('Language');
    expect(el.textContent).toContain('Max file size');
  });

  it('言語セレクタを変更すると onLangChange が呼ばれる', () => {
    const metaList: SettingMeta[] = [
      meta({ key: 'language', kind: 'select', recommended: 'auto', group: '画面' }),
    ];
    const view: SettingsView = {
      values: { language: 'auto' },
      meta: metaList,
      path: '/data/settings.json',
      exists: true,
      error: null,
      repo: { path: null, keys: [], error: null },
    };
    const st: SettingsPanelState = {
      view,
      draft: { language: 'auto' },
      saving: false,
      message: '',
      messageKind: '',
    };
    const onLangChange = vi.fn();
    const act = { save: vi.fn(), saveAndClose: vi.fn(), close: vi.fn(), onLangChange };
    const el = settingsPanel(st, act);

    const select = el.querySelector('select.select') as HTMLSelectElement;
    expect(select).not.toBeNull();
    select.value = 'en';
    select.dispatchEvent(new Event('change', { bubbles: true }));

    expect(st.draft.language).toBe('en');
    expect(onLangChange).toHaveBeenCalledWith('en');
  });

  it('言語 en のドラフトで変更取り消しを押すと onLangChange("ja") が 1 回だけ呼ばれ、個別リセットも二重発火しない', () => {
    const metaList: SettingMeta[] = [
      meta({ key: 'language', kind: 'select', recommended: 'auto', group: '画面' }),
    ];
    const view: SettingsView = {
      values: { language: 'ja' },
      meta: metaList,
      path: '/data/settings.json',
      exists: true,
      error: null,
      repo: { path: null, keys: [], error: null },
    };
    const st: SettingsPanelState = {
      view,
      draft: { language: 'en' },
      saving: false,
      message: '',
      messageKind: '',
    };
    const onLangChange = vi.fn();
    const act = { save: vi.fn(), saveAndClose: vi.fn(), close: vi.fn(), onLangChange };
    const el = settingsPanel(st, act);

    // 「変更を取り消す」（英語モード中は Undo changes）で view.values.language ('ja') に戻り、onLangChange('ja') が 1 回だけ呼ばれる
    const undoBtn = [...el.querySelectorAll('button')].find((b) => b.textContent === 'Undo changes')!;
    undoBtn.click();
    expect(st.draft.language).toBe('ja');
    expect(onLangChange).toHaveBeenCalledTimes(1);
    expect(onLangChange).toHaveBeenCalledWith('ja');

    // 個別リセット（推奨に戻す）を押しても 1 回だけ呼ばれる
    onLangChange.mockClear();
    const resetBtn = [...el.querySelectorAll('button')].find(
      (b) => b.textContent === 'Reset to default' || b.textContent === '推奨に戻す',
    )!;
    resetBtn.click();
    expect(st.draft.language).toBe('auto');
    expect(onLangChange).toHaveBeenCalledTimes(1);
    expect(onLangChange).toHaveBeenCalledWith('auto');
  });
});
