pub use mrcr_core::{agent_report, carry, db, findings, format, i18n, notes, proc, related, snapshot, store, triage, usage};

mod applock;
mod byok;
mod handoff;
mod keychain;
mod provider;
mod recipe;
mod repodata;
mod repos;
mod report;
mod report_xlsx;
mod settings;

use provider::ProviderInfo;
use recipe::Recipe;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use store::{ReviewRecord, ReviewSummary, SnapshotRecord};
use tauri::{Emitter, Manager};

struct AppState {
    /// 履歴の保存先。`MRCR_DATA_DIR` があればそちら（試験用）、無ければアプリのデータ領域。
    data_dir: PathBuf,
    /// 置き場の移しで、利用者に伝えたいことがあるとき（`datadir::resolve`）。画面が、起動時に読む。
    data_notice: Option<String>,
    /// 画面・メニュー・報告書・依頼の文面に出す、このアプリの名前（`display_name`）。
    app_name: String,
    /// 同時起動のロック。**持っているあいだ、ロックが続く**ので、使わなくても、プロセスの終わりまで持つ。
    _lock: Option<applock::AppLock>,
}

/// このアプリの名前（画面・メニュー・窓のタイトル・報告書・依頼の文面）。
#[tauri::command]
fn app_name(state: tauri::State<'_, AppState>) -> String {
    state.app_name.clone()
}

