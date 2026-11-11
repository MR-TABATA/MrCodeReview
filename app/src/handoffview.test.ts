// @vitest-environment jsdom
import { describe, it, expect, vi } from 'vitest';
import { handoffPath, handoffPreview, handoffResult, handoffSendButton, handoffTop, newHandoffUi, type HandoffInfo, type HandoffUi } from './handoffview';

const INFO: HandoffInfo = { fix: 2, done: 0, wait: 1, dismiss: 0, fresh: 3, text: '以下のコードレビューの指摘を直してください。\n…' };
const acts = () => ({ send: vi.fn(), copyPrompt: vi.fn(), copyText: vi.fn(), close: vi.fn() });
const FILE = { path: '/r/.mrcodereview/out/handoff.md', rel: '.mrcodereview/out/handoff.md', count: 1, prompt: '.mrcodereview/out/handoff.md を読んで、直してください。', text: '渡した文面の全文' };
const ui = (over: Partial<HandoffUi> = {}): HandoffUi => ({ ...newHandoffUi(), ...over });
const btn = (el: HTMLElement, label: string) => [...el.querySelectorAll('button')].find((b) => b.textContent === label) as HTMLButtonElement | undefined;

describe('handoffPath', () => {
  it('リポジトリの根の下の、決まった場所。末尾の / は、重ねない', () => {
    expect(handoffPath('/Users/me/Git/MrCodeReview', 'bugs')).toBe('/Users/me/Git/MrCodeReview/.mrcodereview/out/handoff-bugs.md');
    expect(handoffPath('/Users/me/Git/MrCodeReview//', 'a/b')).toBe('/Users/me/Git/MrCodeReview/.mrcodereview/out/handoff-a_b.md');
  });
});

describe('handoffSendButton（各カードの、「対応・待ち・無視」の右端）', () => {
  it('押すと、その番号の 1 件だけを渡す。確認は重ねない', () => {
    const a = acts();
    const b = handoffSendButton(3, ui(), a);
    expect(b.textContent).toBe('CLI に渡す');
    b.click();
    expect(a.send).toHaveBeenCalledExactlyOnceWith(3);
  });
  it('書き出し中は、押せない。「書き出しています…」と出るのは、渡している指摘のボタンだけ', () => {
    expect((handoffSendButton(3, ui({ busy: true, target: 3 }), acts()) as HTMLButtonElement).disabled).toBe(true);
    expect(handoffSendButton(3, ui({ busy: true, target: 3 }), acts()).textContent).toBe('書き出しています…');
    expect(handoffSendButton(4, ui({ busy: true, target: 3 }), acts()).textContent, '別の指摘のボタンは、文字を変えない').toBe('CLI に渡す');
  });
});

describe('handoffResult（次にやること）', () => {
  it('渡した相手の場所にだけ出る', () => {
    expect(handoffResult(1, ui({ target: 1, file: FILE }), acts())).not.toBeNull();
    expect(handoffResult(2, ui({ target: 1, file: FILE }), acts()), '別のカードには、出さない').toBeNull();
    expect(handoffResult('all', ui({ target: 1, file: FILE }), acts())).toBeNull();
    expect(handoffResult('all', ui({ target: 'all', file: FILE }), acts())).not.toBeNull();
    expect(handoffResult(1, ui(), acts())).toBeNull();
  });

  it('番号つきで、次にやることを言う: ファイルに書いた → 1 行をコピーした → CLI に貼って Enter', () => {
    const a = acts();
    const el = handoffResult(1, ui({ target: 1, file: FILE, copiedPrompt: true }), a)!;
    expect(el.textContent).toContain('CLI に渡す準備ができました');
    expect(el.textContent).toContain('① 「対応」の 1 件を、ファイルに書きました（コミットされません）');
    expect(el.querySelector('.hpath')?.textContent).toBe(FILE.path);
    expect(el.textContent).toContain('② 次の 1 行を、コピーしました。');
    expect(el.textContent).toContain('CLI（claude や agy を動かしている端末）に貼って、Enter を押してください。');
    expect(el.textContent).toContain('エージェントが、ファイルを読んで、直します。');
    expect(el.querySelector('.hprompt')?.textContent).toBe(FILE.prompt);
    expect(el.querySelector('details pre')?.textContent, '何を渡したかを、見せる').toBe('渡した文面の全文');
  });

  it('ボタンの文字は、いつも「もう一度コピー」と「閉じる」。「コピーしました」は、文章で言い、ボタンの名前にしない', () => {
    const a = acts();
    const el = handoffResult(1, ui({ target: 1, file: FILE, copiedPrompt: true }), a)!;
    expect([...el.querySelectorAll('button')].map((b) => b.textContent)).toEqual(['もう一度コピー', '閉じる']);
    btn(el, 'もう一度コピー')!.click();
    btn(el, '閉じる')!.click();
    expect(a.copyPrompt).toHaveBeenCalledOnce();
    expect(a.close).toHaveBeenCalledOnce();
  });

  it('1 行をコピーできなかったときは、「コピーしました」と言わず、選んでコピーするよう頼む', () => {
    const el = handoffResult(1, ui({ target: 1, file: FILE, copiedPrompt: false }), acts())!;
    expect(el.textContent).toContain('② 次の 1 行を、コピーできませんでした。選んで、コピーしてください。');
    expect(el.textContent).not.toContain('コピーしました。');
  });

  it('書き出しに失敗したときは、理由だけを出す（結果の枠は、出さない）', () => {
    const el = handoffResult(1, ui({ target: 1, error: '.mrcodereview がシンボリックリンクなので、書き出しません' }), acts())!;
    expect(el.className).toBe('err');
    expect(el.textContent).toContain('シンボリックリンク');
    expect(el.querySelector('.confirm')).toBeNull();
  });
});

