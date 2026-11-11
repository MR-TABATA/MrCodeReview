// src-tauri の構造体と対応する型。

export interface Recipe {
  id: string;
  title: string;
  /** builtin = 組み込み / repo = このリポジトリのレシピ / user = 設定で足したもの（過去の記録に残る） */
  source: 'builtin' | 'repo' | 'user';
  prompt: string;
  /** tool = LLM を使わない実行（過去の記録に残る。今のアプリでは作らない） */
  kind: 'llm' | 'tool';
}

/** ボタンにするレシピと、気づいてほしいこと（設定の間違いなど）。 */
export interface RecipeList {
  recipes: Recipe[];
  warnings: string[];
}

export interface ProviderInfo {
  id: string;
  label: string;
  note: string;
}

/** 登録リポジトリの、いまの状態（設定の「リポジトリ」タブ）。 */
export interface RepoStatus {
  path: string;
  /** ok: git のリポジトリ ／ missing: フォルダが無い ／ not_git: git のリポジトリではない */
  state: 'ok' | 'missing' | 'not_git';
  /** サブディレクトリを登録しているときの、リポジトリの根 */
  root: string | null;
}

/** 1 つのリポジトリの、データの量（履歴・指摘・仕分け・メモ）。 */
export interface RepoData {
  reviews: number;
  findings: number;
  triaged: number;
  memos: number;
  /** 「未」の指摘の数（未仕分け ＋ 対応中） */
  open: number;
}

export interface RepoDataRow extends RepoData {
  path: string;
}

/** BYOK（自分の API キー）の状態。キーそのものは来ない（末尾 4 文字だけ）。 */
export interface ByokStatus {
  /** このマシンで、キーを保管できるか（いまは macOS のキーチェーンだけ） */
  supported: boolean;
  key_set: boolean;
  key_last4: string | null;
  key_error: string | null;
  model: string;
  max_tokens: number;
  config_error: string | null;
}

export interface FileRef {
  path: string;
  bytes: number;
}

export interface Excluded {
  path: string;
  reason: string;
}

/** 比較元（差分の元になるコミット）。差分は、これからいまの作業ツリーまで。 */
export interface Base {
  /** 選んだ値（HEAD・HEAD~1・ハッシュなど） */
  spec: string;
  hash: string;
  /** そのコミットの 1 行説明 */
  subject: string;
}

/** 画面の「比較元」の選択肢。 */
export interface BaseOption {
  value: string;
  label: string;
  hint: string;
}

export interface SnapshotInfo {
  repo: string;
  head: string;
  base: Base;
  diff_hash: string;
  diff_bytes: number;
  files: FileRef[];
  excluded: Excluded[];
  estimated_input_tokens: number;
}

export interface Finding {
  file: string;
  line: number | null;
  severity: string;
  title: string;
  detail: string;
  evidence: string;
  verified: boolean;
  /** linter の規則名（linter の指摘だけ） */
  rule: string | null;
}

/** この指摘が、過去のどの指摘から仕分けを引き継いだか。 */
export interface Carry {
  from_review: string;
  from_index: number;
  state: TriageState;
  /** false = 引用がほぼ一致（完全一致ではない） */
  exact: boolean;
}

/** 前回の「対応」「待ち」の指摘が、今回どうなったか。 */
export interface Fate {
  from_review: string;
  from_index: number;
  title: string;
  file: string;
  state_before: 'fix' | 'wait';
  fate: 'persists' | 'gone' | 'not_reraised' | 'file_missing' | 'unknown';
}

/** 送った関連コード（差分に出ていない使用箇所）の 1 か所。 */
export interface RelatedSnippet {
  name: string;
  file: string;
  start: number;
  end: number;
  /** 行番号つきのコード */
  text: string;
}

/** 調べた名前ごとの結果。 */
export interface RelatedName {
  name: string;
  /** なぜ調べたか */
  why: string;
  /** 差分に出ていない使用箇所の数 */
  hits: number;
  status: 'sent' | 'none' | 'too_many' | 'budget';
}

export interface Related {
  names: RelatedName[];
  snippets: RelatedSnippet[];
  notes: string[];
}

export interface Usage {
  input_tokens: number | null;
  output_tokens: number | null;
  cache_read_tokens: number | null;
  cache_write_tokens: number | null;
}

export interface ReviewRecord {
  id: string;
  created_at: number;
  repo: string;
  recipe: Recipe;
  provider: ProviderInfo;
  snapshot: {
    head: string;
    /** 古い記録には無い（そのときは、HEAD との比較） */
    base?: Base;
    diff_hash: string;
    diff_bytes: number;
    files: FileRef[];
    excluded: Excluded[];
  };
  prompt: string;
  model: string;
  response_text: string;
  raw: string;
  usage: Usage;
  cost_usd: number | null;
  duration_ms: number;
  findings: Finding[];
  /** findings と同じ並び。引き継ぎ元が無ければ null（古い記録では空配列） */
  carries: (Carry | null)[];
  fates: Fate[];
  /** 実行メモ（除外した件数など） */
  notes: string[];
  /** 送った関連コード。関連コードを使う観点のときだけ */
  related: Related | null;
  parse_error: string | null;
  error: string | null;
}

/** 指摘の仕分け。new = 未仕分け */
export type TriageState = 'new' | 'fix' | 'wait' | 'dismiss';

/** 指摘ごとの、対応メモ・対応済みの印・最後に仕分けた時刻。 */
export interface Note {
  memo: string;
  memo_at: number | null;
  /** 対応済みにした時刻。null = 済みにしていない */
  done_at: number | null;
  done_head: string | null;
  /** 最後に仕分けた時刻。null = 履歴なし（この機能より前の記録、または未仕分け） */
  triage_at: number | null;
  triage_head: string | null;
  /** 最後の仕分けが、あなたの操作か、前回のレビューからの引き継ぎか */
  triage_source: 'user' | 'carry' | null;
}

