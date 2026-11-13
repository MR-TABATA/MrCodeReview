import './styles.css';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { message, open, save } from '@tauri-apps/plugin-dialog';
import { confirmDialog, setAppName } from './dialog';
import { findingNo, findingTag, tagLine } from './findingtag';
import { fixStateChip } from './triageview';
import { getAppLang, msg, setAppLang, resolveLang, runLangChange, type Lang } from './i18n';
import { confirmText, runUpdateCheck, type UpdateInfo } from './updateflow';
import { inlineSegments, type DiffRow, type Seg } from './diff';
import { buildView, buildViewFromDiff, type DiffView } from './diffview';
import { baseItems, baseLabel, CUSTOM, HEAD, rangeText } from './baseinfo';
import { dirtyKeys } from './settingsmodel';
import { combo, closeCombo } from './combo';
import type { DurationStats } from './duration';
import { h } from './dom';
import { agentConsentSection, newAgentConsent } from './agentconsent';
import { recipesForDiff } from './recipescope';
import { batchEstimate, newBatchUi, pickedRecipes, STOPPED, type BatchResult } from './batch';
import { detailPanel, GROUNDING_NOTE } from './detailview';
import { findingOrder, isDoneFinding } from './findingorder';
import { agentBox, agentDone, agentNotice, agentStatus, agentStatusBar, foldedRow, newAgentUi, refreshAgentReportsOnFocus, type AgentActions } from './agentview';
import { handoffPreview, handoffResult, handoffSendButton, handoffTop, newHandoffUi, type HandoffActions, type HandoffTarget } from './handoffview';
import { displayRecipeTitle, historyPanel, prunePicks } from './historyview';
import { icon, type IconName } from './icons';
import { debounced } from './debounce';
import { recipeSummary } from './recipeinfo';
import { settingsPanel, type SettingsPanelState } from './settingsui';
import {
  loadRegisteredRepos,
  visibleRepos,
  hiddenRepos,
  slotCount,
  addBlockedReason,
  repoDisplayName,
  REPOS_KEY,
  type RegisteredRepo,
} from './reporegistry';
import { renderAppIcon, repoListPanel } from './repoview';
import { bootRepos, moveConfirmText, planAfterAdd, planBatchAdd, previewErrorText, refreshBasesOnFocus, settleSelection, type BatchAddItem, type RepoAddResult, type RepoList } from './repoflow';

interface RepoUnlinkResult extends RepoList {
  removed: boolean;
  /** フォルダが無くなっていて、どの登録のものか決められない履歴の件数（数えも消しもしていない） */
  unattributed_reviews: number;
}
interface RepoDeleteResult extends RepoList {
  removed: RepoData;
  unattributed_reviews: number;
}

/** 決められない履歴があるときの一言（このリポジトリのものだったかもしれないが、消していない）。 */
function unattributedNote(n: number): string {
  return n > 0 ? `（フォルダが無くなっていて、どの登録のものか分からない履歴が ${n} 件あります。消していません）` : '';
}
import { recipeBar } from './recipebar';
import { groupSkipped, launchOf, notCovered, planRun, type RunPlan } from './runplan';
import { SpecView } from './specview';
import { fixAllBySeverity as runFixAllBySeverity, fixAllSeverityButtons } from './fixall';
import { newReportState, reportFileName, reportOptions, reportPanel, reportRange, htmlNodes, type ReportState } from './reportview';
import { periodSince, usagePanel, type Period, type UsageState } from './usageview';
import { canTrustRerun } from './lintrerun';
import type {
  AgentFetch,
  AgentReport,
  BaseOption,
  CurrentDiff,
  Carry,
  Fate,
  Finding,
  Handling,
  HandoffFile,
  Note,
  ProviderInfo,
  ByokStatus,
  RepoStatus,
  RepoData,
  RepoDataRow,
  Recipe,
  ReportPreview,
  LintItem,
  LintPlan,
  RecipeList,
  Related,
  ReviewSummary,
  ReviewView,
  SectionMeta,
  SettingsView,
  SnapshotInfo,
  SpecParam,
  TriageState,
  UsageSummary,
} from './types';

const REPO_KEY = 'mrcr.repo';
const PROVIDER_KEY = 'mrcr.provider';
const RECIPE_KEY = 'mrcr.recipe';
const CHECKED_RECIPES_KEY = 'mrcr.checked_recipes';
const LEFT_KEY = 'mrcr.left';
const RIGHT_KEY = 'mrcr.right';
const LEFTW_KEY = 'mrcr.leftw';
const RIGHTW_KEY = 'mrcr.rightw';
/** 左右のサイドバーの幅（px）。標準は狭めにして、ドラッグで変えられる */
const SIDE_W = { left: { def: 220, min: 150, max: 500 }, right: { def: 200, min: 150, max: 500 } } as const;
function loadSideW(key: string, side: keyof typeof SIDE_W): number {
  const n = Number(lsGet(key));
  const { def, min, max } = SIDE_W[side];
  return Number.isFinite(n) && n >= min && n <= max ? Math.round(n) : def;
}

function parseCheckedRecipes(raw: string): Set<string> {
  try {
    if (!raw) return new Set();
    const arr = JSON.parse(raw);
    return Array.isArray(arr) ? new Set(arr) : new Set();
  } catch {
    return new Set();
  }
}

function lsGet(k: string): string {
  try {
    return localStorage.getItem(k) ?? '';
  } catch {
    return '';
  }
}
function lsSet(k: string, v: string): void {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* 記憶できなくても動く */
  }
}

// ── 状態 ──────────────────────────────────────────────────────────────────────
type Tab = 'findings' | 'diff' | 'sent' | 'response' | 'detail';
/** 「記録」タブの中身（詳細・送った内容・応答）。上のタブは、指摘・差分・記録の 3 つ */
type RecordSub = 'detail' | 'sent' | 'response';
const RECORD_SUBS: [RecordSub, string][] = [
  ['detail', '詳細'],
  ['sent', '送った内容'],
  ['response', '応答'],
];
const isRecordTab = (t: Tab): t is RecordSub => RECORD_SUBS.some(([k]) => k === t);
type DiffMode = 'unified' | 'split';
const DIFFMODE_KEY = 'mrcr.diffmode';

const state = {
  repo: lsGet(REPO_KEY),
  /** 登録の一覧と枠の上限は、Rust（SQLite）が持つ。ここは写し（構想書 §8.3）。 */
  registeredRepos: [] as RegisteredRepo[],
  repoLimit: null as number | null,
  checkedRecipeIds: parseCheckedRecipes(lsGet(CHECKED_RECIPES_KEY)),
  providerId: lsGet(PROVIDER_KEY),
  /** 左のコンボで選んでいるレビューの種類（レシピの id） */
  recipeId: lsGet(RECIPE_KEY),
  leftOpen: lsGet(LEFT_KEY) !== '0',
  rightOpen: lsGet(RIGHT_KEY) !== '0',
  leftW: loadSideW(LEFTW_KEY, 'left'),
  rightW: loadSideW(RIGHTW_KEY, 'right'),
  /** 各リポジトリのアプリアイコン（base64 data URL、無ければ null）のキャッシュ */
  repoIcons: {} as Record<string, string | null>,
  /** 右サイドバー: 履歴パネルを折り畳んでいるか */
  historyCollapsed: false,
  /** トークン使用量の画面を開いているとき、その状態。閉じているときは null。 */
  usage: null as UsageState | null,
  /** レポート（成果物にできる記録）の画面を開いているとき、その状態。閉じているときは null。 */
  report: null as ReportState | null,
  /** メイン上部の「観点の中身」を開いているか。null = 自動（結果を表示していなければ開く） */
  topOpen: null as boolean | null,
  /** 履歴表示中の観点バーを一時的に開いているか */
  recipeBarExpanded: false,
  /** 差分に向かない観点も、すべて出しているか */
  showAllRecipes: false,
  /** 比較元（差分の元になるコミット）。HEAD = 未コミットの変更だけ。差分は、これからいまの作業ツリーまで。 */
  base: HEAD,
  bases: [] as BaseOption[],
  /** 比較元を自分で入力する欄を開いているか */
  baseCustom: false,
  /** 実行を始めた時刻（ミリ秒）。実行していなければ 0。経過時間の表示に使う。 */
  /** 実行時間の目安（過去の実績）。key は「LLM|観点」。 */
  stats: { key: '', data: null as DurationStats | null },
  diffMode: (lsGet(DIFFMODE_KEY) === 'split' ? 'split' : 'unified') as DiffMode,
  /** 差分タブで見ているもの: レビューした時点か、いま（エージェントが直したあと） */
  diffWhen: 'then' as 'then' | 'now',
  /** 「いまの差分」の読み取り（レビューごと）。まだ読んでいなければ null */
  nowDiff: null as { id: string; loading: boolean; error: string; data: CurrentDiff | null; view: DiffView | null } | null,
  providers: [] as ProviderInfo[],
  recipes: [] as Recipe[],
  /** レシピの一覧に添えて返される警告（linters.json の間違い、読まなかったファイルなど） */
  recipeWarnings: [] as string[],
  /** このリポジトリで出番のある linter と、その状態 */
  lintPlan: null as LintPlan | null,
  /** 「許可して実行しますか」の確認を出しているか */
  lintConfirm: false,
  preview: null as SnapshotInfo | null,
  previewError: '',
  history: [] as ReviewSummary[],
  /** 履歴で、レポートに出すものとして選んだレビューの識別子 */
  picked: new Set<string>(),
  selected: null as ReviewView | null,
  /** 設定画面を開いているとき、その状態。閉じているときは null。 */
  settings: null as SettingsPanelState | null,
  /** BYOK の状態（設定画面を開いたときに読む）と、入力中のモデル名・最大出力トークン。キーの入力値は、状態に持たない。 */
  byok: null as ByokStatus | null,
  byokDraft: { model: '', maxTokens: '' },
  byokMsg: { text: '', kind: '' as '' | 'ok' | 'err' },
  byokConfirmDelete: false,
  /** 設定の「リポジトリ」タブ: 各リポジトリの状態（パス → 状態）・確認中の解除・通知 */
  repoStatus: {} as Record<string, RepoStatus>,
  /** 「データを削除して枠を空ける」の確認中のリポジトリ（パス） */
  repoConfirmDelete: '',
  /** 各リポジトリのデータの量（履歴など）。解除の可否と、削除の確認に使う */
  repoData: {} as Record<string, RepoDataRow>,
  repoMsg: { text: '', kind: '' as '' | 'ok' | 'err' },
  /** 設計書参照レビューの入力パラメータ（ファイル選択または貼り付け） */
  specParam: null as SpecParam | null,
  specValid: true,
  specFiles: [] as string[],
  /** 選んだレビューの「対応」を、CUI に貼る文面にしたもの。空ならコピーするものが無い。 */
  handoff: '',
  /** 「CLI に渡す」の、確認・書き出し後の状態 */
  handoffUi: newHandoffUi(),
  agentUi: newAgentUi(),
  /** 会話の記録を読む許可の状態（設定画面を開いたときに、読む） */
  agentConsent: newAgentConsent(),
  /** 畳まれた指摘のうち、使う人が開いたもの（指摘の番号）。レビューを変えると、畳み直す */
  openCards: new Set<number>(),
  /** 指摘のカードで、開いているもの（番号）。既定は、全部閉じる（2 行だけ）。レビューを変えると、閉じ直す */
  expanded: new Set<number>(),
  /** 履歴で、開いている日付のグループ（`dayKey`） */
  historyDays: new Set<string>(),
  tab: 'findings' as Tab,
  /** 実行バーの下の「観点の中身」で、開いているもの（'*' は全体、あとは観点の id） */
  infoOpen: new Set<string>(),
  /** 「記録」タブを開いたとき、最後に見ていた中身 */
  recordSub: 'detail' as RecordSub,
  running: '' as string, // 実行中のレシピ id。空なら実行していない
  runError: '',
  /** 「まとめて実行」の状態と、選んだ観点の過去の実績（観点の id → 実績。まだ読んでいなければ、無い） */
  batch: newBatchUi(),
  batchStats: {} as Record<string, DurationStats | undefined>,
};

const getRoot = (): HTMLElement | null => (typeof document !== 'undefined' ? document.getElementById('app') : null);

const specViewInstance =
  typeof document !== 'undefined'
    ? new SpecView({
        messages: msg(getAppLang()),
        onChange: (param, valid) => {
          state.specParam = param;
          state.specValid = valid;
          render();
        },
        onFileSelect: async (filePath: string) => {
          if (!state.repo || !filePath) return [];
          try {
            return await invoke<SectionMeta[]>('list_spec_sections', { repo: state.repo, relPath: filePath });
          } catch (e) {
            console.error('Failed to list spec sections', e);
            return [];
          }
        },
        onPickFile: async () => {
          try {
            const selected = await open({
              title: '設計書・仕様書ファイルを選択',
              multiple: false,
              directory: false,
              filters: [{ name: 'Markdown / Text', extensions: ['md', 'markdown', 'txt'] }],
              defaultPath: state.repo || undefined,
            });
            if (typeof selected === 'string') {
              if (state.repo && selected.startsWith(state.repo)) {
                let rel = selected.slice(state.repo.length);
                if (rel.startsWith('/')) rel = rel.slice(1);
                return rel;
              }
              return selected;
            }
            return null;
          } catch (e) {
            console.error('Failed to open file picker', e);
            return null;
          }
        },
      })
    : null;

async function refreshSpecFiles(): Promise<void> {
  if (!state.repo) {
    state.specFiles = [];
    specViewInstance?.setFiles([]);
    return;
  }
  try {
    const files = await invoke<string[]>('list_spec_files', { repo: state.repo });
    state.specFiles = files;
    specViewInstance?.setFiles(files);
  } catch (e) {
    console.error('Failed to list spec files', e);
    state.specFiles = [];
    specViewInstance?.setFiles([]);
  }
}

