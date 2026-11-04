<p align="center"><img src="art/icon.png" alt="MrCodeReview" width="128"></p>

# MrCodeReview CLI (`mrcr`)

**コミットする前に、紛れ込んだバグを潰す。** 
コミット前の未コミット変更を、観点（バグ・例外処理・セキュリティ・性能・テスト・整合性・Terraform 5 観点）ごとに AI にレビューさせ、引用行の厳密照合（ハルシネーション検証）を行うオープンソース CLI ツールです。

[English](README.md)

![mrcr review の流れ](art/flow.svg)

---

## 特徴

- **自分の API キーで直接（BYOK）**: 中継サーバーなし。`ANTHROPIC_API_KEY` 環境変数で手元から Anthropic API を直接呼び出します。
- **観点を混ぜない（1回1実行の原則）**: 1 つのプロンプトに複数の観点を混ぜず、観点ごとに集中して高精度にレビュー。結果は 1 つのレポートに束ねます。
- **でっち上げ（ハルシネーション）の即時検出**: モデルが引用した行が実際の差分に存在するかを文字列照合し、「根拠あり」「⚠️ 根拠不一致」を判定します。
- **コーディングエージェントとの完全連携**: レビュー結果を `.mrcodereview/out/review.md`（または JSON）に自動出力。`agy` や `Claude Code` にそのまま読ませて自動修正ループを回せます。
- **Pure Rust 実装**: macOS でビルド・テストし、リリースも macOS 向けです。Linux や CI/CD（GitHub Actions など）は、まだ動作を確かめていません。
- **GUI アプリとの履歴共有**: macOS で実行した場合、有償 GUI アプリ（MrkCodeReview）と同じローカル SQLite DB にレビュー結果が記録され、後から画面で仕分け・確認が可能です。

---

## インストール

### Homebrew (macOS)

```bash
brew install mr-tabata/tap/mrcr
```

### Cargo でビルド

[Rust のツールチェーン](https://rustup.rs)が必要です。このリポジトリを clone して、そのルートで実行します。

```bash
git clone https://github.com/MR-TABATA/MrCodeReview.git
cd MrCodeReview
cargo install --path crates/mrcr
```

---

## 使い方

### 1. API キーの設定
```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

### 2. レビューの実行

```bash
# 未コミットの変更を「潜在バグ」観点でレビュー（既定）
mrcr review

# 複数観点をカンマ区切りで実行（バグとセキュリティ）
mrcr review -r bugs,security

# 全観点を一括実行（組み込み＋`.mrcodereview/recipes/` に置いた独自のレシピ）
mrcr review --all

# 結果を標準出力（stdout）に出す
mrcr review -r bugs --stdout

# JSON 形式で出力（CI / スクリプト用）
mrcr review -r bugs -f json --stdout

# 差分の比較元を指定（例: HEAD~1 からの変更）
mrcr review -b HEAD~1

# LLM の返事を待つ時間の上限を延ばす（秒。10〜3600。既定 300）
mrcr review --all --timeout 900
```

### 3. 利用可能な観点一覧

```bash
mrcr recipes
```

---

## 実行例

意図的に危ない変更を入れたデモの Terraform（DB パスワードの直書き、5432 番ポートの `0.0.0.0/0` への開放、削除保護と最終スナップショットの無効化、DB の identifier の変更）を、次のコマンドでレビューした例です。

```bash
mrcr review -r tf-security,tf-replace -l en
```

![mrcr review の出力例](art/review-demo.png)

指摘ごとに、差分からの引用行が付きます。引用行が実際の差分にあれば `Verified`（根拠あり）、なければ `Unverified evidence`（根拠不一致）と区別されます。画像は、サマリーと、6 件のうち最初の 2 件です。差分は作ったデモで、実際のコードではありません。画像は `-l en`（英語）の出力です。`-l ja`（既定）なら、見出しとラベルも日本語になります。

---

## レシピ（レビュー観点）一覧

レシピは、**1 つの観点だけ**を見るプロンプトです。`-r <id>[,<id>…]` で選び、`--all` で全部を実行します。
差分（観点によっては、差分の外の関連コードも）から判断し、指摘ごとに実在する行の引用を求めます。

### 汎用

| ID&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; | 見るもの |
|---|---|
| `bugs` | 今は動くが、特定の入力で壊れるコード。1 つのずれ、null・空の扱い、単位・符号・桁あふれ、条件式の誤り、状態の更新漏れ、非同期の競合、閉じ忘れ、文字コード・タイムゾーン、コピーの直し忘れ |
| `errors` | 失敗が握りつぶされる・成功扱いになる・追えなくなるコード。空の `catch`、エラー時の代替値、戻り値・終了コードの未確認、見えない非同期の失敗、原因情報の破棄、`unwrap`、タイムアウト・再試行の上限なし |
| `security` | インジェクション（SQL・コマンド・パス・XSS）、認証・認可の抜け、ハードコードされた秘密、未検証の入力（SSRF・デシリアライズ）、暗号の誤用 |
| `performance` | データが増えると遅くなる・重くなるコード。計算量、N+1・ループ内 I/O、直列の非同期、不要なコピー、メモリの増え続け、UI スレッドのブロック、正規表現のバックトラック、索引の不足 |
| `tests` | 差分内のテストで確かめられていない挙動の変更。未テストの分岐・失敗系、古いままのテスト、弱い・自明に通る検証、時刻・乱数・`sleep` に頼る壊れやすいテスト、回帰テストのない修正 |
| `consistency` | 変えていないコードとの矛盾。シグネチャ・戻り値を変えたのに古い呼び出し元、改名したのに残る参照、対の定義の片側だけの更新。差分の外の関連コードも使う |

### Terraform / HCL

| ID&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp;&nbsp; | 見るもの |
|---|---|
| `tf-state` | アドレスと state の対応。`count`/`for_each` の切り替え・キー変更、`moved` のない改名、`import`/`removed` の誤り |
| `tf-replace` | 意図しない再作成・データ消失。再作成を強いる属性変更、`lifecycle` の誤用、適用順による停止、保護のないデータ保持リソースの削除 |
| `tf-repro` | 環境や時間で結果が変わる点。バージョン制約の欠落・緩さ、ロックファイルのずれ、メジャー更新、`timestamp()`・「最新」の取得 |
| `tf-security` | 広すぎる IAM、外部公開（`0.0.0.0/0`・公開バケット）、暗号化・ログの無効化、ハードコード・`sensitive` 抜けの秘密 |
| `tf-refs` | variable / output / module の参照が、ファイルや環境をまたいで繋がっているか。差分の外の関連ファイルも使う |

Terraform のレシピは、`terraform plan`・state・プロバイダのスキーマを見たふりをしません。差分だけから判断します。

### カスタマイズ

リポジトリの `.mrcodereview/recipes/<id>.md` に Markdown を置きます。

- 組み込みと**同じ ID** → 組み込みを**上書き**（`mrcr recipes` の提供元に `repo` と出ます）。
- **新しい ID** → 独自の観点を**追加**。

組み込みのプロンプトは [`recipes/`](recipes)（日本語。既定）と [`recipes/en/`](recipes/en)（英語。`--lang en`）にあります。

---

## コーディングエージェント（agy / Claude Code）連携

```bash
# 1. レビューを実行（.mrcodereview/out/review.md が生成される）
mrcr review -r bugs

# 2. エージェントにそのまま直させる
agy ".mrcodereview/out/review.md の指摘を確認してコードを修正して"
```

---

## ライセンス

MIT License