describe('handoffTop（一覧の上の、細い 1 行）', () => {
  it('仕分けの件数は、出さない（履歴の行と、詳細タブにある）。まとめて渡すボタンも、ここには置かない', () => {
    const el = handoffTop(INFO);
    expect(el.querySelector('.tally')).toBeNull();
    expect(el.textContent).not.toContain('未仕分け');
    expect(el.querySelector('button')).toBeNull();
  });
  it('仕分けを始める前（「対応」・待ち・無視・済みが、すべて 0 件）だけ、何をすればよいかを言う', () => {
    expect(handoffTop({ ...INFO, fix: 0, wait: 0, text: '' }).querySelector('.hint')?.textContent).toBe('指摘を「対応」にすると、各指摘の横に「CLI に渡す」が出ます。');
    expect(handoffTop({ ...INFO, fix: 0, wait: 1, text: '' }).querySelector('.hint'), '仕分けを始めたら、ヒントは出さない').toBeNull();
    expect(handoffTop({ ...INFO, fix: 0, wait: 0, dismiss: 1, text: '' }).querySelector('.hint')).toBeNull();
    expect(handoffTop({ ...INFO, fix: 0, wait: 0, done: 2, text: '' }).querySelector('.hint')).toBeNull();
    expect(handoffTop(INFO).querySelector('.hint')).toBeNull();
  });
  it('右端に、渡された部品を置く（「すべて開く」「対応内容を取り込む」）', () => {
    const extra = document.createElement('span');
    extra.textContent = '部品';
    expect(handoffTop(INFO, extra).querySelector('.row')?.lastElementChild).toBe(extra);
  });
  it('スティッキー（画面に貼り付く）ための印を、付けない', () => {
    expect(handoffTop(INFO).getAttribute('style')).toBeNull();
  });
});

describe('handoffPreview（一覧の一番下。まとめて渡すボタンと、渡す内容）', () => {
  it('「対応」が 2 件以上なら、「まとめて CLI に渡す」が、渡す内容の直前にある。押すと、全部を渡す', () => {
    const a = acts();
    const el = handoffPreview(INFO, ui(), a)!;
    const kids = [...el.children].map((c) => c.tagName + (c.className ? '.' + c.className : ''));
    expect(kids, 'ボタンの行 → 渡す内容（details）の順').toEqual(['DIV.row', 'DETAILS.handoffprev']);
    btn(el, '対応 2 件を、まとめて CLI に渡す')!.click();
    expect(a.send).toHaveBeenCalledExactlyOnceWith('all');
  });
  it('「対応」が 1 件なら、まとめて渡すボタンは出さない（カードのボタンで足りる）。0 件なら、何も出ない', () => {
    expect(btn(handoffPreview({ ...INFO, fix: 1 }, ui(), acts())!, '対応 1 件を、まとめて CLI に渡す')).toBeUndefined();
    expect(handoffPreview({ ...INFO, fix: 0, text: '' }, ui(), acts())).toBeNull();
  });
  it('渡す内容と、その文面のコピーボタンがある', () => {
    const a = acts();
    const el = handoffPreview(INFO, ui(), a)!;
    expect(el.querySelector('summary')?.textContent).toBe('まとめて渡す内容を見る（対応 2 件）');
    expect(el.querySelector('pre')?.textContent).toContain('以下のコードレビューの指摘を直してください。');
    btn(el, 'この文面をコピー')!.click();
    expect(a.copyText).toHaveBeenCalledOnce();
  });
  it('コピーしたことは、ボタンの名前ではなく、横の文章で言う', () => {
    const el = handoffPreview(INFO, ui({ copiedText: true }), acts())!;
    expect(btn(el, 'この文面をコピー'), 'ボタンの名前は、変わらない').toBeDefined();
    expect(el.querySelector('.muted')?.textContent).toBe('コピーしました');
    expect(handoffPreview(INFO, ui(), acts())!.querySelector('.muted')).toBeNull();
  });
  it('まとめて渡した結果は、ボタンと渡す内容の間に出る。1 件ずつ渡した結果は、ここには出ない', () => {
    const el = handoffPreview(INFO, ui({ target: 'all', file: FILE }), acts())!;
    expect([...el.children].map((c) => c.className)).toEqual(['row', 'confirm done', 'handoffprev']);
    expect(handoffPreview(INFO, ui({ target: 0, file: FILE }), acts())!.querySelector('.confirm.done')).toBeNull();
  });
  it('書き出し中は、ボタンを押せず、「書き出しています…」と出る', () => {
    const b = btn(handoffPreview(INFO, ui({ busy: true, target: 'all' }), acts())!, '書き出しています…')!;
    expect(b.disabled).toBe(true);
  });
});
