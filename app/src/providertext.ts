// LLM プロバイダの名前と補足の、言語に合わせた表示。
//
// バックエンド（`provider.rs`）は、日本語のラベルと補足を返す（言語を知らない）。画面側で、プロバイダの id から、英語の文言に直す。
// 知らない id（自分用ビルドの CLI など）や、日本語のときは、バックエンドの文言をそのまま使う。

import { getAppLang, type Lang } from './i18n';

interface P {
  id: string;
  label: string;
  note: string;
}

const EN_LABEL: Record<string, string> = {
  'anthropic-api': 'Anthropic API (your own key)',
  mock: 'Mock (fake responses, nothing is sent out)',
};

export function providerLabel(p: Pick<P, 'id' | 'label'>, lang: Lang = getAppLang()): string {
  return lang === 'en' ? (EN_LABEL[p.id] ?? p.label) : p.label;
}

/** Anthropic API の補足は、モデル名が入る。日本語の補足から、モデル名を取り出して、英語の文に入れる（取り出せなければ、そのまま）。 */
const MODEL = /モデル: ([^。]+)。/;

export function providerNote(p: P, lang: Lang = getAppLang()): string {
  if (lang !== 'en') return p.note;
  if (p.id === 'mock') {
    return 'For trying out the screens. Returns one finding that quotes the first added line of the diff, and one that quotes a line not in the diff (a sample of an evidence mismatch). Token counts are fake values derived from the number of characters.';
  }
  if (p.id === 'anthropic-api') {
    const model = MODEL.exec(p.note)?.[1];
    if (!model) return p.note;
    return `Calls the Anthropic API directly with your own API key (BYOK). Model: ${model}. What is sent is the full prompt (angle text, diff, related code), to api.anthropic.com. The key is stored only in the macOS Keychain and is not kept in history, logs or records. Tokens are the values measured and returned by the API.`;
  }
  return p.note;
}
