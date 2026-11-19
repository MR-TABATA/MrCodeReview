// 設定画面の、画面に依存しない部分（検証・変更の判定・入力の読み取り・整形）。
//
// サーバー（`settings.rs`）の検証と同じ規則を、入力の途中で理由を出すために写してある。
// 最終的な判定は、保存するときのサーバー側。

import {
  DEFAULT_LANG,
  getAppLang,
  isValidLangSetting,
  msg,
  settingMetaUnit,
  SUPPORTED_LANGS,
  type Lang,
  type SettingMetaText,
  type SettingsTabDef,
} from './i18n';
import { ut } from './uitext';
import type { SettingMeta } from './types';

export type Values = Record<string, unknown>;

const nf = new Intl.NumberFormat('ja-JP');

export const same = (a: unknown, b: unknown): boolean => JSON.stringify(a) === JSON.stringify(b);

/** 値が、推奨値と違う項目（「変更あり」の印を出す）。 */
export function changedKeys(meta: SettingMeta[], values: Values): string[] {
  return meta.filter((m) => !same(values[m.key], m.recommended)).map((m) => m.key);
}

/** 入力中の値が、保存済みの値と違う項目（未保存の変更）。 */
export function dirtyKeys(meta: SettingMeta[], saved: Values, draft: Values): string[] {
  return meta.filter((m) => !same(draft[m.key], saved[m.key])).map((m) => m.key);
}

/** 項目の並び順を保ったまま、重複のないグループの一覧。 */
export function groupsOf(meta: SettingMeta[]): string[] {
  return [...new Set(meta.map((m) => m.group))];
}

/** 数値の入力を読む。空・数値でない入力は NaN（検証で「数値を入れてください」になる）。 */
export function parseNumber(text: string): number {
  const t = text.trim();
  return t === '' ? Number.NaN : Number(t);
}

/** 1 行 1 件のリストを読む。前後の空白を除き、空の行は捨てる。 */
export function parseList(text: string): string[] {
  return text
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l !== '');
}

export function listText(value: unknown): string {
  return Array.isArray(value) ? value.join('\n') : '';
}

export function rangeText(m: SettingMeta, lang: Lang = DEFAULT_LANG): string {
  if (m.min === null || m.max === null) return '';
  const unit = settingMetaUnit(m, lang);
  return msg(lang).formatRange(nf.format(m.min), nf.format(m.max), unit);
}

/** 推奨値などを、画面に出す形にする。 */
export function formatValue(m: SettingMeta, v: unknown, lang: Lang = DEFAULT_LANG): string {
  const dict = msg(lang);
  if (m.key === 'language') {
    if (v === 'auto') return dict.autoLanguageLabel;
    const found = SUPPORTED_LANGS.find((l) => l.code === v);
    if (found) return found.nativeName;
  }
  if (m.key.startsWith('initial_triage_')) {
    const labels: Record<string, string> = {
      fix: lang === 'ja' ? '対応' : 'Fix',
      new: lang === 'ja' ? '未仕分け' : 'Unsorted',
      dismiss: lang === 'ja' ? '無視' : 'Dismiss',
      wait: lang === 'ja' ? '待ち' : 'Wait',
    };
    return labels[String(v)] ?? String(v);
  }
  const unit = settingMetaUnit(m, lang);
  if (Array.isArray(v)) return v.length ? v.join(dict.listSeparator) : dict.noneParen;
  if (typeof v === 'number') return `${nf.format(v)}${unit}`;
  return String(v);
}

/** 項目ごとの、入力の誤り（キー → 理由）。誤りが無ければ空。 */
export function validate(meta: SettingMeta[], draft: Values, lang: Lang = getAppLang()): Record<string, string> {
  const t = ut(lang).settings;
  const errors: Record<string, string> = {};
  for (const m of meta) {
    const v = draft[m.key];
    if (m.kind === 'select') {
      if (m.key === 'language' && !isValidLangSetting(v)) {
        errors[m.key] = t.pickOption;
      } else if (m.key.startsWith('initial_triage_') && !['fix', 'new', 'dismiss', 'wait'].includes(String(v))) {
        errors[m.key] = t.pickOption;
      }
      continue;
    }
    if (m.kind === 'list') {
      if (!Array.isArray(v) || v.some((x) => typeof x !== 'string' || x.trim() === '')) {
        errors[m.key] = t.listFormat;
      } else if (m.key === 'ignored_lockfiles' && v.some((x) => String(x).includes('/'))) {
        errors[m.key] = t.fileNamesOnly;
      }
      continue;
    }
    if (typeof v !== 'number' || !Number.isFinite(v)) {
      errors[m.key] = t.needNumber;
    } else if (m.kind === 'integer' && !Number.isInteger(v)) {
      errors[m.key] = t.needInteger;
    } else if (m.min !== null && m.max !== null && (v < m.min || v > m.max)) {
      errors[m.key] = t.inRange(rangeText(m, lang));
    }
  }
  // 項目をまたぐ規則（サーバーと同じ）
  const file = draft['max_file_bytes'];
  const total = draft['max_total_bytes'];
  if (!errors['max_total_bytes'] && !errors['max_file_bytes'] && typeof file === 'number' && typeof total === 'number' && total < file) {
    errors['max_total_bytes'] = t.totalAtLeast(nf.format(file));
  }
  return errors;
}

export type MetaTextEn = SettingMetaText;
export type SettingsTab = SettingsTabDef;

export const OTHER_TAB: SettingsTab = msg(DEFAULT_LANG).otherTab;
export const OTHER_TAB_EN: SettingsTab = msg('en').otherTab;
export const SETTINGS_TABS: SettingsTab[] = msg(DEFAULT_LANG).settingsTabs;
export const SETTINGS_TABS_EN: SettingsTab[] = msg('en').settingsTabs;
export const GROUP_NAMES_EN: Record<string, string> = msg('en').groupNames;
export const SETTING_META_EN: Record<string, SettingMetaText> = msg('en').settingMeta;

/** グループが入るタブの id。どのタブにも無いグループは「その他」（項目が、見えなくならないように）。 */
export function tabIdOf(group: string): string {
  return SETTINGS_TABS.find((t) => t.groups.includes(group))?.id ?? OTHER_TAB.id;
}

/** 出すタブの一覧。「その他」は、入る項目があるときだけ。 */
export function tabsFor(meta: SettingMeta[], lang: Lang = DEFAULT_LANG): SettingsTab[] {
  const dict = msg(lang);
  const base = dict.settingsTabs;
  const other = dict.otherTab;
  const hasOther = meta.some((m) => tabIdOf(m.group) === OTHER_TAB.id);
  return hasOther ? [...base, other] : base;
}