/** 「対応」の指摘を、CLI に渡すために書き出したファイル。 */
export interface HandoffFile {
  /** 絶対パス */
  path: string;
  /** リポジトリの根からの相対パス */
  rel: string;
  /** 「直す」にした指摘の件数 */
  count: number;
  /** CLI に貼る 1 行 */
  prompt: string;
  /** 書き出した文面そのもの（何を渡したかを、画面で見せる） */
  text: string;
}

/** 仕分けやメモを変えた結果。 */
export interface Handling {
  triage: TriageState[];
  notes: Note[];
}

/** 記録に、指摘ごとの仕分けと、対応の記録を添えたもの（記録そのものは書き換えない）。 */
/** （指摘ごとの記録は `finding_notes`。記録そのものにある `notes` は、実行メモ。） */
export type ReviewView = ReviewRecord & { triage: TriageState[]; finding_notes: Note[]; agent_reports: (AgentReport | null)[] };

/** エージェントの「対応の報告」（会話の記録から取り込んだ申告。実際に直ったかの確認ではない） */
export interface AgentReport {
  verdict: 'did' | 'skipped';
  text: string;
  /** 報告をログに書いた時刻（UNIX ミリ秒） */
  reported_at: number;
  /** 取り出したログの名前（セッションの ID） */
  source: string;
}

/** 「対応内容を取り込む」の結果 */
export interface AgentFetch {
  /** まだ、ログを読む許可をもらっていない（読まずに返した） */
  needs_consent: boolean;
  found: number;
  updated: number;
  files_scanned: number;
  files_with_tag: number;
  unreadable_lines: number;
  ignored: number;
  /** 上限のために、調べなかった古いログの数 */
  skipped_files: number;
  agent_reports: (AgentReport | null)[];
}

/** 履歴に出す、仕分けの件数。4 つを足すと、指摘の数になる（`store.rs` の `TriageCounts`） */
export interface TriageCounts {
  /** 未: 未仕分けと、「対応」にしたが、まだ済んでいないもの */
  open: number;
  /** 済み: 「対応」で、対応済みの印がある、またはエージェントが「対応した」と申告したもの */
  done: number;
  wait: number;
  dismiss: number;
}

export interface ReviewSummary {
  id: string;
  created_at: number;
  repo: string;
  recipe_title: string;
  provider_label: string;
  findings: number;
  verified: number;
  total_tokens: number | null;
  failed: boolean;
  /** 仕分けの件数。読み取りに失敗したときは null */
  counts: TriageCounts | null;
}

/** 設定項目 1 つぶんの説明（`settings.rs` の `meta()`）。 */
export interface SettingMeta {
  key: string;
  group: string;
  label: string;
  kind: 'integer' | 'number' | 'list' | 'select';
  unit: string;
  description: string;
  why_recommended: string;
  when_to_change: string;
  recommended: unknown;
  min: number | null;
  max: number | null;
  /** リポジトリ側の設定（.mrcodereview/settings.json）でも上書きできるか */
  repo_scope: boolean;
}

export interface SettingsView {
  /** いま有効な値（設定ファイルが正しくないときは推奨値） */
  values: Record<string, unknown>;
  meta: SettingMeta[];
  path: string;
  exists: boolean;
  /** 設定ファイルが正しくないときの理由 */
  error: string | null;
  /** 選んでいるリポジトリ側の設定 */
  repo: { path: string | null; keys: string[]; error: string | null };
}

/** トークン使用量の集計の 1 行（測った値だけを足したもの）。 */
export interface UsageRow {
  key: string;
  reviews: number;
  /** 失敗した実行（トークンは足していない） */
  failed: number;
  /** 成功したが、入力か出力のトークンが報告されなかった実行 */
  unreported: number;
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
}

export interface UsageSummary {
  since: number | null;
  total: UsageRow;
  by_recipe: UsageRow[];
  by_repo: UsageRow[];
  by_model: UsageRow[];
  /** 提供元が報告した金額の合計（参考） */
  cost_usd: number | null;
  cost_reviews: number;
  /** 壊れていて読めず、この集計に含めていない記録の件数 */
  unreadable: number;
}

/** レポート（成果物にできるレビューの記録）の、出力の指定。キー名は、Rust の `report::Options` と同じ。 */
export interface ReportOptions {
  repo: string | null;
  since_ms: number | null;
  until_ms: number | null;
  include_evidence: boolean;
  include_prompt: boolean;
  include_tokens: boolean;
  include_log: boolean;
  /** 表示する時刻の時差（分）。日本は 540 */
  tz_offset_min: number;
  /** レビューを 1 件ずつ選んだとき、その識別子。渡すと、その分だけを対象にする（リポジトリと期間は使わない） */
  review_ids: string[] | null;
  /** 出力言語（en / ja） */
  lang?: string;
}

export interface ReportSummary {
  reviews: number;
  failed: number;
  findings: number;
  fix: number;
  wait: number;
  dismiss: number;
  new: number;
  done: number;
  input: number;
  output: number;
  cache_read: number;
  cache_write: number;
  unreported: number;
}

export interface ReportPreview {
  html: string;
  summary: ReportSummary;
}

/** 「いまの差分」: レビューのときの比較元から、いまの作業ツリーまで */
export interface CurrentDiff {
  diff_text: string;
  diff_hash: string;
  /** いまの HEAD */
  head: string;
  files: number;
  excluded: Excluded[];
  /** レビューした時点の差分と、同じか */
  same_as_review: boolean;
}