// ── 整形 ──────────────────────────────────────────────────────────────────────
const nf = new Intl.NumberFormat('ja-JP');
const num = (n: number | null): string => (n === null ? '（報告なし）' : nf.format(n));
const when = (ms: number): string => new Date(ms).toLocaleString('ja-JP');
const dur = (ms: number): string => (ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1)} 秒`);
const shortRepo = (p: string): string => p.split('/').filter(Boolean).slice(-2).join('/');

// ── 取得 ──────────────────────────────────────────────────────────────────────
async function loadRecipes(repo: string | null): Promise<void> {
  const list = await invoke<RecipeList>('list_recipes', { repo, lang: getAppLang() });
  state.recipes = list.recipes;
  if (!list.recipes.some((r) => r.id === state.recipeId)) state.recipeId = list.recipes[0]?.id ?? '';
  state.recipeWarnings = list.warnings;
  state.lintPlan = list.lint;
  if (!list.lint?.items.some((i) => i.state === 'needs_trust')) state.lintConfirm = false;
}

/** 見る範囲の候補を読む。読めなかったら、候補は空にして（別のリポジトリの候補を残さない）、理由を返す（読めたら空文字）。 */
async function loadBases(): Promise<string> {
  if (!state.repo) {
    state.bases = [];
    return '';
  }
  try {
    state.bases = await invoke<BaseOption[]>('list_bases', { repo: state.repo, lang: getAppLang() });
    return '';
  } catch (e) {
    state.bases = [];
    return String(e);
  }
}

/** 比較元を切り替えて、送る内容を取り直す。 */
async function chooseBase(value: string): Promise<void> {
  state.baseCustom = false;
  state.base = value;
  state.recipeBarExpanded = true;
  await refreshPreview();
}

async function refreshPreview(): Promise<void> {
  state.preview = null;
  state.previewError = '';
  if (!state.repo) return render();
  const basesError = await loadBases();
  try {
    state.preview = await invoke<SnapshotInfo>('preview_snapshot', { repo: state.repo, base: state.base });
    await loadRecipes(state.preview.repo);
  } catch (e) {
    state.previewError = String(e);
    await loadRecipes(null);
  }
  // 送る内容は読めたのに、候補だけ読めなかったときも、黙って空にせず、理由を出す
  state.previewError = previewErrorText(state.previewError, basesError);
  render();
  void refreshStats();
}

/** 仕分け・対応済みを続けて押しても、履歴の件数の読み直しは、最後の操作の少しあとに 1 回だけ。 */
const refreshHistoryLater = debounced(() => {
  refreshHistory().catch((e) => {
    state.runError = String(e);
    render();
  });
}, 400);

async function refreshHistory(): Promise<void> {
  // 選んでいるリポジトリの履歴だけ。選んでいなければ、すべて
  state.history = await invoke<ReviewSummary[]>('list_reviews', { repo: state.repo || null });
  state.picked = prunePicks(state.picked, state.history);
  render();
  void refreshRepoCounts(); // 左の一覧の「未」の件数も、履歴と同じ頃に読み直す
}

/** 左の一覧に出す、リポジトリごとの「未」の件数を読み直す（読み取りだけ。変わったときだけ描き直す）。 */
async function refreshRepoCounts(): Promise<void> {
  const paths = state.registeredRepos.map((r) => r.path);
  if (paths.length === 0) return;
  try {
    const data = await invoke<RepoDataRow[]>('repo_data', { paths });
    const next = Object.fromEntries(data.map((x) => [x.path, x]));
    if (JSON.stringify(next) !== JSON.stringify(state.repoData)) {
      state.repoData = next;
      render();
    }
  } catch {
    // 件数が読めなくても、一覧は使える（古い数のまま）
  }
}

/** 選んだ履歴を削除する。元に戻せないので、確認を取る。 */
async function removePicked(): Promise<void> {
  const ids = [...state.picked];
  if (ids.length === 0) return;
  const m = msg(getAppLang()).historyView;
  if (!(await confirmDialog(m.deleteConfirm(ids.length), { title: m.deleteBtn, okLabel: m.deleteOk }))) return;
  try {
    await invoke<number>('delete_reviews', { ids });
    // 開いているレビューを消したときは、画面から外す
    if (state.selected && ids.includes(state.selected.id)) state.selected = null;
    state.picked = new Set();
  } catch (e) {
    state.runError = String(e);
  }
  await refreshHistory();
}

async function openSettings(): Promise<void> {
  state.usage = null;
  closeReport();
  const view = await invoke<SettingsView>('get_settings', { repo: state.repo || null });
  state.settings = { view, draft: JSON.parse(JSON.stringify(view.values)), saving: false, message: '', messageKind: '' };
  state.agentConsent = newAgentConsent();
  state.byokMsg = { text: '', kind: '' };
  state.byokConfirmDelete = false;
  state.repoConfirmDelete = '';
  state.repoMsg = { text: '', kind: '' };
  render();
  void loadByok();
  void loadRepoStatus();
  try {
    state.agentConsent.consented = await invoke<boolean>('agent_log_consent');
  } catch (e) {
    state.agentConsent.error = String(e);
  }
  if (state.settings) render();
}

/** BYOK の状態を読む（キーの有無と末尾 4 文字・モデル名・最大出力トークン）。入力中の欄は、読み直した値に合わせる。 */
async function loadByok(): Promise<void> {
  try {
    const s = await invoke<ByokStatus>('byok_status');
    state.byok = s;
    state.byokDraft = { model: s.model, maxTokens: String(s.max_tokens) };
  } catch (e) {
    state.byokMsg = { text: `BYOK の状態を読めません: ${String(e)}`, kind: 'err' };
  }
  if (state.settings) render();
}

/** 設定を変えたあと、プロバイダの一覧（モデル名つきの注意書き）を読み直す。 */
async function refreshProviders(): Promise<void> {
  try {
    state.providers = await invoke<ProviderInfo[]>('list_providers');
  } catch (e) {
    state.byokMsg = { text: `プロバイダの一覧を読めません: ${String(e)}`, kind: 'err' };
  }
}

async function byokDo(okText: string, op: () => Promise<void>): Promise<void> {
  state.byokMsg = { text: '', kind: '' };
  state.byokConfirmDelete = false;
  try {
    await op();
    state.byokMsg = { text: okText, kind: 'ok' };
  } catch (e) {
    state.byokMsg = { text: String(e), kind: 'err' };
  }
  await refreshProviders();
  await loadByok();
}

const saveByokKey = (key: string): Promise<void> => byokDo('キーをキーチェーンに保存しました', () => invoke('byok_save_key', { key }));
const deleteByokKey = (): Promise<void> => byokDo('キーをキーチェーンから消しました', () => invoke('byok_delete_key'));
const saveByokConfig = (): Promise<void> =>
  byokDo('モデル名と最大出力トークンを保存しました', () => invoke('byok_save_config', { model: state.byokDraft.model, maxTokens: Number(state.byokDraft.maxTokens) }));

/** 会話の記録を読む許可を取り消す。取り込み済みの報告は、残る。 */
async function revokeAgentConsent(): Promise<void> {
  try {
    await invoke('revoke_agent_log_consent');
    state.agentConsent = { consented: false, error: '' };
    state.agentUi = newAgentUi();
  } catch (e) {
    state.agentConsent.error = String(e);
  }
  render();
}

async function applyLangChange(langSetting: string): Promise<string | null> {
  let langErr: string | null = null;
  await runLangChange(langSetting, {
    invoke,
    repo: state.repo || null,
    loadRecipes,
    loadBases,
    render,
    onError: (msg) => {
      langErr = msg;
      if (!state.settings) {
        state.runError = msg;
      }
    },
  });
  specViewInstance?.setMessages(msg(getAppLang()));
  return langErr;
}

async function saveSettings(): Promise<void> {
  const st = state.settings;
  if (!st || st.saving) return;
  st.saving = true;
  st.message = '';
  st.messageKind = '';
  render();
  let view: SettingsView;
  try {
    view = await invoke<SettingsView>('save_settings', { values: st.draft, repo: state.repo || null });
    st.view = view;
    st.draft = JSON.parse(JSON.stringify(view.values));
  } catch (e) {
    st.message = String(e);
    st.messageKind = 'err';
    st.saving = false;
    render();
    return;
  }
  const postErrors: string[] = [];
  const nextSettingLang = String(view.values.language ?? 'auto');
  const langErr = await applyLangChange(nextSettingLang);
  if (langErr) postErrors.push(langErr);
  // 履歴の件数や、送る差分の上限が変わるので、取り直す
  try {
    await Promise.all([refreshHistory(), refreshPreview()]);
  } catch (e) {
    postErrors.push(String(e));
  }
  const su = msg(getAppLang()).settingsUi;
  if (postErrors.length > 0) {
    st.message = su.savedWithErrors(postErrors);
    st.messageKind = 'err';
  } else {
    st.message = su.savedToast;
    st.messageKind = 'ok';
  }
  st.saving = false;
  render();
}

async function refreshHandoff(): Promise<void> {
  state.handoff = state.selected ? await invoke<string>('build_handoff', { id: state.selected.id }) : '';
}

/** 履歴の「詳細」: そのレビューを開いて、「詳細」タブを出す。 */
async function openDetail(id: string): Promise<void> {
  await select(id);
  if (state.selected?.id === id) {
    state.tab = 'detail';
    render();
  }
}

async function select(id: string): Promise<void> {
  if (!canLeaveSettings()) return;
  state.handoffUi = newHandoffUi();
  state.agentUi = newAgentUi();
  state.openCards = new Set();
  state.expanded = new Set();
  state.diffWhen = 'then';
  state.nowDiff = null;
  state.recipeBarExpanded = false;
  state.topOpen = null;
  state.settings = null;
  state.usage = null;
  closeReport();
  state.selected = (await invoke<ReviewView | null>('get_review', { id })) ?? null;
  state.tab = 'findings';
  await refreshHandoff();
  render();
}

/** 重大度が `sev` の指摘のうち、まだ「対応」でないものを、まとめて「対応」にする。 */
async function fixAllBySeverity(sev: string, status: TriageState = 'fix'): Promise<void> {
  const r = state.selected;
  if (!r) return;
  await runFixAllBySeverity(r, sev, {
    setTriage: (index, st) => invoke<Handling>('set_triage', { id: r.id, index, status: st }),
    applyHandling: (handling) => applyHandling(r, handling),
    onError: (error) => { state.runError = error; },
    refreshHistory: () => refreshHistoryLater.call(),
    refreshHandoff,
    render,
  }, status);
}

async function setTriage(index: number, status: TriageState): Promise<void> {
  const r = state.selected;
  if (!r) return;
  try {
    applyHandling(r, await invoke<Handling>('set_triage', { id: r.id, index, status }));
    refreshHistoryLater.call(); // 履歴の件数（未・済み・待ち・無視）は、サーバー側で数える。変えたら、まとめて読み直す
  } catch (e) {
    state.runError = String(e);
  }
  await refreshHandoff();
  render();
}

function applyHandling(r: ReviewView, h: Handling): void {
  r.triage = h.triage;
  r.finding_notes = h.notes;
}

/** 対応メモを保存する（入力欄を離れたとき・Enter）。 */
async function saveMemo(index: number, memo: string): Promise<void> {
  const r = state.selected;
  if (!r || (r.finding_notes[index]?.memo ?? '') === memo.trim()) return;
  try {
    applyHandling(r, await invoke<Handling>('set_memo', { id: r.id, index, memo }));
    state.runError = '';
  } catch (e) {
    state.runError = String(e);
  }
  render();
}

/** 「対応済み」を付ける／外す。 */
async function toggleDone(index: number, done: boolean): Promise<void> {
  const r = state.selected;
  if (!r) return;
  try {
    applyHandling(r, await invoke<Handling>('set_done', { id: r.id, index, done }));
    state.runError = '';
    refreshHistoryLater.call(); // 「済み」の数が変わる
  } catch (e) {
    state.runError = String(e);
  }
  render();
}

/** クリップボードに入れる。入れられたかを返す（入れられなかったときに、「コピーしました」と言わないため）。 */
async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    // クリップボード API が使えない環境向け: 選択してコピーする
    const ta = h('textarea', { class: 'offscreen' });
    ta.value = text;
    document.body.append(ta);
    ta.select();
    let ok = false;
    try {
      ok = document.execCommand('copy');
    } catch {
      ok = false;
    }
    ta.remove();
    return ok;
  }
}

/** 「まとめて渡す内容」の文面を、コピーする。 */
async function copyHandoffText(): Promise<void> {
  if (!state.handoff) return;
  state.handoffUi.copiedText = await copyText(state.handoff);
  render();
  setTimeout(() => {
    state.handoffUi.copiedText = false;
    render();
  }, 2000);
}

function closeHandoff(): void {
  state.handoffUi = newHandoffUi();
  render();
}

/** 「CLI に渡す」: ファイルに書き出して、CLI に貼る 1 行を、クリップボードに入れる。`target` は、その番号の指摘 1 件か、「対応」の全部。 */
async function writeHandoff(target: HandoffTarget): Promise<void> {
  const r = state.selected;
  if (!r || state.handoffUi.busy) return;
  state.handoffUi = { ...newHandoffUi(), target, busy: true };
  render();
  try {
    const file = await invoke<HandoffFile>('write_handoff', { id: r.id, index: target === 'all' ? null : target });
    const copied = await copyText(file.prompt);
    state.handoffUi = { ...newHandoffUi(), target, file, copiedPrompt: copied };
  } catch (e) {
    state.handoffUi = { ...newHandoffUi(), target, error: String(e) };
  }
  render();
}

async function copyHandoffPrompt(): Promise<void> {
  const f = state.handoffUi.file;
  if (!f) return;
  state.handoffUi.copiedPrompt = await copyText(f.prompt);
  render();
}

/** 「対応内容を取り込む」。初めては、ログを読む許可を聞く（許可されるまで、ログには触らない）。 */
async function fetchAgent(consent: boolean): Promise<void> {
  const r = state.selected;
  if (!r || state.agentUi.busy) return;
  state.agentUi = { ...newAgentUi(), busy: true };
  render();
  let next = newAgentUi();
  try {
    const res = await invoke<AgentFetch>('fetch_agent_reports', { id: r.id, consent });
    if (res.needs_consent) next = { ...newAgentUi(), asking: true };
    else {
      r.agent_reports = res.agent_reports;
      state.openCards = new Set();
      // 渡す文面も、申告済みを外した内容に作り直す
      await refreshHandoff();
      next = { ...newAgentUi(), result: res };
      await refreshHistory(); // 「対応した」の申告は、履歴の「済み」に数える
    }
  } catch (e) {
    next = { ...newAgentUi(), error: String(e) };
  }
  // 取り込み中に、別のレビューへ移っていたら、その画面には、出さない
  if (state.selected?.id === r.id) state.agentUi = next;
  else state.agentUi = newAgentUi();
  render();
}

const agentActs = (): AgentActions => ({
  fetch: (c) => void fetchAgent(c),
  cancel: () => {
    state.agentUi = newAgentUi();
    render();
  },
});

const handoffActs = (): HandoffActions => ({
  send: (t) => void writeHandoff(t),
  copyPrompt: () => void copyHandoffPrompt(),
  copyText: () => void copyHandoffText(),
  close: closeHandoff,
});

/** この linter が、今回の差分のファイルを見るか。 */
const itemHitsDiff = (i: LintItem): boolean => !!state.preview?.files.some((f) => i.exts.includes(f.path.split('.').pop() ?? ''));

/** 今回の差分に対象があるのに、許可が無くて走らせられない linter（ESLint・Clippy）。 */
const needsTrustNow = (): LintItem[] => (state.lintPlan?.items ?? []).filter((i) => i.state === 'needs_trust' && itemHitsDiff(i));

/** 観点バーに出している観点。ふだんは、この差分に向くものだけ（構想書 §26）。 */
const shownRecipes = (): Recipe[] => (state.showAllRecipes ? state.recipes : recipesForDiff(state.recipes, state.preview?.files ?? []));

/** 実行の対象 ＝ 選んでいて、いま出ている観点。隠れた観点の選択は、捨てずに覚えておく（実行はしない）。 */
function activeChecked(): Set<string> {
  const shown = new Set(shownRecipes().map((r) => r.id));
  return new Set([...state.checkedRecipeIds].filter((id) => shown.has(id)));
}

/** 「リンター」を含む実行。許可が要る linter が今回の対象にあれば、先に確認する。 */
function startCheckedRecipes(): void {
  // 実行できない観点は外す（設計書が未指定の「設計書・仕様書との一致」など）。外したものは、画面に理由を出している。
  // 何を始めるかは、`launchOf` の結果だけに従う（外した観点は、1 つの実行にも、まとめての実行にも入らない）
  const launch = launchOf(currentRunPlan());
  if (launch.kind === 'none') return;
  if (launch.kind === 'single') {
    state.recipeId = launch.id;
    void runRecipe(launch.id);
    return;
  }
  state.batch.picked = new Set(launch.ids);
  void runBatch();
}

async function trustRepoOnly(): Promise<boolean> {
  try {
    await invoke('trust_repo', { repo: state.repo });
    await loadRecipes(state.preview?.repo ?? state.repo);
    state.runError = '';
    render();
    return true;
  } catch (e) {
    state.runError = String(e);
    render();
    return false;
  }
}

async function trustAndRun(): Promise<void> {
  state.lintConfirm = false;
  if (!(await trustRepoOnly())) return;
  if (state.checkedRecipeIds.has('lint')) {
    startCheckedRecipes();
  } else {
    await runRecipe('lint');
  }
}

async function trustAndRerunLint(): Promise<void> {
  state.lintConfirm = false;
  if (!(await trustRepoOnly())) return;
  await runRecipe('lint');
}

async function untrust(): Promise<void> {
  try {
    await invoke('untrust_repo', { repo: state.repo });
    await loadRecipes(state.preview?.repo ?? state.repo);
  } catch (e) {
    state.runError = String(e);
  }
  render();
}

/** 実績を引く相手: リンターは LLM を使わないので、プロバイダは `tool`。 */
function statsTarget(): { providerId: string; recipeId: string } | null {
  const r = currentRecipe();
  if (!r) return null;
  const providerId = r.kind === 'tool' ? 'tool' : state.providerId;
  return providerId ? { providerId, recipeId: r.id } : null;
}

async function refreshStats(): Promise<void> {
  const t = statsTarget();
  if (!t) {
    state.stats = { key: '', data: null };
    return render();
  }
  const key = `${t.providerId}|${t.recipeId}`;
  if (state.stats.key !== key) state.stats = { key, data: null };
  try {
    const data = await invoke<DurationStats>('duration_stats', t);
    // 引いている間に、別の観点・LLM に変わっていたら、捨てる
    const now = statsTarget();
    if (now && `${now.providerId}|${now.recipeId}` === key) state.stats = { key, data };
  } catch {
    /* 目安が出せなくても、実行はできる */
  }
  render();
}

async function runRecipe(recipeId: string): Promise<void> {
  if (state.running) return;
  state.recipeBarExpanded = false;
  state.running = recipeId;
  state.runError = '';
  render();
  try {
    const rec = await invoke<ReviewView>('run_review', {
      repo: state.repo,
      recipeId,
      providerId: state.providerId,
      base: state.base,
      lang: getAppLang(),
      spec: state.specParam,
    });
    // 設定を開いたまま、未保存の変更があるなら、閉じずに、設定の中で聞く（結果は、そのあとに出る）。黙って捨てない
    if (canLeaveSettings()) state.settings = null;
    state.usage = null;
    closeReport();
    state.topOpen = null;
    state.selected = rec;
    state.expanded = new Set();
    state.diffWhen = 'then';
    state.nowDiff = null;
    state.tab = rec.error || rec.parse_error ? 'response' : 'findings';
    await refreshHandoff();
  } catch (e) {
    state.runError = String(e);
  } finally {
    state.running = '';
    state.batchStats = {}; // 実績が 1 件増えたので、引き直す
    await refreshHistory();
    await refreshPreview();
  }
}

/** リポジトリを切り替えたとき。前のリポジトリの結果をメインに残さず、そのリポジトリの履歴に取り直す。 */
async function changeRepo(repo: string): Promise<void> {
  state.repo = repo;
  lsSet(REPO_KEY, repo);
  state.picked = new Set(); // 履歴は、リポジトリごと。選んだものは、外す
  state.base = HEAD; // 比較元は、リポジトリごとのもの。切り替えたら、未コミットの変更に戻す
  state.bases = [];
  state.baseCustom = false;
  state.selected = null;
  state.recipeBarExpanded = false;
  state.handoff = '';
  state.tab = 'findings';
  state.topOpen = null;
  state.runError = '';
  await refreshHistory();
  await refreshPreview();
}

async function loadRepoIcon(repoPath: string): Promise<void> {
  if (!repoPath || state.repoIcons[repoPath] !== undefined) return;
  try {
    const icon = await invoke<string | null>('find_repo_icon', { repo: repoPath });
    state.repoIcons[repoPath] = icon;
    render();
  } catch {
    state.repoIcons[repoPath] = null;
  }
}

function loadAllRepoIcons(): void {
  for (const r of state.registeredRepos) {
    void loadRepoIcon(r.path);
  }
}

/**
 * 複数のフォルダを、まとめて追加する。1 件ずつ `repo_add` を呼ぶ（上限・登録済み・移動の判断は、1 件ずつの追加と同じ）。
 * 表示名は聞かない（フォルダ名になる。あとで、その場で直せる）。移動したかもしれない登録は、確認が要るので、何も変えない。
 */
async function addReposAtOnce(dirs: string[]): Promise<void> {
  const items: BatchAddItem[] = [];
  for (const path of dirs) {
    try {
      const out = await invoke<RepoAddResult>('repo_add', { path, name: null, confirmMove: false });
      applyRepoList(out);
      items.push({ path, out });
    } catch (e) {
      items.push({ path, error: String(e) });
    }
  }
  const plan = planBatchAdd(items);
  state.repoMsg = { text: plan.message, kind: plan.problems ? 'err' : 'ok' };
  if (plan.problems) window.alert(plan.message);
  loadAllRepoIcons();
  if (plan.select) {
    await changeRepo(plan.select);
    void loadRepoStatus();
  } else {
    render();
  }
}

/** `askName` が偽のとき（設定の「リポジトリ」タブ）は、名前を聞かない（あとで、その場で直せる）。 */
async function chooseAndAddRepo(askName = true): Promise<void> {
  const picked = await open({ directory: true, multiple: true });
  const dirs = (Array.isArray(picked) ? picked : typeof picked === 'string' ? [picked] : []).map((d) => d.trim()).filter(Boolean);
  if (dirs.length === 0) return;
  if (dirs.length > 1) return addReposAtOnce(dirs);
  const path = dirs[0];
  const defaultName = path.replace(/\/+$/, '').split('/').filter(Boolean).pop() || '';
  const inputName = askName ? window.prompt(`リポジトリの表示名を入力してください（省略時は「${defaultName}」になります）:`, '') : '';
  const name = inputName === null ? undefined : inputName.trim();
  const callAdd = async (confirmMove: boolean): Promise<RepoAddResult | null> => {
    try {
      const res = await invoke<RepoAddResult>('repo_add', { path, name: name || null, confirmMove });
      applyRepoList(res);
      return res;
    } catch (e) {
      state.repoMsg = { text: `追加できませんでした: ${String(e)}`, kind: 'err' };
      window.alert(state.repoMsg.text);
      render();
      return null;
    }
  };
  let out = await callAdd(false);
  if (!out) return;
  let plan = planAfterAdd(out);
  if (plan.moveFrom) {
    // 登録のフォルダが見つからない。移動したのか、外付けドライブの未接続などで見えないだけなのかは、ここでは分からない。確認が取れるまで、何も変えない
    const moved = await confirmDialog(moveConfirmText(plan.moveFrom, path), { title: '登録の付け替え', okLabel: '付け替える' });
    if (!moved) {
      state.repoMsg = { text: '登録は、そのままです（元のフォルダが見つかったら、もう一度追加してください）', kind: 'ok' };
      render();
      return;
    }
    out = await callAdd(true);
    if (!out) return;
    plan = planAfterAdd(out);
  }
  if (plan.select === null) {
    state.repoMsg = { text: limitMessage(out.limit), kind: 'err' };
    window.alert(state.repoMsg.text);
    render();
    return;
  }
  state.repoMsg = plan.alreadyAt
    ? { text: `同じリポジトリが、すでに ${plan.alreadyAt} に登録されています（新しい行は作らず、そちらを選びました）`, kind: 'ok' }
    : { text: '', kind: '' };
  void loadRepoIcon(plan.select);
  await changeRepo(plan.select);
  void loadRepoStatus();
}

// ── 設定の「リポジトリ」タブ: 登録したリポジトリの編集（構想書 §8） ──────────────────────────
// 枠は、登録の行数で数える。解除（非表示）した行も数える。履歴が 1 件もないものだけ、解除で行ごと消える。
// 枠を空けるのは、「データを削除して枠を空ける」だけ。

/** 登録リポジトリの、いまの状態（見つかるか・git か）と、データの量（履歴など）を読む。 */
async function loadRepoStatus(icons = true): Promise<void> {
  const paths = state.registeredRepos.map((r) => r.path);
  const before = JSON.stringify([state.repoStatus, state.repoData]);
  try {
    const [list, data] = await Promise.all([invoke<RepoStatus[]>('repo_status', { paths }), invoke<RepoDataRow[]>('repo_data', { paths })]);
    state.repoStatus = Object.fromEntries(list.map((x) => [x.path, x]));
    state.repoData = Object.fromEntries(data.map((x) => [x.path, x]));
  } catch (e) {
    state.repoMsg = { text: `リポジトリの状態を読めません: ${String(e)}`, kind: 'err' };
  }
  if (icons) for (const r of state.registeredRepos) void loadRepoIcon(r.path);
  // 窓に戻ったときの読み直しは、変わったときだけ描き直す（「追加」の押せる・押せないが、フォルダの移動に追いつくように）
  if (icons || state.settings || JSON.stringify([state.repoStatus, state.repoData]) !== before) render();
}

/** Rust が返した登録の一覧を、画面の写しに入れる。 */
function applyRepoList(l: { repos: RegisteredRepo[]; limit: number | null }): void {
  state.registeredRepos = l.repos;
  state.repoLimit = l.limit;
}

/** 「追加」を押せない理由（押せるなら null）。「見つかりません」の登録は、状態を読めているものだけを見る。 */
function addBlockedNow(): string | null {
  return addBlockedReason(state.registeredRepos, state.repoLimit, (path) => state.repoStatus[path]?.state === 'missing');
}

/** 上限に達して追加を断ったときの案内（構想書 §8.3 (3)）。 */
function limitMessage(limit: number | null): string {
  return `登録できるのは、${limit ?? 3} リポジトリ分のデータまでです。解除しても、データがあるうちは、枠は空きません。設定の「リポジトリ」で「データを削除…」して枠を空けてください。`;
}

/** 登録が変わったあと、選択中のリポジトリが、登録に無い・解除済みになっていたら、見えている登録に切り替える（登録に無いパスのまま、残さない）。 */
async function ensureSelectionRegistered(): Promise<void> {
  const sel = settleSelection(state.registeredRepos, state.repo, true);
  if (sel !== state.repo) await changeRepo(sel);
}

/** 登録を変える操作を、Rust に頼む。失敗したら、何も変えず、理由を出す。 */
async function repoOp<T extends RepoList>(cmd: string, args: Record<string, unknown>): Promise<T | null> {
  try {
    const out = await invoke<T>(cmd, args);
    applyRepoList(out);
    await ensureSelectionRegistered();
    return out;
  } catch (e) {
    state.repoMsg = { text: `登録を更新できませんでした（何も変わっていません）: ${String(e)}`, kind: 'err' };
    render();
    return null;
  }
}

/** 表示名を変える（空にすると、フォルダ名に戻る）。 */
async function renameRepoTo(id: string, name: string): Promise<void> {
  if (!(await repoOp('repo_rename', { id, name: name.trim() || null }))) return;
  state.repoMsg = { text: '表示名を保存しました', kind: 'ok' };
  render();
}

async function moveRepo(id: string, delta: -1 | 1): Promise<void> {
  if (!(await repoOp('repo_move', { id, delta }))) return;
  state.repoMsg = { text: '', kind: '' };
  render();
}

/** 一覧から消えたあとの後始末。選択中のものが消えたら、見えているものの先頭に切り替える。 */
async function afterRepoChange(path: string, gone: boolean): Promise<void> {
  if (gone && state.repo === path) await changeRepo(visibleRepos(state.registeredRepos)[0]?.path ?? '');
  else render();
  void loadRepoStatus();
}

/** 解除する。履歴が 1 件もなければ、行ごと消す（枠が戻る）。履歴があれば、非表示にするだけ（枠は使い続ける）。判断は Rust 側。 */
async function unlinkRepo(repo: RegisteredRepo): Promise<void> {
  const title = repoDisplayName(repo);
  const out = await repoOp<RepoUnlinkResult>('repo_unlink', { id: repo.id });
  if (!out) return;
  state.repoMsg = out.removed
    ? { text: `「${title}」を解除しました（履歴がないので、登録ごと消えました）${unattributedNote(out.unattributed_reviews)}`, kind: 'ok' }
    : { text: `「${title}」を解除しました（履歴があるので、一覧から外しただけです。「復元」で戻せます）`, kind: 'ok' };
  await afterRepoChange(repo.path, true);
}

async function restoreRepo(id: string, path: string): Promise<void> {
  if (!(await repoOp('repo_restore', { id }))) return;
  state.repoMsg = { text: '復元しました', kind: 'ok' };
  render();
  void loadRepoIcon(path);
}

/** 登録を消す（履歴が 0 件のとき）。枠が戻る。解除と同じ操作（履歴が 0 件なら、行ごと消える）。 */
async function dropEmptyRepo(repo: RegisteredRepo): Promise<void> {
  const out = await repoOp<RepoUnlinkResult>('repo_unlink', { id: repo.id });
  if (!out) return;
  state.repoMsg = out.removed
    ? { text: `「${repoDisplayName(repo)}」の登録を消しました${unattributedNote(out.unattributed_reviews)}`, kind: 'ok' }
    : { text: `「${repoDisplayName(repo)}」には履歴があるので、登録は消さず、解除のままにしました`, kind: 'err' };
  await afterRepoChange(repo.path, true);
}

/** 「データを削除して、枠を空ける」: このリポジトリの履歴・仕分け・メモ・エージェントの報告を、すべて消し、登録も消す。元に戻せない。 */
async function deleteRepoData(repo: RegisteredRepo): Promise<void> {
  const title = repoDisplayName(repo);
  state.repoConfirmDelete = '';
  let gone = false;
  try {
    const { removed: d, unattributed_reviews, ...list } = await invoke<RepoDeleteResult>('repo_delete_data', { id: repo.id });
    applyRepoList(list);
    await ensureSelectionRegistered();
    gone = true;
    state.repoMsg = { text: `「${title}」のデータを削除しました（レビュー ${d.reviews} 件・指摘 ${d.findings} 件・仕分け ${d.triaged} 件・メモ ${d.memos} 件）${unattributedNote(unattributed_reviews)}`, kind: 'ok' };
  } catch (e) {
    state.repoMsg = { text: `削除できませんでした（何も消えていません）: ${String(e)}`, kind: 'err' };
  }
  await afterRepoChange(repo.path, gone);
}

const REPO_STATE_TEXT: Record<RepoStatus['state'], string> = {
  ok: '',
  missing: '見つかりません（移動・削除された可能性）',
  not_git: 'git のリポジトリではありません',
};

const dataText = (d?: RepoDataRow, lang: Lang = getAppLang()): string => {
  if (!d) return '';
  return msg(lang).mainUi.repoDataSummary(d);
};

function reposSection(lang: Lang = getAppLang()): HTMLElement {
  const m = msg(lang).mainUi;
  const all = state.registeredRepos;
  const shown = visibleRepos(all);
  const hidden = hiddenRepos(all);
  const used = slotCount(all);
  const limit = state.repoLimit;
  const over = limit !== null && used > limit;
  const full = limit !== null && used === limit;

  /** 「データを削除して枠を空ける」の確認（何が消えるかを、数字で出す。元に戻せない） */
  const confirmBox = (r: RegisteredRepo): HTMLElement => {
    const d = state.repoData[r.path];
    const title = repoDisplayName(r);
    return h(
      'div',
      { class: 'confirm redit-confirm' },
      h('p', {}, h('b', {}, m.repoDeleteConfirmTitle(title))),
      h(
        'p',
        {},
        m.repoDeleteConfirmBody(dataText(d, lang)),
        h('b', {}, m.repoDeleteCannotUndo),
      ),
      h(
        'p',
        { class: 'muted' },
        r.hidden ? m.repoDeleteExportHiddenHint : m.repoDeleteExportVisibleHint,
      ),
      h(
        'div',
        { class: 'row' },
        r.hidden ? null : h('button', { class: 'small', onclick: () => void (async () => (await changeRepo(r.path), await toggleReport()))() }, m.openReportBtn),
        h('button', { class: 'small danger', onclick: () => void deleteRepoData(r) }, m.deleteAndFreeSlotBtn),
        h('button', { class: 'small', onclick: () => ((state.repoConfirmDelete = ''), render()) }, m.cancelBtn),
      ),
    );
  };

  /** 「データを削除…」（履歴があるときだけ）。確認中は、押せない。 */
  const deleteBtn = (r: RegisteredRepo): HTMLElement | null =>
    (state.repoData[r.path]?.reviews ?? 0) > 0
      ? h(
          'button',
          {
            class: 'small',
            disabled: state.repoConfirmDelete === r.path,
            title: m.deleteDataTitle,
            onclick: () => ((state.repoConfirmDelete = r.path), render()),
          },
          m.deleteDataBtn,
        )
      : null;

  const row = (r: RegisteredRepo, i: number): HTMLElement => {
    const st = state.repoStatus[r.path];
    const title = repoDisplayName(r);
    const input = h('input', {
      class: 'sinput redit-name',
      type: 'text',
      value: r.name ?? '',
      placeholder: repoDisplayName({ path: r.path }),
      spellcheck: 'false',
      'aria-label': m.displayNameAria(title),
    });
    input.addEventListener('change', () => void renameRepoTo(r.id, input.value));
    return h(
      'div',
      { class: 'redit-item' },
      h(
        'article',
        { class: `redit-row ${st && st.state !== 'ok' ? 'bad' : ''}` },
        h('span', { class: 'repo-item-icon' }, renderAppIcon(title, state.repoIcons[r.path])),
        h(
          'div',
          { class: 'redit-main' },
          h('div', { class: 'redit-line' }, input, r.path === state.repo ? h('span', { class: 'chip did' }, m.inUseChip) : null),
          h('code', { class: 'redit-path', title: r.path }, r.path),
          h('div', { class: 'muted redit-state' }, dataText(state.repoData[r.path], lang)),
          st && st.state !== 'ok' ? h('div', { class: 'warn redit-state' }, `⚠ ${REPO_STATE_TEXT[st.state]}`) : null,
          st?.root ? h('div', { class: 'muted redit-state' }, m.subdirNote(st.root)) : null,
        ),
        h(
          'div',
          { class: 'redit-actions' },
          h('button', { class: 'small', title: m.moveUpTitle, disabled: i === 0, onclick: () => void moveRepo(r.id, -1) }, '↑'),
          h('button', { class: 'small', title: m.moveDownTitle, disabled: i === shown.length - 1, onclick: () => void moveRepo(r.id, 1) }, '↓'),
          h('button', { class: 'small', title: m.unlinkWithHistoryTitle, onclick: () => void unlinkRepo(r) }, m.unlinkBtn),
          deleteBtn(r),
        ),
      ),
      state.repoConfirmDelete === r.path ? confirmBox(r) : null,
    );
  };

  const hiddenRow = (r: RegisteredRepo): HTMLElement => {
    const title = repoDisplayName(r);
    const d = state.repoData[r.path];
    const empty = d !== undefined && d.reviews === 0;
    return h(
      'div',
      { class: 'redit-item' },
      h(
        'article',
        { class: 'redit-row' },
        h('span', { class: 'repo-item-icon' }, renderAppIcon(title, state.repoIcons[r.path])),
        h(
          'div',
          { class: 'redit-main' },
          h('div', { class: 'redit-line' }, h('strong', {}, title)),
          h('code', { class: 'redit-path', title: r.path }, r.path),
          h('div', { class: 'muted redit-state' }, dataText(d, lang)),
        ),
        h(
          'div',
          { class: 'redit-actions' },
          h('button', { class: 'small', onclick: () => void restoreRepo(r.id, r.path) }, m.restoreBtn),
          empty
            ? h('button', { class: 'small', title: m.removeEmptyTitle, onclick: () => void dropEmptyRepo(r) }, m.removeEmptyBtn)
            : deleteBtn(r),
        ),
      ),
      state.repoConfirmDelete === r.path ? confirmBox(r) : null,
    );
  };

  return h(
    'section',
    { class: 'llm-config-section' },
    h('h3', {}, m.registeredReposHeading),
    h('p', { class: 'muted byok-note' }, m.registeredReposIntro1(limit)),
    h('p', { class: 'muted byok-note' }, m.registeredReposIntro2),
    h(
      'div',
      { class: 'brow' },
      h('b', {}, m.slotsLabel),
      h('span', { class: `chip ${over ? 'warn' : full ? 'did' : ''}` }, limit === null ? m.slotsUnlimited(used) : `${used} / ${limit}`),
      over
        ? h('span', { class: 'warn' }, m.slotsOverLimit)
        : full
          ? h('span', { class: 'muted' }, m.slotsFull)
          : null,
      h('span', { class: 'spacer' }),
      h('button', { class: 'small', disabled: addBlockedNow() !== null, title: addBlockedNow() ?? '', onclick: () => void chooseAndAddRepo(false) }, m.addRepoBtn),
    ),
    shown.length ? h('div', { class: 'redit-list' }, ...shown.map(row)) : h('p', { class: 'muted' }, m.noReposRegistered),
    hidden.length
      ? h('div', { class: 'redit-hidden' }, h('h4', {}, m.unlinkedHoldingSlots(hidden.length)), h('div', { class: 'redit-list' }, ...hidden.map(hiddenRow)))
      : null,
    state.repoMsg.text ? h('p', { class: state.repoMsg.kind === 'err' ? 'err' : 'ok-text' }, state.repoMsg.text) : null,
  );
}

/** 左の一覧の「✕」。解除する（履歴があるあいだは、枠は戻らない）。 */
async function handleRemoveRepo(repo: RegisteredRepo): Promise<void> {
  if (!(await confirmDialog(`リポジトリ「${repoDisplayName(repo)}」を解除しますか？\n（履歴は残ります。設定の「リポジトリ」で、復元・データの削除ができます）`, { title: '登録の解除', okLabel: '解除する' }))) return;
  await unlinkRepo(repo);
}

function handleRenameRepo(repo: RegisteredRepo): void {
  const curName = repo.name || '';
  const defaultName = repo.path.replace(/\/+$/, '').split('/').filter(Boolean).pop() || '';
  const inputName = window.prompt(`表示名を変更（省略時は「${defaultName}」になります）:`, curName);
  if (inputName === null) return;
  void renameRepoTo(repo.id, inputName);
}

async function chooseRepo(): Promise<void> {
  await chooseAndAddRepo();
}

// ── 左: リポジトリ・送る内容・プロバイダ・ボタン・履歴 ─────────────────────────────────
const currentRecipe = (): Recipe | undefined => state.recipes.find((r) => r.id === state.recipeId) ?? state.recipes[0];

// ── ヘッダ ────────────────────────────────────────────────────────────────────
function toggleLeft(): void {
  state.leftOpen = !state.leftOpen;
  lsSet(LEFT_KEY, state.leftOpen ? '1' : '0');
  closeCombo();
  render();
}

function toggleRight(): void {
  state.rightOpen = !state.rightOpen;
  lsSet(RIGHT_KEY, state.rightOpen ? '1' : '0');
  render();
}

/** サイドバーの境目（つかんで動かすと幅が変わる。ダブルクリックで標準に戻す）。動かしている間は、作り直さず、列幅の変数だけを書き換える。 */
function sideResizer(side: 'left' | 'right'): HTMLElement {
  const cfg = SIDE_W[side];
  const cssVar = side === 'left' ? '--lw' : '--rw';
  const key = side === 'left' ? LEFTW_KEY : RIGHTW_KEY;
  const apply = (el: HTMLElement, w: number): void => {
    state[side === 'left' ? 'leftW' : 'rightW'] = w;
    el.closest<HTMLElement>('.app')?.style.setProperty(cssVar, `${w}px`);
  };
  const el = h('div', {
    class: `resizer ${side === 'left' ? 'l' : 'r'}`,
    title: 'ドラッグで幅を変える（ダブルクリックで標準の幅）',
    role: 'separator',
    'aria-orientation': 'vertical',
  });
  el.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    el.setPointerCapture(e.pointerId);
    el.classList.add('drag');
    const body = el.parentElement!.getBoundingClientRect();
    const move = (ev: PointerEvent): void => {
      const raw = side === 'left' ? ev.clientX - body.left : body.right - ev.clientX;
      apply(el, Math.round(Math.min(cfg.max, Math.max(cfg.min, raw))));
    };
    const up = (): void => {
      el.classList.remove('drag');
      el.removeEventListener('pointermove', move);
      el.removeEventListener('pointerup', up);
      el.removeEventListener('pointercancel', up);
      lsSet(key, String(state[side === 'left' ? 'leftW' : 'rightW']));
    };
    el.addEventListener('pointermove', move);
    el.addEventListener('pointerup', up);
    el.addEventListener('pointercancel', up);
  });
  el.addEventListener('dblclick', () => {
    apply(el, cfg.def);
    lsSet(key, String(cfg.def));
  });
  return el;
}

/** 設定を開いたまま、ほかの画面へ移ろうとしたとき。未保存の変更があれば、移らず、設定の中で、保存するかを聞く。 */
function canLeaveSettings(): boolean {
  const st = state.settings;
  if (!st) return true;
  if (dirtyKeys(st.view.meta, st.view.values, st.draft).length === 0) return true;
  st.confirmClose = true;
  render();
  return false;
}

/** 「保存して閉じる」: 保存できたときだけ、閉じる（保存に失敗したら、開いたまま、理由を出す）。 */
async function saveAndCloseSettings(): Promise<void> {
  await saveSettings();
  const st = state.settings;
  if (st && st.messageKind !== 'err') {
    state.settings = null;
    render();
  }
}

function toggleSettings(): void {
  if (state.settings) {
    if (!canLeaveSettings()) return;
    state.settings = null;
    render();
  } else {
    void openSettings();
  }
}

async function loadUsage(): Promise<void> {
  const u = state.usage;
  if (!u) return;
  u.loading = true;
  u.error = '';
  render();
  try {
    u.data = await invoke<UsageSummary>('usage_summary', { sinceMs: periodSince(u.period) });
  } catch (e) {
    u.error = String(e);
  } finally {
    u.loading = false;
    render();
  }
}

async function toggleUsage(): Promise<void> {
  if (!canLeaveSettings()) return;
  if (state.usage) {
    state.usage = null;
    return render();
  }
  state.settings = null;
  closeReport();
  state.usage = { period: 'today', data: null, loading: true, error: '' };
  await loadUsage();
}

function setUsagePeriod(p: Period): void {
  if (!state.usage) return;
  state.usage.period = p;
  void loadUsage();
}

// ── レポート（成果物にできる、レビューの記録）─────────────────────────────────────
let reportSeq = 0;

/** 印刷（PDF）用の領域。画面では見えず、印刷のときだけ出る（`styles.css` の `@media print`）。 */
function mountPrintSheet(html: string): void {
  removePrintSheet();
  const sheet = h('div', { id: 'print-sheet' });
  sheet.append(...htmlNodes(html));
  document.body.append(sheet);
}

function removePrintSheet(): void {
  document.getElementById('print-sheet')?.remove();
}

function closeReport(): void {
  state.report = null;
  removePrintSheet();
}

async function loadReport(): Promise<void> {
  const st = state.report;
  if (!st) return;
  st.message = '';
  st.error = '';
  if (reportRange(st).error) {
    st.loading = false;
    return render(); // 期間の誤りは、画面に出る。取りに行かない
  }
  const seq = ++reportSeq;
  st.loading = true;
  render();
  try {
    const lang = getAppLang();
    const p = await invoke<ReportPreview>('report_preview', { options: reportOptions(st, state.repo, new Date(), lang) });
    if (seq === reportSeq) st.preview = p;
  } catch (e) {
    if (seq === reportSeq) st.error = String(e);
  } finally {
    if (seq === reportSeq) {
      st.loading = false;
      render();
    }
  }
}

/** 選んだレビュー（履歴のチェック、または、開いている結果）から、レポートを開く。 */
async function openReportFor(ids: string[]): Promise<void> {
  if (!canLeaveSettings()) return;
  state.settings = null;
  state.usage = null;
  state.report = { ...newReportState(), reviewIds: ids };
  await loadReport();
}

function togglePick(id: string): void {
  const next = new Set(state.picked);
  if (!next.delete(id)) next.add(id);
  state.picked = next;
  render();
}

function togglePickAll(): void {
  state.picked = state.history.length > 0 && state.history.every((s) => state.picked.has(s.id)) ? new Set() : new Set(state.history.map((s) => s.id));
  render();
}

async function toggleReport(): Promise<void> {
  if (!canLeaveSettings()) return;
  if (state.report) {
    closeReport();
    return render();
  }
  state.settings = null;
  state.usage = null;
  state.report = newReportState();
  await loadReport();
}

/** Markdown / Excel を、選んだ場所に保存する。 */
async function saveReport(format: 'md' | 'xlsx'): Promise<void> {
  const st = state.report;
  if (!st || st.busy) return;
  const lang = getAppLang();
  const path = await save({
    defaultPath: reportFileName(state.repo, st.allRepos, format, new Date(), st.reviewIds?.length ?? 0, lang),
    filters: [{ name: format === 'md' ? 'Markdown' : 'Excel', extensions: [format] }],
  });
  if (!path) return; // 保存先の選択を、やめた
  st.busy = format;
  st.message = '';
  st.error = '';
  render();
  try {
    const out = await invoke<string>('report_save', { options: reportOptions(st, state.repo, new Date(), lang), format, path });
    st.message = msg(lang).mainUi.savedReportToast(out);
  } catch (e) {
    st.error = String(e);
  } finally {
    st.busy = '';
    render();
  }
}

/** PDF: 画面のプレビューと同じ内容を、印刷の画面に出す（「PDF」→「PDF として保存」）。 */
async function printReport(): Promise<void> {
  const st = state.report;
  if (!st?.preview || st.busy) return;
  st.busy = 'pdf';
  st.message = '';
  st.error = '';
  mountPrintSheet(st.preview.html);
  render();
  try {
    await invoke('print_page');
    st.message = '印刷の画面を開きました。左下の「PDF」から「PDF として保存」を選んでください。';
  } catch (e) {
    st.error = String(e);
  } finally {
    st.busy = '';
    render();
  }
}

function iconButton(name: IconName, title: string, on: boolean, onclick: () => void, label?: string, disabled = false): HTMLElement {
  return h(
    'button',
    { class: `ibtn ${label ? 'lbl' : ''} ${on ? 'on' : ''}`, title, 'aria-label': title, 'aria-pressed': on ? 'true' : 'false', disabled, onclick },
    icon(name),
    label ? h('span', { class: 'ilabel' }, label) : null,
  );
}

/** 「比較元」: 差分を、どのコミットからいまの状態まで見るか。 */
function baseChip(): HTMLElement | null {
  if (!state.repo) return null;
  const lang = getAppLang();
  return combo({
    id: 'base',
    compact: true,
    prefix: msg(lang).mainUi.rangePrefix,
    placeholder: msg(lang).baseInfo.uncommitted,
    items: baseItems(state.bases, state.base, lang, (id) => {
      const r = state.recipes.find((x) => x.id === id);
      return r ? displayRecipeTitle(r.title, lang) : id;
    }),
    value: state.base,
    rerender: render,
    onChange: (v: string) => {
      if (v === CUSTOM) {
        state.baseCustom = true;
        render();
      } else {
        void chooseBase(v);
      }
    },
  });
}

/** 比較元を、名前（コミット・ブランチ・タグ）で入力する欄。 */
function baseForm(): HTMLElement | null {
  if (!state.baseCustom) return null;
  const m = msg(getAppLang()).mainUi;
  const input = h('input', {
    class: 'base-input',
    type: 'text',
    placeholder: m.baseInputPlaceholder,
    spellcheck: 'false',
  });
  const close = () => {
    state.baseCustom = false;
    render();
  };
  const submit = () => {
    const v = input.value.trim();
    if (v) void chooseBase(v);
  };
  input.addEventListener('keydown', (e) => {
    if (e.key === 'Enter') submit();
    else if (e.key === 'Escape') close();
  });
  queueMicrotask(() => input.focus());
  return h(
    'div',
    { class: 'baseform' },
    h('span', { class: 'muted' }, m.baseFormHint),
    input,
    h('button', { class: 'run small', onclick: submit }, m.baseFormSetBtn),
    h('button', { onclick: close }, m.baseFormCancelBtn),
  );
}

function headerView(): HTMLElement {
  const active = state.registeredRepos.find((r) => r.path === state.repo);
  const activeName = active ? repoDisplayName(active) : (state.repo ? repoDisplayName({ path: state.repo }) : '');

  const addBlocked = addBlockedNow();
  const repoDisplay = h(
    'div',
    {
      class: 'repo-header-display',
      title: state.repo ? `${activeName} (${state.repo})` : 'リポジトリを選択してください',
    },
    iconButton('folder', addBlocked ?? 'リポジトリを登録・追加', false, () => void chooseRepo(), undefined, addBlocked !== null),
    h(
      'span',
      { class: 'current-repo-label' },
      state.repo
        ? h('span', { class: 'repo-name-badge' }, renderAppIcon(activeName, state.repoIcons[state.repo]), activeName)
        : h('span', { class: 'muted' }, '左サイドバーからリポジトリを選択'),
    ),
  );

  const m = msg(getAppLang()).mainUi;
  return h(
    'header',
    { class: 'topbar' },
    iconButton('panel-left', m.toggleLeftTitle, state.leftOpen, toggleLeft),
    repoDisplay,
    baseChip(),
    h('span', { class: 'spacer' }),
    iconButton('chart', m.usageTitle, !!state.usage, () => void toggleUsage(), m.usageBtn),
    iconButton('report', m.reportTitle, !!state.report, () => void toggleReport(), m.reportBtn),
    iconButton('gear', m.settingsTitle, !!state.settings, toggleSettings, m.settingsBtn),
    iconButton('panel-right', m.toggleRightTitle, state.rightOpen, toggleRight),
  );
}

// ── レビューの実行と制御 ──────────────────────────────────────────────────────
/**
 * いま押したら何が走るか。実行できない観点（設計書が未指定・LLM が未選択）は、今回の実行から外して、できるものを走らせる。
 * 実行ボタンも、実行の入口も、同じ計画を使う。全体を止めるのは、リポジトリ未選択・差分なし・外したら 1 つも残らないとき。
 */
function currentRunPlan(): RunPlan {
  const p = state.preview;
  return planRun(pickedRecipes(state.recipes, activeChecked()), {
    hasRepo: !!state.repo,
    filesCount: p ? p.files.length : null,
    baseIsHead: state.base === HEAD,
    hasProvider: !!state.providerId,
    hasSpec: !!state.specParam,
    specValid: state.specValid,
  });
}

/** 「まとめて実行」で選んだ観点の、過去の実績を引く（まだ読んでいないものだけ）。 */
async function loadBatchStats(): Promise<void> {
  const providerId = state.providerId;
  for (const r of pickedRecipes(state.recipes, state.batch.picked)) {
    if (state.batchStats[r.id] !== undefined) continue;
    const pid = r.kind === 'tool' ? 'tool' : providerId;
    if (!pid) continue;
    try {
      const data = await invoke<DurationStats>('duration_stats', { providerId: pid, recipeId: r.id });
      // 引いている間に、LLM が変わっていたら、捨てる
      if (state.providerId === providerId) state.batchStats[r.id] = data;
    } catch {
      // 目安が出ないだけ（実行はできる）
    }
  }
  render();
}

/** 選んだ観点を、順に 1 つずつ実行する。失敗しても、残りは続ける。「残りを中止」は、いまの 1 件が終わってから止める。 */
async function runBatch(): Promise<void> {
  const b = state.batch;
  if (state.running || b.run) return;
  const chosen = pickedRecipes(state.recipes, b.picked);
  if (!chosen.length) return;
  b.confirming = false;
  b.last = null;
  const run = { ids: chosen.map((r) => r.id), index: 0, stop: false, results: [] as BatchResult[] };
  b.run = run;
  render();
  for (const [i, r] of chosen.entries()) {
    if (run.stop) break;
    run.index = i;
    await runRecipe(r.id);
    const rec = state.selected;
    const reason = state.runError || (rec?.recipe.id === r.id ? rec.error || (rec.parse_error ? `指摘を読めなかった: ${rec.parse_error}` : '') : '実行結果を確認できません');
    run.results.push({ id: r.id, title: r.title, ok: !reason, note: reason ?? '' });
  }
  // 中止で、実行しなかった観点は、そう残す（黙って、無かったことにしない）
  for (const r of chosen.slice(run.results.length)) run.results.push({ id: r.id, title: r.title, ok: false, note: STOPPED });
  b.last = run.results;
  b.run = null;
  render();
  void loadBatchStats();
}

function toggleRecipeCheck(id: string, checked?: boolean): void {
  const next = checked !== undefined ? checked : !state.checkedRecipeIds.has(id);
  if (next) state.checkedRecipeIds.add(id);
  else state.checkedRecipeIds.delete(id);
  lsSet(CHECKED_RECIPES_KEY, JSON.stringify([...state.checkedRecipeIds]));
  render();
  void loadBatchStats();
}

function selectAllRecipes(): void {
  state.checkedRecipeIds = new Set([...state.checkedRecipeIds, ...shownRecipes().map((r) => r.id)]);
  lsSet(CHECKED_RECIPES_KEY, JSON.stringify([...state.checkedRecipeIds]));
  render();
  void loadBatchStats();
}

function clearAllRecipes(): void {
  state.checkedRecipeIds.clear();
  lsSet(CHECKED_RECIPES_KEY, JSON.stringify([]));
  render();
  void loadBatchStats();
}

function runCheckedRecipes(): void {
  const plan = currentRunPlan();
  if (plan.block || plan.run.length === 0) return;
  if (plan.run.some((r) => r.id === 'lint') && needsTrustNow().length > 0) {
    state.lintConfirm = true;
    render();
    return;
  }
  startCheckedRecipes();
}

function recipeBarSection(): HTMLElement {
  const p = state.preview;
  const busy = !!state.running || !!state.batch.run;
  // 実行中は、観点を出したまま（何が走っているか見える）。終わって結果が履歴に載ったら、畳む
  const collapsed = !state.recipeBarExpanded && !busy && !!state.selected && !state.runError;
  const active = activeChecked();
  const chosen = pickedRecipes(state.recipes, active);
  const needsSpec = active.has('spec') || chosen.some((r) => r.id === 'spec' || r.needs_spec);
  if (specViewInstance) {
    if (needsSpec && !specViewInstance.isEnabled()) {
      specViewInstance.setEnabled(true);
    } else if (!needsSpec && specViewInstance.isEnabled()) {
      specViewInstance.setEnabled(false);
    }
    specViewInstance.setDisabled(busy);
  }
  const specSlot = needsSpec && specViewInstance ? specViewInstance.root : null;
  const plan = currentRunPlan();
  const disabledReason = plan.block;
  const lang = getAppLang();
  // 見積もりは、実際に走らせる観点だけで出す（全部が外れるときは、0 件）
  const toRun = plan.run;
  // 「前回のレビューの続き」を選んでいるとき、前回の実行に含まれない観点があれば、事実だけを出す
  const lastOpt = state.bases.find((b) => b.value === state.base && b.last);
  const missed = notCovered(toRun, lastOpt?.last?.covered);
  const baseNote = !busy && missed.length ? msg(lang).recipeBar.lastReviewNote(missed.map((r) => displayRecipeTitle(r.title, lang))) : '';

  let currentTitle = state.running;
  let runIdx = 0;
  let totalRuns = active.size;
  let doneIds = new Set<string>();
  if (state.batch.run) {
    doneIds = new Set(state.batch.run.ids.slice(0, state.batch.run.index));
    runIdx = state.batch.run.index;
    totalRuns = state.batch.run.ids.length;
    const curId = state.batch.run.ids[state.batch.run.index];
    const curRecipe = state.recipes.find((r) => r.id === curId);
    if (curRecipe) currentTitle = curRecipe.title;
  }

  const est = batchEstimate(toRun, state.batchStats, p?.estimated_input_tokens ?? null, state.providerId === 'mock');

  return recipeBar(
    {
      recipes: shownRecipes(),
      checkedIds: active,
      running: busy ? (currentTitle || 'レビュー') : '',
      runIndex: runIdx,
      totalRuns,
      disabledReason,
      runCount: plan.block ? undefined : plan.run.length,
      // 全体を止めるときは、止める理由だけを出す（同じ理由を 2 回出さない）
      skipped: plan.block ? [] : groupSkipped(plan.skipped, (r) => displayRecipeTitle(r.title, lang)),
      baseNote,
      collapsed,
      doneIds,
      hiddenCount: state.showAllRecipes ? 0 : state.recipes.length - shownRecipes().length,
      showAll: state.showAllRecipes,
      estimatedTokens: p && toRun.length ? p.estimated_input_tokens * toRun.filter((r) => r.kind !== 'tool').length : null,
      estimatedTimeText: est.timeText,
      runError: state.runError,
      specSlot,
    },
    {
      onToggle: toggleRecipeCheck,
      onSelectAll: selectAllRecipes,
      onClearAll: clearAllRecipes,
      onRun: runCheckedRecipes,
      onCancel: () => {
        if (state.batch.run) state.batch.run.stop = true;
      },
      onToggleShowAll: () => {
        state.showAllRecipes = !state.showAllRecipes;
        render();
        void loadBatchStats();
      },
      onExpand: () => {
        state.recipeBarExpanded = true;
        render();
      },
    },
    getAppLang(),
  );
}

function llmConfigSection(lang: Lang = getAppLang()): HTMLElement {
  const m = msg(lang).mainUi;
  const current = state.providers.find((p) => p.id === state.providerId);
  const select_ = h(
    'select',
    {
      class: 'llm-config-select',
      onchange: (e) => {
        state.providerId = (e.target as HTMLSelectElement).value;
        lsSet(PROVIDER_KEY, state.providerId);
        state.batchStats = {};
        render();
        void refreshStats();
        void loadBatchStats();
      },
    },
    ...state.providers.map((p) => h('option', { value: p.id, ...(p.id === state.providerId ? { selected: true } : {}) }, p.label)),
  );

  return h(
    'section',
    { class: 'llm-config-section' },
    h('h3', {}, m.llmHeading),
    h('p', { class: 'muted', style: 'font-size: 12px; margin-bottom: 8px;' }, m.llmSubtitle),
    select_,
    current?.note ? noteBox(current.note) : null,
    byokSection(lang),
  );
}

/** 自分の API キー（BYOK）の設定。キーは macOS キーチェーンにだけ保存する（設定ファイル・履歴には書かない）。 */
function byokSection(lang: Lang = getAppLang()): HTMLElement {
  const m = msg(lang).mainUi;
  const b = state.byok;
  const keyInput = h('input', {
    class: 'sinput byok-key',
    type: 'password',
    placeholder: m.byokKeyPlaceholder,
    autocomplete: 'off',
    spellcheck: 'false',
  });
  const saveKey = (): void => {
    const key = keyInput.value;
    keyInput.value = '';
    if (key.trim()) void saveByokKey(key);
  };
  keyInput.addEventListener('keydown', (e) => {
    if ((e as KeyboardEvent).key === 'Enter') saveKey();
  });
  const modelInput = h('input', { class: 'sinput byok-model', type: 'text', spellcheck: 'false', value: state.byokDraft.model });
  modelInput.addEventListener('input', () => void (state.byokDraft.model = modelInput.value));
  const tokensInput = h('input', { class: 'sinput byok-tokens', type: 'number', step: '256', value: state.byokDraft.maxTokens });
  tokensInput.addEventListener('input', () => void (state.byokDraft.maxTokens = tokensInput.value));

  const status = !b
    ? h('span', { class: 'muted' }, m.byokLoading)
    : !b.supported
      ? h('span', { class: 'warn' }, m.byokUnsupported)
      : b.key_error
        ? h('span', { class: 'err' }, m.byokVerifyErr(b.key_error))
        : b.key_set
          ? h('span', { class: 'ok-text' }, m.byokConfigured(b.key_last4 ?? '????'))
          : h('span', { class: 'muted' }, m.byokNotConfigured);

  return h(
    'div',
    { class: 'byok' },
    h('h3', {}, m.byokHeading),
    h('p', { class: 'muted byok-note' }, m.byokNote),
    h('div', { class: 'brow' }, h('b', {}, m.byokKeyLabel), status),
    h(
      'div',
      { class: 'brow' },
      keyInput,
      h('button', { class: 'small', disabled: !!b && !b.supported, onclick: saveKey }, b?.key_set ? m.byokReplaceBtn : m.byokSaveBtn),
      b?.key_set
        ? state.byokConfirmDelete
          ? h('span', { class: 'row' }, h('button', { class: 'small', onclick: () => void deleteByokKey() }, m.byokConfirmDeleteBtn), h('button', { class: 'small', onclick: () => ((state.byokConfirmDelete = false), render()) }, m.cancelBtn))
          : h('button', { class: 'small', onclick: () => ((state.byokConfirmDelete = true), render()) }, m.byokDeleteBtn)
        : null,
    ),
    h('div', { class: 'brow' }, h('label', {}, m.byokModelLabel), modelInput, h('label', {}, m.byokMaxTokensLabel), tokensInput, h('button', { class: 'small', onclick: () => void saveByokConfig() }, m.byokSaveBtn)),
    b?.config_error ? h('p', { class: 'err' }, m.byokInvalidConfig(b.config_error)) : null,
    state.byokMsg.text ? h('p', { class: state.byokMsg.kind === 'err' ? 'err' : 'ok-text' }, state.byokMsg.text) : null,
  );
}

/** 実行バーの下: 選んでいる観点の中身（LLM へ送る文章・リンターの状態）。ふだんはたたむ。開閉は、画面を作り直しても残す。 */
function recipeInfoSection(): HTMLElement | null {
  const chosen = pickedRecipes(state.recipes, activeChecked());
  if (!chosen.length) return null;
  const lang = getAppLang();
  const m = msg(lang).mainUi;
  // リンターの許可の確認（「許可して実行」）は、ここに出る。たたんだままだと、止まって見えるので、開く
  const forceLint = state.lintConfirm && chosen.some((r) => r.kind === 'tool');
  const track = (id: string, el: HTMLDetailsElement): HTMLDetailsElement => {
    el.addEventListener('toggle', () => {
      if (el.open) state.infoOpen.add(id);
      else state.infoOpen.delete(id);
    });
    return el;
  };
  const item = (r: Recipe): HTMLElement => {
    const tool = r.kind === 'tool';
    const open = state.infoOpen.has(r.id) || (tool && forceLint);
    const recipeTitle = displayRecipeTitle(r.title, lang);
    const title = tool ? m.recipeInfoToolTitle(recipeTitle) : recipeTitle;
    const source = r.source === 'repo' ? m.recipeInfoRepoSource : '';
    return track(
      r.id,
      h(
        'details',
        { class: 'recipe-info-item', ...(open ? { open: true } : {}) },
        h('summary', {}, title, source),
        h('p', { class: 'muted' }, recipeSummary(r, lang)),
        tool ? lintBody() : h('pre', { class: 'prompt' }, r.prompt),
      ),
    );
  };
  return track(
    '*',
    h(
      'details',
      { class: 'recipe-info', ...(state.infoOpen.has('*') || forceLint ? { open: true } : {}) },
      h('summary', {}, m.recipeInfoSummary(chosen.length)),
      ...chosen.map(item),
      ...state.recipeWarnings.map((w) => h('p', { class: 'note-line' }, w)),
    ),
  );
}

function leftPanel(): HTMLElement {
  const lang = getAppLang();
  const acts = {
    onSelect: (repo: RegisteredRepo) => {
      void changeRepo(repo.path);
    },
    onAdd: () => {
      void chooseAndAddRepo();
    },
    onRemove: (repo: RegisteredRepo) => {
      void handleRemoveRepo(repo);
    },
    onRename: (repo: RegisteredRepo) => {
      handleRenameRepo(repo);
    },
  };

  return h(
    'aside',
    { class: 'left' },
    repoListPanel(visibleRepos(state.registeredRepos), state.repo, acts, state.repoIcons, addBlockedNow(), lang, Object.fromEntries(Object.entries(state.repoData).map(([path, d]) => [path, d.open]))),
  );
}

// ── メイン上部: 観点の中身と、送る内容 ────────────────────────────────────────────────
/** 「リンター」を選んだときの、観点の中身: どの linter が、どの状態か。名前だけでは何の linter か分からないので、言語と、今回の差分に対象があるかも出す。 */
function lintBody(): HTMLElement {
  const plan = state.lintPlan;
  const m = msg(getAppLang()).mainUi;
  const stateMap = m.lintStateText;
  const lintItem = (i: LintItem): HTMLElement =>
    h(
      'li',
      { class: `lintitem ${i.state}` },
      h('div', {}, h('strong', {}, m.lintItemTitle(i.title, i.lang)), h('span', { class: `badge ${i.state === 'ready' ? 'okb' : 'warnb'}` }, stateMap[i.state])),
      h('div', { class: 'muted' }, itemHitsDiff(i) ? m.lintTargetInDiff : m.lintNoTargetInDiff),
      i.command ? h('div', { class: 'muted' }, h('code', {}, i.command)) : null,
      i.state === 'missing' && i.hint ? h('div', { class: 'note-line' }, m.lintInstallHint(i.hint)) : null,
    );
  const hasCodeRunners = !!plan?.items.some((i) => i.runs_repo_code);
  const trustNow = needsTrustNow();
  const confirmBox = state.lintConfirm
    ? h(
        'div',
        { class: 'confirm' },
        h('p', {}, m.lintConfirmBody(trustNow)),
        h('p', { class: 'muted' }, m.lintConfirmRemember),
        h(
          'div',
          { class: 'row' },
          h('button', { class: 'run', onclick: () => void trustAndRun() }, m.lintAllowAndRunBtn),
          h('button', { onclick: () => void ((state.lintConfirm = false), startCheckedRecipes()) }, m.lintRunOthersBtn),
          h('button', { onclick: () => void ((state.lintConfirm = false), render()) }, m.cancelBtn),
        ),
      )
    : null;
  return h(
    'div',
    {},
    confirmBox,
    plan && plan.items.length
      ? h(
          'div',
          {},
          h('p', { class: 'muted' }, m.lintRunningNote),
          h('ul', { class: 'lintlist' }, ...plan.items.map(lintItem)),
          hasCodeRunners
            ? plan.trusted
              ? h('p', { class: 'muted lint-trust-row' }, h('span', {}, m.lintAllowedNote), h('button', { class: 'small', onclick: () => void untrust() }, m.lintRevokeBtn))
              : h('p', { class: 'muted lint-trust-row' }, h('span', {}, m.lintNotAllowedNote), h('button', { class: 'small run', onclick: () => void trustRepoOnly() }, m.lintAllowRepoBtn))
            : null,
        )
      : h('p', { class: 'muted' }, state.preview ? m.lintNoneFound : m.lintSelectRepo),
  );
}

/** 送る内容（見えること）。ふだんは 1 行にたたんでおく。 */
function sendBox(isTool: boolean): HTMLElement {
  const p = state.preview;
  const lang = getAppLang();
  const m = msg(lang).mainUi;
  const title = isTool ? m.sendBoxTitleTool : m.sendBoxTitleLlm;
  let summary: string;
  const body = h('div', { class: 'box' });
  if (state.previewError) {
    summary = `${title}: ${m.sendBoxCannotRead}`;
    body.append(h('p', { class: 'err' }, state.previewError));
  } else if (p) {
    // 比較元は、上部のチップと、開いた中身にある。1 行は短く保つ
    summary = isTool
      ? m.sendBoxSummaryTool(title, p.files.length)
      : m.sendBoxSummaryLlm(title, p.files.length, nf.format(p.estimated_input_tokens), p.excluded.length);
    body.append(
      h('p', { class: 'kv' }, h('b', {}, m.sendBoxRangeLabel), rangeText(p.base, state.bases, lang)),
      h(
        'p',
        { class: 'kv' },
        h('b', {}, 'HEAD '),
        h('code', {}, p.head.slice(0, 8)),
        m.sendBoxDiffSep,
        h('code', {}, p.diff_hash.slice(0, 8)),
        m.sendBoxBytesSuffix(nf.format(p.diff_bytes)),
      ),
    );
    if (p.files.length) body.append(h('ul', { class: 'files' }, ...p.files.map((f) => h('li', {}, h('code', {}, f.path), ` ${nf.format(f.bytes)} B`))));
    if (p.excluded.length) {
      body.append(
        h('p', { class: 'warn' }, m.sendBoxExcludedHeading(p.excluded.length)),
        h('ul', { class: 'files' }, ...p.excluded.map((f) => h('li', {}, h('code', {}, f.path), ` — ${f.reason}`))),
      );
    }
    if (!p.files.length) body.append(h('p', { class: 'warn' }, m.sendBoxNoDiff));
  } else {
    summary = m.sendBoxSelectRepoSummary(title);
    body.append(h('p', { class: 'muted' }, m.sendBoxSelectRepoBody));
  }
  return h('details', { class: 'sendbox' }, h('summary', {}, summary), body);
}

/** 長い注意書きは、1 文だけを見せて、全文は、開いて見る。短ければ、そのまま。 */
function noteBox(note: string): HTMLElement {
  const first = note.split('。')[0];
  if (!note.includes('。') || note.length <= first.length + 1 || note.length < 60) return h('p', { class: 'note' }, note);
  return h('details', { class: 'note notebox' }, h('summary', {}, `${first}。`), h('p', {}, note));
}

// ── 右: 履歴 ──────────────────────────────────────────────────────────────────
function rightPanel(): HTMLElement {
  const lang = getAppLang();
  const m = msg(lang).mainUi;
  const scope = state.repo ? shortRepo(state.preview?.repo ?? state.repo) : m.rightPanelAllRepos;
  const emptyText = state.repo ? m.rightPanelEmptyRepo : m.rightPanelEmptyAll;
  return h(
    'aside',
    { class: 'right' },
    historyPanel(
      state.history,
      state.selected?.id ?? null,
      state.picked,
      scope,
      state.preview?.repo ?? state.repo,
      emptyText,
      {
        open: (id) => void select(id),
        detail: (id) => void openDetail(id),
        toggle: togglePick,
        toggleDay: (key) => {
          if (state.historyDays.has(key)) state.historyDays.delete(key);
          else state.historyDays.add(key);
          render();
        },
        toggleAll: togglePickAll,
        report: () => void openReportFor([...state.picked]),
        remove: () => void removePicked(),
      },
      state.historyDays,
      state.historyCollapsed,
      () => {
        state.historyCollapsed = !state.historyCollapsed;
        render();
      },
      lang,
    ),
  );
}

// ── 右: ログビューア ──────────────────────────────────────────────────────────────

const STATE_LABEL: Record<TriageState, string> = { new: '未仕分け', fix: '対応', wait: '待ち', dismiss: '無視' };

function carryNote(c: Carry | null | undefined): HTMLElement | null {
  if (!c) return null;
  return h(
    'div',
    { class: 'carry' },
    `前回までの仕分け「${STATE_LABEL[c.state]}」を引き継ぎました` + (c.exact ? '' : '（引用がほぼ同じ）'),
  );
}

const FATE_VIEW: Record<Fate['fate'], { badge: string; cls: string; text: string }> = {
  persists: { badge: '継続', cls: 'bad', text: '今回も指摘されました（まだ直っていません）' },
  gone: { badge: '解消', cls: 'ok', text: '引用した行がコードから消えました（変更されたと見られます）' },
  not_reraised: { badge: '再指摘なし', cls: '', text: '今回は指摘されませんでしたが、引用した行はまだコードにあります' },
  file_missing: { badge: 'ファイルなし', cls: '', text: 'このファイルが見つかりません（削除・移動の可能性）' },
  unknown: { badge: '不明', cls: '', text: '引用が空などで、調べられませんでした' },
};

/** 前回の「対応」「待ち」の指摘が、今回どうなったか。「解消」は、引用の行がファイルの現在の内容から消えたことだけを根拠にしている。 */
function fatesPanel(fates: Fate[]): HTMLElement | null {
  if (!fates.length) return null;
  return h(
    'section',
    { class: 'fates' },
    h('h3', {}, '前回のレビューからの変化'),
    h(
      'ul',
      {},
      ...fates.map((x) => {
        const v = FATE_VIEW[x.fate];
        return h(
          'li',
          {},
          h('span', { class: `badge ${v.cls}` }, v.badge),
          h('strong', {}, ` ${x.title || '(見出しなし)'} `),
          h('span', { class: 'muted' }, `（前回は「${STATE_LABEL[x.state_before]}」・${x.file}）`),
          h('div', { class: 'f-fate' }, v.text),
        );
      }),
    ),
    h('p', { class: 'note' }, '「解消」は、引用した行がファイルの現在の内容から消えたことだけを根拠にした推定です。直ったことの証明ではありません。'),
  );
}

/** `compact` は、差分の行の直下に置く小さい表示。場所と引用は、すぐ上の行がそのまま示している。 */
/** 仕分け（対応・待ち・無視）と、「CLI に渡す」（「対応」の指摘の、その 1 件だけ）。 */
function triageLabels(lang: Lang): [TriageState, string][] {
  return msg(lang).mainUi.triageLabels;
}

/** `resolved` は、直し済み（対応済みの印、または、エージェントの「対応した」の申告）。直し済みの指摘には、「CLI に渡す」を出さない。 */
function triageControls(index: number, status: TriageState, hand: boolean, cls = 'triage', resolved = false): HTMLElement {
  const lang = getAppLang();
  const m = msg(lang).mainUi;
  return h(
    'div',
    { class: cls },
    ...triageLabels(lang).map(([k, label]) =>
      h(
        'button',
        {
          class: `t-${k} ${status === k ? 'on' : ''}`,
          title: status === k ? m.triageResetTitle : '',
          // 同じ状態をもう一度押すと、未仕分けに戻す
          onclick: () => void setTriage(index, status === k ? 'new' : k),
        },
        label,
      ),
    ),
    // 直し済みの指摘は、もう渡さない
    hand && status === 'fix' && !resolved ? handoffSendButton(index, state.handoffUi, handoffActs()) : null,
  );
}

const SEV_ICON: Record<string, string> = { high: '▲', medium: '◆', low: '●' };

/** カードの 2 行目に並べる、小さな印（例外だけ・あるときだけ）。 */
function findingChips(f: Finding, status: TriageState, carry: Carry | null | undefined, note: Note | undefined, agent: AgentReport | null | undefined): HTMLElement[] {
  const lang = getAppLang();
  const m = msg(lang).mainUi;
  const chip = (cls: string, text: string, title = ''): HTMLElement => h('span', { class: `chip ${cls}`, ...(title ? { title } : {}) }, text);
  const out: HTMLElement[] = [];
  if (!f.verified) {
    out.push(
      chip(
        'warn',
        m.chipEvidenceMismatch,
        m.chipEvidenceMismatchTitle(GROUNDING_NOTE),
      ),
    );
  }
  if (f.spec_verified === false) {
    out.push(chip('warn', m.chipSpecMismatch, m.specQuoteMismatch));
  } else if (f.spec_verified === true) {
    out.push(chip('ok', m.chipSpecVerified, m.specQuoteVerified));
  }
  if (f.rule) out.push(chip('rule', f.rule));
  if (agent) {
    const verdictText = agent.verdict === 'did' ? m.chipAgentDid : m.chipAgentSkipped;
    out.push(chip(agent.verdict === 'did' ? 'did' : 'skipped', `🤖 ${verdictText}`, m.chipAgentTitle));
  }
  // 「対応」にした指摘は、まだ直していないか（未対応）、直し済みか（対応済み）を、閉じたままでも見えるようにする
  const st = fixStateChip(status, isDoneFinding(status, note, agent), lang);
  if (st) out.push(chip(st.cls, st.text, st.title));
  if (note?.memo) out.push(chip('memo', m.chipMemo, note.memo));
  if (carry) out.push(chip('carry', m.chipCarried, m.chipCarriedTitle(STATE_LABEL[carry.state], carry.state)));
  return out;
}

/** 指摘のカード（通常）。閉じた状態は 2 行（重大度・見出し・場所・仕分け／内容の 1 行目と小さな印）。押すと、全部が開く。 */
function richFindingCard(reviewId: string, f: Finding, index: number, status: TriageState, carry: Carry | null | undefined, note: Note | undefined, hand: boolean, agent: AgentReport | null | undefined): HTMLElement {
  // 対応済み（エージェントが「対応した」と申告した、または、あなたが対応済みにした）の指摘は、1 行に畳む（押すと開き、見出しを押すと閉じる）。
  // 仕分けのボタン（対応・待ち・無視）は、出さない。手で付けた対応済みは、開いたカードの中の「押すと戻す」で戻せる
  const byAgent = hand && agentDone(agent, status);
  const done = isDoneFinding(status, note, byAgent ? agent : null);
  if (done && !state.openCards.has(index)) {
    return foldedRow(
      f,
      () => {
        state.openCards.add(index);
        render();
      },
      { index, tag: findingTag(reviewId, index) },
      byAgent ? 'agent' : 'you',
    );
  }
  const open = done || state.expanded.has(index);
  const toggle = (): void => {
    if (done) state.openCards.delete(index);
    else if (state.expanded.has(index)) state.expanded.delete(index);
    else state.expanded.add(index);
    render();
  };
  const loc = `${f.file || '(ファイル不明)'}${f.line === null ? '' : `:${f.line}`}`;
  const chips = findingChips(f, status, carry, note, agent);
  return h(
    'article',
    { class: `finding fc ${f.verified ? '' : 'unverified'} s-${status} fc-sev-${f.severity} ${open ? 'open' : ''}` },
    h(
      'div',
      {
        class: 'fc-head',
        role: 'button',
        tabindex: '0',
        'aria-expanded': String(open),
        title: open ? '押すと、閉じます' : '押すと、内容と引用が開きます',
        onclick: toggle,
        onkeydown: (ev: Event) => {
          const e = ev as KeyboardEvent;
          // 中のボタンのキー操作で、開閉しない
          if (e.target !== e.currentTarget) return;
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            toggle();
          }
        },
      },
      // 番号は、エージェントの報告（mrcr:…#番号）と照合するための印。並べ替えても、変わらない
      findingNo(index, findingTag(reviewId, index)),
      // 重要度は、記号だけでなく、文字（high / medium / low）で見せる。開かなくても、一覧のまま、見比べられるように
      h('span', { class: `badge fc-sev sev-${f.severity}`, title: `重大度: ${f.severity}`, 'aria-label': `重大度 ${f.severity}` }, `${SEV_ICON[f.severity] ?? '●'} ${f.severity}`),
      h('strong', { class: 'fc-title' }, f.title || '(見出しなし)'),
      h('span', { class: 'fc-caret' }, open ? '▾' : '▸'),
    ),
    // 仕分け（対応・待ち・無視・CLI に渡す）は、見出しと内容の間。閉じていても押せる。「対応した」と申告された指摘は、読むだけ
    done ? null : triageControls(index, status, hand, 'triage fc-triage'),
    // 閉じているときだけ、内容の 1 行目（はみ出す分は「…」）。開いたら、下に全文を出す
    h('div', { class: 'fc-sub' }, open ? null : h('span', { class: 'fc-peek' }, f.detail), ...chips),
    // 「CLI に渡す」を押した直後の案内は、閉じていても、見える所に出す（押した結果が見えなくならないように）
    hand && !done ? handoffResult(index, state.handoffUi, handoffActs()) : null,
    open
      ? h(
          'div',
          { class: 'fc-body' },
          tagLine(findingTag(reviewId, index), copyText),
          h('p', { class: 'fc-detail' }, f.detail),
          // 引用の出どころ（パス:行）は、見出しではなく、ここ。長いパスも、切らずに折り返して全部見せる
          h(
            'div',
            { class: 'fc-evrow' },
            h('div', { class: 'f-ev-label', title: GROUNDING_NOTE }, f.verified ? msg(getAppLang()).mainUi.quoteVerified : msg(getAppLang()).mainUi.quoteNotVerified),
            h('code', { class: 'fc-loc' }, loc),
          ),
          h('pre', { class: 'evidence' }, f.evidence || msg(getAppLang()).mainUi.noQuote),
          f.spec_quote
            ? h(
                'div',
                { class: 'fc-spec-box' },
                h(
                  'div',
                  { class: 'f-ev-label' },
                  f.spec_verified === true
                    ? msg(getAppLang()).mainUi.specQuoteVerified
                    : f.spec_verified === false
                      ? msg(getAppLang()).mainUi.specQuoteMismatch
                      : msg(getAppLang()).mainUi.specQuoteLabel,
                ),
                h('pre', { class: 'spec-quote' }, f.spec_quote),
              )
            : null,
          carryNote(carry),
          hand ? agentBox(agent, when, showNowDiff) : null,
          handlingBox(index, status, note, byAgent),
        )
      : null,
  );
}

function findingCard(reviewId: string, f: Finding, index: number, status: TriageState, carry?: Carry | null, compact = false, note?: Note, hand = false, agent?: AgentReport | null): HTMLElement {
  // 通常のカード（指摘タブ）は、新しい作り（閉じた状態が 2 行）。差分に重ねる小さなカード（compact）は、これまでの作り
  if (!compact) return richFindingCard(reviewId, f, index, status, carry, note, hand, agent);
  const triage = triageControls(index, status, hand, 'triage', isDoneFinding(status, note, agent));
  // 「対応した」と申告された「対応」の指摘は、畳む。開いたあとは、「畳む」で戻せる
  const done = hand && agentDone(agent, status);
  if (done && !state.openCards.has(index)) {
    return foldedRow(
      f,
      () => {
        state.openCards.add(index);
        render();
      },
      { index, tag: findingTag(reviewId, index) },
    );
  }
  return h(
    'article',
    { class: `finding ${f.verified ? '' : 'unverified'} s-${status}` },
    h(
      'div',
      // 開いた「対応済み」の指摘は、見出しの行を押すと、閉じる（開いたのと、同じ場所）
      done
        ? {
            class: 'f-head foldable',
            title: '押すと、畳みます',
            onclick: () => {
              state.openCards.delete(index);
              render();
            },
          }
        : { class: 'f-head' },
      findingNo(index, findingTag(reviewId, index)),
      h('span', { class: `badge sev-${f.severity}` }, f.severity),
      h('strong', {}, f.title || '(見出しなし)'),
      f.rule ? h('span', { class: 'badge rule' }, f.rule) : null,
      h('span', { class: `badge ${f.verified ? 'ok' : 'bad'}` }, f.verified ? '根拠あり' : '根拠不一致'),
      f.spec_verified === true
        ? h('span', { class: 'badge ok' }, '設計書一致')
        : f.spec_verified === false
          ? h('span', { class: 'badge bad' }, '設計書不一致')
          : null,
      done ? h('span', { class: 'foldcaret' }, '▾') : null,
    ),
    compact ? null : h('div', { class: 'f-loc' }, h('code', {}, `${f.file || '(ファイル不明)'}${f.line === null ? '' : `:${f.line}`}`)),
    h('p', {}, f.detail),
    compact ? null : h('div', { class: 'f-ev-label' }, f.verified ? '引用（差分に実在）' : '引用（この差分には見つからない）'),
    compact ? null : h('pre', { class: 'evidence' }, f.evidence || '(引用なし)'),
    compact ? null : (f.spec_quote ? h('div', { class: 'f-ev-label' }, f.spec_verified ? '設計書の引用（原文に実在）' : '設計書の引用（原文と不一致）') : null),
    compact ? null : (f.spec_quote ? h('pre', { class: 'spec-quote' }, f.spec_quote) : null),
    carryNote(carry),
    // 「対応した」と申告された指摘は、読むだけ（仕分けと「CLI に渡す」は、出さない）。やり直すなら、もう一度レビューを実行する
    done ? null : triage,
    hand && !done ? handoffResult(index, state.handoffUi, handoffActs()) : null,
    hand ? agentBox(agent, when, showNowDiff) : null,
    compact ? null : handlingBox(index, status, note, done),
  );
}

/** 最後に仕分けた時刻の 1 行。時刻が無いものは、推定で埋めず、「記録なし」と言う。 */
function triageStamp(status: TriageState, note?: Note): string {
  if (status === 'new') return '';
  const m = msg(getAppLang()).mainUi;
  if (!note || note.triage_at === null) return m.triageNoTimestamp;
  const who = note.triage_source === 'carry' ? m.triageCarrySource : m.triageUserSource;
  return m.triageLastStamp(when(note.triage_at), who, note.triage_head ? note.triage_head.slice(0, 7) : null);
}

/** 対応メモと、「対応済み」。成果物のレポートに載る、「いつ・どう対応したか」の記録。 */
function handlingBox(index: number, status: TriageState, note?: Note, agentDone = false): HTMLElement {
  const m = msg(getAppLang()).mainUi;
  // エージェントが「対応した」と申告したものは、申告が、その記録になる。メモの入力と「対応済みにする」は、出さない
  // （すでに書いたメモと、対応済みの時刻は、読むだけで残す）
  if (agentDone) {
    const stamp = triageStamp(status, note);
    return h(
      'div',
      { class: 'handling' },
      note?.memo ? h('div', { class: 'muted' }, `対応メモ: ${note.memo}`) : null,
      note?.done_at != null ? h('div', { class: 'muted' }, `対応済み: ${when(note.done_at)}${note.done_head ? `・HEAD ${note.done_head.slice(0, 7)}` : ''}`) : null,
      stamp ? h('div', { class: 'muted stamp' }, stamp) : null,
    );
  }
  const memo = h('input', {
    class: 'memo',
    type: 'text',
    maxlength: '2000',
    placeholder: m.memoPlaceholder,
    value: note?.memo ?? '',
    onchange: (e) => void saveMemo(index, (e.target as HTMLInputElement).value),
  });
  const done = note?.done_at ?? null;
  const stamp = triageStamp(status, note);
  return h(
    'div',
    { class: 'handling' },
    h('div', { class: 'row' }, memo),
    status === 'fix'
      ? h(
          'div',
          { class: 'row done-row' },
          h('button', { class: `small done ${done !== null ? 'on' : ''}`, onclick: () => void toggleDone(index, done === null) }, done !== null ? m.doneRevertBtn : m.markDoneBtn),
          done !== null ? h('span', { class: 'muted' }, `${when(done)}${note?.done_head ? `・HEAD ${note.done_head.slice(0, 7)}` : ''}`) : null,
        )
      : null,
    stamp ? h('div', { class: 'muted stamp' }, stamp) : null,
  );
}

/** 仕分けの集計と、CUI に貼る文面のコピー。コピーされる文面は、押す前に開いて確認できる。 */
function handoffInfo(r: ReviewView) {
  const count = (s: TriageState) => r.triage.filter((x) => x === s).length;
  // 「対応した」と申告された指摘は、渡し済み。まだ渡していない「対応」の数に、入れない
  const done = r.triage.filter((x, i) => isDoneFinding(x, r.finding_notes?.[i], r.agent_reports?.[i])).length;
  return { fix: count('fix') - done, done, wait: count('wait'), dismiss: count('dismiss'), fresh: count('new'), text: state.handoff };
}

/** 一覧の上の、細い 1 行（件数と、「対応」が 2 件以上のときだけ、まとめて渡す）。 */
const handoffBar = (r: ReviewView, withExpandAll = false): HTMLElement => {
  const m = msg(getAppLang()).mainUi;
  const info = handoffInfo(r);
  // 「すべて開く／閉じる」（カードは、既定で、閉じている）と、「対応内容を取り込む」を、右端に並べる
  const allOpen = r.findings.every((_, i) => state.expanded.has(i));
  const expandAll =
    withExpandAll && r.findings.length > 1
      ? h(
          'button',
          {
            class: 'small linkish',
            onclick: () => {
              state.expanded = allOpen ? new Set() : new Set(r.findings.map((_, i) => i));
              render();
            },
          },
          allOpen ? m.collapseAllBtn : m.expandAllBtn,
        )
      : null;
  const sevBtns = fixAllSeverityButtons(
    r.findings,
    r.triage,
    withExpandAll,
    (sev, count, status) => (status === 'dismiss' ? m.dismissAllSevBtn(sev, count) : m.fixAllSevBtn(sev, count)),
    (sev, status) => void fixAllBySeverity(sev, status),
  );
  // 申告の状態と、取り込みのボタン。ボタンは、申告が足りない指摘があるときだけ（`mrcr handoff report` で保存された分は、すでに DB にある）
  const fetchBar = agentStatusBar(agentStatus(r.triage, r.finding_notes, r.agent_reports), state.agentUi, agentActs());
  const tools = expandAll || fetchBar || sevBtns.length > 0 ? h('span', { class: 'fc-tools' }, ...sevBtns, expandAll, fetchBar) : null;
  return h('div', {}, handoffTop(info, tools), agentNotice(state.agentUi, agentActs()));
};

// ── 差分に指摘を重ねる ────────────────────────────────────────────────────────────
// レビューの記録ごとに 1 回だけ組み立てる（指摘も差分も、記録の中では変わらない）。
/** 「いまの差分」を読み取る（比較元は、レビューのときと同じ。読み取りだけ）。 */
async function loadNowDiff(): Promise<void> {
  const r = state.selected;
  if (!r) return;
  state.nowDiff = { id: r.id, loading: true, error: '', data: null, view: null };
  render();
  let next: NonNullable<typeof state.nowDiff>;
  try {
    const data = await invoke<CurrentDiff>('current_diff', { id: r.id });
    next = { id: r.id, loading: false, error: '', data, view: buildViewFromDiff(data.diff_text, []) };
  } catch (e) {
    next = { id: r.id, loading: false, error: String(e), data: null, view: null };
  }
  // 読んでいるあいだに、別のレビューへ移っていたら、捨てる
  if (state.selected?.id === r.id) state.nowDiff = next;
  render();
}

/** 「いまの差分を見る」: 差分タブの「いま」を開く。 */
function showNowDiff(): void {
  const r = state.selected;
  if (!r) return;
  state.tab = 'diff';
  state.diffWhen = 'now';
  if (state.nowDiff?.id !== r.id) void loadNowDiff();
  else render();
}

const diffCache = new Map<string, DiffView | null>();
function viewFor(r: ReviewView): DiffView | null {
  if (!diffCache.has(r.id)) diffCache.set(r.id, buildView(r.prompt, r.findings));
  return diffCache.get(r.id) ?? null;
}

function segNodes(parts: Seg[]): Node[] {
  return parts.map((s) => (s.changed ? h('mark', { class: 'diff-seg' }, s.text) : document.createTextNode(s.text)));
}

const lineNo = (n: number | null): HTMLElement => h('span', { class: 'dn' }, n === null ? '' : String(n));

/** 統合表示の行。`side` は、指摘をこの行に重ねるとき、引用が見つかった側。 */
interface ULine {
  el: HTMLElement;
  side: 'old' | 'new' | 'both';
}

function unifiedLines(row: DiffRow): ULine[] {
  const line = (cls: string, o: number | null, n: number | null, mark: string, content: Node[], side: ULine['side']): ULine => ({
    el: h('div', { class: `dl ${cls}` }, lineNo(o), lineNo(n), h('span', { class: 'dm' }, mark), h('span', { class: 'dc' }, ...content)),
    side,
  });
  const text = (s: string | null): Node[] => [document.createTextNode(s ?? '')];
  switch (row.type) {
    case 'eq':
      return [line('eq', row.leftNo, row.rightNo, ' ', text(row.right), 'both')];
    case 'del':
      return [line('del', row.leftNo, null, '-', text(row.left), 'old')];
    case 'ins':
      return [line('ins', null, row.rightNo, '+', text(row.right), 'new')];
    case 'mod': {
      const s = inlineSegments(row.left ?? '', row.right ?? '');
      return [
        line('del', row.leftNo, null, '-', segNodes(s.left), 'old'),
        line('ins', null, row.rightNo, '+', segNodes(s.right), 'new'),
      ];
    }
  }
}

function splitRow(row: DiffRow): HTMLElement {
  let left: Node[] = [document.createTextNode(row.left ?? '')];
  let right: Node[] = [document.createTextNode(row.right ?? '')];
  if (row.type === 'mod' && row.left !== null && row.right !== null) {
    const s = inlineSegments(row.left, row.right);
    left = segNodes(s.left);
    right = segNodes(s.right);
  }
  return h(
    'div',
    { class: `ds ${row.type}` },
    lineNo(row.leftNo),
    h('span', { class: 'dc dl-left' }, ...left),
    lineNo(row.rightNo),
    h('span', { class: 'dc dl-right' }, ...right),
  );
}

function diffTab(r: ReviewView): HTMLElement {
  const now = state.diffWhen === 'now';
  const nd = state.nowDiff?.id === r.id ? state.nowDiff : null;
  // 「レビューした時点」と「いま」の切り替え（いまは、エージェントが直したあとの差分）
  const whenToggle = h(
    'div',
    { class: 'dmode' },
    ...([['then', 'レビューした時点'], ['now', 'いま']] as ['then' | 'now', string][]).map(([w, label]) =>
      h('button', { class: state.diffWhen === w ? 'on' : '', onclick: () => (w === 'now' ? showNowDiff() : ((state.diffWhen = 'then'), render())) }, label),
    ),
  );
  if (now && (!nd || nd.loading)) return h('div', {}, h('div', { class: 'dtool' }, whenToggle), h('p', { class: 'muted' }, 'いまの差分を読み取っています…'));
  if (now && nd?.error) return h('div', {}, h('div', { class: 'dtool' }, whenToggle, h('button', { class: 'small', onclick: () => void loadNowDiff() }, '読み直す')), h('p', { class: 'err' }, nd.error));
  const view = now ? (nd?.view ?? null) : viewFor(r);
  if (!view) {
    return h('p', { class: 'muted' }, 'この記録から差分を取り出せませんでした（「送った内容」タブにプロンプト全文があります）。');
  }
  const mode = state.diffMode;
  const status = (i: number): TriageState => r.triage[i] ?? 'new';
  const card = (i: number, compact: boolean): HTMLElement => findingCard(r.id, r.findings[i], i, status(i), r.carries?.[i], compact);
  const layer = (i: number): HTMLElement => h('div', { class: 'dlayer' }, card(i, true));
  const mark = (el: HTMLElement, i: number): void => el.classList.add('anchored', `sev-${r.findings[i].severity}`);

  const toggle = h(
    'div',
    { class: 'dmode' },
    ...([['unified', '統合'], ['split', '並べて']] as [DiffMode, string][]).map(([m, label]) =>
      h('button', { class: mode === m ? 'on' : '', onclick: () => ((state.diffMode = m), lsSet(DIFFMODE_KEY, m), render()) }, label),
    ),
  );

  const sections: HTMLElement[] = [];
  if (view.orphans.length) {
    sections.push(
      h('section', { class: 'dfile' }, h('div', { class: 'dfile-h' }, '差分に無いファイルへの指摘'), ...view.orphans.map((i) => card(i, false))),
    );
  }
  for (const fv of view.files) {
    const byRow = new Map<string, number[]>();
    const loose: number[] = [];
    for (const p of fv.placed) {
      if (!p.at) {
        loose.push(p.index);
        continue;
      }
      const key = `${p.at.hunk}:${p.at.row}`;
      const list = byRow.get(key) ?? [];
      list.push(p.index);
      byRow.set(key, list);
    }
    const sideOf = new Map<number, 'old' | 'new'>(fv.placed.flatMap((p) => (p.at ? [[p.index, p.at.side] as [number, 'old' | 'new']] : [])));

    const body: HTMLElement[] = [];
    fv.file.hunks.forEach((hk, hi) => {
      body.push(h('div', { class: 'dhunk-h' }, hk.header));
      hk.rows.forEach((row, ri) => {
        const idxs = byRow.get(`${hi}:${ri}`) ?? [];
        if (mode === 'split') {
          const el = splitRow(row);
          if (idxs.length) mark(el, idxs[0]);
          body.push(el, ...idxs.map(layer));
          return;
        }
        const lines = unifiedLines(row);
        let anchorAt = lines.length - 1;
        if (idxs.length) {
          const want = sideOf.get(idxs[0]) ?? 'new';
          const hit = lines.map((l) => l.side).lastIndexOf(want);
          anchorAt = hit >= 0 ? hit : anchorAt;
          mark(lines[anchorAt].el, idxs[0]);
        }
        lines.forEach((l, li) => {
          body.push(l.el);
          if (li === anchorAt) body.push(...idxs.map(layer));
        });
      });
    });

    sections.push(
      h(
        'section',
        { class: 'dfile' },
        h(
          'div',
          { class: 'dfile-h' },
          h('code', {}, fv.file.path),
          fv.file.isNew ? h('span', { class: 'badge ok' }, '新規') : null,
          h('span', { class: 'muted' }, `+${fv.file.added} −${fv.file.removed}`),
          fv.placed.length ? h('span', { class: 'badge' }, `指摘 ${fv.placed.length}`) : null,
        ),
        loose.length
          ? h('div', { class: 'dloose' }, h('p', { class: 'note' }, '差分のどの行にも結びつかなかった指摘（引用も行番号も、この差分に合いません）'), ...loose.map((i) => card(i, false)))
          : null,
        h('div', { class: `dbody ${mode}` }, ...body),
      ),
    );
  }

  const nowNote = (): HTMLElement => {
    const d = nd!.data!;
    const base = baseLabel(r.snapshot.base) || '未コミットの変更（HEAD から）';
    return h(
      'div',
      { class: 'nownote' },
      h(
        'p',
        { class: 'note' },
        `いまの作業ツリーまでの差分です。見る範囲は、レビューのときと同じです（${base}）。`,
        d.same_as_review ? 'レビューした時点から、変わっていません。' : 'レビューした時点から、変わっています。',
        d.head !== r.snapshot.head ? `HEAD は、レビューのときから、進んでいます（${r.snapshot.head.slice(0, 8)} → ${d.head.slice(0, 8)}）。` : '',
        '指摘は、レビューした時点の行番号なので、ここには、重ねていません。',
        d.excluded.length ? `送らない設定のファイルが ${d.excluded.length} 件あります。` : '',
      ),
      h('button', { class: 'small', onclick: () => void loadNowDiff() }, '読み直す'),
    );
  };
  return h(
    'div',
    {},
    now ? null : r.findings.length ? handoffBar(r) : null,
    h('div', { class: 'dtool' }, whenToggle, toggle, now ? null : h('span', { class: 'muted' }, '指摘は、引用した行の文字列で差分の行に結びつけています（行番号は参考）。')),
    now ? nowNote() : null,
    ...sections,
  );
}

const RELATED_STATUS: Record<string, string> = {
  sent: '送った',
  none: '差分の外に使用箇所なし',
  too_many: 'ありふれていて除いた',
  budget: '上限に達して送らなかった',
};

/** 送った関連コード（差分に出ていない使用箇所）。何を根拠に見たのかを、あとから確かめられる。 */
function relatedSection(rel: Related | null): HTMLElement | null {
  if (!rel) return null;
  const sent = rel.snippets.length;
  return h(
    'div',
    {},
    h('h3', {}, `送った関連コード（${sent} か所）`),
    h('p', { class: 'note' }, '差分で変更した名前が、差分に出ていない場所でどう使われているかです。この観点では、これも一緒に送っています。'),
    h(
      'ul',
      { class: 'files' },
      ...rel.names.map((n) =>
        h('li', {}, h('code', {}, n.name), ` — ${n.why} — 使用箇所 ${n.hits} か所（${RELATED_STATUS[n.status] ?? n.status}）`),
      ),
    ),
    ...rel.snippets.map((s) =>
      h('details', {}, h('summary', {}, `${s.name}: ${s.file} ${s.start}〜${s.end} 行`), h('pre', { class: 'prompt' }, s.text)),
    ),
  );
}

/** 結果（指摘・差分・送った内容・応答・詳細）。 */
function resultView(): HTMLElement {
  const r: ReviewView | null = state.selected;
  const lang = getAppLang();
  const m = msg(lang).mainUi;
  if (!r) {
    return h(
      'div',
      { class: 'result empty' },
      h('p', {}, m.resultEmpty),
    );
  }
  if (isRecordTab(state.tab)) state.recordSub = state.tab;
  const tabs: [Tab, string, boolean][] = [
    ['findings', m.tabFindings(r.findings.length), state.tab === 'findings'],
    ['diff', m.tabDiff, state.tab === 'diff'],
    [state.recordSub, m.tabRecord, isRecordTab(state.tab)],
  ];
  const head = h(
    'div',
    { class: 'd-head' },
    h('div', { class: 'row dhead-row' }, h('h2', {}, `${r.recipe.title} — ${shortRepo(r.repo)}`), h('span', { class: 'spacer' }), h('button', { class: 'small', title: m.reportThisReviewTitle, onclick: () => void openReportFor([r.id]) }, m.reportThisReviewBtn)),
    // 見出しは、観点とリポジトリだけ。日時・LLM・モデル・所要時間・HEAD・差分・比較元は、履歴の行と、「詳細」タブにある
  );
  const nav = h(
    'nav',
    { class: 'tabs' },
    ...tabs.map(([k, label, on]) => h('button', { class: on ? 'on' : '', onclick: () => ((state.tab = k), render()) }, label)),
  );
  const recordSubLabels: Record<RecordSub, string> = {
    detail: m.recordSubDetail,
    sent: m.recordSubSent,
    response: m.recordSubResponse,
  };
  // 「記録」の中: 詳細 / 送った内容 / 応答
  const subnav = isRecordTab(state.tab)
    ? h(
        'div',
        { class: 'subtabs' },
        ...RECORD_SUBS.map(([k]) => h('button', { class: state.tab === k ? 'on' : '', onclick: () => ((state.tab = k), render()) }, recordSubLabels[k])),
      )
    : null;

  let body: HTMLElement;
  if (state.tab === 'findings') {
    const canRerun = canTrustRerun(r.recipe.id, r.error, state.lintPlan);
    body = h(
      'div',
      {},
      r.error
        ? h(
            'p',
            { class: canRerun ? 'err err-row' : 'err' },
            h('span', {}, m.execFailed(r.error)),
            canRerun
              ? h('button', { class: 'small run', onclick: () => void trustAndRerunLint() }, m.allowAndRerunBtn)
              : null,
          )
        : null,
      ...(r.notes ?? []).map((n) => h('p', { class: 'note-line' }, n)),
      r.parse_error ? h('p', { class: 'err' }, m.parseFailed(r.parse_error)) : null,
      !r.error && !r.parse_error && !r.findings.length ? h('p', { class: 'summary' }, m.noFindings) : null,
      r.findings.length ? handoffBar(r, true) : null,
      fatesPanel(r.fates ?? []),
      // 対応済みは、下に置く（「対応済みにする」を押した直後に、下へ移る）
      ...findingOrder(r.findings.length, (i) => isDoneFinding(r.triage[i] ?? 'new', r.finding_notes?.[i], r.agent_reports?.[i]), (i) => r.findings[i].severity).map((i) => findingCard(r.id, r.findings[i], i, r.triage[i] ?? 'new', r.carries?.[i], false, r.finding_notes?.[i], true, r.agent_reports?.[i])),
      r.findings.length ? handoffPreview(handoffInfo(r), state.handoffUi, handoffActs()) : null,
    );
  } else if (state.tab === 'diff') {
    body = diffTab(r);
  } else if (state.tab === 'sent') {
    body = h(
      'div',
      {},
      h('p', { class: 'note' }, r.provider.note),
      h('h3', {}, `送ったファイル（${r.snapshot.files.length}）`),
      h('ul', { class: 'files' }, ...r.snapshot.files.map((f) => h('li', {}, h('code', {}, f.path), ` ${nf.format(f.bytes)} B`))),
      h('h3', {}, `送らなかったもの（${r.snapshot.excluded.length}）`),
      r.snapshot.excluded.length
        ? h('ul', { class: 'files' }, ...r.snapshot.excluded.map((f) => h('li', {}, h('code', {}, f.path), ` — ${f.reason}`)))
        : h('p', { class: 'muted' }, 'なし'),
      relatedSection(r.related),
      h('h3', {}, `プロンプト全文（${nf.format(r.prompt.length)} 文字）`),
      h('pre', { class: 'prompt tall' }, r.prompt),
    );
  } else if (state.tab === 'response') {
    body = h(
      'div',
      {},
      h('h3', {}, '応答の本文'),
      h('pre', { class: 'prompt' }, r.response_text || '(なし)'),
      h('h3', {}, '生の出力（提供元が返したまま）'),
      h('pre', { class: 'prompt tall' }, r.raw || '(なし)'),
    );
  } else {
    body = detailPanel(r, { when, dur, num, base: baseLabel(r.snapshot.base) });
  }

  return h('div', { class: 'result' }, head, nav, subnav, body);
}

function detailView(): HTMLElement {
  const lang = getAppLang();
  if (state.settings) {
    const settingsLang = resolveLang(state.settings.draft.language ?? state.settings.view.values.language ?? lang);
    // 項目ではない欄は、それぞれのタブに置く（LLM・API キー／連携）
    const extraContent = {
      llm: llmConfigSection(settingsLang),
      repos: reposSection(settingsLang),
      link: agentConsentSection(state.agentConsent, () => void revokeAgentConsent(), settingsLang),
    };
    return h(
      'main',
      { class: 'detail' },
      settingsPanel(
        state.settings,
        {
          save: () => void saveSettings(),
          saveAndClose: () => void saveAndCloseSettings(),
          close: () => ((state.settings = null), render()),
          onLangChange: (_newLang) => {
            // 未保存の段階ではアプリ全体やメニューの永続言語は変えない（保存時に saveSettings 内で適用される）。
          },
        },
        extraContent,
      ),
    );
  }
  if (state.report) {
    const act = {
      changed: () => void loadReport(),
      save: (f: 'md' | 'xlsx') => void saveReport(f),
      print: () => void printReport(),
      clearSelection: () => {
        if (state.report) state.report.reviewIds = null;
        void loadReport();
      },
      close: () => {
        closeReport();
        render();
      },
    };
    return h('main', { class: 'detail' }, reportPanel(state.report, act, !!state.repo, lang));
  }
  if (state.usage) {
    return h('main', { class: 'detail' }, usagePanel(state.usage, { setPeriod: setUsagePeriod, close: () => ((state.usage = null), render()) }, lang));
  }
  const r = currentRecipe();
  const tool = r ? r.kind === 'tool' : false;
  // 実行バーと「送る内容」は、1 枚のカードにまとめる（指摘を、できるだけ上から始める）
  const bar = recipeBarSection();
  if (!bar.classList.contains('recipe-bar-folded')) {
    bar.append(h('div', { class: 'bar-foot' }, recipeInfoSection(), sendBox(tool)));
  }
  return h('main', { class: 'detail' }, bar, resultView());
}

function render(): void {
  const root = getRoot();
  if (!root) return;
  const keep = ['.left', '.detail', '.right'].map((sel) => root.querySelector(sel)?.scrollTop ?? 0);
  root.replaceChildren(
    h(
      'div',
      { class: `app ${state.leftOpen ? '' : 'no-left'} ${state.rightOpen ? '' : 'no-right'}`, style: `--lw: ${state.leftW}px; --rw: ${state.rightW}px` },
      h('div', { class: 'top' }, headerView(), baseForm()),
      h(
        'div',
        { class: 'body' },
        state.leftOpen ? leftPanel() : null,
        detailView(),
        state.rightOpen ? rightPanel() : null,
        state.leftOpen ? sideResizer('left') : null,
        state.rightOpen ? sideResizer('right') : null,
      ),
    ),
  );
  ['.left', '.detail', '.right'].forEach((sel, i) => {
    const el = root.querySelector(sel);
    if (el) el.scrollTop = keep[i] ?? 0;
  });
}

async function boot(): Promise<void> {
  // メニューの「設定…」（⌘,）。開いているときは何もしない（未保存の編集を巻き込まない）
  void listen('menu-settings', () => {
    if (!state.settings) void openSettings();
  });
  // メニューの「アップデートを確認…」。押したときだけ通信する（起動時には、問い合わせない）。二重に走らせない
  let updating = false;
  void listen('menu-update', () => {
    if (updating) return;
    updating = true;
    void runUpdateCheck({
      check: () => invoke<UpdateInfo | null>('update_check'),
      confirm: (info) => confirmDialog(confirmText(info), { title: 'アップデート', okLabel: '更新する' }),
      install: () => invoke<void>('update_install'),
      notify: (text, kind) => void message(text, { title: 'アップデート', kind: kind === 'err' ? 'error' : 'info' }),
    }).finally(() => {
      updating = false;
    });
  });
  try {
    setAppName(await invoke<string>('app_name'));
  } catch (e) {
    console.error('app_name', e);
  }
  state.providers = await invoke<ProviderInfo[]>('list_providers');
  if (!state.providers.some((p) => p.id === state.providerId)) state.providerId = state.providers[0]?.id ?? '';

  // 登録は Rust（SQLite）が持つ。以前の版が画面に持っていた登録は、初回だけ取り込む（取り込み済みなら、読むだけ。元の値は消さない）
  const booted = await bootRepos({
    legacy: loadRegisteredRepos(lsGet(REPOS_KEY)),
    selected: state.repo,
    load: (legacy) => invoke<RepoList>('repos_load', { legacy }),
  });
  if (booted.list) applyRepoList(booted.list);
  void loadRepoStatus();
  if (booted.error) state.runError = booted.error;
  // 取り込みで、最初のコミットを調べられなかった登録がある（移動・クローンを同じものと見分けられないことがある。次に開いたとき、また調べる）
  if (booted.warnings.length) {
    state.repoMsg = { text: `最初のコミットを調べられなかった登録があります（次に開いたとき、もう一度調べます）: ${booted.warnings.join(' / ')}`, kind: 'err' };
  }
  // データの置き場を移せなかった・2 か所に分かれていたとき。履歴が消えたように見えるので、帯で知らせる（起動のたびに、条件が続くかぎり出る）
  try {
    const dataNotice = await invoke<string | null>('data_dir_notice');
    if (dataNotice) state.repoMsg = { text: [dataNotice, state.repoMsg.text].filter(Boolean).join(' ／ '), kind: 'err' };
  } catch (e) {
    console.error('data_dir_notice', e);
  }
  // 選択中のものを、登録に揃えたとき（サブディレクトリ・解除済み・登録なし）だけ、保存し直す。登録を読めなかったときは、触らない
  if (booted.selected !== state.repo) {
    state.repo = booted.selected;
    lsSet(REPO_KEY, state.repo);
  }

  try {
    const s = await invoke<SettingsView>('get_settings', { repo: null });
    const lang = resolveLang(s.values.language);
    setAppLang(lang);
    await invoke('set_menu_language', { lang: String(s.values.language ?? 'auto') });
  } catch (e) {
    console.error('get_settings language', e);
    const msg = `言語設定を読み込めませんでした（既定の言語で表示しています）: ${String(e)}`;
    state.repoMsg = { text: [state.repoMsg.text, msg].filter(Boolean).join(' ／ '), kind: 'err' };
  }

  await loadRecipes(state.repo || null);
  if (state.checkedRecipeIds.size === 0 && state.recipes.length > 0) {
    state.checkedRecipeIds = new Set(state.recipes.map((r) => r.id));
    lsSet(CHECKED_RECIPES_KEY, JSON.stringify([...state.checkedRecipeIds]));
  }

  render();
  loadAllRepoIcons();
  await refreshSpecFiles();
  await refreshHistory();
  await refreshPreview();
  void refreshStats();
}

// ⌘, で設定を開閉する（macOS の慣習。入力欄にいても効く。「閉じる」ボタンと同じ動き）
window.addEventListener('keydown', (e) => {
  if (e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey && e.key === ',') {
    e.preventDefault();
    toggleSettings();
  }
});

// コミットは、アプリの外（端末・エージェント）で起きる。窓に戻ってきたとき、比較元の一覧（直近のコミットなど）を取り直す。
// 失敗しても、いまの一覧は消さない（一時的な失敗で、選択肢が空になるのを避ける）。変わったときだけ、描き直す。
const basesGate = { at: 0 };
let repoStatusAt = 0;
window.addEventListener('focus', () => {
  // Finder などでフォルダを動かして戻ってきたとき、「見つかりません」の状態を読み直す（間隔は 2 秒以上空ける）
  const now = Date.now();
  if (!state.registeredRepos.length || now - repoStatusAt < 2000) return;
  repoStatusAt = now;
  void loadRepoStatus(false);
});
// エージェントが `mrcr handoff report` で保存した申告は、アプリの外で、DB に入る。窓に戻ってきたとき、開いているレビューの申告だけを、読み直す。
const agentRefreshGate = { at: 0 };
window.addEventListener('focus', () => {
  void refreshAgentReportsOnFocus({
    current: () => state.selected,
    busy: () => state.agentUi.busy || Boolean(state.running),
    now: () => Date.now(),
    gate: agentRefreshGate,
    load: async (id) => (await invoke<ReviewView | null>('get_review', { id }))?.agent_reports ?? null,
    apply: (id, next) => {
      const r = state.selected;
      if (!r || r.id !== id) return;
      r.agent_reports = next;
      // 渡す文面も、申告済みを外した内容に作り直す（取り込んだときと同じ）
      void refreshHandoff().then(() => render());
    },
  });
});
// 履歴の件数（未・済み・待ち・無視）も、アプリの外で変わる（上の申告と同じ）。窓に戻ってきたとき、一覧を読み直す。
// 実行中・取り込み中は触らない。間隔は 2 秒以上空ける（読み直しの失敗は、他の読み直しと同じく画面に出る）。
const historyFocusGate = { at: 0 };
window.addEventListener('focus', () => {
  const now = Date.now();
  if (state.running || state.agentUi.busy || now - historyFocusGate.at < 2000) return;
  historyFocusGate.at = now;
  refreshHistoryLater.call();
});
let focusError = '';
window.addEventListener('focus', () => {
  void refreshBasesOnFocus({
    repo: () => state.repo,
    running: () => state.running,
    bases: () => state.bases,
    now: () => Date.now(),
    fetch: (repo) => invoke<BaseOption[]>('list_bases', { repo, lang: getAppLang() }),
    apply: (next) => {
      state.bases = next;
      render();
    },
    // 取得に失敗したとき（登録が外れた・解除した、など）は、古い一覧を黙って残さず、理由を「送る内容」の欄に出す。成功したら、その表示を消す
    fail: (message) => {
      focusError = message;
      state.previewError = message;
      render();
    },
    ok: () => {
      if (focusError && state.previewError === focusError) {
        state.previewError = '';
        render();
      }
      focusError = '';
    },
    gate: basesGate,
  });
});

if (import.meta.env?.MODE !== 'test') {
  void boot();
}

export { fixAllBySeverity, handoffBar, state, render, refreshHandoff };
