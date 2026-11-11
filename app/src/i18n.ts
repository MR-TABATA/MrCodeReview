// 多言語化の辞書とヘルパー。
// 対応言語の一覧とメッセージ本体は `locales.ts` にある。

import { MESSAGES, SUPPORTED_LANGS, type Lang, type Messages } from './locales';

export { MESSAGES, SUPPORTED_LANGS, settingMetaUnit, type Lang, type Messages, type SettingMetaText, type SettingsTabDef } from './locales';

export const DEFAULT_LANG: Lang = SUPPORTED_LANGS[0].code;

export function isSupportedLang(val: unknown): val is Lang {
  return typeof val === 'string' && SUPPORTED_LANGS.some((l) => l.code === val);
}

export function isValidLangSetting(val: unknown): boolean {
  return val === 'auto' || isSupportedLang(val);
}

export function resolveLang(settingValue?: unknown): Lang {
  if (isSupportedLang(settingValue)) return settingValue;
  if (typeof navigator !== 'undefined') {
    const langs = navigator.languages && navigator.languages.length > 0 ? navigator.languages : [navigator.language];
    for (const l of langs) {
      if (!l) continue;
      const lower = l.toLowerCase();
      const matched = SUPPORTED_LANGS.find((s) => lower === s.code || lower.startsWith(`${s.code}-`) || lower.startsWith(`${s.code}_`));
      if (matched) return matched.code;
    }
  }
  return DEFAULT_LANG;
}

const DICT: Record<Lang, Record<string, string>> = {
  ja: {
    // 状態（仕分け）
    status_new: '未対応',
    status_fix: '対応する',
    status_wait: '保留',
    status_dismiss: '却下',
    status_done: '対応済',
    // 重大度
    severity_high: '高',
    severity_medium: '中',
    severity_low: '低',
    // 根拠
    evidence_verified: '根拠あり',
    evidence_mismatch: '根拠不一致',
    // 共通ボタン・操作
    save: '保存',
    cancel: 'キャンセル',
    close: '閉じる',
    copy: 'コピー',
    copied: 'コピーしました',
    ok: 'OK',
    reset: '推奨に戻す',
    reset_all: 'すべて推奨に戻す',
    custom_value: '推奨と違う',
    repo_overridden: 'このリポジトリが上書き中',
    repo_scope_ok: 'リポジトリ側でも変えられる',
    repo_scope_personal: 'あなた個人の設定だけ',
    // レシピバー・操作バー
    run_review: 'レビュー実行',
    run_batch: '選択した観点をまとめて実行',
    running: '実行中…',
    select_all: 'すべて選択',
    clear_selection: '選択解除',
    no_diff: '差分がありません',
    loading_diff: '差分を読み込み中…',
    compare_base: '比較元',
    uncommitted: '未コミットの変更',
    // 指摘・トリアージ
    findings: '指摘',
    no_findings: '指摘はありませんでした',
    evidence_quote: '根拠の引用行',
    detail: '詳細・壊れる筋書き',
    rule: '観点・ルール',
    handoff: 'エージェント引き継ぎ',
    copy_handoff: '引き継ぎ文をコピー',
    // ナビ・タブ
    tab_findings: '指摘一覧',
    tab_diff: '送った差分',
    tab_raw: '生データ',
    tab_history: '履歴',
    tab_usage: '利用量',
    tab_settings: '設定',
    tab_license: 'ライセンス',
    tab_report: 'レポート',
  },
  en: {
    // Triage statuses
    status_new: 'New',
    status_fix: 'Fix',
    status_wait: 'Wait',
    status_dismiss: 'Dismiss',
    status_done: 'Done',
    // Severities
    severity_high: 'High',
    severity_medium: 'Medium',
    severity_low: 'Low',
    // Evidence
    evidence_verified: 'Evidence found',
    evidence_mismatch: 'Evidence mismatch',
    // Common buttons & actions
    save: 'Save',
    cancel: 'Cancel',
    close: 'Close',
    copy: 'Copy',
    copied: 'Copied',
    ok: 'OK',
    reset: 'Reset to default',
    reset_all: 'Reset all to defaults',
    custom_value: 'Customized',
    repo_overridden: 'Overridden by repository',
    repo_scope_ok: 'Can be overridden by repository',
    repo_scope_personal: 'Personal setting only',
    // Recipe bar
    run_review: 'Run Review',
    run_batch: 'Run Selected Angles',
    running: 'Running…',
    select_all: 'Select All',
    clear_selection: 'Deselect All',
    no_diff: 'No uncommitted changes',
    loading_diff: 'Loading diff…',
    compare_base: 'Compare Base',
    uncommitted: 'Uncommitted changes',
    // Findings & Triage
    findings: 'Findings',
    no_findings: 'No findings detected',
    evidence_quote: 'Quoted evidence line',
    detail: 'Failure scenario & detail',
    rule: 'Rule / Angle',
    handoff: 'Agent Handoff',
    copy_handoff: 'Copy Handoff',
    // Nav & Tabs
    tab_findings: 'Findings',
    tab_diff: 'Sent Diff',
    tab_raw: 'Raw Output',
    tab_history: 'History',
    tab_usage: 'Usage',
    tab_settings: 'Settings',
    tab_license: 'License',
    tab_report: 'Report',
  },
};

export type TranslationKey = keyof typeof DICT['ja'];

let currentLang: Lang = DEFAULT_LANG;

export function setAppLang(lang: Lang): void {
  currentLang = lang;
}

export function getAppLang(): Lang {
  return currentLang;
}

export function msg(lang?: Lang | string): Messages {
  if (lang && isSupportedLang(lang)) return MESSAGES[lang];
  return MESSAGES[currentLang] ?? MESSAGES[DEFAULT_LANG];
}

export function t(key: TranslationKey): string {
  return DICT[currentLang]?.[key] ?? DICT[DEFAULT_LANG][key] ?? String(key);
}

export function tWithLang(lang: Lang, key: TranslationKey): string {
  return DICT[lang]?.[key] ?? DICT[DEFAULT_LANG][key] ?? String(key);
}

export interface LangChangeDeps {
  invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
  repo: string | null;
  loadRecipes: (repo: string | null) => Promise<void>;
  loadBases: () => Promise<unknown>;
  render: () => void;
  onError?: (msg: string) => void;
}

export async function runLangChange(langSetting: string, d: LangChangeDeps): Promise<Lang> {
  const lang = resolveLang(langSetting);
  if (lang === currentLang) {
    return lang;
  }
  setAppLang(lang);
  const errors: string[] = [];
  await Promise.all([
    d.invoke('set_menu_language', { lang: langSetting }).catch((e) => {
      console.error('set_menu_language error:', e);
      errors.push(String(e));
    }),
    Promise.resolve()
      .then(() => d.loadRecipes(d.repo))
      .catch((e) => {
        console.error('loadRecipes error:', e);
        errors.push(String(e));
      }),
    Promise.resolve()
      .then(() => d.loadBases())
      .catch((e) => {
        console.error('loadBases error:', e);
        errors.push(String(e));
      }),
  ]);
  if (errors.length > 0) {
    d.onError?.(errors.join(' ／ '));
  }
  d.render();
  return lang;
}
