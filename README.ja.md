<p align="center"><img src="art/icon.png" alt="MrCodeReview" width="128"></p>

# MrCodeReview

**コミットする前に、紛れ込んだバグを潰す。**  
コミット前の未コミット変更を、観点（バグ・例外処理・セキュリティ・性能・テスト・整合性・Terraform 5 観点・文書 3 観点）ごとに AI にレビューさせ、引用行の厳密照合（ハルシネーション検証）を行うオープンソース開発ツールです。

**Mac デスクトップアプリ（GUI）** と **ターミナル用 CLI（`mrcr`）** の両方を、制限なし・完全無料で提供しています。

[English](README.md)

<p align="center">
  <img src="art/hero.png" alt="MrCodeReview デスクトップアプリの画面" width="900">
</p>

---

## 特徴

- **GUI も CLI も好みに合わせて使える**: どちらも MIT ライセンス、制限なしにフル機能。履歴や仕分けデータは同じローカル SQLite DB でシームレスに共有されます。
- **自分の API キーで直接（BYOK）**: 中継サーバーなし。手元の環境変数や macOS キーチェーンから Anthropic API を直接呼び出します（完全ローカル完結・プライバシー安全）。
- **観点を混ぜない（1回1実行の原則）**: 1 つのプロンプトに複数の観点を混ぜず、観点ごとに集中して高精度にレビュー。結果は 1 つのレポートに束ねます。
- **でっち上げ（ハルシネーション）の即時検出**: モデルが引用した行が実際の差分に存在するかを文字列照合し、「根拠あり」「⚠️ 根拠不一致」を自動判定します。
- **コーディングエージェントとの完全連携**: レビュー結果を `.mrcodereview/out/review.md`（または JSON）に自動出力。`Claude Code` や `agy` にそのまま読ませて自動修正ループを回せます。
- **コミットのあとの自動レビュー（Git フック）＆ 実行ログ**: デスクトップアプリの設定（Git フック）か `mrcr hook enable` で ON にします。直前のコミットだけを、バックグラウンドでレビューし、コミットは止めません。結果は初期画面の実行ログタイムラインに日付ごとに並び、画面遷移なしでインライン展開して指摘の確認・仕分けができます。
- **Pure Rust & Tauri v2 実装**: 超軽量・高速。Apple Silicon ネイティブ対応（Developer ID 署名 ＋ Apple 公証済み）。

<p align="center">
  <img src="art/runlog-feed.png" alt="MrCodeReview 初期画面の実行ログタイムラインとインライン展開" width="900">
</p>

---

## インストール

### 1. Mac デスクトップアプリ（GUI）

公式サイトまたは GitHub Releases から DMG をダウンロードしてインストールできます。

