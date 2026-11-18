import { describe, expect, it } from 'vitest';
import { providerLabel, providerNote } from './providertext';

const JAPANESE = /[぀-ヿ一-鿿]/;
const ANTHROPIC = { id: 'anthropic-api', label: 'Anthropic API（自分のキー）', note: '自分の API キーで、Anthropic の API を直接呼ぶ（BYOK）。モデル: claude-sonnet-5-5。送るのは、…。' };
const MOCK = { id: 'mock', label: 'モック（偽の応答・外へ送らない）', note: '画面の試験用。…' };

describe('providerLabel / providerNote（LLM の名前と補足）', () => {
  it('英語では英語にする。日本語は混ざらない', () => {
    for (const p of [ANTHROPIC, MOCK]) {
      expect(providerLabel(p, 'en')).not.toMatch(JAPANESE);
      expect(providerNote(p, 'en')).not.toMatch(JAPANESE);
    }
  });

  it('日本語では、バックエンドの文言をそのまま使う', () => {
    expect(providerLabel(ANTHROPIC, 'ja')).toBe(ANTHROPIC.label);
    expect(providerNote(MOCK, 'ja')).toBe(MOCK.note);
  });

  it('Anthropic API の補足に、モデル名が入る。モデル名を取り出せなければ、補足をそのまま使う', () => {
    expect(providerNote(ANTHROPIC, 'en')).toContain('claude-sonnet-5-5');
    const odd = { ...ANTHROPIC, note: 'モデルの記載がない補足' };
    expect(providerNote(odd, 'en')).toBe(odd.note);
  });

  it('知らない id は、そのまま使う（自分用ビルドの CLI など）', () => {
    const cli = { id: 'claude-cli', label: 'Claude CLI', note: 'メモ' };
    expect(providerLabel(cli, 'en')).toBe('Claude CLI');
    expect(providerNote(cli, 'en')).toBe('メモ');
  });
});
