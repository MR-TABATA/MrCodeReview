/// <reference types="vite/client" />
import { describe, it, expect } from 'vitest';
import { msg } from './i18n';
import { OTHER_TAB, SETTINGS_TABS, changedKeys, dirtyKeys, formatValue, groupsOf, listText, parseList, parseNumber, rangeText, tabIdOf, tabsFor, validate } from './settingsmodel';
import type { SettingMeta } from './types';

function meta(over: Partial<SettingMeta> & { key: string }): SettingMeta {
  return {
    group: 'g',
    label: over.key,
    kind: 'integer',
    unit: '',
    description: 'd',
    why_recommended: 'w',
    when_to_change: 'c',
    recommended: 10,
    min: 1,
    max: 100,
    repo_scope: true,
    ...over,
  };
}

const M: SettingMeta[] = [
  meta({ key: 'max_file_bytes', recommended: 60_000, min: 1_000, max: 5_000_000, unit: 'バイト', group: '送る差分' }),
  meta({ key: 'max_total_bytes', recommended: 200_000, min: 1_000, max: 20_000_000, unit: 'バイト', group: '送る差分' }),
  meta({ key: 'fuzzy_min', kind: 'number', recommended: 0.9, min: 0.5, max: 1, unit: '', group: '引き継ぎ' }),
  meta({ key: 'ignored_lockfiles', kind: 'list', recommended: ['a.lock'], min: null, max: null, group: '送る差分' }),
];

const rec = () => ({ max_file_bytes: 60_000, max_total_bytes: 200_000, fuzzy_min: 0.9, ignored_lockfiles: ['a.lock'] });

describe('validate', () => {
  it('推奨値は、すべて通る', () => {
    expect(validate(M, rec())).toEqual({});
  });

  it('範囲外は、項目ごとに範囲つきの理由を出す', () => {
    const e = validate(M, { ...rec(), max_file_bytes: 10, fuzzy_min: 0.1 });
    expect(e.max_file_bytes).toBe('1,000〜5,000,000バイト の範囲で入れてください');
    expect(e.fuzzy_min).toContain('0.5〜1');
    expect(Object.keys(e).sort()).toEqual(['fuzzy_min', 'max_file_bytes']);
  });

  it('数値でない・整数でない入力を、見分ける', () => {
    expect(validate(M, { ...rec(), max_file_bytes: Number.NaN }).max_file_bytes).toBe('数値を入れてください');
    expect(validate(M, { ...rec(), max_file_bytes: Infinity }).max_file_bytes).toBe('数値を入れてください');
    expect(validate(M, { ...rec(), max_file_bytes: 1500.5 }).max_file_bytes).toBe('整数を入れてください');
    expect(validate(M, { ...rec(), fuzzy_min: 0.95 })).toEqual({}); // number は小数でよい
  });

  it('リストは、空の項目とパスを断る（ファイル名だけ）', () => {
    expect(validate(M, { ...rec(), ignored_lockfiles: [] })).toEqual({});
    expect(validate(M, { ...rec(), ignored_lockfiles: ['a/b.lock'] }).ignored_lockfiles).toContain('ファイル名だけ');
    expect(validate(M, { ...rec(), ignored_lockfiles: [' '] }).ignored_lockfiles).toContain('1 行に 1 つ');
  });

  it('合計の上限は、1 ファイルの上限以上', () => {
    const e = validate(M, { ...rec(), max_file_bytes: 500_000, max_total_bytes: 100_000 });
    expect(e.max_total_bytes).toBe('1 ファイルの上限（500,000）以上にしてください');
    expect(e.max_file_bytes).toBeUndefined();
    // 範囲外のときは、範囲の理由だけを出す（理由を重ねない）
    expect(validate(M, { ...rec(), max_total_bytes: 5 }).max_total_bytes).toContain('の範囲で');
  });

  it('select 種別の language は auto / ja / en だけを通し、それ以外はエラーを返す', () => {
    const langMeta = meta({ key: 'language', kind: 'select', recommended: 'auto', min: null, max: null, group: '画面' });
    expect(validate([langMeta], { language: 'auto' })).toEqual({});
    expect(validate([langMeta], { language: 'ja' })).toEqual({});
    expect(validate([langMeta], { language: 'en' })).toEqual({});
    const err = validate([langMeta], { language: 'fr' });
    expect(err.language).toBe('選択肢から選んでください');
  });
});