/// 置き場について、利用者に伝えること（無ければ `None`）。起動のたびに、同じ条件なら、同じ文が返る。
#[tauri::command]
fn data_dir_notice(state: tauri::State<'_, AppState>) -> Option<String> {
    state.data_notice.clone()
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

#[derive(Serialize)]
struct SnapshotInfo {
    repo: String,
    head: String,
    /// 比較元（差分の元になるコミット）
    base: snapshot::Base,
    diff_hash: String,
    diff_bytes: usize,
    files: Vec<store::FileRef>,
    excluded: Vec<snapshot::Excluded>,
    /// 文字数ではなくバイト数からの粗い推定（4 バイト ≒ 1 トークン）。実測ではない。
    estimated_input_tokens: usize,
}

/// 記録に、仕分けの状態を添えて画面に渡す（記録そのものは書き換えない）。
#[derive(Serialize)]
struct ReviewView {
    #[serde(flatten)]
    record: ReviewRecord,
    /// 指摘ごとの状態: "new" | "fix" | "wait" | "dismiss"
    triage: Vec<String>,
    /// 指摘ごとの、対応メモ・対応済みの印・最後に仕分けた時刻。
    /// （名前は `finding_notes`。記録に、実行メモの `notes` があり、同じ名前だと、平らに重ねたときにぶつかる）
    finding_notes: Vec<notes::Note>,
    /// 指摘ごとの、エージェントの「対応の報告」（会話の記録から取り込んだ申告）。取り込んでいなければ null
    agent_reports: Vec<Option<agent_report::AgentReport>>,
}

fn view(data_dir: &Path, record: ReviewRecord) -> ReviewView {
    let triage = triage::get(data_dir, &record.id, record.findings.len());
    let finding_notes = notes::get_all(data_dir, &record.id, record.findings.len());
    let agent_reports = agent_report::get_all(data_dir, &record.id, record.findings.len());
    ReviewView { record, triage, finding_notes, agent_reports }
}

/// 「対応内容を取り込む」の結果。
#[derive(Serialize, Debug)]
struct AgentFetch {
    /// まだ、ログを読む許可を、もらっていない（読まずに返した）
    needs_consent: bool,
    /// ログから見つかった報告の数（このレビューの指摘に当たるもの）
    found: usize,
    /// 新しく保存した・内容が変わった件数
    updated: usize,
    files_scanned: usize,
    files_with_tag: usize,
    unreadable_lines: usize,
    ignored: usize,
    /// 上限のために調べなかった、古いログの数
    skipped_files: usize,
    agent_reports: Vec<Option<agent_report::AgentReport>>,
}

/// 会話の記録（Claude Code のログ）から、このレビューの「対応の報告」を取り込む。
/// 初めては、`consent` が true のときだけ読む（許可は、1 回、覚える）。読むのは、識別子を含む行だけ。
#[tauri::command]
async fn fetch_agent_reports(state: tauri::State<'_, AppState>, id: String, consent: bool) -> Result<AgentFetch, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || fetch_agent_reports_in(&data_dir, &agent_report::Sources::from_env(), &id, consent)).await.map_err(|e| e.to_string())?
}

/// 「いまの差分」: レビューしたときの比較元（固定したコミット）から、**いまの作業ツリー**までの差分。
/// エージェントが直したあとを、見るためのもの（読み取りだけ。LLM は使わない）。
#[derive(Serialize, Debug)]
struct CurrentDiff {
    /// いまの差分の本文（送ったときと、同じ作り）
    diff_text: String,
    diff_hash: String,
    /// いまの HEAD
    head: String,
    /// いまの差分に含まれるファイル数と、含めなかったもの
    files: usize,
    excluded: Vec<snapshot::Excluded>,
    /// レビューした時点の差分と、同じか（識別子が同じ）
    same_as_review: bool,
}

#[tauri::command]
async fn current_diff(state: tauri::State<'_, AppState>, id: String) -> Result<CurrentDiff, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || current_diff_in(&data_dir, &id)).await.map_err(|e| e.to_string())?
}

fn current_diff_in(data_dir: &Path, id: &str) -> Result<CurrentDiff, String> {
    let rec = find_review(data_dir, id).ok_or_else(|| format!("レビューが見つからない: {id}"))?;
    // 比較元は、レビューのときに解決した**コミットのハッシュ**で固定する（`HEAD` のままだと、あいだにコミットすると、基準が動く）。
    // ハッシュの無い古い記録は、そのときの HEAD で代える（そのころは、いつも HEAD との比較だった）。
    let base = if rec.snapshot.base.hash.is_empty() { rec.snapshot.head.clone() } else { rec.snapshot.base.hash.clone() };
    let top = snapshot::toplevel(&rec.repo).map_err(|e| format!("リポジトリを開けません（{}）: {e}", rec.repo))?;
    let eff = settings::load_effective(data_dir, Some(Path::new(&top)))?;
    let snap = snapshot::take_with_base(&rec.repo, &eff.settings.snapshot_limits(), Some(&base))?;
    Ok(CurrentDiff {
        same_as_review: snap.diff_hash == rec.snapshot.diff_hash,
        diff_hash: snap.diff_hash,
        head: snap.head,
        files: snap.files.len(),
        excluded: snap.excluded,
        diff_text: snap.diff_text,
    })
}

/// 会話の記録を読む許可が、出ているか。
#[tauri::command]
fn agent_log_consent(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    agent_report::consented(&state.data_dir)
}

/// 会話の記録を読む許可を、取り消す（取り込み済みの報告は、残る）。
#[tauri::command]
fn revoke_agent_log_consent(state: tauri::State<'_, AppState>) -> Result<(), String> {
    agent_report::revoke_consent(&state.data_dir)
}

fn fetch_agent_reports_in(data_dir: &Path, sources: &agent_report::Sources, id: &str, consent: bool) -> Result<AgentFetch, String> {
    let rec = store::get(data_dir, id).ok_or_else(|| format!("レビュー {id} が見つかりません"))?;
    let n = rec.findings.len();
    if !agent_report::consented(data_dir)? {
        if !consent {
            // 許可がなければ、ログには、触らない
            return Ok(AgentFetch { needs_consent: true, found: 0, updated: 0, files_scanned: 0, files_with_tag: 0, unreadable_lines: 0, ignored: 0, skipped_files: 0, agent_reports: agent_report::try_get_all(data_dir, &rec.id, n)? });
        }
        agent_report::set_consent(data_dir)?;
    }
    let scan = agent_report::scan(sources, &rec.id, n, rec.created_at)?;
    let updated = agent_report::save(data_dir, &rec.id, &scan.reports)?;
    Ok(AgentFetch {
        needs_consent: false,
        found: scan.reports.len(),
        updated,
        files_scanned: scan.files_scanned,
        files_with_tag: scan.files_with_tag,
        unreadable_lines: scan.unreadable_lines,
        ignored: scan.ignored,
        skipped_files: scan.skipped_files,
        agent_reports: agent_report::try_get_all(data_dir, &rec.id, n)?,
    })
}

/// 仕分けとメモを変えた結果。画面は、これでカードを更新する。
#[derive(Serialize)]
struct Handling {
    triage: Vec<String>,
    notes: Vec<notes::Note>,
}

fn handling(data_dir: &Path, rec: &ReviewRecord) -> Handling {
    Handling { triage: triage::get(data_dir, &rec.id, rec.findings.len()), notes: notes::get_all(data_dir, &rec.id, rec.findings.len()) }
}

/// ボタンにするレシピと、気づいてほしいこと（設定の間違い、読まなかったファイルなど）。
#[derive(Serialize)]
struct RecipeList {
    recipes: Vec<Recipe>,
    warnings: Vec<String>,
}

#[tauri::command]
fn list_recipes(repo: Option<String>, lang: Option<String>) -> RecipeList {
    let lang_str = lang.as_deref().unwrap_or_else(|| i18n::Lang::default().code());
    recipe_list_with_lang(repo.as_deref(), lang_str)
}

fn recipe_list_with_lang(repo: Option<&str>, lang: &str) -> RecipeList {
    let mut warnings = Vec::new();
    let resolved_lang = match resolve_lang_arg(Some(lang)) {
        Ok(l) => l,
        Err(e) => {
            warnings.push(e);
            "ja"
        }
    };
    let root = repo.filter(|r| !r.is_empty()).and_then(|r| snapshot::toplevel(r).ok());
    let recipes = recipe::list_with_lang(root.as_deref().map(Path::new), resolved_lang);
    RecipeList { recipes, warnings }
}

/// トークン使用量の集計（記録した、測った値だけ）。`since_ms` 以降だけを数える。None なら、すべて。
#[tauri::command]
async fn usage_summary(state: tauri::State<'_, AppState>, since_ms: Option<u64>) -> Result<usage::Summary, String> {
    let data_dir = state.data_dir.clone();
    // 期間で絞り、必要な列だけを読む。重い読み取りは、ほかのコマンドを待たせないよう、別のスレッドで行う
    tauri::async_runtime::spawn_blocking(move || {
        // 読み取りに失敗したら、ゼロに見せず、エラーを返す。読めなかった記録は、件数を添える
        let (rows, unreadable) = store::usage_rows(&data_dir, since_ms).map_err(|e| format!("トークン使用量の元になる記録を読み取れませんでした（{e}）"))?;
        let mut s = usage::summarize_slim(&rows, since_ms);
        s.unreadable = unreadable;
        Ok(s)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 過去の実績からの、所要時間の目安。同じ LLM・同じ観点で、新しい順に 10 件まで。モックは、本物の時間ではないので数えない。
#[tauri::command]
fn duration_stats(state: tauri::State<'_, AppState>, provider_id: String, recipe_id: String) -> usage::Durations {
    if provider_id == "mock" {
        return usage::durations(&[]);
    }
    usage::durations(&store::recent_durations(&state.data_dir, &provider_id, &recipe_id, 10))
}

/// レポート（成果物にできるレビューの記録）の、プレビュー。画面に出す HTML と、件数のまとめ。
#[derive(Serialize)]
struct ReportPreview {
    html: String,
    summary: report::Summary,
}

/// 画面から渡された指定を整える。リポジトリは、記録に残っている根のパスに直す。
fn report_options(mut o: report::Options) -> report::Options {
    o.repo = history_repo(o.repo.as_deref());
    o
}

#[tauri::command]
async fn report_preview(state: tauri::State<'_, AppState>, options: report::Options) -> Result<ReportPreview, String> {
    let data_dir = state.data_dir.clone();
    let app_name = state.app_name.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 読み取りに失敗したら、エラーを返す（画面に理由が出て、出力できなくなる）
        let r = report::build(&data_dir, &report_options(options), &app_name, env!("CARGO_PKG_VERSION"))?;
        Ok(ReportPreview { html: report::html(&r), summary: r.summary.clone() })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 書き出し先として使えるパスか。絶対パスで、拡張子が形式と合い、置く場所（フォルダ）があること。
fn check_output_path(path: &str, ext: &str) -> Result<std::path::PathBuf, String> {
    let p = std::path::PathBuf::from(path);
    if !p.is_absolute() {
        return Err(format!("保存先は、絶対パスで指定してください: {path}"));
    }
    if p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() != Some(ext) {
        return Err(format!("保存先の拡張子は .{ext} にしてください: {path}"));
    }
    if p.is_dir() {
        return Err(format!("保存先がフォルダです: {path}"));
    }
    match p.parent() {
        Some(dir) if dir.is_dir() => Ok(p),
        _ => Err(format!("保存先のフォルダが見つかりません: {path}")),
    }
}

/// レポートを、ファイルに書き出す。`format` は "md" か "xlsx"（PDF は、画面の印刷から）。書いたパスを返す。
#[tauri::command]
async fn report_save(state: tauri::State<'_, AppState>, options: report::Options, format: String, path: String) -> Result<String, String> {
    let ext = match format.as_str() {
        "md" => "md",
        "xlsx" => "xlsx",
        other => return Err(format!("未知の形式: {other}（md か xlsx）")),
    };
    let out = check_output_path(&path, ext)?;
    let data_dir = state.data_dir.clone();
    let app_name = state.app_name.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let r = report::build(&data_dir, &report_options(options), &app_name, env!("CARGO_PKG_VERSION"))?;
        match ext {
            "md" => std::fs::write(&out, report::markdown(&r)).map_err(|e| format!("書き込めません: {}: {e}", out.display()))?,
            _ => report_xlsx::write(&r, &out)?,
        }
        Ok(out.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// この窓の印刷を開く（macOS の印刷画面から、「PDF として保存」できる）。レポートの PDF 出力に使う。
#[tauri::command]
fn print_page(window: tauri::WebviewWindow) -> Result<(), String> {
    window.print().map_err(|e| format!("印刷を開けません: {e}"))
}

#[tauri::command]
fn list_providers(state: tauri::State<'_, AppState>) -> Vec<ProviderInfo> {
    // 設定ファイルが壊れているときも、一覧は出す（理由は、設定画面の「LLM プロバイダ」に出る）
    let cfg = byok::load(&state.data_dir).unwrap_or_default();
    provider::all(state.data_dir.clone(), cfg).iter().map(|p| p.info()).collect()
}

/// 登録リポジトリの状態（設定の「リポジトリ」タブ）。読み取りだけ（`git rev-parse`）。
#[derive(Serialize, Debug, PartialEq)]
struct RepoStatus {
    path: String,
    /// "ok"（git のリポジトリ）| "missing"（フォルダが無い）| "not_git"（git のリポジトリではない）
    state: &'static str,
    /// 登録したのがサブディレクトリのとき、リポジトリの根（履歴は、根のパスで結びつく）
    root: Option<String>,
}

fn repo_status_of(path: &str) -> RepoStatus {
    let p = Path::new(path);
    if !p.is_dir() {
        return RepoStatus { path: path.into(), state: "missing", root: None };
    }
    match snapshot::toplevel(path) {
        Ok(top) => {
            let same = match (std::fs::canonicalize(&top), std::fs::canonicalize(p)) {
                (Ok(a), Ok(b)) => a == b,
                _ => top == path.trim_end_matches('/'),
            };
            RepoStatus { path: path.into(), state: "ok", root: if same { None } else { Some(top) } }
        }
        Err(_) => RepoStatus { path: path.into(), state: "not_git", root: None },
    }
}

/// 登録したリポジトリが、いまも使えるか（移動・削除・git でなくなった、に気づくため）。
#[tauri::command]
async fn repo_status(paths: Vec<String>) -> Result<Vec<RepoStatus>, String> {
    tauri::async_runtime::spawn_blocking(move || paths.iter().map(|p| repo_status_of(p)).collect()).await.map_err(|e| e.to_string())
}

/// 各リポジトリの、データの量（履歴・指摘・仕分け・メモ）。読み取りだけ。解除の可否の判断と、削除の確認に使う。
#[tauri::command]
async fn repo_data(state: tauri::State<'_, AppState>, paths: Vec<String>) -> Result<Vec<repodata::RepoDataRow>, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || repodata::summaries(&data_dir, &paths)).await.map_err(|e| e.to_string())?
}

/// 登録の一覧と、枠の上限。初回だけ、以前の版（画面の `localStorage`）の登録を取り込む（上限は見ない。取り込み済みなら、読むだけ）。
#[tauri::command]
async fn repos_load(state: tauri::State<'_, AppState>, legacy: Vec<repos::LegacyRepo>) -> Result<repos::RepoList, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || repos::import_legacy(&data_dir, &legacy)).await.map_err(|e| e.to_string())?
}

/// リポジトリを登録する。上限に達していて、新しい行が要るときは、何も変えず `limit_reached` を返す。
#[tauri::command]
async fn repo_add(state: tauri::State<'_, AppState>, path: String, name: Option<String>, confirm_move: Option<bool>) -> Result<repos::AddOutcome, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || repos::add_with(&data_dir, &path, name.as_deref(), confirm_move.unwrap_or(false)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn repo_rename(state: tauri::State<'_, AppState>, id: String, name: Option<String>) -> Result<repos::RepoList, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || repos::rename(&data_dir, &id, name.as_deref())).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn repo_restore(state: tauri::State<'_, AppState>, id: String) -> Result<repos::RepoList, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || repos::restore(&data_dir, &id)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn repo_move(state: tauri::State<'_, AppState>, id: String, delta: i32) -> Result<repos::RepoList, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || repos::move_row(&data_dir, &id, delta)).await.map_err(|e| e.to_string())?
}

/// 解除する。履歴が 1 件もなければ登録ごと消え（枠が戻る）、あれば非表示にするだけ（枠は戻らない）。
#[tauri::command]
async fn repo_unlink(state: tauri::State<'_, AppState>, id: String) -> Result<repos::UnlinkOutcome, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || repos::unlink(&data_dir, &id)).await.map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct DeleteOutcome {
    removed: repodata::RepoData,
    #[serde(flatten)]
    list: repos::RepoList,
    /// フォルダが無くなっていて、どの登録のものか決められない履歴の件数（消していない）
    unattributed_reviews: usize,
}

/// 「データを削除して、枠を空ける」: データ（履歴・仕分け・メモ・エージェントの報告）と登録を、すべて削除する。**元に戻せない。**
/// 1 つのトランザクションで消す。リポジトリの中身や、リポジトリ内に書き出したファイルには、触れない。
#[tauri::command]
async fn repo_delete_data(state: tauri::State<'_, AppState>, id: String) -> Result<DeleteOutcome, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || repos::delete_data(&data_dir, &id).map(|(removed, list, unattributed_reviews)| DeleteOutcome { removed, list, unattributed_reviews }))
        .await
        .map_err(|e| e.to_string())?
}

/// BYOK の状態（キーの有無と末尾 4 文字・モデル名・最大出力トークン）。キーそのものは返さない。
#[tauri::command]
fn byok_status(state: tauri::State<'_, AppState>) -> byok::Status {
    byok::status(&state.data_dir)
}

/// API キーを、macOS キーチェーンに保存する（`settings.json` などには書かない）。
#[tauri::command]
fn byok_save_key(key: String) -> Result<(), String> {
    let key = keychain::validate_key(&key)?;
    keychain::set(keychain::SERVICE, keychain::ACCOUNT_ANTHROPIC, &key)
}

/// 保存したキーを、キーチェーンから消す。
#[tauri::command]
fn byok_delete_key() -> Result<(), String> {
    keychain::delete(keychain::SERVICE, keychain::ACCOUNT_ANTHROPIC)
}

/// モデル名と最大出力トークンを保存する（キーではない設定。`byok.json`）。
#[tauri::command]
fn byok_save_config(state: tauri::State<'_, AppState>, model: String, max_tokens: u64) -> Result<(), String> {
    byok::save(&state.data_dir, &byok::Config { model: model.trim().to_string(), max_tokens })
}

/// 画面の「比較元」に出す選択肢（未コミットの変更・直近のコミット・ブランチの分岐点・最近のコミット）。
#[tauri::command]
async fn list_bases(state: tauri::State<'_, AppState>, repo: String) -> Result<Vec<snapshot::BaseOption>, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || list_bases_checked(&data_dir, &repo)).await.map_err(|e| e.to_string())?
}

/// 実行の入口の確認（登録済みか）つき。コマンドは、この関数だけを呼ぶ（配線の抜けを、テストで見つけるため）。
fn list_bases_checked(data_dir: &Path, repo: &str) -> Result<Vec<snapshot::BaseOption>, String> {
    repos::require_registered(data_dir, repo)?;
    snapshot::list_bases(repo)
}

/// 押す前の確認。何を送り、何を送らないか。
#[tauri::command]
async fn preview_snapshot(state: tauri::State<'_, AppState>, repo: String, base: Option<String>) -> Result<SnapshotInfo, String> {
    let data_dir = state.data_dir.clone();
    let snap = tauri::async_runtime::spawn_blocking(move || preview_checked(&data_dir, &repo, base.as_deref()))
        .await
        .map_err(|e| e.to_string())??;
    Ok(SnapshotInfo {
        estimated_input_tokens: snap.diff_bytes / 4,
        repo: snap.repo,
        head: snap.head,
        base: snap.base,
        diff_hash: snap.diff_hash,
        diff_bytes: snap.diff_bytes,
        files: snap.files.iter().map(|f| store::FileRef { path: f.path.clone(), bytes: f.bytes }).collect(),
        excluded: snap.excluded,
    })
}

/// 実行の入口の確認（登録済みか）つき。
fn preview_checked(data_dir: &Path, repo: &str, base: Option<&str>) -> Result<snapshot::Snapshot, String> {
    repos::require_registered(data_dir, repo)?;
    // 個人の設定に、このリポジトリ側の設定を重ねたもので、取り込む
    let top = snapshot::toplevel(repo)?;
    let eff = settings::load_effective(data_dir, Some(Path::new(&top)))?;
    snapshot::take_with_base(repo, &eff.settings.snapshot_limits(), base)
}

/// 実行の入口の確認（登録済みか）つき。
#[allow(dead_code)]
fn run_review_checked(
    data_dir: &Path,
    repo: &str,
    recipe_id: &str,
    provider_id: &str,
    base: Option<&str>,
) -> Result<ReviewRecord, String> {
    run_review_checked_with_lang(data_dir, repo, recipe_id, provider_id, base, None)
}

fn run_review_checked_with_lang(
    data_dir: &Path,
    repo: &str,
    recipe_id: &str,
    provider_id: &str,
    base: Option<&str>,
    lang: Option<&str>,
) -> Result<ReviewRecord, String> {
    repos::require_registered(data_dir, repo)?;
    run_review_with_base_and_lang(data_dir, repo, recipe_id, provider_id, base, lang)
}

#[tauri::command]
async fn run_review(
    state: tauri::State<'_, AppState>,
    repo: String,
    recipe_id: String,
    provider_id: String,
    base: Option<String>,
    lang: Option<String>,
) -> Result<ReviewView, String> {
    let data_dir = state.data_dir.clone();
    let rec = tauri::async_runtime::spawn_blocking({
        let data_dir = data_dir.clone();
        move || run_review_checked_with_lang(&data_dir, &repo, &recipe_id, &provider_id, base.as_deref(), lang.as_deref())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(view(&data_dir, rec))
}

/// 同じリポジトリ・同じ観点・成功した過去のレビューから、仕分けを引き継ぐ。
/// 戻り値: 指摘ごとの引き継ぎ元、前回の指摘の行方、新しいレビューの仕分けの初期値。
fn carry_over(
    data_dir: &Path,
    repo: &str,
    recipe_id: &str,
    new_findings: &[findings::Finding],
    settings: &settings::Settings,
) -> (Vec<Option<carry::Carry>>, Vec<carry::Fate>, Vec<String>) {
    // 新しい順
    let prevs: Vec<ReviewRecord> = store::recent_ok(data_dir, repo, recipe_id, settings.carry_window);
    let states: Vec<Vec<String>> =
        prevs.iter().map(|r| triage::get(data_dir, &r.id, r.findings.len())).collect();

    let mut known = Vec::new();
    for (r, t) in prevs.iter().zip(&states) {
        for (i, f) in r.findings.iter().enumerate() {
            known.push(carry::Known {
                review_id: r.id.clone(),
                index: i,
                file: f.file.clone(),
                evidence: f.evidence.clone(),
                state: t[i].clone(),
                rule: f.rule.clone(),
            });
        }
    }
    let carries = carry::match_known_with(new_findings, &known, settings.fuzzy_min, settings.fuzzy_min_tokens);

    let fates = match (prevs.first(), states.first()) {
        (Some(p), Some(t)) => {
            let carried: std::collections::HashSet<usize> = carries
                .iter()
                .flatten()
                .filter(|c| c.from_review == p.id)
                .map(|c| c.from_index)
                .collect();
            carry::fates(&p.id, &p.findings, t, &carried, Path::new(repo))
        }
        _ => Vec::new(),
    };
    let inherited = carries
        .iter()
        .map(|c| c.as_ref().map(|c| c.state.clone()).unwrap_or_else(|| triage::NEW.to_string()))
        .collect();
    (carries, fates, inherited)
}

#[cfg_attr(not(test), allow(dead_code))]
fn run_review_blocking(
    data_dir: &Path,
    repo: &str,
    recipe_id: &str,
    provider_id: &str,
) -> Result<ReviewRecord, String> {
    run_review_with_base(data_dir, repo, recipe_id, provider_id, None)
}

/// `base` は、比較元（差分の元になるコミット）。None なら `HEAD`（未コミットの変更だけ）。
fn run_review_with_base(
    data_dir: &Path,
    repo: &str,
    recipe_id: &str,
    provider_id: &str,
    base: Option<&str>,
) -> Result<ReviewRecord, String> {
    run_review_with_base_and_lang(data_dir, repo, recipe_id, provider_id, base, None)
}

fn run_review_with_base_and_lang(
    data_dir: &Path,
    repo: &str,
    recipe_id: &str,
    provider_id: &str,
    base: Option<&str>,
    lang: Option<&str>,
) -> Result<ReviewRecord, String> {
    // 設定が正しくなければ、黙って推奨値に戻さず、理由つきで止める。
    // 個人の設定に、このリポジトリ側の設定（.mrcodereview/settings.json）を重ねたものを使う
    let top = snapshot::toplevel(repo)?;
    let effective = settings::load_effective(data_dir, Some(Path::new(&top)))?;
    let settings = effective.settings;
    let caller_lang = resolve_lang_arg(lang)?;
    let resolved_lang = match settings.language.as_str() {
        "auto" => if lang.is_some() { caller_lang } else { system_lang().code() },
        code => i18n::Lang::from_code(code)
            .ok_or_else(|| format!("不正な言語設定: {code}（{} のいずれか）", i18n::Lang::valid_arg_list()))?
            .code(),
    };
    let snap = snapshot::take_with_base(repo, &settings.snapshot_limits(), base)?;
    let recipe = recipe::find_with_lang(Some(Path::new(&snap.repo)), recipe_id, resolved_lang)
        .ok_or_else(|| format!("レシピが見つからない: {recipe_id}"))?;
    if snap.files.is_empty() {
        return Err("送る差分がありません（HEAD からの変更が無い、または全部が対象外）".into());
    }

    let mut related: Option<related::Related> = None;
    let started = Instant::now();
    let byok_cfg = byok::load(&data_dir)?;
    let provider = provider::get_with(provider_id, data_dir.to_path_buf(), settings.llm_timeout(), byok_cfg)
        .ok_or_else(|| format!("プロバイダが見つからない: {provider_id}"))?;
    let provider_info: ProviderInfo = provider.info();
    // 差分に出ていない関連コードも見る観点（`<!-- related -->`）は、使用箇所を集めて、差分の前に添える
    if recipe.needs_related {
        let r = related::collect(Path::new(&snap.repo), &snap, &settings.related_limits());
        related = Some(r);
    }
    let block = related.as_ref().map(related::render).unwrap_or_default();
    let prompt = recipe::compose_with_lang(&recipe, &block, &snap.diff_text, resolved_lang);
    let result = provider.run(&prompt);
    let notes = related.as_ref().map(|r| r.notes.clone()).unwrap_or_default();
    let duration_ms = started.elapsed().as_millis() as u64;

    // リポジトリ側の設定で上書きした項目は、画面にも見せる（結果が、いつもと違う設定で出たと分かるように）
    let mut notes = notes;
    if let Some(file) = &effective.repo_file {
        if !effective.repo_keys.is_empty() {
            notes.insert(0, format!("このリポジトリの設定（{}）で、次の項目を上書きしています: {}", file.display(), effective.repo_keys.join("、")));
        }
    }

    let mut rec = ReviewRecord {
        id: format!("{}-{}", now_ms(), &snap.diff_hash[..8]),
        created_at: now_ms(),
        repo: snap.repo.clone(),
        recipe,
        provider: provider_info,
        snapshot: SnapshotRecord::from(&snap),
        prompt,
        model: String::new(),
        response_text: String::new(),
        raw: String::new(),
        usage: Default::default(),
        cost_usd: None,
        duration_ms,
        findings: Vec::new(),
        carries: Vec::new(),
        fates: Vec::new(),
        notes,
        related,
        repo_overrides: effective.repo_keys,
        settings: Some(settings.to_record()),
        parse_error: None,
        error: None,
    };
    let mut inherited: Vec<String> = Vec::new();
    match result {
        Ok(resp) => {
            rec.model = resp.model;
            rec.usage = resp.usage;
            rec.cost_usd = resp.cost_usd;
            rec.raw = resp.raw;
            match findings::parse(&resp.text) {
                Ok(mut f) => {
                    findings::verify(&mut f, &snap.files);
                    (rec.carries, rec.fates, inherited) = carry_over(data_dir, &rec.repo, &rec.recipe.id, &f, &settings);
                    rec.findings = f;
                }
                Err(e) => rec.parse_error = Some(e),
            }
            rec.response_text = resp.text;
        }
        Err(e) => rec.error = Some(e),
    }
    // 失敗した実行も、送った内容ごと残す。
    store::append(data_dir, &rec)?;
    // 引き継いだ仕分けを、このレビューの初期値にする
    triage::put(data_dir, &rec.id, inherited)?;
    Ok(rec)
}

#[tauri::command]
fn list_reviews(state: tauri::State<'_, AppState>, repo: Option<String>) -> Vec<ReviewSummary> {
    // 設定が壊れていても、履歴は見られるようにする（実行するときに、理由つきで止まる）
    let limit = settings::load(&state.data_dir).map(|s| s.history_limit).unwrap_or(settings::Settings::default().history_limit);
    store::summaries_in(&state.data_dir, history_repo(repo.as_deref()).as_deref(), limit)
}

/// 履歴を絞るリポジトリ。記録には、リポジトリの根（`git rev-parse --show-toplevel`）を残しているので、選んだパスを根に直す。
/// git のリポジトリでなければ、選んだパスのまま（何も一致せず、空になる）。リポジトリを選んでいなければ None（すべて）。
fn history_repo(repo: Option<&str>) -> Option<String> {
    let r = repo.map(str::trim).filter(|r| !r.is_empty())?;
    Some(snapshot::toplevel(r).unwrap_or_else(|_| r.to_string()))
}

/// 画面の設定に渡すもの。設定ファイルが正しくなくても、画面は開けるよう、エラーと推奨値を返す。
#[derive(Serialize)]
struct SettingsView {
    /// いま有効な値（ファイルが正しくないときは推奨値）
    values: Value,
    meta: Vec<settings::Meta>,
    /// 設定ファイルの場所
    path: String,
    exists: bool,
    /// ファイルが正しくないときの理由
    error: Option<String>,
    /// 選んでいるリポジトリ側の設定（あれば）
    repo: RepoSettings,
}

/// 選んでいるリポジトリの、`.mrcodereview/settings.json` の状態。
#[derive(Serialize, Default)]
struct RepoSettings {
    /// ファイルの場所（ファイルがあるときだけ）
    path: Option<String>,
    /// 上書きしている項目
    keys: Vec<String>,
    /// ファイルが正しくないときの理由
    error: Option<String>,
}

fn repo_settings_view(dir: &Path, repo: Option<&str>) -> RepoSettings {
    let Some(repo) = repo.filter(|r| !r.is_empty()) else { return RepoSettings::default() };
    let Ok(top) = snapshot::toplevel(repo) else { return RepoSettings::default() };
    let file = settings::repo_path(Path::new(&top));
    if !file.exists() || settings::load(dir).is_err() {
        // 個人の設定が正しくないときは、そちらの理由を先に直してもらう
        return RepoSettings::default();
    }
    match settings::load_effective(dir, Some(Path::new(&top))) {
        Ok(e) => RepoSettings { path: Some(file.to_string_lossy().into_owned()), keys: e.repo_keys, error: None },
        Err(e) => RepoSettings { path: Some(file.to_string_lossy().into_owned()), keys: Vec::new(), error: Some(e) },
    }
}

fn settings_view(dir: &Path, repo: Option<&str>) -> SettingsView {
    let (values, error) = match settings::load(dir) {
        Ok(s) => (s.to_record(), None),
        Err(e) => (settings::Settings::default().to_record(), Some(e)),
    };
    SettingsView {
        values,
        meta: settings::meta(),
        path: settings::path(dir).to_string_lossy().into_owned(),
        exists: settings::path(dir).exists(),
        error,
        repo: repo_settings_view(dir, repo),
    }
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>, repo: Option<String>) -> SettingsView {
    settings_view(&state.data_dir, repo.as_deref())
}

/// 画面の設定を検証して保存する。検証に通らなければ、何も書かず、理由を返す。
#[tauri::command]
fn save_settings(state: tauri::State<'_, AppState>, values: Value, repo: Option<String>) -> Result<SettingsView, String> {
    settings::save(&state.data_dir, &values)?;
    Ok(settings_view(&state.data_dir, repo.as_deref()))
}

fn find_review(data_dir: &Path, id: &str) -> Option<ReviewRecord> {
    store::get(data_dir, id)
}

#[tauri::command]
fn get_review(state: tauri::State<'_, AppState>, id: String) -> Option<ReviewView> {
    find_review(&state.data_dir, &id).map(|r| view(&state.data_dir, r))
}

/// 指摘 1 件の仕分けを変える。履歴に、時刻と、そのときのリポジトリの HEAD を残す。
#[tauri::command]
fn set_triage(state: tauri::State<'_, AppState>, id: String, index: usize, status: String) -> Result<Handling, String> {
    let rec = find_review(&state.data_dir, &id).ok_or_else(|| format!("レビューが見つからない: {id}"))?;
    triage::set_with(&state.data_dir, &id, index, &status, rec.findings.len(), db::now_ms(), snapshot::head_of(&rec.repo).as_deref())?;
    Ok(handling(&state.data_dir, &rec))
}

/// 履歴のレビューを、選んだ分だけ消す（同じ試験を何度も走らせたときなど）。**元に戻せない。** 仕分け・申告・メモ・「渡した」記録も、一緒に消える。
/// 確認は、画面で取る。消した件数を返す。
#[tauri::command]
async fn delete_reviews(state: tauri::State<'_, AppState>, ids: Vec<String>) -> Result<usize, String> {
    let data_dir = state.data_dir.clone();
    tauri::async_runtime::spawn_blocking(move || store::delete_reviews(&data_dir, &ids)).await.map_err(|e| e.to_string())?
}

/// 指摘 1 件の、対応メモを書く（空にすると、メモだけを消す）。
#[tauri::command]
fn set_memo(state: tauri::State<'_, AppState>, id: String, index: usize, memo: String) -> Result<Handling, String> {
    let rec = find_review(&state.data_dir, &id).ok_or_else(|| format!("レビューが見つからない: {id}"))?;
    notes::set_memo(&state.data_dir, &id, index, rec.findings.len(), &memo, db::now_ms())?;
    Ok(handling(&state.data_dir, &rec))
}

/// 指摘 1 件を、対応済みにする／戻す。「対応」にした指摘だけが、済みにできる。
#[tauri::command]
fn set_done(state: tauri::State<'_, AppState>, id: String, index: usize, done: bool) -> Result<Handling, String> {
    let rec = find_review(&state.data_dir, &id).ok_or_else(|| format!("レビューが見つからない: {id}"))?;
    notes::set_done(&state.data_dir, &id, index, rec.findings.len(), done, db::now_ms(), snapshot::head_of(&rec.repo).as_deref())?;
    Ok(handling(&state.data_dir, &rec))
}

/// 「対応」の指摘を、リポジトリの中の `.mrcodereview/out/handoff-<観点の id>.md` に書き出す（CLI に読ませるため）。
/// CLI に渡す対象の仕分け。エージェントが「対応した」と申告した「対応」の指摘と、あなたが「対応済み」にした指摘は、渡し済み・直し済みなので、
/// "done" にして外す（また渡さない。「今回は直さない」の欄にも、入れない）。
/// 規則は、コア（`handoff_api::handoff_states`）にある（CLI の `mrcr handoff` と共有）。ここは、読み取りだけ（画面を開けるよう、失敗しても、寛容に読む）。
fn handoff_triage(data_dir: &Path, id: &str, n: usize) -> Vec<String> {
    let t = triage::get(data_dir, id, n);
    let reports = agent_report::get_all(data_dir, id, n);
    let notes = notes::get_all(data_dir, id, n);
    mrcr_core::handoff_api::handoff_states(&t, &reports, &notes)
}

/// `index` を渡すと、その 1 件だけ。CLI に貼る 1 行と、書いた文面も返す。「対応」が無ければ、エラー。
#[tauri::command]
fn write_handoff(state: tauri::State<'_, AppState>, id: String, index: Option<usize>) -> Result<handoff::HandoffFile, String> {
    let rec = find_review(&state.data_dir, &id).ok_or_else(|| format!("レビューが見つからない: {id}"))?;
    let t = handoff_triage(&state.data_dir, &id, rec.findings.len());
    handoff::write(&rec, &t, index, &state.app_name)
}

/// CUI に貼る文面。「対応」が無ければ空文字。
#[tauri::command]
fn build_handoff(state: tauri::State<'_, AppState>, id: String) -> Result<String, String> {
    let rec = find_review(&state.data_dir, &id).ok_or_else(|| format!("レビューが見つからない: {id}"))?;
    let t = handoff_triage(&state.data_dir, &id, rec.findings.len());
    Ok(triage::handoff_text(&rec, &t, &state.app_name))
}

fn to_base64(bytes: &[u8]) -> String {
    const CHARSET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((bytes.len() + 2) / 3 * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        out.push(CHARSET[(b0 >> 2) & 0x3F] as char);
        out.push(CHARSET[((b0 << 4) | (b1 >> 4)) & 0x3F] as char);
        if chunk.len() > 1 {
            out.push(CHARSET[((b1 << 2) | (b2 >> 6)) & 0x3F] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARSET[b2 & 0x3F] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// リポジトリ内のアプリアイコン（Tauriアイコン、favicon、icon.png等）を探索し、base64 Data URL を返す。
#[tauri::command]
fn find_repo_icon(repo: String) -> Option<String> {
    let base = Path::new(&repo);
    let candidates = [
        "src-tauri/icons/32x32.png",
        "src-tauri/icons/128x128.png",
        "src-tauri/icons/icon.png",
        "art/icon.png",
        "icon.png",
        "app/icon.png",
        "public/icon.png",
        "public/favicon.ico",
        "public/favicon.png",
        "assets/icon.png",
        "assets/logo.png",
        // Swift など、Tauri 以外のアプリは、SVG で置くことが多い（.icns は、画面で表示できないので、対象外）。PNG のあとに探す
        "art/icon.svg",
        "icon.svg",
        "assets/icon.svg",
        "public/icon.svg",
    ];
    for rel in candidates {
        let p = base.join(rel);
        if p.is_file() {
            if let Ok(bytes) = std::fs::read(&p) {
                if !bytes.is_empty() && bytes.len() <= 1_500_000 {
                    let mime = if rel.ends_with(".ico") {
                        "image/x-icon"
                    } else if rel.ends_with(".svg") {
                        "image/svg+xml"
                    } else {
                        "image/png"
                    };
                    return Some(format!("data:{};base64,{}", mime, to_base64(&bytes)));
                }
            }
        }
    }
    None
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
#[derive(Serialize)]
struct UpdateInfo {
    version: String,
    current: String,
    notes: Option<String>,
}

/// 新しい版があるか、確かめる。**通信するのは、メニューの「アップデートを確認…」を押したときだけ**（起動時には、問い合わせない）。
#[tauri::command]
async fn update_check(app: tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| format!("更新の仕組みを使えません: {e}"))?;
    let found = updater.check().await.map_err(|e| format!("更新を確認できませんでした: {e}"))?;
    Ok(found.map(|u| UpdateInfo { version: u.version, current: u.current_version, notes: u.body }))
}

/// 新しい版を、ダウンロードして（署名を検証して）入れ替え、再起動する。
#[tauri::command]
async fn update_install(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;
    let updater = app.updater().map_err(|e| format!("更新の仕組みを使えません: {e}"))?;
    let Some(update) = updater.check().await.map_err(|e| format!("更新を確認できませんでした: {e}"))? else {
        return Err("更新はありません".into());
    };
    update.download_and_install(|_, _| {}, || {}).await.map_err(|e| format!("更新を入れられませんでした: {e}"))?;
    app.restart()
}

/// 画面・メニューに出す、アプリの名前（`productName`）。
fn display_name(app: &tauri::AppHandle) -> String {
    app.package_info().name.clone()
}

/// メニューバー。言語に応じて構築する。
/// 「設定…」（⌘,）を押すと、画面に `menu-settings` を送る（画面が、設定を開く）。
fn build_menu(app: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    build_menu_with_lang(app, system_lang().code())
}

fn build_menu_with_lang(app: &tauri::AppHandle, lang: &str) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use tauri::menu::{MenuBuilder, MenuItemBuilder, SubmenuBuilder};
    let t = i18n::Lang::from_code(lang).unwrap_or_default().texts();
    let settings = MenuItemBuilder::with_id("settings", t.menu_settings).accelerator("CmdOrCtrl+,").build(app)?;
    let update = MenuItemBuilder::with_id("update", t.menu_update).build(app)?;
    let app_name = display_name(app);
    let app_menu = SubmenuBuilder::new(app, &app_name)
        .about_with_text(t.menu_about(&app_name), None)
        .item(&update)
        .separator()
        .item(&settings)
        .separator()
        .services_with_text(t.menu_services)
        .separator()
        .hide_with_text(t.menu_hide(&app_name))
        .hide_others_with_text(t.menu_hide_others)
        .show_all_with_text(t.menu_show_all)
        .separator()
        .quit_with_text(t.menu_quit(&app_name))
        .build()?;
    let edit = SubmenuBuilder::new(app, t.menu_edit)
        .undo_with_text(t.menu_undo)
        .redo_with_text(t.menu_redo)
        .separator()
        .cut_with_text(t.menu_cut)
        .copy_with_text(t.menu_copy)
        .paste_with_text(t.menu_paste)
        .select_all_with_text(t.menu_select_all)
        .build()?;
    let view = SubmenuBuilder::new(app, t.menu_view)
        .fullscreen_with_text(t.menu_fullscreen)
        .build()?;
    let window = SubmenuBuilder::new(app, t.menu_window)
        .minimize_with_text(t.menu_minimize)
        .maximize_with_text(t.menu_zoom)
        .separator()
        .close_window_with_text(t.menu_close_window)
        .build()?;
    MenuBuilder::new(app).item(&app_menu).item(&edit).item(&view).item(&window).build()
}

#[cfg(target_os = "macos")]
pub(crate) fn system_lang() -> i18n::Lang {
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;
    unsafe {
        if let Some(cls) = AnyClass::get(c"NSLocale") {
            let langs: *mut AnyObject = msg_send![cls, preferredLanguages];
            if !langs.is_null() {
                let count: usize = msg_send![langs, count];
                for i in 0..count {
                    let elem: *mut AnyObject = msg_send![langs, objectAtIndex: i];
                    if !elem.is_null() {
                        let utf8: *const std::ffi::c_char = msg_send![elem, UTF8String];
                        if !utf8.is_null() {
                            if let Ok(s) = std::ffi::CStr::from_ptr(utf8).to_str() {
                                if let Some(l) = i18n::Lang::from_locale_prefix(s) {
                                    return l;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    i18n::Lang::default()
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn system_lang() -> i18n::Lang {
    std::env::var("LANG")
        .ok()
        .and_then(|l| i18n::Lang::from_locale_prefix(&l))
        .unwrap_or_default()
}

pub(crate) fn resolve_lang_arg(lang: Option<&str>) -> Result<&'static str, String> {
    i18n::Lang::resolve_arg(lang, system_lang()).map(i18n::Lang::code)
}

#[cfg(target_os = "macos")]
fn clear_apple_languages_override() -> Result<(), String> {
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;
    unsafe {
        let cls = AnyClass::get(c"NSUserDefaults")
            .ok_or_else(|| "NSUserDefaults クラスを取得できませんでした".to_string())?;
        let defaults: *mut AnyObject = msg_send![cls, standardUserDefaults];
        if defaults.is_null() {
            return Err("standardUserDefaults が null を返しました".to_string());
        }
        let str_cls = AnyClass::get(c"NSString")
            .ok_or_else(|| "NSString クラスを取得できませんでした".to_string())?;
        let key_str: *mut AnyObject = msg_send![str_cls, stringWithUTF8String: c"AppleLanguages".as_ptr()];
        if key_str.is_null() {
            return Err("AppleLanguages キー文字列を生成できませんでした".to_string());
        }
        let _: () = msg_send![defaults, removeObjectForKey: key_str];
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn localize_system_menus_on_main_thread(app: &tauri::AppHandle, lang: &str) -> Result<(), String> {
    let target_lang = i18n::Lang::from_code(lang).unwrap_or_default();
    app.run_on_main_thread(move || {
        use objc2::runtime::{AnyClass, AnyObject};
        use objc2::msg_send;
        unsafe {
            let Some(app_cls) = AnyClass::get(c"NSApplication") else {
                eprintln!("NSApplication クラスを取得できませんでした");
                return;
            };
            let ns_app: *mut AnyObject = msg_send![app_cls, sharedApplication];
            if ns_app.is_null() {
                eprintln!("sharedApplication が null を返しました");
                return;
            }
            let main_menu: *mut AnyObject = msg_send![ns_app, mainMenu];
            if main_menu.is_null() { return; }
            localize_menu_items_recursive(main_menu, target_lang);
        }
    })
    .map_err(|e| format!("メインスレッドでのメニュー翻訳の委譲に失敗しました: {e}"))
}

#[cfg(target_os = "macos")]
unsafe fn localize_menu_items_recursive(menu: *mut objc2::runtime::AnyObject, lang: i18n::Lang) {
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::msg_send;
    if menu.is_null() { return; }
    let items: *mut AnyObject = msg_send![menu, itemArray];
    if items.is_null() { return; }
    let count: usize = msg_send![items, count];
    let str_cls = match AnyClass::get(c"NSString") {
        Some(c) => c,
        None => return,
    };
    let map = lang.texts().macos_sys_menu_map;
    for i in 0..count {
        let item: *mut AnyObject = msg_send![items, objectAtIndex: i];
        if item.is_null() { continue; }
        let title: *mut AnyObject = msg_send![item, title];
        if !title.is_null() {
            let utf8: *const std::ffi::c_char = msg_send![title, UTF8String];
            if !utf8.is_null() {
                if let Ok(s) = std::ffi::CStr::from_ptr(utf8).to_str() {
                    if let Some((_, rep)) = map.iter().find(|(from, _)| *from == s) {
                        if let Ok(c_rep) = std::ffi::CString::new(*rep) {
                            let new_title: *mut AnyObject = msg_send![str_cls, stringWithUTF8String: c_rep.as_ptr()];
                            let _: () = msg_send![item, setTitle: new_title];
                        }
                    }
                }
            }
        }
        let submenu: *mut AnyObject = msg_send![item, submenu];
        if !submenu.is_null() {
            localize_menu_items_recursive(submenu, lang);
        }
    }
}

#[cfg(target_os = "macos")]
fn schedule_localize_system_menus(app: &tauri::AppHandle, resolved: &str) -> Result<(), String> {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::Mutex;
    static SEQ: AtomicU64 = AtomicU64::new(0);
    static TARGET_LANG: Mutex<&'static str> = Mutex::new("ja");
    static RUNNING: AtomicBool = AtomicBool::new(false);

    let code = i18n::Lang::from_code(resolved).unwrap_or_default().code();
    if let Ok(mut g) = TARGET_LANG.lock() {
        *g = code;
    }
    let seq = SEQ.fetch_add(1, Ordering::SeqCst) + 1;
    localize_system_menus_on_main_thread(app, code)?;

    // 既に待機スレッドが動いていれば、新しいスレッドは起こさず最新の世代番号（SEQ）の反映を任せる
    if RUNNING
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }

    let app_clone = app.clone();
    std::thread::spawn(move || {
        let mut cur_seq = seq;
        loop {
            let mut interrupted = false;
            for delay_ms in [50_u64, 250, 600] {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                let latest = SEQ.load(Ordering::SeqCst);
                if latest != cur_seq {
                    cur_seq = latest;
                    interrupted = true;
                    break;
                }
                let lang = TARGET_LANG.lock().map(|g| *g).unwrap_or("ja");
                if let Err(e) = localize_system_menus_on_main_thread(&app_clone, lang) {
                    eprintln!("{e}");
                    break;
                }
            }
            if !interrupted {
                RUNNING.store(false, Ordering::SeqCst);
                let latest = SEQ.load(Ordering::SeqCst);
                if latest != cur_seq
                    && RUNNING
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                {
                    cur_seq = latest;
                    continue;
                }
                break;
            }
        }
    });
    Ok(())
}

#[tauri::command]
async fn set_menu_language(app: tauri::AppHandle, lang: String) -> Result<(), String> {
    let resolved = resolve_lang_arg(Some(lang.as_str()))?;
    let menu = build_menu_with_lang(&app, resolved).map_err(|e| e.to_string())?;
    app.set_menu(menu).map_err(|e| e.to_string())?;
    #[cfg(target_os = "macos")]
    {
        clear_apple_languages_override()?;
        schedule_localize_system_menus(&app, resolved)?;
    }
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .menu(build_menu)
        .on_menu_event(|app, event| {
            match event.id().as_ref() {
                "settings" => {
                    let _ = app.emit("menu-settings", ());
                }
                "update" => {
                    let _ = app.emit("menu-update", ());
                }
                _ => {}
            }
        })
        .setup(|app| {
            let explicit = std::env::var_os("MRCR_DATA_DIR").map(PathBuf::from);
            let app_data_dir = app.path().app_data_dir()?;
            // 同時起動を防ぐ。**置き場を触る前**（移しも含む）に取る。取れなければ、案内を出して終了する。
            let base = app_data_dir.parent().unwrap_or(&app_data_dir).to_path_buf();
            let (lock, lock_notice) = match applock::acquire(&mrcr_core::datadir::lock_file(&base, explicit.as_deref())) {
                applock::Acquired::Locked(l) => (Some(l), None),
                applock::Acquired::AlreadyRunning => applock::tell_already_running_and_exit(),
                applock::Acquired::Unsupported(why) => {
                    let notice = applock::unsupported_notice(&why);
                    eprintln!("{notice}");
                    (None, Some(notice))
                }
            };
            let (data_dir, mut data_notice) = match explicit {
                Some(p) => (p, lock_notice),
                None => {
                    let r = mrcr_core::datadir::resolve_in(&base, mrcr_core::datadir::Mode::Migrate);
                    (r.dir, [r.notice, lock_notice].into_iter().flatten().reduce(|a, b| format!("{a} ／ {b}")))
                }
            };
            std::fs::create_dir_all(&data_dir)?;
            // 上限の旗は、通信せずに、記録だけから決める（再確認は、画面が `license_status` を呼ぶときに行う）。
            // 窓のタイトルは、殻の設定ではなく、実行時の名前で付ける（殻ごとに、窓の設定を持たなくて済む）
            let app_name = display_name(app.handle());
            if let Some(w) = app.get_webview_window("main") {
                if let Err(e) = w.set_title(&app_name) {
                    eprintln!("窓のタイトルを付けられませんでした: {e}");
                }
            }
            let initial_lang = match settings::load(&data_dir) {
                Ok(s) => s.language,
                Err(e) => {
                    eprintln!("設定を読み込めませんでした（{e}）");
                    data_notice = [data_notice, Some(e)].into_iter().flatten().reduce(|a, b| format!("{a} ／ {b}"));
                    "auto".to_string()
                }
            };
            #[cfg(target_os = "macos")]
            if let Err(e) = clear_apple_languages_override() {
                eprintln!("AppleLanguages の初期化に失敗しました（{e}）");
            }
            let resolved_initial_lang = resolve_lang_arg(Some(initial_lang.as_str())).unwrap_or_else(|e| {
                eprintln!("{e}");
                system_lang().code()
            });
            if let Err(e) = build_menu_with_lang(app.handle(), resolved_initial_lang)
                .and_then(|m| app.set_menu(m).map(|_| ()))
            {
                let msg = format!("メニューの言語を設定できませんでした（{e}）");
                eprintln!("{msg}");
                data_notice = [data_notice, Some(msg)].into_iter().flatten().reduce(|a, b| format!("{a} ／ {b}"));
            }
            #[cfg(target_os = "macos")]
            if let Err(e) = localize_system_menus_on_main_thread(app.handle(), resolved_initial_lang) {
                eprintln!("{e}");
            }
            app.manage(AppState { data_dir, data_notice, app_name, _lock: lock });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            find_repo_icon,
            list_recipes,
            usage_summary,
            fetch_agent_reports,
            agent_log_consent,
            current_diff,
            revoke_agent_log_consent,
            report_preview,
            report_save,
            print_page,
            list_bases,
            duration_stats,
            list_providers,
            repo_status,
            repo_data,
            repos_load,
            repo_add,
            repo_rename,
            repo_restore,
            repo_move,
            repo_unlink,
            repo_delete_data,
            data_dir_notice,
            app_name,
            update_check,
            update_install,
            byok_status,
            byok_save_key,
            byok_delete_key,
            byok_save_config,
            preview_snapshot,
            run_review,
            list_reviews,
            get_settings,
            save_settings,
            get_review,
            set_triage,
            delete_reviews,
            set_memo,
            set_done,
            build_handoff,
            write_handoff,
            set_menu_language
        ])
        .build(tauri::generate_context!())
        .expect("error while building MrCodeReview")
        .run(|_app, _event| {});
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C").arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
            .args(args)
            .status().unwrap().success();
        assert!(ok, "git {args:?}");
    }

    /// 架空のリポジトリ（テスト用の一時ディレクトリ）を作る。
    #[test]
    fn the_entry_points_refuse_an_unregistered_repo_and_pass_a_registered_one() {
        let repo = demo_repo("gate");
        let repo_s = repo.to_str().unwrap();
        let data = std::env::temp_dir().join(format!("mrcr-gate-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let refused = |e: String| assert!(e.contains("登録されていません"), "{e}");
        // 未登録: 3 つの入口が、断る（何も実行せず、履歴にも残さない）
        refused(list_bases_checked(&data, repo_s).unwrap_err());
        refused(preview_checked(&data, repo_s, None).err().expect("断る"));
        refused(run_review_checked(&data, repo_s, "security", "mock", None).unwrap_err());
        assert!(store::load_all(&data).is_empty(), "断ったとき、履歴は増えない");
        // 登録済み: 通る
        repos::add(&data, repo_s, None).unwrap();
        assert!(list_bases_checked(&data, repo_s).is_ok());
        assert!(preview_checked(&data, repo_s, None).is_ok());
        let rec = run_review_checked(&data, repo_s, "security", "mock", None).unwrap();
        assert!(rec.error.is_none());
        // 解除すると、また断る（履歴があるので、非表示で残る）
        let id = repos::list(&data).unwrap().repos[0].id.clone();
        repos::unlink(&data, &id).unwrap();
        assert!(run_review_checked(&data, repo_s, "security", "mock", None).unwrap_err().contains("解除されています"));
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn repo_status_tells_missing_not_git_ok_and_a_subdirectory_apart() {
        let base = std::env::temp_dir().join(format!("mrcr-repostatus-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let missing = base.join("gone");
        assert_eq!(repo_status_of(missing.to_str().unwrap()).state, "missing");
        let plain = base.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        assert_eq!(repo_status_of(plain.to_str().unwrap()).state, "not_git");
        let repo = base.join("repo");
        std::fs::create_dir_all(repo.join("sub")).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        let ok = repo_status_of(repo.to_str().unwrap());
        assert_eq!((ok.state, ok.root), ("ok", None));
        // 末尾の `/` があっても、同じリポジトリ
        assert_eq!(repo_status_of(&format!("{}/", repo.display())).root, None);
        // サブディレクトリを登録したときは、根を知らせる
        let sub = repo_status_of(repo.join("sub").to_str().unwrap());
        assert_eq!(sub.state, "ok");
        assert!(sub.root.is_some_and(|r| std::fs::canonicalize(r).unwrap() == std::fs::canonicalize(&repo).unwrap()));
        let _ = std::fs::remove_dir_all(&base);
    }

    fn demo_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mrcr-e2e-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("db.js"), "export function find(id) {\n  return null;\n}\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "init"]);
        std::fs::write(dir.join("db.js"), "export function find(id) {\n  return db.query(\"SELECT * FROM u WHERE id=\" + id);\n}\n").unwrap();
        std::fs::write(dir.join("new.js"), "untracked\n").unwrap();
        dir
    }

    #[test]
    fn end_to_end_with_mock_records_snapshot_exclusions_and_verification() {
        let repo = demo_repo("mock");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);

        let rec = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        // 追跡済みの変更と、未追跡の新規ファイルの両方が対象になる
        let paths: Vec<&str> = rec.snapshot.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["db.js", "new.js"]);
        assert!(rec.snapshot.excluded.is_empty());
        assert!(rec.prompt.contains("SELECT * FROM u WHERE id="));
        assert!(rec.prompt.contains("+untracked"), "未追跡ファイルの内容が追加行として入っている");
        assert!(rec.prompt.contains("## 出力形式"));
        assert_eq!(rec.findings.len(), 2);
        assert!(rec.findings[0].verified);
        assert!(!rec.findings[1].verified);
        assert!(rec.error.is_none() && rec.parse_error.is_none());

        // 履歴に残り、読み戻せる
        let all = store::load_all(&data);
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, rec.id);
        assert_eq!(ReviewSummary::from(&all[0]).verified, 1);

        // 同じ作業ツリーなら、固定した差分のハッシュは同じ
        let again = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        assert_eq!(again.snapshot.diff_hash, rec.snapshot.diff_hash);
        assert_eq!(store::load_all(&data).len(), 2);

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn triage_flows_into_the_handoff_text_and_survives_reload() {
        let repo = demo_repo("triage");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-triage-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let rec = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        assert_eq!(rec.findings.len(), 2);

        // 何も仕分けていなければ、コピーするものは無い
        assert_eq!(view(&data, rec.clone()).triage, vec!["new", "new"]);
        let t = triage::get(&data, &rec.id, 2);
        assert_eq!(triage::handoff_text(&rec, &t, "MrCodeReview"), "");

        // 1 つ目（根拠あり）を「対応」、2 つ目（根拠不一致）を「待ち」に
        triage::set(&data, &rec.id, 0, "fix", 2).unwrap();
        triage::set(&data, &rec.id, 1, "wait", 2).unwrap();

        // 読み戻しても残る（別の呼び出しで取り直す）
        let reloaded = find_review(&data, &rec.id).unwrap();
        let t = triage::get(&data, &reloaded.id, reloaded.findings.len());
        assert_eq!(t, vec!["fix", "wait"]);
        let text = triage::handoff_text(&reloaded, &t, "MrCodeReview");
        assert!(text.contains("## 直す"));
        assert!(text.contains(&format!("場所: {}:1", reloaded.findings[0].file)));
        assert!(text.contains("## 今回は直さない"));
        assert!(text.contains("根拠不一致の見本"), "待ちの見出しが入る");
        assert!(!text.contains("引用: eval(request.body.code)"), "待ちの中身は依頼に入れない");

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn recipe_list_with_lang_returns_english_recipes_when_en() {
        let data = std::env::temp_dir().join(format!("mrcr-recipe-lang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let list = recipe_list_with_lang(None, "en");
        assert_eq!(list.recipes[0].title, "Latent Bugs");
        assert_eq!(list.recipes[1].title, "Error Handling");
        assert_eq!(list.recipes[2].title, "Security");
        assert_eq!(list.recipes[3].title, "Performance");
        assert_eq!(list.recipes[4].title, "Missing Tests");
        assert_eq!(list.recipes[5].title, "Cross-file Inconsistencies");
        assert!(list.warnings.is_empty());

        let invalid = recipe_list_with_lang(None, "fr");
        assert!(invalid.warnings.iter().any(|w| w.contains("不正な言語指定: fr")));
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn run_review_uses_english_prompt_and_recipe_when_language_is_en() {
        let repo = demo_repo("lang-en");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-lang-en-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        std::fs::write(settings::path(&data), r#"{"language": "en"}"#).unwrap();

        let rec = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        assert_eq!(rec.recipe.title, "Security");
        assert!(rec.prompt.contains("## Output Format"), "英語の出力形式セクションが入る:\n{}", rec.prompt);
        assert!(rec.prompt.contains("## Diff"), "英語の差分セクションが入る:\n{}", rec.prompt);
        assert!(!rec.prompt.contains("## 出力形式"), "日本語の出力形式セクションは入らない:\n{}", rec.prompt);

        // language="auto" のとき、フロントエンドから渡された lang ("en" / "ja") に従う
        std::fs::write(settings::path(&data), r#"{"language": "auto"}"#).unwrap();
        let rec_auto_en = run_review_with_base_and_lang(&data, repo.to_str().unwrap(), "security", "mock", None, Some("en")).unwrap();
        assert_eq!(rec_auto_en.recipe.title, "Security");
        assert!(rec_auto_en.prompt.contains("## Output Format"));
        let rec_auto_ja = run_review_with_base_and_lang(&data, repo.to_str().unwrap(), "security", "mock", None, Some("ja")).unwrap();
        assert_eq!(rec_auto_ja.recipe.title, "セキュリティ");
        assert!(rec_auto_ja.prompt.contains("## 出力形式"));

        // 未知の言語値（"fr"）は黙って "ja" に落とさずエラーを返す
        let bad_lang = run_review_with_base_and_lang(&data, repo.to_str().unwrap(), "security", "mock", None, Some("fr"));
        assert!(bad_lang.is_err());
        assert!(bad_lang.unwrap_err().contains("不正な言語指定: fr"));

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn rerun_inherits_wait_and_dismiss_and_reports_the_finding_as_persisting() {
        let repo = demo_repo("carry1");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-carry1-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let r1 = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        assert!(r1.carries.iter().all(Option::is_none) && r1.fates.is_empty(), "初回は引き継ぐものが無い");
        triage::set(&data, &r1.id, 0, "wait", 2).unwrap();
        triage::set(&data, &r1.id, 1, "dismiss", 2).unwrap();

        // 何も変えずに押し直す
        let r2 = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        assert_ne!(r1.id, r2.id);
        assert_eq!(view(&data, r2.clone()).triage, vec!["wait", "dismiss"]);
        assert!(r2.carries.iter().all(|c| c.as_ref().is_some_and(|c| c.exact && c.from_review == r1.id)));
        // 「待ち」にした前回の指摘は、今回も指摘された。「無視」は一覧に出さない
        assert_eq!(r2.fates.len(), 1);
        assert_eq!((r2.fates[0].from_index, r2.fates[0].fate.as_str(), r2.fates[0].state_before.as_str()), (0, "persists", "wait"));

        // 3 回目は、2 回目から引き継ぐ（仕分けが途切れない）
        let r3 = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        assert_eq!(view(&data, r3).triage, vec!["wait", "dismiss"]);

        // 別のリポジトリのレビューには引き継がない
        let other = demo_repo("carry1-other");
        let r4 = run_review_blocking(&data, other.to_str().unwrap(), "security", "mock").unwrap();
        assert_eq!(view(&data, r4).triage, vec!["new", "new"]);

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&other);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn fixing_the_code_makes_the_finding_gone_and_leaves_it_unclassified() {
        let repo = demo_repo("carry2");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-carry2-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let r1 = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        triage::set(&data, &r1.id, 0, "fix", 2).unwrap();
        triage::set(&data, &r1.id, 1, "dismiss", 2).unwrap();

        // 指摘された行を直す（パラメータ化）
        std::fs::write(
            repo.join("db.js"),
            "export function find(id) {\n  return db.query(\"SELECT * FROM u WHERE id=?\", [id]);\n}\n",
        )
        .unwrap();
        let r2 = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();

        // 直した行は、別の指摘（未仕分け）として出る。「対応」はほぼ一致では引き継がない。
        // 無視にした架空の指摘は、そのまま引き継ぐ
        assert_eq!(view(&data, r2.clone()).triage, vec!["new", "dismiss"]);
        // 前回「対応」にした指摘は、引用の行がファイルから消えた＝解消
        assert_eq!(r2.fates.len(), 1);
        assert_eq!((r2.fates[0].from_index, r2.fates[0].fate.as_str()), (0, "gone"));

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn committing_unfixed_code_is_not_mistaken_for_a_fix() {
        let repo = demo_repo("carry3");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-carry3-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let r1 = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        assert!(r1.snapshot.files.iter().any(|f| f.path == "db.js"));
        triage::set(&data, &r1.id, 0, "fix", 2).unwrap();

        // 直さずにコミットする → db.js は差分から消える。別のファイルだけ変更が残る。
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "wip"]);
        std::fs::write(repo.join("new.js"), "untracked\nmore\n").unwrap();
        let r2 = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        assert!(r2.snapshot.files.iter().all(|f| f.path != "db.js"), "db.js はもう差分に出ない");

        // 差分から消えただけで「解消」とは言わない。ファイルには引用の行がまだある。
        assert_eq!(r2.fates.len(), 1);
        assert_eq!(r2.fates[0].fate, "not_reraised");

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_clean_repo_can_be_reviewed_from_an_earlier_commit_and_the_record_says_which() {
        let dir = std::env::temp_dir().join(format!("mrcr-e2e-base-clean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("a.js"), "export const A = 1;\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "最初"]);
        std::fs::write(dir.join("a.js"), "export const A = 1;\nexport function f(x) { return x / 0; }\n").unwrap();
        git(&dir, &["commit", "-q", "-am", "f を足す"]);
        let data = std::env::temp_dir().join(format!("mrcr-e2e-base-clean-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let root = dir.to_str().unwrap();

        // 未コミットの変更が無い（これまでは、ここで行き止まり）
        let none = run_review_with_base(&data, root, "bugs", "mock", None).unwrap_err();
        assert!(none.contains("送る差分がありません"), "{none}");
        // 直近のコミットを比較元にすれば、そのコミットを見られる
        let rec = run_review_with_base(&data, root, "bugs", "mock", Some("HEAD~1")).unwrap();
        assert!(rec.error.is_none() && rec.parse_error.is_none(), "{:?} {:?}", rec.error, rec.parse_error);
        assert_eq!(rec.snapshot.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a.js"]);
        assert_eq!((rec.snapshot.base.spec.as_str(), rec.snapshot.base.subject.as_str()), ("HEAD~1", "最初"), "比較元を記録に残す");
        assert_eq!(rec.snapshot.base.hash.len(), 40);
        assert!(rec.prompt.contains("+export function f(x)"), "送った差分は、そのコミットの変更");
        assert!(rec.findings.iter().any(|f| f.verified), "引用は、差分の行そのもの");
        // 取り出し直しても、比較元が残っている
        let back = store::get(&data, &rec.id).unwrap();
        assert_eq!(back.snapshot.base, rec.snapshot.base);
        // 使えない比較元は、レビューを始める前に、理由つきで断る（記録は残らない）
        let n = store::load_all(&data).len();
        let bad = run_review_with_base(&data, root, "bugs", "mock", Some("--output=/tmp/x")).unwrap_err();
        assert!(bad.contains("比較元"), "{bad}");
        assert_eq!(store::load_all(&data).len(), n);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn report_output_paths_must_be_absolute_with_the_right_extension_in_an_existing_folder() {
        let dir = std::env::temp_dir().join(format!("mrcr-e2e-outpath-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("記録.md");
        assert_eq!(check_output_path(ok.to_str().unwrap(), "md").unwrap(), ok);
        assert_eq!(check_output_path(dir.join("a.XLSX").to_str().unwrap(), "xlsx").unwrap(), dir.join("a.XLSX"), "拡張子は、大文字小文字を問わない");
        for (path, ext, why) in [
            ("relative/out.md", "md", "絶対パス"),
            ("out.md", "md", "絶対パス"),
            (dir.join("out.txt").to_str().unwrap(), "md", "拡張子"),
            (dir.join("out").to_str().unwrap(), "md", "拡張子"),
            (dir.join("no-such-dir/out.md").to_str().unwrap(), "md", "フォルダが見つかりません"),
        ] {
            let e = check_output_path(path, ext).unwrap_err();
            assert!(e.contains(why), "{path}: {e}");
        }
        std::fs::create_dir_all(dir.join("folder.md")).unwrap();
        assert!(check_output_path(dir.join("folder.md").to_str().unwrap(), "md").unwrap_err().contains("フォルダです"));
        let o = report_options(report::Options { repo: Some("  ".into()), ..report::Options::default() });
        assert!(o.repo.is_none(), "空のリポジトリは、すべて");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn current_diff_shows_what_changed_since_the_review_against_the_same_fixed_base() {
        let repo = demo_repo("current-diff");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-current-diff-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let rec = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();

        // 何も変えていなければ、レビューした時点と同じ
        let same = current_diff_in(&data, &rec.id).unwrap();
        assert!(same.same_as_review, "変えていない: 識別子が同じ");
        assert_eq!(same.diff_hash, rec.snapshot.diff_hash);
        assert!(same.diff_text.contains("db.query"));

        // エージェントが直したように、ファイルを変える
        std::fs::write(repo.join("db.js"), "export function find(id) {\n  return db.query(\"SELECT * FROM u WHERE id=?\", [id]);\n}\n").unwrap();
        let after = current_diff_in(&data, &rec.id).unwrap();
        assert!(!after.same_as_review);
        assert_ne!(after.diff_hash, rec.snapshot.diff_hash);
        assert!(after.diff_text.contains("WHERE id=?") && !after.diff_text.contains("\"SELECT * FROM u WHERE id=\" + id"), "直したあとの行が出て、直す前の行は出ない");

        // あいだにコミットしても、比較元は、レビューのときのコミットに固定されている（基準が動かない）
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "fix"]);
        let committed = current_diff_in(&data, &rec.id).unwrap();
        assert!(committed.diff_text.contains("WHERE id=?"), "コミットしたあとも、レビュー時点からの変更が見える");
        assert_ne!(committed.head, rec.snapshot.head, "HEAD は進んだ");
        assert!(current_diff_in(&data, "nope").unwrap_err().contains("見つからない"));
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn findings_the_agent_reported_as_done_are_left_out_of_what_is_handed_over_again() {
        let repo = demo_repo("handoff-done");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-handoff-done-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let rec = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        let n = rec.findings.len();
        assert!(n >= 1);
        triage::set_with(&data, &rec.id, 0, "fix", n, 1000, None).unwrap();
        let before = triage::handoff_text(&rec, &handoff_triage(&data, &rec.id, n), "MrCodeReview");
        assert!(before.contains(&triage::finding_tag(&rec.id, 0)), "申告の前は、渡す対象");

        let rep = |verdict: &str| agent_report::AgentReport { verdict: verdict.into(), text: "x".into(), reported_at: 5, source: "s".into() };
        // 「対応しなかった」の申告は、まだ判断が残っているので、渡す対象のまま
        agent_report::save(&data, &rec.id, &[(0, rep(agent_report::SKIPPED))]).unwrap();
        assert_eq!(handoff_triage(&data, &rec.id, n)[0], "fix");
        // 「対応した」の申告が付いたら、外す（文面にも、「今回は直さない」にも、出ない）
        agent_report::save(&data, &rec.id, &[(0, rep(agent_report::DID))]).unwrap();
        let t = handoff_triage(&data, &rec.id, n);
        assert_eq!(t[0], "done");
        assert!(!triage::handoff_text(&rec, &t, "MrCodeReview").contains(&triage::finding_tag(&rec.id, 0)));
        assert!(handoff::write(&rec, &t, Some(0), "MrCodeReview").unwrap_err().contains("「対応」にした指摘だけ"), "渡し済みの指摘は、1 件だけでも、渡せない");
        // 使う人の仕分けは、書き換えない
        assert_eq!(triage::get(&data, &rec.id, n)[0], "fix");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn findings_marked_done_by_hand_are_left_out_of_what_is_handed_over_again() {
        let repo = demo_repo("handoff-marked");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-handoff-marked-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let rec = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        let n = rec.findings.len();
        assert!(n >= 1);
        triage::set_with(&data, &rec.id, 0, "fix", n, 1000, None).unwrap();
        assert_eq!(handoff_triage(&data, &rec.id, n)[0], "fix", "対応済みにする前は、渡す対象");
        notes::set_done(&data, &rec.id, 0, n, true, 2000, Some("abc")).unwrap();
        let t = handoff_triage(&data, &rec.id, n);
        assert_eq!(t[0], "done", "手で対応済みにしたものは、また渡さない（エージェントの申告と同じ）");
        assert!(!triage::handoff_text(&rec, &t, "MrCodeReview").contains(&triage::finding_tag(&rec.id, 0)));
        // 使う人の仕分けは、書き換えない
        assert_eq!(triage::get(&data, &rec.id, n)[0], "fix");
        // 対応済みを戻すと、また渡す対象
        notes::set_done(&data, &rec.id, 0, n, false, 3000, None).unwrap();
        assert_eq!(handoff_triage(&data, &rec.id, n)[0], "fix");
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn the_report_lines_in_the_handoff_text_name_the_review_and_an_agent_copying_them_back_is_read() {
        let repo = demo_repo("report-roundtrip");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-roundtrip-data-{}", std::process::id()));
        let projects = std::env::temp_dir().join(format!("mrcr-e2e-roundtrip-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&projects);
        let rec = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        let n = rec.findings.len();
        assert!(n >= 1);
        for i in 0..n {
            triage::set_with(&data, &rec.id, i, "fix", n, 1000, None).unwrap();
        }
        let text = triage::handoff_text(&rec, &handoff_triage(&data, &rec.id, n), "MrCodeReview");
        // 依頼の末尾の、報告の行: 識別子のあとに、観点の名前
        let label = triage::review_label(&rec.recipe.title);
        assert!(!label.is_empty());
        let lines: Vec<&str> = text.lines().filter(|l| l.starts_with("mrcr:") && l.contains("<何をしたか>")).collect();
        assert_eq!(lines.len(), n);
        assert!(lines.iter().all(|l| l.contains(&label)), "{lines:?}");
        // エージェントが、行を写して、「対応した: <何をしたか>」のところだけを埋める（ひな形の後半は、消す）
        let reply: String = lines
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{}\n", l.replace("<何をしたか> ／ 対応しなかった: <理由>", &format!("{} 番を直した", i + 1))))
            .collect();
        let log = serde_json::json!({
            "type": "assistant", "timestamp": "2099-01-01T00:00:00.000Z",
            "message": { "role": "assistant", "content": [{ "type": "text", "text": format!("直しました。\n{reply}") }] }
        });
        std::fs::create_dir_all(projects.join("-proj")).unwrap();
        std::fs::write(projects.join("-proj").join("sess.jsonl"), format!("{log}\n")).unwrap();
        let here = agent_report::Sources { claude: projects.clone(), agy: projects.join("no-agy"), copilot: projects.join("no-copilot") };
        let got = fetch_agent_reports_in(&data, &here, &rec.id, true).unwrap();
        assert_eq!((got.found, got.updated), (n, n), "観点の名前つきの行を、全部読める");
        let v = serde_json::to_value(view(&data, find_review(&data, &rec.id).unwrap())).unwrap();
        for i in 0..n {
            assert_eq!(v["agent_reports"][i]["verdict"], "did");
            assert_eq!(v["agent_reports"][i]["text"], format!("{} 番を直した", i + 1), "観点の名前は、本文に混ざらない");
        }
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&projects);
    }

    #[test]
    fn fetching_agent_reports_needs_consent_first_then_imports_only_matching_replies_and_shows_them_in_the_view() {
        let repo = demo_repo("agent-fetch");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-agent-fetch-data-{}", std::process::id()));
        let projects = std::env::temp_dir().join(format!("mrcr-e2e-agent-fetch-logs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&projects);
        let rec = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        let n = rec.findings.len();
        assert!(n >= 1);
        let reply = serde_json::json!({
            "type": "assistant", "timestamp": "2099-01-01T00:00:00.000Z",
            "message": { "role": "assistant", "content": [{ "type": "text", "text": format!("直しました。\n{} 対応した: 境界を直した\n", triage::finding_tag(&rec.id, 0)) }] }
        });
        let here = agent_report::Sources { claude: projects.clone(), agy: projects.join("no-agy"), copilot: projects.join("no-copilot") };
        let nowhere = agent_report::Sources { claude: projects.join("no-such"), agy: projects.join("no-agy"), copilot: projects.join("no-copilot") };
        std::fs::create_dir_all(projects.join("-proj")).unwrap();
        std::fs::write(projects.join("-proj").join("sess.jsonl"), format!("{reply}\n")).unwrap();

        // 許可がないうちは、ログに触らない（ログの置き場所が、存在しなくても、エラーにならない）
        let none = fetch_agent_reports_in(&data, &nowhere, &rec.id, false).unwrap();
        assert!(none.needs_consent && none.found == 0 && none.files_scanned == 0 && none.agent_reports.iter().all(Option::is_none));
        assert!(view(&data, find_review(&data, &rec.id).unwrap()).agent_reports.iter().all(Option::is_none));

        // 許可して読む。許可は、覚える（2 回目は、許可なしで読める）
        let got = fetch_agent_reports_in(&data, &here, &rec.id, true).unwrap();
        assert!(!got.needs_consent);
        assert_eq!((got.found, got.updated, got.files_with_tag), (1, 1, 1));
        let again = fetch_agent_reports_in(&data, &here, &rec.id, false).unwrap();
        assert!(!again.needs_consent);
        assert_eq!((again.found, again.updated), (1, 0), "同じ内容なら、保存し直さない");

        let v = serde_json::to_value(view(&data, find_review(&data, &rec.id).unwrap())).unwrap();
        assert_eq!(v["agent_reports"][0]["verdict"], "did");
        assert_eq!(v["agent_reports"][0]["text"], "境界を直した");
        assert_eq!(v["agent_reports"][0]["source"], "sess");
        if n > 1 {
            assert!(v["agent_reports"][1].is_null(), "報告の無い指摘は、null（記録なし）");
        }

        // 見つからないレビュー、ログの置き場所が無いときは、エラー（黙って空にしない）
        assert!(fetch_agent_reports_in(&data, &here, "nope", true).unwrap_err().contains("見つかりません"));
        assert!(fetch_agent_reports_in(&data, &nowhere, &rec.id, false).unwrap_err().contains("会話の記録の置き場所を開けません"));
        let _ = std::fs::remove_dir_all(&data);
        let _ = std::fs::remove_dir_all(&projects);
    }

    #[test]
    fn the_review_view_keeps_run_notes_and_per_finding_records_apart() {
        let repo = demo_repo("view-shape");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-view-shape-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let rec = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap();
        let n = rec.findings.len();
        assert!(n >= 1);
        triage::set_with(&data, &rec.id, 0, "fix", n, 1000, Some("abc1234")).unwrap();
        notes::set_memo(&data, &rec.id, 0, n, "境界を直した", 2000).unwrap();
        notes::set_done(&data, &rec.id, 0, n, true, 3000, Some("def5678")).unwrap();
        let v = serde_json::to_value(view(&data, find_review(&data, &rec.id).unwrap())).unwrap();
        assert!(v["notes"].as_array().is_some_and(|a| a.iter().all(|x| x.is_string())), "notes は、実行メモ（文字列の配列）のまま: {}", v["notes"]);
        let fnotes = v["finding_notes"].as_array().expect("finding_notes");
        assert_eq!(fnotes.len(), n, "指摘と同じ数");
        assert_eq!(
            (fnotes[0]["memo"].as_str(), fnotes[0]["done_at"].as_u64(), fnotes[0]["done_head"].as_str(), fnotes[0]["triage_at"].as_u64(), fnotes[0]["triage_source"].as_str()),
            (Some("境界を直した"), Some(3000), Some("def5678"), Some(1000), Some("user"))
        );
        assert_eq!(v["triage"][0], "fix");
        assert!(fnotes.get(1).is_none_or(|x| x["triage_at"].is_null() && x["memo"] == ""), "仕分けていない指摘は、記録が空");
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn history_is_limited_to_the_selected_repo_even_when_a_subdirectory_is_given() {
        let (a, b) = (demo_repo("hist-a"), demo_repo("hist-b"));
        let data = std::env::temp_dir().join(format!("mrcr-e2e-hist-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        run_review_blocking(&data, a.to_str().unwrap(), "security", "mock").unwrap();
        run_review_blocking(&data, a.to_str().unwrap(), "bugs", "mock").unwrap();
        run_review_blocking(&data, b.to_str().unwrap(), "security", "mock").unwrap();
        let titles = |repo: Option<&str>| store::summaries_in(&data, history_repo(repo).as_deref(), 50).into_iter().map(|s| s.recipe_title).collect::<Vec<_>>();
        assert_eq!(titles(a.to_str()).len(), 2, "A の履歴は 2 件");
        assert_eq!(titles(b.to_str()).len(), 1, "B の履歴は 1 件");
        // サブディレクトリを選んでも、リポジトリの根の履歴になる
        std::fs::create_dir_all(a.join("sub")).unwrap();
        assert_eq!(titles(a.join("sub").to_str()).len(), 2);
        assert_eq!(history_repo(a.join("sub").to_str()).unwrap(), snapshot::toplevel(a.to_str().unwrap()).unwrap());
        // 選んでいないときは、すべて。git のリポジトリでないパスは、何も一致しない
        assert_eq!(titles(None).len(), 3);
        assert_eq!(titles(Some("  ")).len(), 3);
        assert!(titles(Some("/definitely/not/a/repo")).is_empty());
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn settings_change_what_is_sent_and_each_review_records_the_settings_it_used() {
        let repo = demo_repo("settings");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-settings-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let root = repo.to_str().unwrap();

        // 設定ファイルが無ければ、推奨値で動き、その推奨値が記録に残る
        let r1 = run_review_blocking(&data, root, "security", "mock").unwrap();
        assert_eq!(r1.settings.as_ref().unwrap()["max_file_bytes"], 60_000);
        assert_eq!(r1.settings.as_ref().unwrap()["carry_window"], 30);
        assert_eq!(r1.snapshot.files.len(), 2);

        // 「送らないファイル名」を変えると、その差分が送られなくなり、設定の値も記録に残る
        std::fs::write(settings::path(&data), r#"{"ignored_lockfiles": ["db.js"], "history_limit": 20}"#).unwrap();
        let r2 = run_review_blocking(&data, root, "security", "mock").unwrap();
        let sent: Vec<&str> = r2.snapshot.files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(sent, vec!["new.js"]);
        assert!(r2.snapshot.excluded.iter().any(|e| e.path == "db.js"));
        assert_eq!(r2.settings.as_ref().unwrap()["ignored_lockfiles"], serde_json::json!(["db.js"]));
        assert_eq!(r2.settings.as_ref().unwrap()["max_file_bytes"], 60_000, "書かなかった項目は推奨値");

        // 上限を下げると、大きな差分が送られなくなる
        std::fs::write(settings::path(&data), r#"{"max_file_bytes": 1000, "max_total_bytes": 1000}"#).unwrap();
        std::fs::write(repo.join("new.js"), "x\n".repeat(2000)).unwrap();
        let r3 = run_review_blocking(&data, root, "security", "mock").unwrap();
        assert!(r3.snapshot.excluded.iter().any(|e| e.path == "new.js" && e.reason.contains("1 ファイルの上限を超えた（")), "{:?}", r3.snapshot.excluded);
        assert_eq!(r3.snapshot.files.len(), 1, "db.js だけが残る");

        // 履歴の一覧の件数も、設定に従う（20 件。ここでは 3 件なので全部）
        assert_eq!(store::summaries(&data, settings::load(&data).unwrap().history_limit).len(), 3);

        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn repo_settings_override_user_settings_and_are_recorded() {
        let repo = demo_repo("repo-settings");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-repo-settings-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        let root = repo.to_str().unwrap();
        // 個人の設定: db.js を送らない。リポジトリ側: それを、new.js に変える
        std::fs::write(settings::path(&data), r#"{"ignored_lockfiles": ["db.js"], "history_limit": 40}"#).unwrap();
        std::fs::create_dir_all(repo.join(".mrcodereview")).unwrap();
        std::fs::write(settings::repo_path(&repo), r#"{"ignored_lockfiles": ["new.js"], "carry_window": 7}"#).unwrap();

        let rec = run_review_blocking(&data, root, "security", "mock").unwrap();
        let sent: Vec<&str> = rec.snapshot.files.iter().map(|f| f.path.as_str()).collect();
        assert!(sent.contains(&"db.js") && !sent.contains(&"new.js"), "リポジトリ側の設定が優先: {sent:?}");
        let s = rec.settings.as_ref().unwrap();
        assert_eq!((s["carry_window"].as_u64(), s["history_limit"].as_u64()), (Some(7), Some(40)), "個人の設定は、リポジトリに無い項目で生きる");
        assert_eq!(rec.repo_overrides, vec!["ignored_lockfiles", "carry_window"], "どれがリポジトリ側かを記録する");
        assert!(rec.notes[0].contains(".mrcodereview/settings.json") && rec.notes[0].contains("ignored_lockfiles、carry_window"), "{:?}", rec.notes);

        // リポジトリ側の設定が無ければ、個人の設定のまま
        std::fs::remove_file(settings::repo_path(&repo)).unwrap();
        let rec2 = run_review_blocking(&data, root, "security", "mock").unwrap();
        assert!(rec2.repo_overrides.is_empty() && !rec2.snapshot.files.iter().any(|f| f.path == "db.js"));
        assert!(rec2.notes.iter().all(|n| !n.contains("このリポジトリの設定")), "{:?}", rec2.notes);

        // 設定画面に出す、リポジトリ側の状態
        std::fs::write(settings::repo_path(&repo), r#"{"carry_window": 7}"#).unwrap();
        let v = repo_settings_view(&data, Some(root));
        assert_eq!(v.keys, vec!["carry_window"]);
        assert!(v.path.as_deref().unwrap().ends_with(".mrcodereview/settings.json") && v.error.is_none());
        std::fs::write(settings::repo_path(&repo), r#"{"llm_timeout_secs": 60}"#).unwrap();
        assert!(repo_settings_view(&data, Some(root)).error.unwrap().contains("変えられません"));
        assert!(repo_settings_view(&data, None).path.is_none());
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_repo_setting_that_only_the_user_may_change_stops_the_review_with_the_reason() {
        let repo = demo_repo("repo-settings-bad");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-repo-settings-bad-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(repo.join(".mrcodereview")).unwrap();
        std::fs::write(settings::repo_path(&repo), r#"{"llm_timeout_secs": 5000}"#).unwrap();
        let err = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap_err();
        assert!(err.contains("llm_timeout_secs") && err.contains("変えられません"), "{err}");
        assert!(store::load_all(&data).is_empty(), "止めたレビューは履歴に残さない");
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn a_broken_settings_file_stops_the_review_with_the_reason_instead_of_falling_back() {
        let repo = demo_repo("settings-bad");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-settings-bad-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        std::fs::create_dir_all(&data).unwrap();
        for (text, key) in [(r#"{"carry_window": 0}"#, "carry_window"), (r#"{"max_file_byts": 5}"#, "max_file_byts"), ("{broken", "settings.json")] {
            std::fs::write(settings::path(&data), text).unwrap();
            let err = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap_err();
            assert!(err.contains(key), "{err}");
        }
        assert!(store::load_all(&data).is_empty(), "止めたレビューは履歴に残さない");
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    /// 実際に claude -p を呼んで、レシピが、仕込んだ問題を指摘できるかを見る（サブスクを消費する）。
    /// 架空のコードを、一時ディレクトリの git リポジトリに作って送る。通常は走らせない。

    /// `files` は、（ファイル名, コミット済みの内容, 変更後の内容）。

    /// 実物の agy（自分のサブスクを使う）。架空のコードだけを送る。`cargo test live_agy_bugs -- --ignored --nocapture`

    /// `cargo test live_claude_bugs -- --ignored --nocapture`

    /// `cargo test live_claude_errors -- --ignored --nocapture`

    /// `cargo test live_claude_performance -- --ignored --nocapture`

    /// `cargo test live_claude_tests -- --ignored --nocapture`

    /// 関数の引数を変えたが、呼び出し元（差分に出ない）は古いまま、というリポジトリ。
    fn mismatch_repo(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mrcr-e2e-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q", "-b", "main"]);
        std::fs::write(dir.join("lib.js"), "export function computeArea(width, height) {\n  return width * height;\n}\n").unwrap();
        std::fs::write(dir.join("main.js"), "import { computeArea } from './lib.js';\n\nconst a = computeArea(2, 3);\nconsole.log(a);\n").unwrap();
        git(&dir, &["add", "."]);
        git(&dir, &["commit", "-q", "-m", "init"]);
        std::fs::write(dir.join("lib.js"), "export function computeArea(size) {\n  return size * size;\n}\n").unwrap();
        dir
    }

    #[test]
    fn the_consistency_recipe_sends_callers_outside_the_diff_and_records_them() {
        let repo = mismatch_repo("consistency");
        let data = std::env::temp_dir().join(format!("mrcr-e2e-consistency-data-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&data);
        let root = repo.to_str().unwrap();

        let rec = run_review_blocking(&data, root, "consistency", "mock").unwrap();
        // 関連コード（呼び出し元）が、差分の前に入っている
        let related_at = rec.prompt.find("## 関連するコード").expect("関連コードの節がある");
        let diff_at = rec.prompt.rfind("\n## 差分\n\n```diff\n").unwrap();
        assert!(related_at < diff_at, "差分は最後");
        assert!(rec.prompt.contains("const a = computeArea(2, 3);") && rec.prompt.contains("`main.js`"), "{}", rec.prompt);
        assert!(rec.prompt.ends_with("```\n") && rec.prompt.contains("+  return size * size;"));
        // 記録に、何を送ったかが残る
        let r = rec.related.as_ref().expect("記録に関連コードが残る");
        assert!(r.snippets.iter().any(|s| s.file == "main.js" && s.name == "computeArea"));
        assert_eq!(r.names.iter().find(|n| n.name == "computeArea").unwrap().status, "sent");
        // 差分タブが読む差分は、これまでどおり取り出せる
        assert_eq!(rec.snapshot.files.len(), 1);

        // 関連コードを使わない観点には、添えない
        let plain = run_review_blocking(&data, root, "bugs", "mock").unwrap();
        assert!(plain.related.is_none() && !plain.prompt.contains("## 関連するコード"));

        // 設定で、前後の行数を 0 にすると、使用箇所の 1 行だけになる
        std::fs::write(settings::path(&data), r#"{"related_context_lines": 0}"#).unwrap();
        let tight = run_review_blocking(&data, root, "consistency", "mock").unwrap();
        let s = tight.related.as_ref().unwrap().snippets.iter().find(|s| s.file == "main.js" && s.start == 3).expect("3 行目だけ");
        assert_eq!((s.start, s.end), (3, 3));
        assert_eq!(tight.settings.as_ref().unwrap()["related_context_lines"], 0, "使った設定が記録に残る");
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn no_changes_is_an_error_not_an_empty_review() {
        let repo = demo_repo("clean");
        git(&repo, &["checkout", "-q", "--", "."]);
        std::fs::remove_file(repo.join("new.js")).unwrap(); // 未追跡も対象なので、これも消して「変更なし」にする
        let data = std::env::temp_dir().join(format!("mrcr-e2e-data2-{}", std::process::id()));
        let err = run_review_blocking(&data, repo.to_str().unwrap(), "security", "mock").unwrap_err();
        assert!(err.contains("送る差分がありません"));
        assert!(store::load_all(&data).is_empty(), "送っていない実行は履歴に残さない");
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&data);
    }

    #[test]
    fn not_a_git_repo_is_reported() {
        let dir = std::env::temp_dir().join(format!("mrcr-e2e-nogit-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let err = snapshot::take(dir.to_str().unwrap()).err().unwrap();
        assert!(err.contains("git のリポジトリではありません"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_repo_icon_returns_data_url_when_file_exists() {
        let dir = std::env::temp_dir().join(format!("mrcr-test-icon-{}", std::process::id()));
        let icon_dir = dir.join("src-tauri").join("icons");
        std::fs::create_dir_all(&icon_dir).unwrap();
        std::fs::write(icon_dir.join("32x32.png"), b"fakepngdata").unwrap();

        let res = find_repo_icon(dir.to_str().unwrap().to_string());
        assert!(res.is_some());
        let url = res.unwrap();
        assert!(url.starts_with("data:image/png;base64,"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_find_repo_icon_finds_svg_in_art_and_prefers_png() {
        let dir = std::env::temp_dir().join(format!("mrcr-test-icon-svg-{}", std::process::id()));
        let art = dir.join("art");
        std::fs::create_dir_all(&art).unwrap();
        std::fs::write(art.join("icon.svg"), b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>").unwrap();
        let url = find_repo_icon(dir.to_str().unwrap().to_string()).unwrap();
        assert!(url.starts_with("data:image/svg+xml;base64,"));

        // PNG があれば、PNG を先に使う
        std::fs::write(art.join("icon.png"), b"fakepngdata").unwrap();
        let url = find_repo_icon(dir.to_str().unwrap().to_string()).unwrap();
        assert!(url.starts_with("data:image/png;base64,"));

        let _ = std::fs::remove_dir_all(&dir);
    }

}
