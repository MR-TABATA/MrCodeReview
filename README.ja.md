# MrCodeReview CLI (`mrcr`)

**AI が書いた変更を、コミットする前に潰す。** 
コミット前の未コミット変更を、観点（バグ・例外処理・セキュリティ・性能・テスト・整合性・Terraform 5 観点）ごとに AI にレビューさせ、引用行の厳密照合（ハルシネーション検証）を行うオープンソース CLI ツールです。

[English](README.md)

---

## 特徴

- **自分の API キーで直接（BYOK）**: 中継サーバーなし。`ANTHROPIC_API_KEY` 環境変数で手元から Anthropic API を直接呼び出します。
- **観点を混ぜない（1回1実行の原則）**: 1 つのプロンプトに複数の観点を混ぜず、観点ごとに集中して高精度にレビュー。結果は 1 つのレポートに束ねます。
- **でっち上げ（ハルシネーション）の即時検出**: モデルが引用した行が実際の差分に存在するかを文字列照合し、「根拠あり」「⚠️ 根拠不一致」を判定します。
- **コーディングエージェントとの完全連携**: レビュー結果を `.mrcodereview/out/review.md`（または JSON）に自動出力。`agy` や `Claude Code` にそのまま読ませて自動修正ループを回せます。
- **Mac / Linux クロスプラットフォーム**: Pure Rust 実装で、手元のターミナルはもちろん GitHub Actions などの CI/CD でも軽快に動作します。
- **GUI アプリとの履歴共有**: macOS で実行した場合、有償 GUI アプリ（MrkCodeReview）と同じローカル SQLite DB にレビュー結果が記録され、後から画面で仕分け・確認が可能です。

---

## インストール

### Homebrew (macOS)

```bash
brew install mr-tabata/tap/mrcr
```

### Cargo でビルド

```bash
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

# 全観点を一括実行
mrcr review --all

# 結果を標準出力（stdout）に出す
mrcr review -r bugs --stdout

# JSON 形式で出力（CI / スクリプト用）
mrcr review -r bugs -f json --stdout

# 差分の比較元を指定（例: HEAD~1 からの変更）
mrcr review -b HEAD~1
```

### 3. 利用可能な観点一覧

```bash
mrcr recipes
```

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
