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

    /// 全観点を一括実行する（組み込み＋リポジトリの .mrcodereview/recipes/ の独自レシピ）
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

    /// LLM の返事を待つ時間の上限（秒。10〜3600）。大きな差分や、考える時間の長いモデルでは延ばす
    #[arg(long, default_value_t = 300, value_parser = clap::value_parser!(u64).range(10..=3600))]
    pub timeout: u64,

    /// 試験用: 外部 API を呼ばずにモック応答を使用する
    #[arg(long, hide = true)]
    pub mock: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `mrcr review --timeout N` と、サブコマンドなしの `mrcr --timeout N` の、どちらの入口でも同じ値になる。
    fn timeout_of(args: &[&str]) -> Result<u64, clap::Error> {
        let mut argv = vec!["mrcr"];
        argv.extend_from_slice(args);
        let cli = Cli::try_parse_from(argv)?;
        Ok(match cli.command {
            Some(Commands::Review(r)) => r.timeout,
            _ => cli.review.timeout,
        })
    }

    #[test]
    fn timeout_defaults_to_300_seconds_on_both_entry_points() {
        assert_eq!(timeout_of(&["review"]).unwrap(), 300);
        assert_eq!(timeout_of(&[]).unwrap(), 300);
    }

    #[test]
    fn timeout_accepts_the_range_limits_and_values_inside() {
        for entry in [&["review"][..], &[][..]] {
            for n in ["10", "11", "900", "3599", "3600"] {
                let mut a = entry.to_vec();
                a.extend_from_slice(&["--timeout", n]);
                assert_eq!(timeout_of(&a).unwrap(), n.parse::<u64>().unwrap(), "{n} は範囲内のはず");
            }
        }
    }

    #[test]
    fn timeout_rejects_values_just_outside_the_range_and_names_the_range() {
        for entry in [&["review"][..], &[][..]] {
            for n in ["9", "3601", "0", "99999"] {
                let mut a = entry.to_vec();
                a.extend_from_slice(&["--timeout", n]);
                let e = timeout_of(&a).expect_err(&format!("{n} は範囲外のはず"));
                assert_eq!(e.kind(), clap::error::ErrorKind::ValueValidation, "{n}: {e}");
                assert!(e.to_string().contains("10..=3600"), "エラーに範囲が出る: {e}");
            }
        }
    }

    #[test]
    fn timeout_rejects_non_numbers_negative_numbers_and_a_missing_value() {
        for bad in ["abc", "", "1.5", "-5", "10s"] {
            let e = timeout_of(&["review", &format!("--timeout={bad}")]).expect_err(&format!("{bad:?} は数ではない/負"));
            assert_eq!(e.kind(), clap::error::ErrorKind::ValueValidation, "{bad:?}: {e}");
        }
        let e = timeout_of(&["review", "--timeout"]).expect_err("値が無い");
        assert!(e.to_string().contains("--timeout"), "値が無いときも、どのオプションかを言う: {e}");
    }
}
