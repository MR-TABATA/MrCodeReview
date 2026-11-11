//! 設定。アプリの中に固定で埋まっていた値を、ここに集める。
//!
//! - **初期値は、標準値・推奨値。** 何が正解か分からない値ばかりなので、既定のままで困らないものにしてある。
//!   設定ファイルには、変えたい項目だけを書けばよい（書かなかった項目は推奨値）。
//! - **全部の項目に、説明がある。** 何の設定か・なぜその値が推奨か・どんなときに変えるか・有効な範囲（`meta`）。
//!   項目を足して説明を書き忘れると、テストが落ちる。
//! - **不正な値は、黙って直さず、理由つきでエラーにする。**
//! - 設定ファイルは、あなた個人のもの（アプリのデータ領域の `settings.json`）だけを読む。
//!   リポジトリ側の設定は、まだ読まない（リポジトリ側から、実行するコマンドを差し込ませないため）。

use crate::{carry, provider, related, snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const FILE: &str = "settings.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    // 送る差分
    pub max_file_bytes: usize,
    pub max_total_bytes: usize,
    pub max_untracked_files: usize,
    pub ignored_lockfiles: Vec<String>,
    // 引き継ぎ
    pub carry_window: usize,
    pub fuzzy_min: f64,
    pub fuzzy_min_tokens: usize,
    // 実行
    pub llm_timeout_secs: u64,
    /// 廃止（linter を外した）。既存の設定ファイルを読めなくしないために、受け取って無視する（書き出さない）。
    #[serde(skip_serializing)]
    pub tool_timeout_secs: u64,
    #[serde(skip_serializing)]
    pub max_tool_files: usize,
    #[serde(skip_serializing)]
    pub max_raw_bytes: usize,
    // 関連コード（「他ファイルとの不整合」のように、差分に出ていないコードも見る観点で使う）
    pub related_max_bytes: usize,
    pub related_max_names: usize,
    pub related_hits_per_name: usize,
    pub related_context_lines: usize,
    pub related_skip_over_hits: usize,
    // 画面
    pub history_limit: usize,
    pub language: String,
}

impl Default for Settings {
    /// 推奨値。値の出どころは、各モジュールの定数（1 か所だけ）。
    fn default() -> Self {
        Settings {
            max_file_bytes: snapshot::MAX_FILE_BYTES,
            max_total_bytes: snapshot::MAX_TOTAL_BYTES,
            max_untracked_files: snapshot::MAX_UNTRACKED,
            ignored_lockfiles: snapshot::DEFAULT_LOCKFILES.map(String::from).to_vec(),
            carry_window: 30,
            fuzzy_min: carry::FUZZY_MIN,
            fuzzy_min_tokens: carry::FUZZY_MIN_TOKENS,
            llm_timeout_secs: provider::DEFAULT_TIMEOUT.as_secs(),
            tool_timeout_secs: 900,
            max_tool_files: 200,
            max_raw_bytes: 1_000_000,
            related_max_bytes: related::MAX_BYTES,
            related_max_names: related::MAX_NAMES,
            related_hits_per_name: related::HITS_PER_NAME,
            related_context_lines: related::CONTEXT_LINES,
            related_skip_over_hits: related::SKIP_OVER_HITS,
            history_limit: 500,
            language: "auto".into(),
        }
    }
}

impl Settings {
    pub fn snapshot_limits(&self) -> snapshot::Limits {
        snapshot::Limits {
            max_file_bytes: self.max_file_bytes,
            max_total_bytes: self.max_total_bytes,
            max_untracked: self.max_untracked_files,
            lockfiles: self.ignored_lockfiles.clone(),
        }
    }

    pub fn related_limits(&self) -> related::Limits {
        related::Limits {
            max_bytes: self.related_max_bytes,
            max_names: self.related_max_names,
            hits_per_name: self.related_hits_per_name,
            context_lines: self.related_context_lines,
            skip_over_hits: self.related_skip_over_hits,
            lockfiles: self.ignored_lockfiles.clone(),
        }
    }

    pub fn llm_timeout(&self) -> Duration {
        Duration::from_secs(self.llm_timeout_secs)
    }

