use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "mrcr",
    version,
    about = "AI が書いた変更を、コミットする前に潰す。観点特化型コードレビュー CLI"
)]
pub struct Cli {
    /// シェル補完スクリプトを出力する (bash, zsh, fish)
    #[arg(long, value_name = "SHELL")]
    pub completions: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,

    #[command(flatten)]
    pub review: ReviewArgs,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 変更をレビューする（既定コマンド）
    Review(ReviewArgs),
    /// 利用可能な観点（レシピ）の一覧を表示する
    Recipes {
        /// 対象リポジトリのパス
        #[arg(short = 'C', long, default_value = ".")]
        repo: PathBuf,

        /// 表示言語 (ja | en)
        #[arg(short = 'l', long, default_value = "ja")]
        lang: String,
    },
    /// シェル補完スクリプトを出力する
    Completions {
        /// 対象シェル (bash, zsh, fish, powershell)
        shell: String,
    },
}

#[derive(Args, Debug, Clone)]
pub struct ReviewArgs {
    /// レビュー観点（カンマ区切りで複数指定可。例: bugs,security）
    #[arg(short = 'r', long, default_value = "bugs")]
    pub recipe: String,

    /// 組み込みの全観点を一括実行する
    #[arg(long)]
    pub all: bool,

    /// 対象リポジトリのパス
    #[arg(short = 'C', long, default_value = ".")]
    pub repo: PathBuf,

    /// 差分の比較元（HEAD, HEAD~1, ブランチ名など）
    #[arg(short = 'b', long, default_value = "HEAD")]
    pub base: String,

    /// 出力形式 (md | json)
    #[arg(short = 'f', long, default_value = "md", value_parser = ["md", "json"])]
    pub format: String,

    /// 出力先ファイルパス（指定なしの場合は既定で .mrcodereview/out/review.<format> に保存）
    #[arg(short = 'o', long)]
    pub output: Option<PathBuf>,

    /// 結果を標準出力にのみ出力する（ファイルに書き出さない）
    #[arg(long)]
    pub stdout: bool,

    /// レビュー結果をローカル SQLite DB に保存しない（CI / ステートレス実行用）
    #[arg(long)]
    pub no_save: bool,

    /// 使用する LLM モデル名（未指定時はデフォルト: claude-sonnet-5-5）
    #[arg(short = 'm', long)]
    pub model: Option<String>,

    /// 表示・プロンプトの言語 (ja | en)
    #[arg(short = 'l', long, default_value = "ja")]
    pub lang: String,

    /// 試験用: 外部 API を呼ばずにモック応答を使用する
    #[arg(long, hide = true)]
    pub mock: bool,
}