describe('changedKeys / dirtyKeys', () => {
  it('推奨値との違いと、保存済みとの違いは、別に数える', () => {
    const saved = { ...rec(), max_file_bytes: 120_000 };
    expect(changedKeys(M, saved)).toEqual(['max_file_bytes']);
    const draft = { ...saved, fuzzy_min: 0.8, ignored_lockfiles: ['a.lock', 'b.lock'] };
    expect(dirtyKeys(M, saved, draft)).toEqual(['fuzzy_min', 'ignored_lockfiles']);
    expect(dirtyKeys(M, saved, { ...saved })).toEqual([]);
    expect(changedKeys(M, rec())).toEqual([]);
  });

  it('リストは、中身で比べる（同じ中身の別の配列は、変更なし）', () => {
    expect(dirtyKeys(M, rec(), { ...rec(), ignored_lockfiles: ['a.lock'] })).toEqual([]);
  });
});

describe('入力の読み取りと整形', () => {
  it('数値: 前後の空白を許し、空・文字は NaN', () => {
    expect(parseNumber(' 120000 ')).toBe(120000);
    expect(parseNumber('0.85')).toBe(0.85);
    expect(parseNumber('')).toBeNaN();
    expect(parseNumber('たくさん')).toBeNaN();
    expect(parseNumber('1,000')).toBeNaN();
  });

  it('リスト: 1 行 1 件、前後の空白と空の行は捨てる', () => {
    expect(parseList('a.lock\n  b.lock  \n\n \nc')).toEqual(['a.lock', 'b.lock', 'c']);
    expect(parseList('')).toEqual([]);
    expect(listText(['a', 'b'])).toBe('a\nb');
    expect(listText(undefined)).toBe('');
  });

  it('グループは、出てきた順に重複なく並べる', () => {
    expect(groupsOf(M)).toEqual(['送る差分', '引き継ぎ']);
  });

  it('推奨値と範囲を、単位つきで出す', () => {
    expect(formatValue(M[0], 60_000)).toBe('60,000バイト');
    expect(formatValue(M[3], ['a.lock', 'b.lock'])).toBe('a.lock、b.lock');
    expect(formatValue(M[3], [])).toBe('（なし）');
    expect(formatValue(M[2], 0.9)).toBe('0.9');
    expect(rangeText(M[0])).toBe('1,000〜5,000,000バイト');
    expect(rangeText(M[3])).toBe('');
  });
});

describe('設定画面のタブ', () => {
  it('グループは、決めたタブへ入り、知らないグループは「その他」へ入る', () => {
    expect(tabIdOf('送る差分')).toBe('diff');
    expect(tabIdOf('関連コード')).toBe('diff');
    expect(tabIdOf('実行')).toBe('run');
    expect(tabIdOf('仕分けの引き継ぎ')).toBe('triage');
    expect(tabIdOf('画面')).toBe('display');
    expect(tabIdOf('新しいグループ')).toBe(OTHER_TAB.id);
  });

  it('「その他」は、入る項目があるときだけ出る', () => {
    expect(tabsFor(M).map((t) => t.id)).toContain('other'); // 試験用の「引き継ぎ」は、どのタブにも無い
    const known = [meta({ key: 'a', group: '実行' }), meta({ key: 'b', group: '画面' })];
    expect(tabsFor(known)).toBe(SETTINGS_TABS);
    expect(tabsFor(known, 'en')).toBe(msg('en').settingsTabs);
    expect(tabsFor(M, 'en')).toEqual([...msg('en').settingsTabs, msg('en').otherTab]);
  });

  it('タブの id は重ならず、1 つのグループは 1 つのタブにだけ入る', () => {
    const ids = SETTINGS_TABS.map((t) => t.id);
    expect(new Set(ids).size).toBe(ids.length);
    const groups = SETTINGS_TABS.flatMap((t) => t.groups);
    expect(new Set(groups).size).toBe(groups.length);
  });

  it('settings.rs の、実際のグループは、すべて「その他」以外のタブへ入る（グループ名を変えたら、ここで気づく）', () => {
    const files = import.meta.glob('../src-tauri/src/settings.rs', { query: '?raw', import: 'default', eager: true }) as Record<string, string>;
    const src = Object.values(files)[0] ?? '';
    const groups = [...new Set([...src.matchAll(/group: "([^"]+)"/g)].map((m) => m[1]))];
    expect(groups.length).toBeGreaterThan(0);
    for (const g of groups) expect(tabIdOf(g), g).not.toBe(OTHER_TAB.id);
  });
});
