// 「対応」にした指摘を、CLI（あなたのコーディングエージェント）に渡す画面の部品。
//
// 置き場所:
//  - 各指摘のカードの、「対応・待ち・無視」の右端に、「CLI に渡す」（「対応」の指摘にだけ。**その 1 件だけ**を渡す）。
//    押すと、すぐに、ファイルに書き出して、CLI に貼る 1 行をコピーし、**次にやること**を、そのカードの中に出す。
//  - 一覧の上の細い 1 行: 件数。
//  - 一覧の一番下: 「対応」が 2 件以上あるときだけ「まとめて CLI に渡す」。その下に「まとめて渡す内容を見る」（文面と、その文面のコピー）。
//    渡す文面と、渡すボタンを、並べる。
// 「対応」を押しただけでは、何も渡らない。「CLI に渡す」を押すことが、渡す意思（確認を、さらに重ねない）。
// このアプリは、動いている CLI の中に、直接、文字を打ち込めない。そのため、貼る 1 行は、使う人が、CLI に貼る。
// 画面の上に貼り付く（スティッキー）部品や、入れ子のスクロールは、作らない（スクロールの違和感と、指摘の欄が狭くなる原因だった）。

import { h } from './dom';
import { getAppLang, msg } from './i18n';
import type { HandoffFile } from './types';

/** 渡す相手: 'all' = 「対応」の全部、数字 = その番号（0 始まり）の指摘 1 件 */
export type HandoffTarget = 'all' | number;

export interface HandoffUi {
  /** 渡した（渡そうとした）相手。結果の枠は、その相手の場所に出す */
  target: HandoffTarget | null;
  file: HandoffFile | null;
  error: string;
  /** CLI に貼る 1 行を、コピーできたか */
  copiedPrompt: boolean;
  /** 「まとめて渡す内容」の文面を、コピーした直後 */
  copiedText: boolean;
  /** 書き出し中 */
  busy: boolean;
}

export const newHandoffUi = (): HandoffUi => ({ target: null, file: null, error: '', copiedPrompt: false, copiedText: false, busy: false });

export interface HandoffInfo {
  /** まだ渡していない「対応」（エージェントが「対応した」と申告したものは、含めない） */
  fix: number;
  /** 「対応した」と申告された（渡し済みの）「対応」 */
  done: number;
  wait: number;
  dismiss: number;
  fresh: number;
  /** 「対応」の全部の文面。「対応」が無ければ空 */
  text: string;
}

export interface HandoffActions {
  /** 「CLI に渡す」: ファイルに書き出して、貼る 1 行をコピーする */
  send(target: HandoffTarget): void;
  /** 貼る 1 行を、もう一度、コピーする */
  copyPrompt(): void;
  /** 「まとめて渡す内容」の文面を、コピーする */
  copyText(): void;
  /** 結果の枠を閉じる */
  close(): void;
}

/** 書き出し先（絶対パス）。バックエンド（`handoff.rs`）と同じ場所。 */
export const handoffPath = (repo: string, recipeId: string): string => `${repo.replace(/\/+$/, '')}/.mrcodereview/out/handoff-${recipeId.replace(/[^A-Za-z0-9_-]/g, '_')}.md`;

/** 各カードの、「対応・待ち・無視」の右端に置くボタン。 */
export function handoffSendButton(index: number, ui: HandoffUi, act: HandoffActions): HTMLElement {
  const m = msg(getAppLang()).mainUi;
  const mine = ui.target === index;
  return h(
    'button',
    { class: 'send', disabled: ui.busy, title: m.sendToCliTitle, onclick: () => act.send(index) },
    ui.busy && mine ? m.writingCliBtn : m.sendToCliBtn,
  );
}

/** 結果の枠（次にやること）。`target` が、渡した相手と同じときだけ出す。失敗したときは、理由だけを出す。 */
export function handoffResult(target: HandoffTarget, ui: HandoffUi, act: HandoffActions): HTMLElement | null {
  if (ui.target !== target) return null;
  if (ui.error && !ui.file) return h('p', { class: 'err' }, ui.error);
  const f = ui.file;
  if (!f) return null;
  return h(
    'div',
    { class: 'confirm done' },
    h('p', {}, h('strong', {}, 'CLI に渡す準備ができました')),
    h('p', {}, `① 「対応」の ${f.count} 件を、ファイルに書きました（コミットされません）`),
    h('p', {}, h('code', { class: 'hpath' }, f.path)),
    h(
      'p',
      {},
      ui.copiedPrompt ? '② 次の 1 行を、コピーしました。' : '② 次の 1 行を、コピーできませんでした。選んで、コピーしてください。',
      h('strong', {}, ' CLI（claude や agy を動かしている端末）に貼って、Enter を押してください。'),
      ' エージェントが、ファイルを読んで、直します。',
    ),
    h('pre', { class: 'prompt hprompt' }, f.prompt),
    h('details', {}, h('summary', {}, '渡した内容を見る'), h('pre', { class: 'prompt hprompt' }, f.text)),
    h('div', { class: 'row' }, h('button', { onclick: () => act.copyPrompt() }, 'もう一度コピー'), h('button', { onclick: () => act.close() }, '閉じる')),
  );
}

/**
 * 一覧の上の、細い 1 行。仕分けの件数は、出さない（履歴の行と、詳細タブにある）。
 * 仕分けを始める前だけ、ヒントを出す。`extra` は、右端に置く部品（「すべて開く」「対応内容を取り込む」）。
 */
export function handoffTop(info: HandoffInfo, extra?: HTMLElement | null): HTMLElement {
  const hint = info.fix === 0 && info.done === 0 && info.wait === 0 && info.dismiss === 0 ? h('span', { class: 'muted hint' }, '指摘を「対応」にすると、各指摘の横に「CLI に渡す」が出ます。') : null;
  return h('div', { class: 'handoff' }, h('div', { class: 'row' }, hint, extra ?? null));
}

/** 一覧の一番下: 「対応」が 2 件以上なら「まとめて CLI に渡す」。その下に、まとめて渡す内容と、その文面のコピー。 */
export function handoffPreview(info: HandoffInfo, ui: HandoffUi, act: HandoffActions): HTMLElement | null {
  if (info.fix === 0 || !info.text) return null;
  const m = msg(getAppLang()).mainUi;
  return h(
    'div',
    { class: 'handoffall' },
    info.fix >= 2
      ? h(
          'div',
          { class: 'row' },
          h('button', { class: 'run copy', disabled: ui.busy, title: m.sendToCliTitle, onclick: () => act.send('all') }, ui.busy && ui.target === 'all' ? m.writingCliBtn : m.sendAllToCliBtn(info.fix)),
        )
      : null,
    handoffResult('all', ui, act),
    h(
      'details',
      { class: 'handoffprev' },
      h('summary', {}, m.viewAllHandoffSummary(info.fix)),
      h('pre', { class: 'prompt hprompt' }, info.text),
      h('div', { class: 'row' }, h('button', { class: 'small', onclick: () => act.copyText() }, m.copyHandoffTextBtn), ui.copiedText ? h('span', { class: 'muted' }, m.copiedBtn) : null),
    ),
  );
}