    /// 各レビューの記録に残す、そのとき使った設定（あとで「なぜ違ったか」を説明できるように）。
    /// 型を持たない JSON で残す。将来、項目が増減しても、古い記録が読めなくならない。
    pub fn to_record(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

/// 画面に出す、設定項目 1 つぶんの説明。
#[derive(Debug, Clone, Serialize)]
pub struct Meta {
    pub key: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    /// "integer" | "number" | "list"
    pub kind: &'static str,
    pub unit: &'static str,
    /// 何の設定か
    pub description: &'static str,
    /// なぜその値が推奨か
    pub why_recommended: &'static str,
    /// どんなときに変えるか
    pub when_to_change: &'static str,
    pub recommended: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// リポジトリ側の設定（`.mrcodereview/settings.json`）でも上書きできるか。
    /// できるのは、そのリポジトリのレビューに関わる項目だけ。待ち時間や履歴の件数など、
    /// マシンやあなたの好みに関わる項目は、あなた個人の設定だけ。
    pub repo_scope: bool,
}

pub fn meta() -> Vec<Meta> {
    let d = Settings::default();
    vec![
        Meta {
            key: "max_file_bytes",
            group: "送る差分",
            label: "1 ファイルの差分の上限",
            kind: "integer",
            unit: "バイト",
            description: "1 ファイルの差分がこれを超えたら、送らずに「送らないもの」に出します。",
            why_recommended: "およそ 1,500 行ぶんの差分です。これより大きいのは、生成ファイルや一括置換であることが多く、費用と待ち時間が膨らむ割に、指摘の質は上がりにくいためです。",
            when_to_change: "大きなファイルもまとめて見たいときだけ上げます。上げるほど、LLM の費用と待ち時間が増えます。",
            recommended: json!(d.max_file_bytes),
            min: Some(1_000.0),
            max: Some(5_000_000.0),
            repo_scope: true,
        },
        Meta {
            key: "max_total_bytes",
            group: "送る差分",
            label: "送る差分の合計の上限",
            kind: "integer",
            unit: "バイト",
            description: "送る差分の合計がこれを超えたら、後ろのファイルから送りません。",
            why_recommended: "4 バイトでおよそ 1 トークンとして、約 50,000 トークンです。1 回の費用が膨らみすぎず、LLM が全体を見渡せる大きさです。",
            when_to_change: "一度に大きな変更をまとめてレビューしたいときに上げます。普通は、変更を分けて何度か押すほうが、質も費用も良くなります。",
            recommended: json!(d.max_total_bytes),
            min: Some(1_000.0),
            max: Some(20_000_000.0),
            repo_scope: true,
        },
        Meta {
            key: "max_untracked_files",
            group: "送る差分",
            label: "未追跡の新規ファイルの取り込み件数",
            kind: "integer",
            unit: "件",
            description: "まだ git に追加していない新規ファイルを、差分として取り込む件数の上限です。超えた分は 1 行にまとめて「送らないもの」に出します。",
            why_recommended: "ビルドの成果物などを .gitignore に入れ忘れたとき、数千件を読み込んで遅くなるのを防ぎます。普通の作業で、新規ファイルが 200 件を超えることはまずありません。",
            when_to_change: "多数の新規ファイルを一度に足す作業のときに上げます。先に .gitignore を直せないか確認してください。",
            recommended: json!(d.max_untracked_files),
            min: Some(0.0),
            max: Some(5_000.0),
            repo_scope: true,
        },
        Meta {
            key: "ignored_lockfiles",
            group: "送る差分",
            label: "送らないファイル名",
            kind: "list",
            unit: "",
            description: "このファイル名の差分は、送らずに「送らないもの」に出します（ロックファイルなど）。",
            why_recommended: "ロックファイルは機械が作る巨大な差分で、レビューで見る価値がほとんどありません。",
            when_to_change: "ほかの生成ファイル（例: bun.lockb）を除きたいときに足します。ファイル名だけを書きます（パスは書きません）。",
            recommended: json!(d.ignored_lockfiles),
            min: None,
            max: None,
            repo_scope: true,
        },
        Meta {
            key: "carry_window",
            group: "仕分けの引き継ぎ",
            label: "遡って引き継ぐレビューの数",
            kind: "integer",
            unit: "回",
            description: "同じリポジトリ・同じ観点の、何回前のレビューまで遡って、仕分けを引き継ぐかです。",
            why_recommended: "AI がたまたま 1 回出さなかった指摘の仕分けを失わない程度に遡り、古すぎる判断は引きずらない数です。",
            when_to_change: "1 日に何度も押すなら増やします。仕分けを引き継ぎたくないときは 1 にします。",
            recommended: json!(d.carry_window),
            min: Some(1.0),
            max: Some(500.0),
            repo_scope: true,
        },
        Meta {
            key: "fuzzy_min",
            group: "仕分けの引き継ぎ",
            label: "「ほぼ同じ引用」の一致率",
            kind: "number",
            unit: "",
            description: "引用した行が少し変わっていても、同じ指摘と見なす一致率の下限です。「待ち」と「無視」にだけ使います（「対応」は、完全に同じ引用のときだけ引き継ぎます）。",
            why_recommended: "0.9 は、1 語だけ変わった程度を同じと見なし、直した行（記号が変わる）は同じと見なさない値です。テストで確かめてあります。",
            when_to_change: "下げると、別の指摘を同じものと取り違えやすくなります。上げると、軽い書き換えで仕分けが外れます。1 にすると、完全に同じ引用のときだけ引き継ぎます。",
            recommended: json!(d.fuzzy_min),
            min: Some(0.5),
            max: Some(1.0),
            repo_scope: true,
        },
        Meta {
            key: "fuzzy_min_tokens",
            group: "仕分けの引き継ぎ",
            label: "「ほぼ同じ」を判定する最小の長さ",
            kind: "integer",
            unit: "トークン",
            description: "引用がこれより短いときは、「ほぼ同じ」の判定をせず、完全に同じときだけ引き継ぎます。",
            why_recommended: "`x = 1` のような短い行は、1 文字違うだけで別の意味になります。短い引用では、ほぼ一致の判定をしないほうが安全です。",
            when_to_change: "短い引用でも柔軟に引き継ぎたいときに下げます。",
            recommended: json!(d.fuzzy_min_tokens),
            min: Some(1.0),
            max: Some(50.0),
            repo_scope: true,
        },
        Meta {
            key: "llm_timeout_secs",
            group: "実行",
            label: "LLM の返事を待つ時間の上限",
            kind: "integer",
            unit: "秒",
            description: "LLM の返事が、この時間を過ぎても返らなければ、中断してエラーにします。",
            why_recommended: "大きな差分では、数十秒から数分かかることがあります。5 分は、通常は十分で、固まったときに延々と待たない長さです。",
            when_to_change: "大きな差分を、考える時間の長いモデルで見るときに上げます。",
            recommended: json!(d.llm_timeout_secs),
            min: Some(10.0),
            max: Some(3_600.0),
            repo_scope: false,
        },
        Meta {
            key: "related_max_bytes",
            group: "関連コード",
            label: "関連コードの合計の上限",
            kind: "integer",
            unit: "バイト",
            description: "「他ファイルとの不整合」のように、差分に出ていないコードも見る観点で、差分に添える関連コードの合計の大きさです。超えたら、後ろの名前から送りません。",
            why_recommended: "差分の上限（200,000 バイト）の 3 分の 1 以下に収め、差分そのものを読む邪魔にならない大きさです。呼び出し元を確かめるには、数十か所の前後数行で足ります。",
            when_to_change: "大きなリポジトリで、使用箇所が多い名前を、もっと広く確かめたいときに上げます。上げるほど、費用と待ち時間が増えます。",
            recommended: json!(d.related_max_bytes),
            min: Some(1_000.0),
            max: Some(2_000_000.0),
            repo_scope: true,
        },
        Meta {
            key: "related_max_names",
            group: "関連コード",
            label: "調べる名前の数",
            kind: "integer",
            unit: "個",
            description: "差分で変更した名前（関数・型・定数・設定のキーなど）のうち、使用箇所を調べる名前の数の上限です。削除・変更された名前、定義らしい行にある名前の順に選びます。",
            why_recommended: "変更の中心になる名前は、多くても十数個です。それより多く調べても、ありふれた名前のノイズが増えるだけです。",
            when_to_change: "大きな変更で、多くの名前が絡むときに上げます。",
            recommended: json!(d.related_max_names),
            min: Some(1.0),
            max: Some(100.0),
            repo_scope: true,
        },
        Meta {
            key: "related_hits_per_name",
            group: "関連コード",
            label: "1 つの名前あたりの使用箇所の数",
            kind: "integer",
            unit: "か所",
            description: "1 つの名前について、差分に出ていない使用箇所を、いくつまで送るかの上限です。",
            why_recommended: "呼び出し元が食い違っているかは、数か所を見れば分かります。全部を送ると、同じ種類の箇所の繰り返しで、費用だけが増えます。",
            when_to_change: "使用箇所が多い名前を、もれなく確かめたいときに上げます。",
            recommended: json!(d.related_hits_per_name),
            min: Some(1.0),
            max: Some(100.0),
            repo_scope: true,
        },
        Meta {
            key: "related_context_lines",
            group: "関連コード",
            label: "使用箇所の前後の行数",
            kind: "integer",
            unit: "行",
            description: "関連コードの使用箇所を送るとき、その行の前後を、何行ずつ添えるかです。",
            why_recommended: "呼び出しの引数や、直前の代入が見える長さです。短すぎると文脈が分からず、長すぎると、無関係なコードが増えます。",
            when_to_change: "関数の呼び出しが複数行にまたがるコードベースでは、増やします。",
            recommended: json!(d.related_context_lines),
            min: Some(0.0),
            max: Some(30.0),
            repo_scope: true,
        },
        Meta {
            key: "related_skip_over_hits",
            group: "関連コード",
            label: "「ありふれた名前」と見なす使用箇所の数",
            kind: "integer",
            unit: "か所",
            description: "使用箇所が、これより多い名前は、ありふれていて手がかりにならないと見なして、調べません（除いた名前は、実行メモに残ります）。",
            why_recommended: "`name` や `value` のような名前は、リポジトリ中に何百か所もあり、どこを見ても、食い違いの手がかりにはなりません。",
            when_to_change: "大きなリポジトリで、重要な名前まで除かれてしまうときに上げます。",
            recommended: json!(d.related_skip_over_hits),
            min: Some(5.0),
            max: Some(5_000.0),
            repo_scope: true,
        },
        Meta {
            key: "history_limit",
            group: "画面",
            label: "履歴の一覧に出す件数",
            kind: "integer",
            unit: "件",
            description: "左の履歴の一覧に出す、レビューの件数です。古いものは、一覧には出ませんが、消えません。",
            why_recommended: "一覧は、記録の全文を読まずに作るので、数百件でも遅くなりません。",
            when_to_change: "古いレビューまで一覧で見たいときに上げます。",
            recommended: json!(d.history_limit),
            min: Some(10.0),
            max: Some(10_000.0),
            repo_scope: false,
        },
        Meta {
            key: "language",
            group: "画面",
            label: "言語",
            kind: "select",
            unit: "",
            description: "画面の表示言語と、レビューに使う組み込みプロンプトの言語（auto はシステムの設定に合わせる）。",
            why_recommended: "「自動」にしておくと、お使いの環境（OS）の言語に自然に揃います。",
            when_to_change: "画面やレビュー結果を英語で使いたいときや、日本語に固定したいとき。",
            recommended: json!(d.language),
            min: None,
            max: None,
            repo_scope: false,
        },
    ]
}

pub fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

fn describe(m: &Meta) -> String {
    match (m.min, m.max) {
        (Some(lo), Some(hi)) => format!("{lo}〜{hi}{}", m.unit),
        _ => "有効な値".to_string(),
    }
}

/// 値の範囲を確かめる。違反は、項目名と有効な範囲と推奨値を添えて返す。
pub fn validate(s: &Settings) -> Result<(), String> {
    let v = s.to_record();
    for m in meta() {
        let Some(value) = v.get(m.key) else { continue };
        if let (Some(n), Some(lo), Some(hi)) = (value.as_f64(), m.min, m.max) {
            if n < lo || n > hi {
                return Err(format!(
                    "設定 `{}`（{}）が範囲外です: {n}。{} の範囲で指定してください（推奨: {}）",
                    m.key,
                    m.label,
                    describe(&m),
                    m.recommended
                ));
            }
        }
    }
    if s.max_total_bytes < s.max_file_bytes {
        return Err(format!(
            "設定 `max_total_bytes`（{}）は、`max_file_bytes`（{}）以上にしてください",
            s.max_total_bytes, s.max_file_bytes
        ));
    }
    for name in &s.ignored_lockfiles {
        if name.trim().is_empty() || name.contains('/') {
            return Err(format!("設定 `ignored_lockfiles` には、ファイル名だけを書いてください（パスは不可）: {name:?}"));
        }
    }
    if !crate::i18n::Lang::is_valid_setting(&s.language) {
        return Err(format!("設定 `language` は {} のいずれかを指定してください: {:?}", crate::i18n::Lang::valid_setting_list(), s.language));
    }
    Ok(())
}

/// リポジトリ側の設定のディレクトリ名と、ファイル名。
pub const REPO_DIR: &str = ".mrcodereview";

pub fn repo_path(repo_root: &Path) -> PathBuf {
    repo_root.join(REPO_DIR).join(FILE)
}

/// 実際に使う設定（推奨値 → あなた個人の設定 → リポジトリ側の設定）と、リポジトリ側で上書きした項目。
#[derive(Debug, Clone)]
pub struct Effective {
    pub settings: Settings,
    /// リポジトリ側の設定で上書きした項目（`meta()` の並び順）。
    pub repo_keys: Vec<String>,
    /// リポジトリ側の設定ファイル（あれば）。
    pub repo_file: Option<PathBuf>,
}

/// 推奨値 → あなた個人の設定 → リポジトリ側の設定、の順に重ねて、実際に使う設定にする。
///
/// リポジトリ側で上書きできるのは、**そのリポジトリのレビューに関わる項目**（`repo_scope`）だけ。
/// それ以外の項目や、知らない項目は、黙って無視せず、理由つきのエラーにする。
pub fn load_effective(dir: &Path, repo_root: Option<&Path>) -> Result<Effective, String> {
    let user = load(dir)?;
    let none = || Effective { settings: user.clone(), repo_keys: Vec::new(), repo_file: None };
    let Some(root) = repo_root else { return Ok(none()) };
    let file = repo_path(root);
    if !file.exists() {
        return Ok(none());
    }
    let text = std::fs::read_to_string(&file).map_err(|e| format!("リポジトリ側の設定を読めない（{}）: {e}", file.display()))?;
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("リポジトリ側の設定 {} が正しくありません: {e}", file.display()))?;
    let Value::Object(overrides) = v else {
        return Err(format!("リポジトリ側の設定 {} は、{{ \"項目\": 値 }} の形で書いてください", file.display()));
    };
    let metas = meta();
    for k in overrides.keys() {
        match metas.iter().find(|m| m.key == k) {
            None => return Err(format!("リポジトリ側の設定 {} に、知らない項目 `{k}` があります", file.display())),
            Some(m) if !m.repo_scope => {
                return Err(format!(
                    "リポジトリ側の設定 {} では、`{k}`（{}）は変えられません。この項目は、あなた個人の設定だけで変えられます",
                    file.display(),
                    m.label
                ))
            }
            Some(_) => {}
        }
    }
    let mut merged = user.to_record();
    let obj = merged.as_object_mut().expect("設定は、オブジェクト");
    for (k, val) in &overrides {
        obj.insert(k.clone(), val.clone());
    }
    let settings: Settings = serde_json::from_value(merged).map_err(|e| format!("リポジトリ側の設定 {} の値が正しくありません: {e}", file.display()))?;
    let repo_keys: Vec<String> = metas.iter().filter(|m| overrides.contains_key(m.key)).map(|m| m.key.to_string()).collect();
    // あなた個人の設定は、単独では正しい（読み込みで確かめ済み）。ここで落ちるのは、リポジトリ側の値との組み合わせ
    validate(&settings).map_err(|e| {
        format!(
            "{e}（原因: リポジトリ側の設定 {} の {} と、あなた個人の設定との組み合わせです。どちらかを直してください）",
            file.display(),
            repo_keys.iter().map(|k| format!("`{k}`")).collect::<Vec<_>>().join("・")
        )
    })?;
    Ok(Effective { settings, repo_keys, repo_file: Some(file) })
}

/// 設定ファイルを読む。無ければ推奨値。書かれていない項目は推奨値のまま。
pub fn load(dir: &Path) -> Result<Settings, String> {
    let p = path(dir);
    if !p.exists() {
        return Ok(Settings::default());
    }
    let text = std::fs::read_to_string(&p).map_err(|e| format!("設定ファイルを読めない（{}）: {e}", p.display()))?;
    let s: Settings = serde_json::from_str(&text).map_err(|e| format!("設定ファイル {} が正しくありません: {e}", p.display()))?;
    validate(&s).map_err(|e| format!("{e}（ファイル: {}）", p.display()))?;
    Ok(s)
}

/// 画面から受け取った値を検証して、設定ファイルに保存する。
///
/// - 書くのは、**推奨値と違う項目だけ**。将来、推奨値を見直したとき、変えていない項目に自然に届く。
/// - 検証に通らなければ、何も書かない（理由つきでエラー）。
/// - いまのファイルが正しくないとき（綴りの間違いなど）は、上書きする前に `settings.json.bak` に残す。
pub fn save(dir: &Path, values: &Value) -> Result<(), String> {
    let s: Settings = serde_json::from_value(values.clone()).map_err(|e| format!("設定の値が正しくありません: {e}"))?;
    validate(&s)?;

    let defaults = Settings::default().to_record();
    let mine = s.to_record();
    let overrides: serde_json::Map<String, Value> = meta()
        .iter()
        .filter(|m| mine[m.key] != defaults[m.key])
        .map(|m| (m.key.to_string(), mine[m.key].clone()))
        .collect();
    let text = serde_json::to_string_pretty(&Value::Object(overrides)).map_err(|e| e.to_string())? + "\n";

    std::fs::create_dir_all(dir).map_err(|e| format!("保存先を作れない: {e}"))?;
    let file = path(dir);
    if file.exists() && load(dir).is_err() {
        // 壊れた設定の退避。すでに退避があれば、上書きせず、別の名前にする（前に壊れていたものを、失わない）
        let bak = free_backup_path(dir)?;
        std::fs::copy(&file, &bak).map_err(|e| format!("元のファイルを {} に残せない: {e}", bak.display()))?;
    }
    // 書きかけで壊さないよう、一時ファイルに書いてから置き換える
    replace_file(&dir.join(format!("{FILE}.tmp")), &file, &text)
}

/// 退避の置き場。`settings.json.bak` が空いていればそれ、あれば `.bak.1`、`.bak.2`…（先に退避したものを、上書きしない）。
fn free_backup_path(dir: &Path) -> Result<PathBuf, String> {
    let first = dir.join(format!("{FILE}.bak"));
    if !first.exists() {
        return Ok(first);
    }
    (1..1000)
        .map(|n| dir.join(format!("{FILE}.bak.{n}")))
        .find(|p| !p.exists())
        .ok_or_else(|| "元のファイルを残す場所（.bak）が、いっぱいです".to_string())
}

/// 一時ファイルに書いて、`file` に置き換える。どちらかが失敗したら、一時ファイルを残さない（消せなければ、それも伝える）。
fn replace_file(tmp: &Path, file: &Path, text: &str) -> Result<(), String> {
    let result = std::fs::write(tmp, text)
        .map_err(|e| format!("設定を書けない: {e}"))
        .and_then(|()| std::fs::rename(tmp, file).map_err(|e| format!("設定を保存できない: {e}")));
    if let Err(e) = result {
        let leftover = match std::fs::remove_file(tmp) {
            Ok(()) => String::new(),
            Err(r) if r.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(r) => format!("（一時ファイル {} も消せませんでした: {r}）", tmp.display()),
        };
        return Err(format!("{e}{leftover}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("mrcr-settings-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(dir: &Path, text: &str) {
        std::fs::write(path(dir), text).unwrap();
    }

    #[test]
    fn every_setting_has_a_complete_explanation_and_the_recommended_value_is_the_default() {
        let defaults = Settings::default().to_record();
        let keys: Vec<&str> = defaults.as_object().unwrap().keys().map(String::as_str).collect();
        let metas = meta();
        // 項目を足したのに説明を書き忘れたら、ここで落ちる（「全部設定できる」を崩さない）
        for k in &keys {
            assert!(metas.iter().any(|m| m.key == *k), "設定 `{k}` の説明（meta）が無い");
        }
        for m in &metas {
            assert!(keys.contains(&m.key), "meta `{}` に対応する設定項目が無い", m.key);
            for (name, text) in [("label", m.label), ("description", m.description), ("why_recommended", m.why_recommended), ("when_to_change", m.when_to_change), ("group", m.group)] {
                assert!(!text.trim().is_empty(), "meta `{}` の {name} が空", m.key);
            }
            assert_eq!(m.recommended, defaults[m.key], "meta `{}` の推奨値が、既定の値と違う", m.key);
            if m.kind != "list" && m.kind != "select" {
                assert!(m.min.is_some() && m.max.is_some(), "数値の `{}` には範囲が要る", m.key);
                let (lo, hi) = (m.min.unwrap(), m.max.unwrap());
                let rec = m.recommended.as_f64().unwrap();
                assert!(lo <= rec && rec <= hi, "推奨値が範囲の外: {}", m.key);
            }
        }
        assert_eq!(metas.len(), keys.len(), "meta の重複・不足");
        assert!(validate(&Settings::default()).is_ok(), "推奨値は、検証を通る");
    }

    #[test]
    fn recommended_values_are_the_ones_the_modules_were_tested_with() {
        let d = Settings::default();
        assert_eq!((d.max_file_bytes, d.max_total_bytes, d.max_untracked_files), (60_000, 200_000, 200));
        assert_eq!((d.carry_window, d.fuzzy_min, d.fuzzy_min_tokens), (30, 0.9, 4));
        assert_eq!((d.llm_timeout_secs, d.tool_timeout_secs, d.max_tool_files, d.max_raw_bytes, d.history_limit), (300, 900, 200, 1_000_000, 500));
        assert_eq!(d.language, "auto");
        assert!(d.ignored_lockfiles.contains(&"package-lock.json".to_string()) && d.ignored_lockfiles.contains(&"Cargo.lock".to_string()));
        // 既定の設定から作った上限は、各モジュールの既定と同じ
        let l = d.snapshot_limits();
        assert_eq!((l.max_file_bytes, l.max_total_bytes, l.max_untracked), (60_000, 200_000, 200));
        assert_eq!(d.llm_timeout(), Duration::from_secs(300));
    }

    #[test]
    fn missing_file_means_recommended_and_partial_files_keep_the_rest() {
        let d = scratch("partial");
        assert_eq!(load(&d).unwrap(), Settings::default());
        write(&d, r#"{"max_file_bytes": 120000, "history_limit": 50}"#);
        let s = load(&d).unwrap();
        assert_eq!((s.max_file_bytes, s.history_limit), (120_000, 50));
        assert_eq!(s.max_total_bytes, Settings::default().max_total_bytes, "書かなかった項目は推奨値");
        write(&d, "{}");
        assert_eq!(load(&d).unwrap(), Settings::default());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn bad_files_are_errors_with_the_reason_never_silent_fallbacks() {
        let d = scratch("bad");
        write(&d, r#"{"max_file_byts": 1}"#);
        let e = load(&d).unwrap_err();
        assert!(e.contains("max_file_byts") && e.contains("settings.json"), "綴りの間違いを黙って無視しない: {e}");
        write(&d, "{broken");
        assert!(load(&d).unwrap_err().contains("正しくありません"));
        write(&d, r#"{"max_file_bytes": "多い"}"#);
        assert!(load(&d).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn out_of_range_values_name_the_key_the_range_and_the_recommendation() {
        let d = scratch("range");
        write(&d, r#"{"fuzzy_min": 0.1}"#);
        let e = load(&d).unwrap_err();
        assert!(e.contains("fuzzy_min") && e.contains("0.5〜1 の範囲") && e.contains("推奨: 0.9"), "範囲の表記が崩れていない: {e}");
        write(&d, r#"{"carry_window": 0}"#);
        assert!(load(&d).unwrap_err().contains("carry_window"));
        write(&d, r#"{"llm_timeout_secs": 99999}"#);
        assert!(load(&d).unwrap_err().contains("llm_timeout_secs"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn language_accepts_auto_ja_en_and_rejects_unknown_values() {
        for valid in ["auto", "ja", "en"] {
            let mut s = Settings::default();
            s.language = valid.to_string();
            assert!(validate(&s).is_ok(), "language={valid} は許可されるべき");
        }
        for invalid in ["fr", "zh", ""] {
            let mut s = Settings::default();
            s.language = invalid.to_string();
            let err = validate(&s).unwrap_err();
            assert!(err.contains("language"), "language={invalid:?} のエラーに項目名が入る: {err}");
        }
        let d = scratch("lang-validate");
        write(&d, r#"{"language": "fr"}"#);
        assert!(load(&d).unwrap_err().contains("language"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_writes_only_values_that_differ_from_the_recommended_ones() {
        let d = scratch("save");
        let mut v = Settings::default().to_record();
        v["max_file_bytes"] = json!(120_000);
        v["ignored_lockfiles"] = json!(["bun.lockb"]);
        save(&d, &v).unwrap();
        let written: Value = serde_json::from_str(&std::fs::read_to_string(path(&d)).unwrap()).unwrap();
        assert_eq!(written, json!({"max_file_bytes": 120_000, "ignored_lockfiles": ["bun.lockb"]}));
        // 読み戻すと、同じ値
        let s = load(&d).unwrap();
        assert_eq!((s.max_file_bytes, s.ignored_lockfiles.clone()), (120_000, vec!["bun.lockb".to_string()]));
        assert_eq!(s.history_limit, 500);

        // すべて推奨値に戻すと、何も書かない（空のオブジェクト）
        save(&d, &Settings::default().to_record()).unwrap();
        assert_eq!(std::fs::read_to_string(path(&d)).unwrap().trim(), "{}");
        assert_eq!(load(&d).unwrap(), Settings::default());
        assert!(!d.join("settings.json.tmp").exists(), "一時ファイルを残さない");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_rejects_bad_values_and_writes_nothing() {
        let d = scratch("save-bad");
        let mut v = Settings::default().to_record();
        v["fuzzy_min"] = json!(0.1);
        let e = save(&d, &v).unwrap_err();
        assert!(e.contains("fuzzy_min") && e.contains("推奨: 0.9"), "{e}");
        assert!(!path(&d).exists(), "検証に通らなければ、ファイルを作らない");

        let mut v = Settings::default().to_record();
        v["no_such_setting"] = json!(1);
        assert!(save(&d, &v).unwrap_err().contains("no_such_setting"));
        let mut v = Settings::default().to_record();
        v["carry_window"] = json!("たくさん");
        assert!(save(&d, &v).is_err());
        let mut v = Settings::default().to_record();
        v["max_total_bytes"] = json!(2_000);
        assert!(save(&d, &v).unwrap_err().contains("max_total_bytes"), "合計は 1 ファイルの上限以上");
        assert!(!path(&d).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_second_broken_file_does_not_overwrite_the_first_backup() {
        let d = scratch("bak-twice");
        write(&d, r#"{"max_file_byts": 5}"#);
        save(&d, &Settings::default().to_record()).unwrap();
        write(&d, r#"{"max_total_byts": 9}"#);
        save(&d, &Settings::default().to_record()).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("settings.json.bak")).unwrap(), r#"{"max_file_byts": 5}"#, "最初の退避は、そのまま");
        assert_eq!(std::fs::read_to_string(d.join("settings.json.bak.1")).unwrap(), r#"{"max_total_byts": 9}"#, "次の壊れたファイルは、別の名前で残る");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn a_failed_replace_leaves_no_temp_file_and_the_original_untouched() {
        let d = scratch("replace-fail");
        let file = d.join("settings.json");
        let tmp = d.join("settings.json.tmp");
        // 置き換え先が「中身のあるフォルダ」だと、rename できない
        std::fs::create_dir_all(file.join("inner")).unwrap();
        let e = replace_file(&tmp, &file, "{}").unwrap_err();
        assert!(e.contains("設定を保存できない"), "{e}");
        assert!(!tmp.exists(), "書きかけの一時ファイルを残さない");
        // 書き込みの失敗（一時ファイルの場所がフォルダ）も、理由を言う
        std::fs::create_dir_all(tmp.join("inner")).unwrap();
        let e = replace_file(&tmp, &d.join("other.json"), "{}").unwrap_err();
        assert!(e.contains("設定を書けない"), "{e}");
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn save_over_a_broken_file_keeps_the_original_as_a_backup() {
        let d = scratch("save-bak");
        write(&d, r#"{"max_file_byts": 5}"#); // 綴りの間違い
        assert!(load(&d).is_err());
        let mut v = Settings::default().to_record();
        v["history_limit"] = json!(50);
        save(&d, &v).unwrap();
        assert_eq!(std::fs::read_to_string(d.join("settings.json.bak")).unwrap(), r#"{"max_file_byts": 5}"#);
        assert_eq!(load(&d).unwrap().history_limit, 50);

        // 正しいファイルを上書きするときは、バックアップを作り直さない
        std::fs::remove_file(d.join("settings.json.bak")).unwrap();
        save(&d, &v).unwrap();
        assert!(!d.join("settings.json.bak").exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    fn repo_with(tag: &str, settings_json: Option<&str>) -> PathBuf {
        let r = scratch(&format!("repo-{tag}"));
        if let Some(text) = settings_json {
            std::fs::create_dir_all(r.join(REPO_DIR)).unwrap();
            std::fs::write(repo_path(&r), text).unwrap();
        }
        r
    }

    #[test]
    fn a_repo_value_that_only_fails_in_combination_names_the_repo_file_and_key() {
        let user = scratch("combo-user");
        write(&user, r#"{"max_file_bytes": 100000}"#);
        let repo = repo_with("combo", Some(r#"{"max_total_bytes": 50000}"#));
        let e = load_effective(&user, Some(&repo)).unwrap_err();
        assert!(e.contains("max_total_bytes") && e.contains("組み合わせ") && e.contains(&repo_path(&repo).display().to_string()), "どちらの設定のどの項目かを言う: {e}");
        assert!(e.contains("あなた個人の設定"), "{e}");
        let _ = std::fs::remove_dir_all(&user);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn repo_settings_layer_over_user_settings_over_recommended() {
        let user = scratch("layer-user");
        write(&user, r#"{"max_file_bytes": 100000, "history_limit": 50, "carry_window": 10}"#);
        let repo = repo_with("layer", Some(r#"{"carry_window": 5, "related_context_lines": 1, "ignored_lockfiles": ["x.lock"]}"#));
        let e = load_effective(&user, Some(&repo)).unwrap();
        let s = &e.settings;
        assert_eq!(s.carry_window, 5, "リポジトリ側が、個人の設定（10）より優先");
        assert_eq!(s.max_file_bytes, 100_000, "リポジトリ側に無い項目は、個人の設定");
        assert_eq!(s.history_limit, 50);
        assert_eq!(s.related_context_lines, 1);
        assert_eq!(s.max_total_bytes, Settings::default().max_total_bytes, "どちらにも無い項目は、推奨値");
        assert_eq!(s.ignored_lockfiles, vec!["x.lock"]);
        assert_eq!(e.repo_keys, vec!["ignored_lockfiles", "carry_window", "related_context_lines"], "上書きした項目（meta の並び順）");
        assert_eq!(e.repo_file.as_deref(), Some(repo_path(&repo).as_path()));
        let _ = std::fs::remove_dir_all(&user);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn without_a_repo_file_the_user_settings_are_used_as_they_are() {
        let user = scratch("nofile-user");
        let repo = repo_with("nofile", None);
        let e = load_effective(&user, Some(&repo)).unwrap();
        assert_eq!(e.settings, Settings::default());
        assert!(e.repo_keys.is_empty() && e.repo_file.is_none());
        assert!(load_effective(&user, None).unwrap().repo_file.is_none());
        let _ = std::fs::remove_dir_all(&user);
        let _ = std::fs::remove_dir_all(&repo);
    }

    #[test]
    fn repo_files_cannot_change_machine_level_settings_or_use_unknown_keys() {
        let user = scratch("deny-user");
        for key in ["llm_timeout_secs", "history_limit"] {
            let repo = repo_with(&format!("deny-{key}"), Some(&format!(r#"{{"{key}": 60}}"#)));
            let e = load_effective(&user, Some(&repo)).unwrap_err();
            assert!(e.contains(key) && e.contains("変えられません") && e.contains("settings.json"), "{key}: {e}");
            let _ = std::fs::remove_dir_all(&repo);
        }
        let repo = repo_with("unknown", Some(r#"{"max_file_byts": 1}"#));
        let e = load_effective(&user, Some(&repo)).unwrap_err();
        assert!(e.contains("max_file_byts") && e.contains("知らない項目"), "{e}");
        // コマンドを実行する設定（linter の定義など）は、この仕組みでは受け取らない
        let repo2 = repo_with("linters", Some(r#"{"linters": []}"#));
        assert!(load_effective(&user, Some(&repo2)).unwrap_err().contains("知らない項目 `linters`"));
        let _ = std::fs::remove_dir_all(&user);
        let _ = std::fs::remove_dir_all(&repo);
        let _ = std::fs::remove_dir_all(&repo2);
    }

    #[test]
    fn repo_settings_are_validated_like_user_settings() {
        let user = scratch("val-user");
        let bad = repo_with("val-range", Some(r#"{"fuzzy_min": 0.1}"#));
        let e = load_effective(&user, Some(&bad)).unwrap_err();
        assert!(e.contains("fuzzy_min") && e.contains("推奨: 0.9") && e.contains("リポジトリ側の設定"), "{e}");
        let broken = repo_with("val-broken", Some("{broken"));
        assert!(load_effective(&user, Some(&broken)).unwrap_err().contains("正しくありません"));
        let notobj = repo_with("val-array", Some("[1]"));
        assert!(load_effective(&user, Some(&notobj)).unwrap_err().contains("の形で書いてください"));
        // 個人の設定と合わせて、項目をまたぐ規則も見る（合計 < 1 ファイル）
        write(&user, r#"{"max_total_bytes": 50000}"#);
        let cross = repo_with("val-cross", Some(r#"{"max_file_bytes": 90000}"#));
        assert!(load_effective(&user, Some(&cross)).unwrap_err().contains("max_total_bytes"));
        // 個人の設定が壊れていれば、そちらの理由で止まる
        write(&user, "{broken");
        assert!(load_effective(&user, Some(&repo_with("val-ok", Some("{}")))).unwrap_err().contains("settings.json"));
        for d in [&user, &bad, &broken, &notobj, &cross] {
            let _ = std::fs::remove_dir_all(d);
        }
    }

    #[test]
    fn every_setting_says_whether_the_repo_may_change_it() {
        let m = meta();
        let repo_ok: Vec<&str> = m.iter().filter(|x| x.repo_scope).map(|x| x.key).collect();
        let user_only: Vec<&str> = m.iter().filter(|x| !x.repo_scope).map(|x| x.key).collect();
        assert_eq!(user_only, vec!["llm_timeout_secs", "history_limit", "language"]);
        assert!(repo_ok.contains(&"max_file_bytes") && repo_ok.contains(&"carry_window") && repo_ok.contains(&"related_max_bytes"));
        assert_eq!(repo_ok.len() + user_only.len(), m.len());
    }

    #[test]
    fn cross_field_and_list_checks() {
        let d = scratch("cross");
        write(&d, r#"{"max_file_bytes": 500000, "max_total_bytes": 100000}"#);
        assert!(load(&d).unwrap_err().contains("max_total_bytes"));
        write(&d, r#"{"ignored_lockfiles": ["a/b.lock"]}"#);
        assert!(load(&d).unwrap_err().contains("ファイル名だけ"));
        write(&d, r#"{"ignored_lockfiles": [" "]}"#);
        assert!(load(&d).is_err());
        write(&d, r#"{"ignored_lockfiles": ["bun.lockb"]}"#);
        assert_eq!(load(&d).unwrap().ignored_lockfiles, vec!["bun.lockb"], "空にも差し替えにもできる");
        let _ = std::fs::remove_dir_all(&d);
    }
}