- **[公式サイトからダウンロード（mrktools.app）](https://mrktools.app/mrcodereview/download/)**
- **[GitHub Releases からダウンロード](https://github.com/MR-TABATA/MrCodeReview/releases)**

> **動作環境**: macOS 13.0 以降（Apple Silicon）

### 2. CLI ツール (`mrcr`)

#### Homebrew (macOS)

```bash
brew install mr-tabata/tap/mrcr
```

#### npx（macOS・インストール不要）

Node.js 18 以降が要ります。版は固定してください。版を付けないと、`npx` は起動のたびに最新のコードを実行します。

```bash
npx @mr-tabata/mrcr@1.13.1 --help
```

いまは macOS だけです。バイナリは universal（Apple Silicon と Intel）ですが、動作を確かめたのは Apple Silicon だけです。

#### Cargo でビルド

```bash
git clone https://github.com/MR-TABATA/MrCodeReview.git
cd MrCodeReview
cargo install --path crates/mrcr
```

---

## 使い方

### 1. API キーの設定

中継サーバーはありません。手元の API キーで直接 LLM を呼び出します。

- **デスクトップアプリ**: 画面右上の「設定（⌘,）」から Anthropic API キーを入力（macOS キーチェーンに安全に保存されます）。
- **CLI**: 環境変数に設定します。
  ```bash
  export ANTHROPIC_API_KEY="sk-ant-..."
  ```

---

### 2. デスクトップアプリでの操作

1. アプリを起動し、左サイドバーに Git リポジトリを追加（または選択）します。
2. 上部の観点バー（`潜在バグ`、`セキュリティ` など）を選び、**「レビュー実行」** を押します。
3. 出てきた指摘を「対応」「保留」「却下」に仕分けます。
4. **「CLI に渡す」** ボタンを押すと、Claude Code や agy に渡すプロンプトと Handoff ファイルが自動生成されます。

---

### 3. CLI (`mrcr`) での操作

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

# 直近のコミット（HEAD~1..HEAD）だけ、または指定した 1 コミットだけをレビュー（未コミットの変更は含めない）
mrcr review --commit
mrcr review --commit <ハッシュ>

# 仕様書・設計書などの文書ファイル（.md / .txt）を直接レビュー（未コミット差分ではなく文書全体を検査）
mrcr review docs/spec.md
mrcr review docs/spec.md -r doc-core,doc-claims

# 利用可能な観点一覧を表示
mrcr recipes
```

---

### 4. コミットのあとの自動レビュー（Git フック）

デスクトップアプリ（リポジトリを選んで、**設定 → Git フック**）か、CLI で ON にします。

```bash
mrcr hook enable -r bugs,errors   # フックを入れて ON にする（観点は保存される）
mrcr hook status                  # ON/OFF・観点・フックが呼ぶ mrcr・直近の実行
mrcr hook log                     # 最近の実行（コミット・観点ごとの件数と時間。-n 件数、--json）
mrcr hook disable                 # フックを外す（観点の設定は残る）
```

- **動き**: そのリポジトリで `git commit` するたびに、`post-commit` フックが、**そのコミットだけ**（`mrcr review --commit <ハッシュ>`）を、バックグラウンドで、観点を 1 つずつレビューします。1 つの観点が失敗しても、残りは走ります。結果は、通常のレビュー履歴に入るので、デスクトップアプリで仕分けできます。コミットは**止めません**（止める機能は、まだありません）。
- **既存のフックは壊しません**: `.git/hooks/post-commit` に、印の付いた範囲だけを足す（外す）形です。`core.hooksPath` が `.git` の外を指すリポジトリ（husky など）は、書き換えずに、断ります。
- **設計書が要る観点**（`spec` など）には、リポジトリ内の設計書（`.md` / `.txt`、相対パス）が要ります。「Git フック」タブか、`mrcr hook enable -r spec --spec docs/spec.md` で指定します。
- **設定は、アプリのデータ置き場**（`hooks.json`、実行ログは `hooks.log`）に保存し、リポジトリの中には置きません。clone したリポジトリの設定で、コミットのたびに走る内容が変わることはありません。

**必要なものと制限**

- `mrcr`（CLI）が入っていること（Homebrew、または `npm i -g @mr-tabata/mrcr`）。`npx` で起動したものは、一時のキャッシュにあるので、フックからは使えません。
- フックから呼ばれる CLI は、**環境変数 `ANTHROPIC_API_KEY`** を読みます。デスクトップアプリに保存したキー（キーチェーン）は、見えません。キーが無いときは、実行ログに失敗として記録されます（コミットには、影響しません）。
- **コミットのたびに、選んだ観点の数だけ、LLM の API を呼びます**（従量課金）。必要な観点だけを選んでください。
- macOS のみ。`git commit --amend` や rebase での動きは、確認していません。

---

## CLI の実行例

意図的に危ない変更を入れたデモの Terraform（DB パスワードの直書き、5432 番ポートの `0.0.0.0/0` への開放、削除保護と最終スナップショットの無効化、DB の identifier の変更）をレビューした例です。

```bash
mrcr review -r tf-security,tf-replace -l en
```

![mrcr review の出力例](art/review-demo.png)

指摘ごとに、差分からの引用行が付きます。引用行が実際の差分にあれば `Verified`（根拠あり）、なければ `Unverified evidence`（根拠不一致）と区別されます。

---

## レシピ（レビュー観点）一覧

レシピは、**1 つの観点だけ**を見るプロンプトです。`-r <id>[,<id>…]` で選び、`--all` で全部を実行します。
差分（観点によっては、差分の外の関連コードも）から判断し、指摘ごとに実在する行の引用を求めます。

### 汎用コード（6 観点）

| ID | 見るもの |
|---|---|
| `bugs` | 今は動くが、特定の入力で壊れるコード。1 つのずれ、null・空の扱い、単位・符号・桁あふれ、条件式の誤り、状態の更新漏れ、非同期の競合、閉じ忘れ、文字コード・タイムゾーン、コピーの直し忘れ |
| `errors` | 失敗が握りつぶされる・成功扱いになる・追えなくなるコード。空の `catch`、エラー時の代替値、戻り値・終了コードの未確認、見えない非同期の失敗、原因情報の破棄、`unwrap`、タイムアウト・再試行の上限なし |
| `security` | インジェクション（SQL・コマンド・パス・XSS）、認証・認可の抜け、ハードコードされた秘密、未検証の入力（SSRF・デシリアライズ）、暗号の誤用 |
| `performance` | データが増えると遅くなる・重くなるコード。計算量、N+1・ループ内 I/O、直列の非同期、不要なコピー、メモリの増え続け、UI スレッドのブロック、正規表現のバックトラック、索引の不足 |
| `tests` | 差分内のテストで確かめられていない挙動の変更。未テストの分岐・失敗系、古いままのテスト、弱い・自明に通る検証、時刻・乱数・`sleep` に頼る壊れやすいテスト、回帰テストのない修正 |
| `consistency` | 変えていないコードとの矛盾。シグネチャ・戻り値を変えたのに古い呼び出し元、改名したのに残る参照、対の定義の片側だけの更新。差分の外の関連コードも使う |

### Terraform / HCL（5 観点）

| ID | 見るもの |
|---|---|
| `tf-state` | アドレスと state の対応。`count`/`for_each` の切り替え・キー変更、`moved` のない改名、`import`/`removed` の誤り |
| `tf-replace` | 意図しない再作成・データ消失。再作成を強いる属性変更、`lifecycle` の誤用、適用順による停止、保護のないデータ保持リソースの削除 |
| `tf-repro` | 環境や時間で結果が変わる点。バージョン制約の欠落・緩さ、ロックファイルのずれ、メジャー更新、`timestamp()`・「最新」の取得 |
| `tf-security` | 広すぎる IAM、外部公開（`0.0.0.0/0`・公開バケット）、暗号化・ログの無効化、ハードコード・`sensitive` 抜けの秘密 |
| `tf-refs` | variable / output / module の参照が、ファイルや環境をまたいで繋がっているか。差分の外の関連ファイルも使う |

### 文書（3 観点）

設計メモ・仕様書・README 向けです。差分に `.md`・`.mdx`・`.txt`・`.rst` があるときに自動判定されます。

| ID | 見るもの |
|---|---|
| `doc-core` | 設計・仕様の目的、それによって何が変わるか、他の手段・代替案の検証不足、「それホントに必要？」の確認 |
| `doc-claims` | 出典・測定・確認の方法がないまま書かれた、数字・結果・言い切り（「必ず」「無い」「無関係」など） |
| `doc-reality`（実験的） | 確かめた形跡がないまま事実として書かれた、外部のコマンド・パッケージ・API の名前、バージョン、料金、制限 |
| `doc-steps` | 読み手が行動に移れない箇所。前提の抜け、「後で決める」を決まったものとして使う、理由や戻し方のない決定、食い違う状態の記述 |

### カスタマイズ（独自レシピの追加）

リポジトリの `.mrcodereview/recipes/<id>.md` に Markdown を置くだけで自由に拡張できます。

- 組み込みと**同じ ID** → 組み込みを**上書き**
- **新しい ID** → 独自の観点を**追加**

組み込みのプロンプトは [`recipes/`](recipes)（日本語。既定）と [`recipes/en/`](recipes/en)（英語。`--lang en`）にあります。

---

## コーディングエージェント（Claude Code / agy）連携

### 1. シンプルな連携
```bash
# レビューを実行（.mrcodereview/out/review.md が生成される）
mrcr review -r bugs

# エージェントにそのまま直させる
claude ".mrcodereview/out/review.md の指摘を確認してコードを修正して"
```

### 2. 人間が仕分けた指摘だけを直させる（`mrcr handoff`）
1. デスクトップアプリ（または CLI）でレビューし、直してほしい指摘だけを **「対応」** に仕分けます。
2. エージェントが未対応の指摘を取得して修正します。
   ```bash
   mrcr handoff get             # 直す指摘をまとめて取得
   mrcr handoff get --json      # 構造化した JSON で取得
   ```
3. エージェントが結果を報告します。
   ```bash
   mrcr handoff report <review_id> 1,2 --did --text "何を直したか"
   ```
4. 報告はデスクトップアプリの画面にリアルタイムで反映されます。

> Claude Code 用のスキルを [`skills/mrcr-handoff`](skills/mrcr-handoff) に同梱しています。

### 3. MCP（Model Context Protocol）による双方向連携（推奨）

`mrcr mcp` により、Claude Code や `agy`（Google Antigravity）、VS Code Copilot から直接 MCR の SQLite DB を読み書きできます。手動でファイルを開いたり取り込みボタンを押す必要はありません。

```bash
# agy や Claude Code の設定ファイルに自動登録
mrcr mcp setup
```

`mrcr` をインストールせずに、`npx` でも使えます。ここでも版を固定してください。

```bash
npx @mr-tabata/mrcr@1.13.1 mcp setup
```

設定を自分で書く場合（たとえば `.vscode/mcp.json`）:

```json
{
  "mcpServers": {
    "mrcodereview": {
      "command": "npx",
      "args": ["-y", "@mr-tabata/mrcr@1.13.1", "mcp"]
    }
  }
}
```

版を上げたときは、この設定の版も、自分で更新してください。新しい版はレビューの DB を更新し、固定した古い版は、新しい DB を開けません（「このアプリを更新してください」と出ます）。Dock や Finder から起動したエージェントは、`PATH` に `npx` が無いことがあります。その場合は、`command` に `npx` のフルパスを書いてください。

登録後は、普段通りターミナルでエージェントに伝えるだけです：
> 「MrDiff の指摘を確認して直して」

エージェントが自動で指摘一覧を読み取り、コードを修正し、MCR の DB に「対応完了」を書き戻します。デスクトップアプリの画面もリアルタイムに「対応済み」に同期します。

---

## ライセンス

MIT License © 2026 Hitoshi TABATA
