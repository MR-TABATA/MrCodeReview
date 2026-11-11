// 設定画面の末尾の「会話の記録の読み取り」。「対応内容を取り込む」の許可の状態を見せ、取り消せるようにする。
//
// - 許可は、初めて「対応内容を取り込む」を押したときに、1 回だけ聞いて、覚えている。ここで取り消すと、次に押したとき、もう一度聞く。
// - 取り込み済みの報告は、取り消しても、残る（読む許可を取り消すだけで、記録は消さない）。

import { h } from './dom';
import { getAppLang, msg, type Lang } from './i18n';

export interface AgentConsentState {
  /** 許可が出ているか。読み取れていなければ null */
  consented: boolean | null;
  error: string;
}

export const newAgentConsent = (): AgentConsentState => ({ consented: null, error: '' });

export function agentConsentSection(st: AgentConsentState, revoke: () => void, lang: Lang = getAppLang()): HTMLElement {
  const m = msg(lang).agentConsent;

  let body: HTMLElement;
  if (st.error) {
    body = h('p', { class: 'err' }, st.error);
  } else if (st.consented === null) {
    body = h('p', { class: 'muted' }, m.checking);
  } else if (st.consented) {
    body = h(
      'div',
      {},
      h('p', {}, h('strong', {}, m.consentedStrong), m.consentedBody),
      h('div', { class: 'row' }, h('button', { onclick: revoke }, m.revokeBtn)),
      h('p', { class: 'muted' }, m.revokedNote),
    );
  } else {
    body = h('p', {}, h('strong', {}, m.notConsentedStrong), m.notConsentedBody);
  }

  return h('section', { class: 'sgroup agentconsent' }, h('h3', {}, m.title), h('p', { class: 'muted' }, m.desc), body);
}
